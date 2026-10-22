pub use sag_protocol::explore::*;
use crate::{
    store::Store,
    text,
    types::DocumentTerm,
};
use anyhow::{bail, Result};
use indexmap::IndexMap;
use rusqlite::{params, OptionalExtension};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;
use unicode_normalization::UnicodeNormalization;

pub const VERSION: &str = "local-tfidf-v1";
pub const OVERVIEW_LIMIT: usize = 24;
pub const NODE_LIMIT: usize = 80;
pub const PAGE_SIZE: usize = 12;







#[derive(Clone)]
struct Neighborhood {
    scope: String,
    version: i64,
    center: ExploreDocument,
    documents: Vec<ExploreDocument>,
    edges: Vec<ExploreEdge>,
    candidate_limit_reached: bool,
}



pub struct Explorer {
    store: Arc<Store>,
    snapshots: Mutex<IndexMap<String, Neighborhood>>,
    frequencies: Mutex<HashMap<(String, i64, String), usize>>,
    graphs: Mutex<IndexMap<String, ExploreGraph>>,
}
impl Explorer {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            snapshots: Mutex::new(IndexMap::new()),
            frequencies: Mutex::new(HashMap::new()),
            graphs: Mutex::new(IndexMap::new()),
        }
    }
    fn scope(&self, scope: &str) -> Result<String> {
        let path = self.store.authorize(Path::new(scope))?;
        if !path.is_dir() {
            bail!("请选择一个已接入的文件夹")
        }
        let settings = self.store.settings()?;
        let roots = self.store.roots()?;
        let allowed = roots
            .iter()
            .filter(|root| path.starts_with(&root.path))
            .any(|root| {
                !text::in_hidden_directory(Path::new(&root.path), &path, true)
                    && !path
                        .strip_prefix(&root.path)
                        .expect("authorized root")
                        .components()
                        .any(|part| {
                            settings
                                .excluded
                                .iter()
                                .any(|name| part.as_os_str() == name.as_str())
                        })
            });
        if !allowed {
            bail!("此文件夹不参与索引，无法探索")
        }
        crate::store::path_text(&path)
    }
    fn documents(&self, scope: &str) -> Result<Vec<ExploreDocument>> {
        let settings = self.store.settings()?;
        let db = self.store.connection();
        let mut query = db.prepare("SELECT id,path,name,hash FROM documents WHERE hash<>'' AND instr(replace(path,char(92),'/')||'/',?1)=1 ORDER BY name COLLATE NOCASE,path")?;
        let prefix = format!("{}/", scope.replace('\\', "/").trim_end_matches('/'));
        let documents = query
            .query_map(params![prefix], |row| {
                Ok(ExploreDocument {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    name: row.get(2)?,
                    hash: row.get(3)?,
                    file_type: String::new(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(documents
            .into_iter()
            .filter_map(|mut doc| {
                let path = Path::new(&doc.path);
                let relative = path.strip_prefix(scope).expect("scoped document path");
                if !text::is_indexable(path, &settings.file_types)
                    || text::in_hidden_directory(Path::new(scope), path, false)
                    || relative.components().any(|part| {
                        settings
                            .excluded
                            .iter()
                            .any(|name| part.as_os_str() == name.as_str())
                    })
                {
                    return None;
                }
                doc.file_type = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                Some(doc)
            })
            .collect())
    }
    fn consistent(&self, version: i64) -> Result<()> {
        if self.store.version()? != version {
            bail!("文件已更新，请刷新探索")
        }
        Ok(())
    }
    pub fn overview(&self, scope: &str, query: &str, visible: &[i64]) -> Result<ExploreOverview> {
        if query.chars().count() > 200 || visible.len() > NODE_LIMIT {
            bail!("探索请求超出允许范围")
        }
        let scope = self.scope(scope)?;
        let version = self.store.version()?;
        let documents = self.documents(&scope)?;
        let total = documents.len();
        let needle = query.to_lowercase();
        let known_documents = documents
            .iter()
            .filter(|doc| visible.contains(&doc.id))
            .cloned()
            .collect();
        let documents = documents
            .into_iter()
            .filter(|doc| doc.name.to_lowercase().contains(&needle))
            .take(OVERVIEW_LIMIT)
            .collect();
        let roots = self.store.roots()?;
        let active_roots = roots
            .iter()
            .filter(|root| Path::new(&scope).starts_with(&root.path));
        let indexing = active_roots
            .clone()
            .any(|root| !matches!(root.status.as_str(), "ready" | "partial" | "error"));
        let partial = active_roots
            .clone()
            .any(|root| matches!(root.status.as_str(), "partial" | "error"));
        self.consistent(version)?;
        Ok(ExploreOverview {
            scope,
            version,
            algorithm: VERSION.into(),
            total,
            documents,
            known_documents,
            indexing,
            partial,
        })
    }
    pub fn graph(&self, scope: &str, cancel: &CancellationToken) -> Result<ExploreGraph> {
        let scope = self.scope(scope)?;
        let version = self.store.version()?;
        if let Some(saved) = self.graphs.lock().expect("graph cache mutex poisoned")
            .get(&scope).filter(|saved| saved.version == version).cloned() {
            return Ok(saved);
        }
        let documents = self.documents(&scope)?;
        let mut summaries = Vec::with_capacity(documents.len());
        let mut frequencies: HashMap<String, usize> = HashMap::new();
        for document in &documents {
            if cancel.is_cancelled() { bail!("已取消探索") }
            let terms = self.summary(document)?;
            for term in &terms { *frequencies.entry(term.term.clone()).or_default() += 1; }
            summaries.push(terms);
        }
        let count = documents.len() as f64;
        let mut postings: HashMap<String, Vec<(usize, f64)>> = HashMap::new();
        let vectors: Vec<BTreeMap<String, f64>> = summaries.iter().enumerate().map(|(index, terms)| {
            let mut vector: BTreeMap<_, _> = terms.iter().map(|term| {
                let df = frequencies[&term.term] as f64;
                let idf = (1.0 + (count - df + 0.5) / (df + 0.5)).ln();
                (term.term.clone(), term.boost * (1.0 + (term.count as f64).ln()) * idf)
            }).collect();
            let norm = vector.values().map(|weight| weight * weight).sum::<f64>().sqrt();
            for (term, weight) in &mut vector {
                *weight /= norm;
                postings.entry(term.clone()).or_default().push((index, *weight));
            }
            vector
        }).collect();
        // Bound candidate work for common vocabulary without limiting document coverage.
        for entries in postings.values_mut() {
            entries.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            entries.truncate(101);
        }
        let snapshot = uuid::Uuid::new_v4().to_string();
        let mut edges = BTreeMap::new();
        for (index, vector) in vectors.iter().enumerate() {
            if cancel.is_cancelled() { bail!("已取消探索") }
            let mut candidates: HashMap<usize, f64> = HashMap::new();
            for (term, weight) in vector {
                for (other, other_weight) in &postings[term] {
                    if *other != index { *candidates.entry(*other).or_default() += weight * other_weight; }
                }
            }
            let mut candidates: Vec<_> = candidates.into_iter().collect();
            candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            candidates.truncate(100);
            let mut ranked = Vec::new();
            for (other, _) in candidates {
                let mut shared: Vec<_> = vector.iter().filter_map(|(term, weight)|
                    vectors[other].get(term).map(|other_weight| (term.clone(), weight * other_weight))
                ).collect();
                let score: f64 = shared.iter().map(|(_, weight)| weight).sum();
                shared.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
                let mut terms: Vec<String> = Vec::new();
                for (term, _) in shared {
                    if !terms.iter().any(|existing| existing.contains(&term) || term.contains(existing)) {
                        terms.push(term);
                    }
                    if terms.len() == 16 { break; }
                }
                if score <= 0.0 || terms.is_empty() || (terms.len() == 1 && terms[0].chars().count() < 4) { continue; }
                ranked.push((other, score, terms));
            }
            ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            // A sparse union of each document's strongest five relationships.
            for (other, score, terms) in ranked.into_iter().take(5) {
                let (left, right) = if documents[index].id < documents[other].id {
                    (&documents[index], &documents[other])
                } else { (&documents[other], &documents[index]) };
                edges.entry((left.id, right.id)).or_insert_with(|| ExploreEdge {
                    source: left.id, target: right.id, source_hash: left.hash.clone(),
                    target_hash: right.hash.clone(), score, terms, snapshot: snapshot.clone(),
                });
            }
        }
        self.consistent(version)?;
        let result = ExploreGraph { scope: scope.clone(), version, snapshot,
            documents, edges: edges.into_values().collect() };
        let mut cache = self.graphs.lock().expect("graph cache mutex poisoned");
        cache.retain(|_, saved| saved.version == version);
        cache.insert(scope, result.clone());
        if cache.len() > 8 { cache.shift_remove_index(0); }
        Ok(result)
    }
    fn summary(&self, document: &ExploreDocument) -> Result<Vec<DocumentTerm>> {
        let saved: Option<String> = self.store.connection().query_row(
            "SELECT terms FROM explore_summaries WHERE document_id=?1 AND hash=?2 AND algorithm=?3",
            params![document.id, document.hash, text::EXPLORE_SUMMARY_VERSION], |row| row.get(0),
        ).optional()?;
        if let Some(saved) = saved {
            return Ok(serde_json::from_str(&saved)?);
        }
        let source = self.store.document(Path::new(&document.path))?;
        if source.hash != document.hash {
            bail!("文件已更新，请刷新探索")
        }
        let chunks = text::chunks_for_file(
            &source.body,
            self.store.settings()?.retrieval.chunk_chars,
            Path::new(&document.path),
        );
        let terms = text::explore_summary(
            Path::new(&document.path),
            chunks
                .iter()
                .map(|chunk| (chunk.heading.as_str(), chunk.content.as_str())),
        );
        let db = self.store.connection();
        db.execute("INSERT INTO explore_summaries(document_id,hash,algorithm,terms) SELECT ?1,?2,?3,?4 WHERE EXISTS(SELECT 1 FROM documents WHERE id=?1 AND hash=?2) ON CONFLICT(document_id) DO UPDATE SET hash=excluded.hash,algorithm=excluded.algorithm,terms=excluded.terms",
            params![document.id, document.hash, text::EXPLORE_SUMMARY_VERSION, serde_json::to_string(&terms)?])?;
        Ok(terms)
    }
    fn page(
        snapshot: &str,
        neighborhood: &Neighborhood,
        offset: usize,
    ) -> Result<ExploreNeighbors> {
        if offset > neighborhood.documents.len() {
            bail!("无效的探索分页位置")
        }
        let end = (offset + PAGE_SIZE).min(neighborhood.documents.len());
        Ok(ExploreNeighbors {
            version: neighborhood.version,
            algorithm: VERSION.into(),
            snapshot: snapshot.into(),
            center: neighborhood.center.clone(),
            documents: neighborhood.documents[offset..end].to_vec(),
            edges: neighborhood.edges[offset..end].to_vec(),
            total: neighborhood.documents.len(),
            has_more: end < neighborhood.documents.len(),
            next_offset: end,
            candidate_limit_reached: neighborhood.candidate_limit_reached,
        })
    }
    pub fn neighbors(
        &self,
        scope: &str,
        center_id: i64,
        snapshot: Option<&str>,
        offset: usize,
        cancel: &CancellationToken,
    ) -> Result<ExploreNeighbors> {
        let scope = self.scope(scope)?;
        let version = self.store.version()?;
        if cancel.is_cancelled() {
            bail!("已取消探索")
        }
        {
            let mut cache = self
                .snapshots
                .lock()
                .expect("explore snapshot mutex poisoned");
            cache.retain(|_, value| value.version == version);
            if let Some(id) = snapshot {
                let saved = cache
                    .get(id)
                    .ok_or_else(|| anyhow::anyhow!("关联结果已过期，请重新设为中心"))?;
                if saved.scope != scope || saved.center.id != center_id {
                    bail!("探索快照与当前范围不一致")
                }
                self.consistent(version)?;
                return Self::page(id, saved, offset);
            }
            if let Some((id, saved)) = cache
                .iter()
                .find(|(_, value)| value.scope == scope && value.center.id == center_id)
            {
                self.consistent(version)?;
                return Self::page(id, saved, offset);
            }
        }
        if offset != 0 {
            bail!("继续展开需要有效的探索快照")
        }
        let documents = self.documents(&scope)?;
        let by_id: HashMap<_, _> = documents.iter().map(|doc| (doc.id, doc)).collect();
        let center = by_id
            .get(&center_id)
            .ok_or_else(|| anyhow::anyhow!("中心文档已移除或不在当前范围"))?;
        let center_terms = self.summary(center)?;
        let expression = format!(
            "body : ({})",
            center_terms
                .iter()
                .map(|term| text::quote(&term.term))
                .collect::<Vec<_>>()
                .join(" OR ")
        );
        let candidate_ids = if center_terms.is_empty() {
            vec![]
        } else {
            let db = self.store.connection();
            let mut query = db.prepare("WITH hits AS MATERIALIZED (SELECT c.document_id,bm25(chunk_fts,0,0,1) AS score FROM chunk_fts JOIN chunks c ON c.id=chunk_fts.rowid JOIN documents d ON d.id=c.document_id WHERE chunk_fts MATCH ?1 AND d.id<>?2 AND d.id IN (SELECT value FROM json_each(?4)) AND instr(replace(d.path,char(92),'/')||'/',?3)=1) SELECT document_id FROM hits GROUP BY document_id ORDER BY min(score),document_id LIMIT 100")?;
            let ids = query
                .query_map(
                    params![
                        expression,
                        center_id,
                        format!("{}/", scope.replace('\\', "/").trim_end_matches('/')),
                        serde_json::to_string(
                            &documents.iter().map(|doc| doc.id).collect::<Vec<_>>()
                        )?
                    ],
                    |row| row.get::<_, i64>(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids
        };
        let candidate_limit_reached = candidate_ids.len() == 100;
        let mut summaries = vec![(center_id, center_terms)];
        for id in candidate_ids {
            if cancel.is_cancelled() {
                bail!("已取消探索")
            }
            if let Some(doc) = by_id.get(&id) {
                summaries.push((id, self.summary(doc)?));
            }
        }
        let vocabulary: HashSet<_> = summaries
            .iter()
            .flat_map(|(_, terms)| terms.iter().map(|term| term.term.clone()))
            .collect();
        let mut idf = HashMap::new();
        let mut frequencies = self
            .frequencies
            .lock()
            .expect("explore frequency mutex poisoned");
        frequencies.retain(|(_, cached_version, _), _| *cached_version == version);
        if frequencies.len() > 8192 {
            frequencies.clear();
        }
        {
            let db = self.store.connection();
            let mut query = db.prepare("SELECT DISTINCT c.document_id FROM chunk_fts JOIN chunks c ON c.id=chunk_fts.rowid JOIN documents d ON d.id=c.document_id WHERE chunk_fts MATCH ?1 AND instr(replace(d.path,char(92),'/')||'/',?2)=1")?;
            let prefix = format!("{}/", scope.replace('\\', "/").trim_end_matches('/'));
            for term in vocabulary {
                if cancel.is_cancelled() {
                    bail!("已取消探索")
                }
                let key = (scope.clone(), version, term.clone());
                let df = if let Some(df) = frequencies.get(&key) {
                    *df
                } else {
                    let df = query
                        .query_map(
                            params![format!("body : {}", text::quote(&term)), prefix],
                            |row| row.get::<_, i64>(0),
                        )?
                        .collect::<rusqlite::Result<Vec<_>>>()?
                        .into_iter()
                        .filter(|id| by_id.contains_key(id))
                        .count();
                    frequencies.insert(key, df);
                    df
                };
                let n = documents.len() as f64;
                idf.insert(term, (1.0 + (n - df as f64 + 0.5) / (df as f64 + 0.5)).ln());
            }
        }
        drop(frequencies);
        let vectors: Vec<_> = summaries
            .iter()
            .map(|(id, terms)| {
                let vector: BTreeMap<_, _> = terms
                    .iter()
                    .map(|term| {
                        (
                            term.term.clone(),
                            term.boost * (1.0 + (term.count as f64).ln()) * idf[&term.term],
                        )
                    })
                    .collect();
                let norm = vector
                    .values()
                    .map(|weight| weight * weight)
                    .sum::<f64>()
                    .sqrt();
                (*id, vector, norm)
            })
            .collect();
        let (_, source, source_norm) = &vectors[0];
        let snapshot = uuid::Uuid::new_v4().to_string();
        let mut ranked = Vec::new();
        for (id, vector, norm) in vectors.iter().skip(1) {
            let mut shared: Vec<_> = source
                .iter()
                .filter_map(|(term, weight)| {
                    vector.get(term).map(|other| (term.clone(), weight * other))
                })
                .collect();
            let score = if *source_norm > 0.0 && *norm > 0.0 {
                shared.iter().map(|(_, weight)| weight).sum::<f64>() / (source_norm * norm)
            } else {
                0.0
            };
            shared.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            let mut terms: Vec<String> = Vec::new();
            for (term, _) in shared {
                if !terms
                    .iter()
                    .any(|existing| existing.contains(&term) || term.contains(existing))
                {
                    terms.push(term);
                }
                if terms.len() == 16 {
                    break;
                }
            }
            if score <= 0.0
                || terms.is_empty()
                || (terms.len() == 1 && terms[0].chars().count() < 4)
            {
                continue;
            }
            let doc = by_id[id];
            let (left, right) = if center.id < doc.id {
                (*center, doc)
            } else {
                (doc, *center)
            };
            ranked.push((
                doc.clone(),
                ExploreEdge {
                    source: left.id,
                    target: right.id,
                    source_hash: left.hash.clone(),
                    target_hash: right.hash.clone(),
                    score,
                    terms,
                    snapshot: snapshot.clone(),
                },
            ));
        }
        ranked.sort_by(|a, b| {
            b.1.score
                .total_cmp(&a.1.score)
                .then(a.0.path.cmp(&b.0.path))
        });
        let (neighbors, edges) = ranked.into_iter().unzip();
        let neighborhood = Neighborhood {
            scope,
            version,
            center: (*center).clone(),
            documents: neighbors,
            edges,
            candidate_limit_reached,
        };
        self.consistent(version)?;
        let page = Self::page(&snapshot, &neighborhood, 0)?;
        let mut cache = self
            .snapshots
            .lock()
            .expect("explore snapshot mutex poisoned");
        cache.insert(snapshot, neighborhood);
        if cache.len() > 32 {
            cache.shift_remove_index(0);
        }
        Ok(page)
    }
    pub fn evidence(
        &self,
        scope: &str,
        snapshot: &str,
        source: i64,
        target: i64,
    ) -> Result<ExploreEvidence> {
        let scope = self.scope(scope)?;
        let local = self.snapshots.lock().expect("explore snapshot mutex poisoned")
            .get(snapshot).cloned();
        let (saved_scope, version, edges) = if let Some(saved) = local {
            (saved.scope, saved.version, saved.edges)
        } else {
            let cache = self.graphs.lock().expect("graph cache mutex poisoned");
            let saved = cache.get(&scope).filter(|saved| saved.snapshot == snapshot)
                .ok_or_else(|| anyhow::anyhow!("关联结果已过期，请刷新探索"))?;
            (saved.scope.clone(), saved.version, saved.edges.clone())
        };
        if saved_scope != scope { bail!("关联不在当前探索范围内") }
        self.consistent(version)?;
        let edge = edges.iter().find(|edge| edge.source == source && edge.target == target)
            .ok_or_else(|| anyhow::anyhow!("找不到当前关联"))?;
        let documents = self.documents(&scope)?;
        let left = documents
            .iter()
            .find(|doc| doc.id == source && doc.hash == edge.source_hash)
            .ok_or_else(|| anyhow::anyhow!("文件已更新，请刷新探索"))?;
        let right = documents
            .iter()
            .find(|doc| doc.id == target && doc.hash == edge.target_hash)
            .ok_or_else(|| anyhow::anyhow!("文件已更新，请刷新探索"))?;
        // Locate both sides with the same highest-weight term. Independent OR
        // ranking can select disjoint passages even when the documents overlap.
        let expression = format!("body : {}", text::quote(&edge.terms[0]));
        let a = self.store.candidate_query(
            &expression,
            None,
            std::slice::from_ref(&left.path),
            &[],
            &[],
            3,
        )?;
        let b = self.store.candidate_query(
            &expression,
            None,
            std::slice::from_ref(&right.path),
            &[],
            &[],
            3,
        )?;
        let normalized = |text: &str| text.nfkc().collect::<String>().to_lowercase();
        let mut pairs = Vec::new();
        for left in &a {
            for right in &b {
                let l = normalized(&left.content);
                let r = normalized(&right.content);
                let terms: Vec<_> = edge
                    .terms
                    .iter()
                    .filter(|term| l.contains(term.as_str()) && r.contains(term.as_str()))
                    .cloned()
                    .collect();
                if !terms.is_empty() {
                    pairs.push(EvidencePair {
                        left: left.clone(),
                        right: right.clone(),
                        terms,
                    });
                }
            }
        }
        pairs.sort_by(|a, b| {
            b.terms
                .len()
                .cmp(&a.terms.len())
                .then(a.left.id.cmp(&b.left.id))
                .then(a.right.id.cmp(&b.right.id))
        });
        let mut seen = HashSet::new();
        pairs.retain(|pair| seen.insert((pair.left.content.clone(), pair.right.content.clone())));
        pairs.truncate(3);
        if pairs.is_empty() {
            bail!("未找到可核对的共同原文，请刷新探索")
        }
        self.consistent(version)?;
        Ok(ExploreEvidence {
            version,
            algorithm: VERSION.into(),
            left: left.clone(),
            right: right.clone(),
            pairs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, sync::Arc};
    #[test]
    fn graph_covers_all_documents_and_keeps_sparse_verifiable_edges() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");
        fs::create_dir(&root).unwrap();
        for index in 0..36 {
            fs::write(root.join(format!("{index:02}.md")),
                "# 增量索引\n文件监听收到重命名通知后，只更新变化路径的本地索引。") .unwrap();
        }
        fs::write(root.join("isolated.md"), "# Astronomy\nSaturn has spectacular rings composed of ice.").unwrap();
        let store = Arc::new(Store::open(&dir.path().join("index.sqlite")).unwrap());
        let scope = store.add_root(&root).unwrap();
        store.scan(&scope, |_| {}).unwrap();
        let explorer = Explorer::new(store.clone());
        let graph = explorer.graph(&scope, &CancellationToken::new()).unwrap();
        assert_eq!(graph.documents.len(), 37);
        assert!(!graph.edges.is_empty());
        assert!(graph.edges.len() <= graph.documents.len() * 5);
        let isolated = graph.documents.iter().find(|doc| doc.name == "isolated.md").unwrap();
        assert!(graph.edges.iter().all(|edge| edge.source != isolated.id && edge.target != isolated.id));
        let edge = &graph.edges[0];
        let evidence = explorer.evidence(&scope, &graph.snapshot, edge.source, edge.target).unwrap();
        assert!(!evidence.pairs.is_empty());
        for pair in evidence.pairs {
            for term in pair.terms {
                assert!(pair.left.content.contains(&term));
                assert!(pair.right.content.contains(&term));
            }
        }
        assert_eq!(graph.snapshot, explorer.graph(&scope, &CancellationToken::new()).unwrap().snapshot);
        fs::write(root.join("00.md"), "# Biology\nPhotosynthesis releases oxygen in chloroplasts.").unwrap();
        fs::remove_file(root.join("01.md")).unwrap();
        fs::rename(root.join("02.md"), root.join("renamed.md")).unwrap();
        store.scan(&scope, |_| {}).unwrap();
        assert!(explorer.evidence(&scope, &graph.snapshot, edge.source, edge.target).is_err());
        let updated = explorer.graph(&scope, &CancellationToken::new()).unwrap();
        assert_eq!(updated.documents.len(), 36);
        assert_ne!(updated.snapshot, graph.snapshot);
        assert!(updated.documents.iter().any(|doc| doc.name == "renamed.md"));
        let modified = updated.documents.iter().find(|doc| doc.name == "00.md").unwrap();
        assert!(updated.edges.iter().all(|edge| edge.source != modified.id && edge.target != modified.id));
        assert_eq!(fs::read_to_string(root.join("00.md")).unwrap(), "# Biology\nPhotosynthesis releases oxygen in chloroplasts.");
    }
    #[test]
    #[ignore = "manual graph benchmark using disposable files and index"]
    fn graph_large_libraries() {
        for count in [1_000, 5_000] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");fs::create_dir(&root).unwrap();
        for index in 0..count {
            fs::write(root.join(format!("note-{index}.md")), format!(
                "# Research group{}\nKnowledge discovery document indexing retrieval group{} topic{}",
                index % 20, index % 20, index % 50)).unwrap();
        }
        let store = Arc::new(Store::open(&dir.path().join("index.sqlite")).unwrap());
        let scope = store.add_root(&root).unwrap();store.scan(&scope, |_| {}).unwrap();
        let explorer = Explorer::new(store);let start = std::time::Instant::now();
        let graph = explorer.graph(&scope, &CancellationToken::new()).unwrap();
        assert_eq!(graph.documents.len(), count);assert!(graph.edges.len() <= count * 5);
        let cold = start.elapsed();let start = std::time::Instant::now();
        assert_eq!(explorer.graph(&scope, &CancellationToken::new()).unwrap().snapshot, graph.snapshot);
        println!("GRAPH_BENCH documents={} edges={} cold_ms={:.1} cached_ms={:.1}",graph.documents.len(),graph.edges.len(),cold.as_secs_f64()*1000.,start.elapsed().as_secs_f64()*1000.);
        }
    }
    #[test]
    fn graph_cancelled_request_does_not_publish_partial_cache() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");fs::create_dir(&root).unwrap();
        fs::write(root.join("a.md"), "# 文件监听\n重命名后增量更新索引。").unwrap();
        let store = Arc::new(Store::open(&dir.path().join("index.sqlite")).unwrap());
        let scope = store.add_root(&root).unwrap();store.scan(&scope, |_| {}).unwrap();
        let explorer = Explorer::new(store);let cancel = CancellationToken::new();cancel.cancel();
        assert!(explorer.graph(&scope, &cancel).is_err());
        assert!(explorer.graphs.lock().unwrap().is_empty());
        assert_eq!(explorer.graph(&scope, &CancellationToken::new()).unwrap().documents.len(), 1);
    }
    #[test]
    fn overview_uses_only_indexed_documents_in_the_selected_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("a.md"), "# 文件监听\n重命名后增量更新索引。").unwrap();
        fs::write(
            root.join("nested/b.md"),
            "# 增量索引\n文件监听自动发现修改。",
        )
        .unwrap();
        fs::write(root.join("ignored.png"), b"not text").unwrap();
        let store = Arc::new(crate::store::Store::open(&dir.path().join("index.sqlite")).unwrap());
        let scope = store.add_root(&root).unwrap();
        store.scan(&scope, |_| {}).unwrap();
        let explorer = Explorer::new(store);
        let view = explorer.overview(&scope, "", &[]).unwrap();
        assert_eq!(view.total, 2);
        assert_eq!(
            view.documents
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>(),
            ["a.md", "b.md"]
        );
        assert_eq!(
            explorer
                .overview(root.join("nested").to_str().unwrap(), "", &[])
                .unwrap()
                .total,
            1
        );
    }
    #[test]
    fn related_documents_have_verifiable_source_evidence_without_model_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");
        fs::create_dir_all(&root).unwrap();
        let left = "# 文件监听\n文件监听收到重命名通知后，只更新变化路径的本地索引。";
        let right =
            "# 索引更新\n重命名通知来自文件监听。系统按变化路径更新本地索引，保留未变化的文件。";
        fs::write(root.join("watch.md"), left).unwrap();
        fs::write(root.join("index.md"), right).unwrap();
        fs::write(
            root.join("stars.md"),
            "# 天文学\n恒星的光谱可以帮助测量宇宙中的元素组成。",
        )
        .unwrap();
        let store = Arc::new(crate::store::Store::open(&dir.path().join("index.sqlite")).unwrap());
        let scope = store.add_root(&root).unwrap();
        store.scan(&scope, |_| {}).unwrap();
        let explorer = Explorer::new(store);
        let overview = explorer.overview(&scope, "", &[]).unwrap();
        let center = overview
            .documents
            .iter()
            .find(|d| d.name == "watch.md")
            .unwrap();
        let result = explorer
            .neighbors(
                &scope,
                center.id,
                None,
                0,
                &tokio_util::sync::CancellationToken::new(),
            )
            .unwrap();
        assert_eq!(result.documents[0].name, "index.md");
        let edge = &result.edges[0];
        let evidence = explorer
            .evidence(&scope, &result.snapshot, edge.source, edge.target)
            .unwrap();
        assert!(!evidence.pairs.is_empty());
        for pair in &evidence.pairs {
            for candidate in [&pair.left, &pair.right] {
                let original = fs::read_to_string(&candidate.path).unwrap();
                for span in &candidate.source_spans {
                    assert!(candidate
                        .content
                        .contains(&original[span.start_byte..span.end_byte]));
                }
            }
        }
    }
    #[test]
    fn shared_import_statements_do_not_create_a_relationship() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("orbit.py"), "from common_package import shared_token\n\ndef calculate_orbit():\n    return star_mass * orbital_radius\n").unwrap();
        fs::write(root.join("photo.py"), "from common_package import shared_token\n\ndef render_photo():\n    return image_width * pixel_brightness\n").unwrap();
        let store = Arc::new(crate::store::Store::open(&dir.path().join("index.sqlite")).unwrap());
        let mut settings = store.settings().unwrap();
        settings.file_types.code = true;
        store.save_settings(&settings).unwrap();
        let scope = store.add_root(&root).unwrap();
        store.scan(&scope, |_| {}).unwrap();
        let explorer = Explorer::new(store);
        let start = explorer
            .overview(&scope, "orbit", &[])
            .unwrap()
            .documents
            .remove(0);
        let result = explorer
            .neighbors(&scope, start.id, None, 0, &CancellationToken::new())
            .unwrap();
        assert!(result.documents.is_empty());
    }
    #[test]
    fn pages_keep_their_order_and_changed_files_invalidate_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");
        fs::create_dir(&root).unwrap();
        for index in 0..16 {
            fs::write(
                root.join(format!("{index:02}.md")),
                format!(
                    "# 文件监听\n重命名通知用于增量更新索引，文件变化按路径处理。记录 {index}。"
                ),
            )
            .unwrap();
        }
        let store = Arc::new(crate::store::Store::open(&dir.path().join("index.sqlite")).unwrap());
        let scope = store.add_root(&root).unwrap();
        store.scan(&scope, |_| {}).unwrap();
        let explorer = Explorer::new(store.clone());
        let center = explorer
            .overview(&scope, "00", &[])
            .unwrap()
            .documents
            .remove(0);
        let first = explorer
            .neighbors(&scope, center.id, None, 0, &CancellationToken::new())
            .unwrap();
        assert_eq!(first.documents.len(), 12);
        assert!(first.has_more);
        let next = explorer
            .neighbors(
                &scope,
                center.id,
                Some(&first.snapshot),
                first.next_offset,
                &CancellationToken::new(),
            )
            .unwrap();
        assert_eq!(next.documents.len(), 3);
        assert!(!next.has_more);
        assert!(first
            .documents
            .iter()
            .all(|doc| next.documents.iter().all(|other| doc.id != other.id)));
        let back = explorer
            .neighbors(
                &scope,
                center.id,
                Some(&first.snapshot),
                0,
                &CancellationToken::new(),
            )
            .unwrap();
        assert_eq!(first.documents, back.documents);
        let peer = &first.documents[0];
        let reverse = explorer
            .neighbors(&scope, peer.id, None, 0, &CancellationToken::new())
            .unwrap();
        let forward_edge = &first.edges[0];
        let reverse_edge = reverse
            .edges
            .iter()
            .find(|edge| edge.source == forward_edge.source && edge.target == forward_edge.target)
            .unwrap();
        assert!((forward_edge.score - reverse_edge.score).abs() < 1e-12);
        fs::write(&peer.path, "完全不同的新内容，关于天文学与恒星光谱。").unwrap();
        store
            .update_paths(&scope, &[peer.path.clone().into()], |_| {})
            .unwrap();
        assert!(explorer
            .evidence(
                &scope,
                &first.snapshot,
                forward_edge.source,
                forward_edge.target
            )
            .is_err());
        assert!(explorer
            .neighbors(
                &scope,
                center.id,
                Some(&first.snapshot),
                0,
                &CancellationToken::new()
            )
            .is_err());
    }
    #[test]
    fn scope_boundaries_deletion_and_rename_use_current_index() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");
        fs::create_dir_all(root.join(".hidden")).unwrap();
        fs::write(
            root.join("a.md"),
            "# 文件监听\n文件变动采用增量索引，重命名按照路径处理。",
        )
        .unwrap();
        fs::write(
            root.join("b.md"),
            "# 增量索引\n文件监听处理路径变动以及重命名。",
        )
        .unwrap();
        fs::write(root.join(".hidden/secret.md"), "文件监听以及重命名。").unwrap();
        let store = Arc::new(Store::open(&dir.path().join("index.sqlite")).unwrap());
        let scope = store.add_root(&root).unwrap();
        store.scan(&scope, |_| {}).unwrap();
        let root = std::path::PathBuf::from(&scope);
        let explorer = Explorer::new(store.clone());
        assert!(explorer
            .overview(root.join(".hidden").to_str().unwrap(), "", &[])
            .is_err());
        assert!(explorer
            .overview(dir.path().to_str().unwrap(), "", &[])
            .is_err());
        assert_eq!(explorer.overview(&scope, "", &[]).unwrap().total, 2);
        let a = explorer
            .overview(&scope, "a.md", &[])
            .unwrap()
            .documents
            .remove(0);
        let original = fs::read(root.join("a.md")).unwrap();
        let result = explorer
            .neighbors(&scope, a.id, None, 0, &CancellationToken::new())
            .unwrap();
        let edge = &result.edges[0];
        fs::rename(root.join("b.md"), root.join("renamed.md")).unwrap();
        store
            .update_paths(
                &scope,
                &[root.join("b.md"), root.join("renamed.md")],
                |_| {},
            )
            .unwrap();
        assert!(explorer
            .evidence(&scope, &result.snapshot, edge.source, edge.target)
            .is_err());
        let renamed = explorer
            .neighbors(&scope, a.id, None, 0, &CancellationToken::new())
            .unwrap();
        assert_eq!(renamed.documents[0].name, "renamed.md");
        fs::remove_file(root.join("renamed.md")).unwrap();
        store
            .update_paths(&scope, &[root.join("renamed.md")], |_| {})
            .unwrap();
        assert!(explorer
            .neighbors(&scope, a.id, None, 0, &CancellationToken::new())
            .unwrap()
            .documents
            .is_empty());
        assert_eq!(fs::read(root.join("a.md")).unwrap(), original);
    }
    #[test]
    fn old_indexes_get_only_requested_summaries_and_cancellation_is_respected() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("first.md"), "# 文件监听\n文件变化增量更新索引。").unwrap();
        fs::write(
            root.join("second.md"),
            "# 索引更新\n通过文件监听处理文件变化。",
        )
        .unwrap();
        fs::write(root.join("unrelated.md"), "星系光谱解析与天文观测。").unwrap();
        let store = Arc::new(Store::open(&dir.path().join("index.sqlite")).unwrap());
        let scope = store.add_root(&root).unwrap();
        store.scan(&scope, |_| {}).unwrap();
        store
            .connection()
            .execute("DELETE FROM explore_summaries", [])
            .unwrap();
        let version = store.version().unwrap();
        let explorer = Explorer::new(store.clone());
        let a = explorer
            .overview(&scope, "first", &[])
            .unwrap()
            .documents
            .remove(0);
        assert_eq!(
            store
                .connection()
                .query_row("SELECT count(*) FROM explore_summaries", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            0
        );
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(explorer.neighbors(&scope, a.id, None, 0, &cancel).is_err());
        explorer
            .neighbors(&scope, a.id, None, 0, &CancellationToken::new())
            .unwrap();
        assert_eq!(
            store
                .connection()
                .query_row("SELECT count(*) FROM explore_summaries", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            2
        );
        assert_eq!(store.version().unwrap(), version);
    }
    #[test]
    fn copyright_footer_does_not_link_unrelated_documents() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");
        fs::create_dir(&root).unwrap();
        let footer="\nCopyright Example Company. All rights reserved. Redistribution requires written permission.\n";
        fs::write(
            root.join("orbit.md"),
            format!("# Astronomy\nStellar spectroscopy measures hydrogen absorption.{footer}"),
        )
        .unwrap();
        fs::write(
            root.join("photo.md"),
            format!("# Photography\nCamera exposure combines aperture shutter ISO.{footer}"),
        )
        .unwrap();
        let store = Arc::new(Store::open(&dir.path().join("index.sqlite")).unwrap());
        let scope = store.add_root(&root).unwrap();
        store.scan(&scope, |_| {}).unwrap();
        let explorer = Explorer::new(store);
        let center = explorer
            .overview(&scope, "orbit", &[])
            .unwrap()
            .documents
            .remove(0);
        assert!(explorer
            .neighbors(&scope, center.id, None, 0, &CancellationToken::new())
            .unwrap()
            .documents
            .is_empty());
    }
    #[test]
    fn evidence_pairs_the_same_term_across_long_documents() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");
        fs::create_dir(&root).unwrap();
        let terms = [
            "alphaunique",
            "betaunique",
            "gammaunique",
            "deltaunique",
            "epsilonunique",
            "zetaunique",
            "etaunique",
            "thetaunique",
            "iotaunique",
            "kappaunique",
            "lambdaunique",
            "muunique",
        ];
        let make = |reverse: bool| {
            terms
                .iter()
                .enumerate()
                .map(|(i, term)| {
                    let short = (i < 6) != reverse;
                    format!(
                        "{} {}",
                        format!("{term} ").repeat(if short { 2 } else { 1 }),
                        if short {
                            " ".into()
                        } else {
                            format!("{} ", if reverse { "astronomy" } else { "photography" })
                                .repeat(300)
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        fs::write(root.join("a.md"), make(false)).unwrap();
        fs::write(root.join("b.md"), make(true)).unwrap();
        let store = Arc::new(Store::open(&dir.path().join("index.sqlite")).unwrap());
        let scope = store.add_root(&root).unwrap();
        store.scan(&scope, |_| {}).unwrap();
        let explorer = Explorer::new(store);
        let center = explorer
            .overview(&scope, "a.md", &[])
            .unwrap()
            .documents
            .remove(0);
        let result = explorer
            .neighbors(&scope, center.id, None, 0, &CancellationToken::new())
            .unwrap();
        assert_eq!(result.documents.len(), 1);
        let edge = &result.edges[0];
        let evidence = explorer
            .evidence(&scope, &result.snapshot, edge.source, edge.target)
            .unwrap();
        assert!(!evidence.pairs.is_empty());
        for pair in evidence.pairs {
            assert!(!pair.terms.is_empty());
            for term in pair.terms {
                assert!(pair.left.content.contains(&term));
                assert!(pair.right.content.contains(&term));
            }
        }
    }
    #[test]
    fn file_type_settings_invalidate_existing_exploration() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");
        fs::create_dir(&root).unwrap();
        for name in ["a.md", "b.md"] {
            fs::write(root.join(name), "文件监听按照变化路径处理增量索引。").unwrap();
        }
        let store = Arc::new(Store::open(&dir.path().join("index.sqlite")).unwrap());
        let scope = store.add_root(&root).unwrap();
        store.scan(&scope, |_| {}).unwrap();
        let explorer = Explorer::new(store.clone());
        let center = explorer
            .overview(&scope, "a.md", &[])
            .unwrap()
            .documents
            .remove(0);
        let result = explorer
            .neighbors(&scope, center.id, None, 0, &CancellationToken::new())
            .unwrap();
        let mut settings = store.settings().unwrap();
        settings.file_types.markdown = false;
        store.save_settings(&settings).unwrap();
        assert!(explorer
            .neighbors(
                &scope,
                center.id,
                Some(&result.snapshot),
                0,
                &CancellationToken::new()
            )
            .is_err());
        assert_eq!(explorer.overview(&scope, "", &[]).unwrap().total, 0);
    }
    #[test]
    #[ignore = "manual local index benchmark; creates only a temporary SQLite database"]
    fn benchmark_large_indexes() {
        use std::time::Instant;
        for count in [1_000, 10_000, 100_000] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("library");
            fs::create_dir(&root).unwrap();
            let store = Arc::new(Store::open(&dir.path().join("index.sqlite")).unwrap());
            let scope = store.add_root(&root).unwrap();
            let root = std::path::PathBuf::from(&scope);
            let start = Instant::now();
            {
                let mut db = store.connection();
                let tx = db.transaction().unwrap();
                for id in 1..=count {
                    let title = format!("document-{id:06}.md");
                    let path = root.join(&title).to_string_lossy().into_owned();
                    let body=format!("文件监听增量索引 重命名路径 notification watcher indexing domain{} identifier{}",id%100,id%37);
                    let terms = text::explore_summary(
                        Path::new(&title),
                        std::iter::once(("文件监听", body.as_str())),
                    );
                    tx.execute("INSERT INTO documents(id,root,path,name,hash,mtime,size) VALUES(?1,?2,?3,?4,'fixture',0,100)",params![id,scope,path,title]).unwrap();
                    tx.execute("INSERT INTO chunks(id,document_id,heading,ranges,start_line,end_line,spans) VALUES(?1,?1,'文件监听','[]',1,1,'[]')",params![id]).unwrap();
                    tx.execute("INSERT INTO chunk_fts(rowid,title,heading,body) VALUES(?1,?2,'文件监听',?3)",params![id,text::index_text(&title),text::index_text(&body)]).unwrap();
                    tx.execute(
                        "INSERT INTO explore_summaries VALUES(?1,'fixture',?2,?3)",
                        params![
                            id,
                            text::EXPLORE_SUMMARY_VERSION,
                            serde_json::to_string(&terms).unwrap()
                        ],
                    )
                    .unwrap();
                }
                tx.commit().unwrap();
            }
            let summary_bytes: usize = store
                .connection()
                .query_row(
                    "SELECT sum(length(CAST(terms AS BLOB))) FROM explore_summaries",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let explorer = Explorer::new(store);
            let setup_ms = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            let overview = explorer.overview(&scope, "", &[]).unwrap();
            let overview_ms = start.elapsed().as_secs_f64() * 1000.;
            assert_eq!(overview.total, count as usize);
            let mut cold = Vec::new();
            let mut cached = Vec::new();
            for id in 1..=5 {
                let now = Instant::now();
                let result = explorer
                    .neighbors(&scope, id, None, 0, &CancellationToken::new())
                    .unwrap();
                cold.push(now.elapsed().as_secs_f64() * 1000.);
                assert_eq!(result.documents.len(), 12);
            }
            for _ in 0..20 {
                let now = Instant::now();
                explorer
                    .neighbors(&scope, 1, None, 0, &CancellationToken::new())
                    .unwrap();
                cached.push(now.elapsed().as_secs_f64() * 1000.);
            }
            cold.sort_by(f64::total_cmp);
            cached.sort_by(f64::total_cmp);
            println!("EXPLORE_BENCH docs={count} summary_bytes={summary_bytes} setup_ms={setup_ms:.1} overview_ms={overview_ms:.1} cold_p95_ms={:.1} cache_p95_ms={:.1}",cold[4],cached[18]);
        }
    }
}
