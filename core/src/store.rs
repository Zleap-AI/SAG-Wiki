use crate::{ignore_rules, text, types::*};
use anyhow::{bail, Context, Result};
use indexmap::IndexMap;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    collections::{hash_map::Entry as MapEntry, HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

// This is an application allocation budget, not a promise about exact OS RSS.
// Leave room for source/decoded strings, chunk metadata, tokenization and vocabulary.
const TEXT_PREPARATION_BUDGET: usize = 256 * 1024 * 1024;
const MAX_TEXT_INPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_DECODED_TEXT_BYTES: usize = 32 * 1024 * 1024;
const MAX_PREPARED_TEXT_BYTES: usize = 64 * 1024 * 1024;

struct PreparationBudget {
    limit: usize,
    used: Mutex<usize>,
    released: Condvar,
}
struct PreparationPermit {
    budget: Arc<PreparationBudget>,
    bytes: usize,
}
impl PreparationBudget {
    fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self { limit, used: Mutex::new(0), released: Condvar::new() })
    }
    fn reserve(self: &Arc<Self>, bytes: usize, cancelled: impl Fn() -> bool) -> Result<PreparationPermit> {
        anyhow::ensure!(bytes <= self.limit, "文本准备预算不足");
        let mut used = self.used.lock().expect("text preparation budget poisoned");
        loop {
            if cancelled() { bail!("索引已停止") }
            if *used <= self.limit - bytes {
                *used += bytes;
                return Ok(PreparationPermit { budget: self.clone(), bytes });
            }
            used = self.released.wait_timeout(used, Duration::from_millis(50)).unwrap().0;
        }
    }
}
impl Drop for PreparationPermit {
    fn drop(&mut self) {
        *self.budget.used.lock().expect("text preparation budget poisoned") -= self.bytes;
        self.budget.released.notify_all();
    }
}
fn preparation_reservation(path: &Path) -> usize {
    let bytes = fs::metadata(path).map_or(0, |metadata| metadata.len().min(MAX_TEXT_INPUT_BYTES as u64) as usize);
    (bytes.saturating_mul(16) + 16 * 1024 * 1024).min(TEXT_PREPARATION_BUDGET)
}
struct PreparedChunk {
    chunk: text::Chunk,
    heading: String,
    content: String,
}
struct PreparedFile {
    _permit: Option<PreparationPermit>,
    path: String,
    name: String,
    fingerprint: String,
    mtime: i64,
    size: i64,
    title: String,
    chunks: Vec<PreparedChunk>,
    explore_terms: Vec<DocumentTerm>,
    retrieval_terms: HashSet<String>,
}
pub struct Store {
    pub indexing: crate::index_status::IndexControl,
    pub(crate) docling: Arc<crate::docling::Docling>,
    db: Mutex<Connection>,
    query_readers: Vec<Mutex<Connection>>,
    query_cache: Mutex<QueryCache>,
    text_preparation: Arc<PreparationBudget>,
}
const QUERY_WORKERS: usize = 8;
const QUERY_CACHE_ENTRIES: usize = 256;
const CORPUS_CACHE_ENTRIES: usize = 8;

#[derive(Clone, Hash, PartialEq, Eq)]
struct CorpusScope {
    scope: Vec<String>,
    references: Vec<String>,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct CandidateQuery {
    expression: String,
    required: Option<String>,
    scope: Vec<String>,
    references: Vec<String>,
    excluded: Vec<i64>,
    limit: usize,
}

#[derive(Default)]
struct QueryCache {
    version: Option<i64>,
    entries: IndexMap<CandidateQuery, Vec<Candidate>>,
    hits: usize,
    misses: usize,
    rank_stats: IndexMap<String, Vec<u8>>,
    frequencies: HashMap<String, usize>,
    corpora: IndexMap<CorpusScope, Arc<crate::sag2_v10::Corpus>>,
    scope_documents: IndexMap<CorpusScope, Option<Arc<str>>>,
}

impl QueryCache {
    fn refresh(&mut self, version: i64) -> bool {
        if self.version.is_some_and(|current| version < current) { return false; }
        if self.version != Some(version) {
            self.entries.clear();
            self.rank_stats.clear();
            self.frequencies.clear();
            self.corpora.clear();
            self.scope_documents.clear();
            self.version = Some(version);
        }
        true
    }
    fn get(&mut self, version: i64, key: &CandidateQuery) -> Option<Vec<Candidate>> {
        if !self.refresh(version) {
            self.misses += 1;
            return None;
        }
        if let Some(rows) = self.entries.shift_remove(key) {
            self.hits += 1;
            self.entries.insert(key.clone(), rows.clone());
            Some(rows)
        } else {
            self.misses += 1;
            None
        }
    }

    fn insert(&mut self, version: i64, key: CandidateQuery, rows: Vec<Candidate>) {
        // A reader of an older snapshot must not replace a newer cache generation.
        if self.version == Some(version) {
            self.entries.insert(key, rows);
            if self.entries.len() > QUERY_CACHE_ENTRIES {
                self.entries.shift_remove_index(0);
            }
        }
    }
}
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
pub fn path_text(path: &Path) -> Result<String> {
    Ok(path.to_str().context("路径包含不支持的字符")?.to_owned())
}
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn body(path: &Path, bytes: &[u8]) -> Result<String> {
    Ok(crate::document_text::read(path, bytes)?)
}
fn candidate_from_row(
    r: &rusqlite::Row<'_>,
    order: usize,
    documents: &mut HashMap<i64, Option<String>>,
) -> Result<Option<Candidate>> {
    hydrate_candidate(candidate_metadata(r, order)?, documents)
}

fn candidate_metadata(r: &rusqlite::Row<'_>, order: usize) -> Result<Candidate> {
    Ok(Candidate {
            previous_score: None,
        id: r.get(0)?,
        document_id: r.get(1)?,
        path: r.get(2)?,
        title: r.get(3)?,
        heading: r.get(4)?,
        content: String::new(),
        start_line: r.get::<_, i64>(5)? as usize,
        end_line: r.get::<_, i64>(6)? as usize,
        source_ranges: serde_json::from_str(&r.get::<_, String>(7)?)?,
        source_spans: serde_json::from_str(&r.get::<_, String>(10)?)?,
        bm25: r.get(8)?,
        score: 0.,
        order,
        hash: r.get(9)?,
    })
}

/// Loads the current source text for a candidate's document.
///
/// A document whose file is unreadable (unmounted drive, permission change)
/// or whose content no longer matches the indexed hash is skipped rather
/// than failing the whole query: one stale file must not hide every other
/// result. The watcher or a manual refresh brings the index back in sync.
/// Reads a document only when its content still matches the indexed hash.
fn read_indexed_source(path: &str, expected: &str) -> Option<String> {
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) => {
            eprintln!("跳过无法读取的原文件：{path}：{error}");
            return None;
        }
    };
    if hash(&raw) != expected {
        eprintln!("跳过已修改但尚未重新索引的文件：{path}");
        return None;
    }
    match body(Path::new(path), &raw) {
        Ok(text) => Some(text),
        Err(error) => {
            eprintln!("跳过无法解码的原文件：{path}：{error}");
            None
        }
    }
}
fn hydrate_candidate(
    mut candidate: Candidate,
    documents: &mut HashMap<i64, Option<String>>,
) -> Result<Option<Candidate>> {
    let source = match documents.entry(candidate.document_id) {
        MapEntry::Occupied(entry) => entry.into_mut(),
        MapEntry::Vacant(entry) => entry.insert(read_indexed_source(&candidate.path, &candidate.hash)),
    };
    let Some(source) = source else { return Ok(None) };
    let lines: Vec<&str> = source
        .split('\n')
        .map(|l| l.trim_end_matches('\r'))
        .collect();
    candidate.content = if !candidate.source_spans.is_empty() {
        text::source_content(source, &candidate.source_spans)
    } else {
        candidate.source_ranges
            .iter()
            .map(|(s, e)| lines[s - 1..*e].join("\n"))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    Ok(Some(candidate))
}
unsafe extern "C" {
    fn sag_register_rank(db: *mut rusqlite::ffi::sqlite3) -> std::os::raw::c_int;
}
fn register_rank(connection: &Connection) -> Result<()> {
    // Registration is synchronous and the connection outlives its FTS callbacks.
    let code = unsafe { sag_register_rank(connection.handle()) };
    if code != rusqlite::ffi::SQLITE_OK {
        return Err(rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None).into());
    }
    connection.pragma_update(None, "mmap_size", 2_147_483_648_i64)?;
    Ok(())
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let db = Connection::open(path)?;
        register_rank(&db)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;
 CREATE TABLE IF NOT EXISTS roots(path TEXT PRIMARY KEY,name TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'pending',error TEXT);
 CREATE TABLE IF NOT EXISTS root_exclusions(path TEXT PRIMARY KEY,root TEXT NOT NULL REFERENCES roots(path) ON DELETE CASCADE);
 CREATE TABLE IF NOT EXISTS index_issues(path TEXT PRIMARY KEY,root TEXT NOT NULL REFERENCES roots(path) ON DELETE CASCADE,error TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS index_history(id INTEGER PRIMARY KEY AUTOINCREMENT,path TEXT NOT NULL,name TEXT NOT NULL,started_at INTEGER NOT NULL,finished_at INTEGER NOT NULL,completed INTEGER NOT NULL,total INTEGER NOT NULL,status TEXT NOT NULL,error TEXT);
 CREATE TABLE IF NOT EXISTS conversions(source TEXT PRIMARY KEY,root TEXT NOT NULL REFERENCES roots(path) ON DELETE CASCADE,source_hash TEXT NOT NULL,output TEXT NOT NULL,output_hash TEXT NOT NULL,valid INTEGER NOT NULL DEFAULT 1);
 CREATE INDEX IF NOT EXISTS conversions_output ON conversions(output);
 CREATE INDEX IF NOT EXISTS index_issues_root ON index_issues(root);
 CREATE TABLE IF NOT EXISTS documents(id INTEGER PRIMARY KEY AUTOINCREMENT,root TEXT NOT NULL,path TEXT UNIQUE NOT NULL,name TEXT NOT NULL,hash TEXT NOT NULL,mtime INTEGER NOT NULL,size INTEGER NOT NULL DEFAULT 0);
 CREATE TABLE IF NOT EXISTS chunks(id INTEGER PRIMARY KEY AUTOINCREMENT,document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,heading TEXT NOT NULL,ranges TEXT NOT NULL,start_line INTEGER NOT NULL,end_line INTEGER NOT NULL,spans TEXT NOT NULL DEFAULT '[]');
 CREATE INDEX IF NOT EXISTS chunks_doc ON chunks(document_id);
 CREATE INDEX IF NOT EXISTS docs_root ON documents(root);
 CREATE VIRTUAL TABLE IF NOT EXISTS chunk_fts USING fts5(title,heading,body,content='',contentless_delete=1);
 CREATE VIRTUAL TABLE IF NOT EXISTS chunk_vocab USING fts5vocab(chunk_fts,'col');
 CREATE TABLE IF NOT EXISTS preferences(id INTEGER PRIMARY KEY CHECK(id=1),value TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS conversations(id TEXT PRIMARY KEY,title TEXT NOT NULL,messages TEXT NOT NULL,updated INTEGER NOT NULL,deleted INTEGER NOT NULL DEFAULT 0);
 CREATE TABLE IF NOT EXISTS drafts(path TEXT PRIMARY KEY,value TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS metadata(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL);
 CREATE TABLE IF NOT EXISTS retrieval_vocabulary(document_id INTEGER PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,terms TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS explore_summaries(document_id INTEGER PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,hash TEXT NOT NULL,algorithm INTEGER NOT NULL,terms TEXT NOT NULL);
 INSERT OR IGNORE INTO metadata VALUES(1,0);
")?;
        let has_size: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('documents') WHERE name='size')",
            [],
            |r| r.get(0),
        )?;
        let has_file_log: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('index_history') WHERE name='file_log')",
            [], |r| r.get(0),
        )?;
        if !has_file_log {
            db.execute("ALTER TABLE index_history ADD COLUMN file_log TEXT", [])?;
        }
        if !has_size {
            db.execute(
                "ALTER TABLE documents ADD COLUMN size INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        let has_spans: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('chunks') WHERE name='spans')",
            [],
            |r| r.get(0),
        )?;
        if !has_spans {
            db.execute_batch("BEGIN; ALTER TABLE chunks ADD COLUMN spans TEXT NOT NULL DEFAULT '[]'; DELETE FROM chunk_fts; DELETE FROM chunks; UPDATE documents SET hash=''; UPDATE metadata SET version=version+1; COMMIT;")?;
        }
        let stores_body: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('documents') WHERE name='body')",
            [],
            |r| r.get(0),
        )?;
        if stores_body {
            db.execute_batch("BEGIN;
                DROP TABLE chunk_vocab;
                DROP TABLE chunk_fts;
                DELETE FROM chunks;
                ALTER TABLE documents DROP COLUMN body;
                CREATE VIRTUAL TABLE chunk_fts USING fts5(title,heading,body,content='',contentless_delete=1);
                CREATE VIRTUAL TABLE chunk_vocab USING fts5vocab(chunk_fts,'col');
                UPDATE documents SET hash='';
                UPDATE roots SET status='pending',error=NULL;
                UPDATE metadata SET version=version+1;
                COMMIT;
                VACUUM;")?;
        }
        let schema_version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if schema_version > 0 && schema_version < 7 {
            db.execute_batch("BEGIN; DELETE FROM chunk_fts; DELETE FROM chunks; UPDATE documents SET hash=''; UPDATE roots SET status='pending',error=NULL; UPDATE metadata SET version=version+1; COMMIT;")?;
        }
        if schema_version < 9 && db.query_row(
            "SELECT EXISTS(SELECT 1 FROM documents WHERE lower(path) GLOB '*.html' OR lower(path) GLOB '*.htm' OR lower(path) GLOB '*.md' OR lower(path) GLOB '*.markdown')",
            [], |row| row.get::<_, bool>(0),
        )? {
            // Refresh document indexes for the expanded embedded-resource filter.
            // Source files and indexes for other file types remain intact.
            db.execute_batch("BEGIN;
                CREATE TEMP TABLE resource_refresh AS SELECT id FROM documents WHERE lower(path) GLOB '*.html' OR lower(path) GLOB '*.htm' OR lower(path) GLOB '*.md' OR lower(path) GLOB '*.markdown';
                DELETE FROM chunk_fts WHERE rowid IN (SELECT id FROM chunks WHERE document_id IN (SELECT id FROM resource_refresh));
                DELETE FROM chunks WHERE document_id IN (SELECT id FROM resource_refresh);
                DELETE FROM retrieval_vocabulary WHERE document_id IN (SELECT id FROM resource_refresh);
                DELETE FROM explore_summaries WHERE document_id IN (SELECT id FROM resource_refresh);
                UPDATE documents SET hash='' WHERE id IN (SELECT id FROM resource_refresh);
                UPDATE roots SET status='pending',error=NULL WHERE path IN (SELECT root FROM documents WHERE id IN (SELECT id FROM resource_refresh));
                DROP TABLE resource_refresh;
                UPDATE metadata SET version=version+1;
                PRAGMA user_version=9;
                COMMIT;")?;
        } else {
            db.pragma_update(None, "user_version", 9)?;
        }
        let query_readers = (0..QUERY_WORKERS)
            .map(|_| {
                let reader = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
                register_rank(&reader)?;
                reader.busy_timeout(std::time::Duration::from_secs(5))?;
                Ok(Mutex::new(reader))
            })
            .collect::<Result<Vec<_>>>()?;
        let saved: Option<String> = db.query_row("SELECT value FROM preferences WHERE id=1", [], |r| r.get(0)).optional()?;
        let store = Self {
            indexing: crate::index_status::IndexControl::default(),
            docling: Arc::new(crate::docling::Docling::new(path.parent().context("数据库缺少父目录")?)),
            db: Mutex::new(db),
            query_readers,
            query_cache: Mutex::new(QueryCache::default()),
            text_preparation: PreparationBudget::new(TEXT_PREPARATION_BUDGET),
        };
        if let Some(saved) = saved {
            let previous: Settings = serde_json::from_str(&saved)?;
            if previous.retrieval_version < crate::sag2_v10::VERSION {
                let updated = store.settings()?;
                if previous.retrieval.chunk_chars != updated.retrieval.chunk_chars {
                    store.invalidate_index()?;
                }
                store.save_settings(&updated)?;
            }
        }
        Ok(store)
    }
    pub fn connection(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().expect("database mutex poisoned")
    }
    pub fn version(&self) -> Result<i64> {
        Ok(self
            .connection()
            .query_row("SELECT version FROM metadata WHERE id=1", [], |r| r.get(0))?)
    }
    pub fn settings(&self) -> Result<Settings> {
        let value: Option<String> = self
            .connection()
            .query_row("SELECT value FROM preferences WHERE id=1", [], |r| r.get(0))
            .optional()?;
        match value {
            Some(v) => {
                let mut settings: Settings = serde_json::from_str(&v)?;
                if settings.retrieval_version < crate::sag2_v10::VERSION {
                    let r = &settings.retrieval;
                    if r.candidate_k == 50 && r.seed_k == 10 && r.max_hops == 1 && r.expansion_term_limit == 20 && r.top_k == 10 && r.chunk_chars == 1000 {
                        settings.retrieval = Retrieval::default();
                    }
                    settings.retrieval.max_hops = settings.retrieval.max_hops.min(2);
                    settings.retrieval_version = crate::sag2_v10::VERSION;
                }
                Ok(settings)
            },
            None => Ok(Settings::default()),
        }
    }
    pub fn validate_settings(s: &Settings) -> Result<()> {
        if s.answer.auth_mode != ModelAuthMode::Bearer {
            bail!("问答模型需要 API Key 鉴权；无需鉴权选项仅适用于 Jev")
        }
        anyhow::ensure!(s.search_summary_sources > 0, "自动问答参考数量必须大于 0");
        if !matches!(s.language.as_str(), "zh-CN" | "en") {
            bail!("不支持的界面语言")
        }
        if !(1..=100).contains(&s.max_tool_calls) {
            bail!("每轮工具调用上限应为 1–100")
        }
        let mut ids = HashSet::new();
        for server in &s.mcp_servers {
            crate::mcp::validate(server)?;
            if !ids.insert(&server.id) {
                bail!("MCP 服务 ID 重复")
            }
        }
        for profile in s.model_profiles.values() {
            if profile.mcp_servers.iter().any(|id| !ids.contains(id))
                || profile.mcp_tools.keys().any(|id| id != crate::sag_mcp::ID && !ids.contains(id))
            {
                bail!("模型绑定的 MCP 服务不存在")
            }
            if profile.mcp_tools.get(crate::sag_mcp::ID).is_some_and(|tools| tools.iter().any(|tool| !crate::sag_mcp::NAMES.contains(&tool.as_str()))) {
                bail!("模型绑定了未知的 SAG 工具")
            }
        }
        if !(1..=64).contains(&s.index_workers) {
            bail!("索引并行数应为 1–64")
        }
        if !(1..=16).contains(&s.conversion_workers) {
            bail!("转换并行数应为 1–16")
        }
        let r = &s.retrieval;
        if !(10..=100).contains(&r.candidate_k)
            || !(1..=20).contains(&r.seed_k)
            || !(1..=500).contains(&r.expansion_term_limit)
            || r.max_hops > 2
            || !(1..=100).contains(&r.top_k)
            || !(400..=12000).contains(&r.chunk_chars)
        {
            bail!("检索参数超出允许范围")
        }
        for m in [&s.answer, &s.jev] {
            let u = reqwest::Url::parse(&m.url).context("接口地址无效")?;
            if !matches!(u.scheme(), "http" | "https") || m.model.trim().is_empty() {
                bail!("请填写 HTTP(S) 接口地址与模型名称")
            }
        }
        if s.answer_context_tokens < 4096 {
            bail!("回答上下文应至少为 4096 tokens")
        }
        if !(10..=600).contains(&s.timeout_secs) {
            bail!("超时应为 10–600 秒")
        }
        Ok(())
    }
    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        Self::validate_settings(s)?;
        let previous = self.settings()?;
        let mut db = self.connection();
        let transaction = db.transaction()?;
        transaction.execute("INSERT INTO preferences VALUES(1,?) ON CONFLICT(id) DO UPDATE SET value=excluded.value",[serde_json::to_string(s)?])?;
        if previous.file_types != s.file_types || previous.excluded != s.excluded || previous.convert_other_formats != s.convert_other_formats || previous.conversion_formats != s.conversion_formats {
            transaction.execute("UPDATE metadata SET version=version+1", [])?;
        }
        transaction.commit()?;
        Ok(())
    }
    pub fn roots(&self) -> Result<Vec<Root>> {
        let c = self.connection();
        let mut q=c.prepare("SELECT path,name,status,error,(SELECT COUNT(*) FROM documents d WHERE d.root=r.path) FROM roots r ORDER BY rowid")?;
        let rows = q
            .query_map([], |r| {
                Ok(Root {
                    path: r.get(0)?,
                    name: r.get(1)?,
                    status: r.get(2)?,
                    error: r.get(3)?,
                    count: r.get::<_, i64>(4)? as usize,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
    pub fn indexed_file_count(&self, directory: &Path) -> Result<usize> {
        let path = path_text(directory)?;
        let prefix = format!("{}{}", path.trim_end_matches(std::path::MAIN_SEPARATOR), std::path::MAIN_SEPARATOR);
        Ok(self.connection().query_row(
            "SELECT COUNT(*) FROM documents WHERE path=?1 OR instr(path,?2)=1",
            params![path, prefix], |row| row.get::<_, i64>(0),
        )? as usize)
    }
    pub fn folder_index_options(&self, directory: &Path) -> Result<FolderIndexOptions> {
        let directory = self.authorize(directory)?;
        if !directory.is_dir() { bail!("请选择文件夹") }
        let root = self.roots()?.into_iter().find(|root| directory.starts_with(&root.path)).unwrap();
        let mut rules = crate::folder_index::Rules::new(Path::new(&root.path), &self.settings()?)?;
        Ok(rules.directory(&directory)?.clone())
    }
    pub fn save_folder_index_options(&self, path: &Path, options: &FolderIndexOptions, rebuild: bool) -> Result<String> {
        let path = self.authorize(path)?;
        if !path.is_dir() { bail!("请选择文件夹") }
        let previous = self.folder_index_options(&path)?;
        if options.convert_other_formats
            && (!previous.convert_other_formats || previous.conversion_formats != options.conversion_formats)
        {
            self.docling.require_formats(&options.conversion_formats)?;
        }
        let root = self.roots()?.into_iter().find(|root| path.starts_with(&root.path)).unwrap().path;
        let mut db = self.connection();
        let tx = db.transaction()?;
        let status: String = tx.query_row("SELECT status FROM roots WHERE path=?", [&root], |row| row.get(0))?;
        if status == "pending" || status == "indexing" { bail!("此文件夹正在索引，请完成后再修改索引设置") }
        crate::folder_index::write(&path, options)?;
        if rebuild {
            Self::remove_indexed_under_tx(&tx, &root, &path)?;
        }
        tx.execute("UPDATE roots SET status='pending',error=NULL WHERE path=?", [&root])?;
        tx.commit()?;
        Ok(root)
    }
    pub fn prepare_reindex(&self) -> Result<Vec<String>> {
        let mut db = self.connection();
        let tx = db.transaction()?;
        let busy: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM roots WHERE status IN ('pending','indexing'))",
            [], |row| row.get(0),
        )?;
        if busy { bail!("正在索引，请等待当前任务完成") }
        let roots = {
            let mut query = tx.prepare("SELECT path FROM roots ORDER BY rowid")?;
            let rows = query.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
            rows
        };
        tx.execute("UPDATE roots SET status='pending'", [])?;
        tx.commit()?;
        Ok(roots)
    }
    fn reclaim_storage(&self, db: &Connection) -> Result<()> {
        // Finish active read snapshots before reclaiming pages and truncating WAL.
        let _readers: Vec<_> = self.query_readers.iter()
            .map(|reader| reader.lock().expect("query reader mutex poisoned"))
            .collect();
        db.execute_batch("VACUUM;").context("数据已删除，但数据库空间回收失败")?;
        let busy: i64 = db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        if busy != 0 { bail!("数据已删除，但数据库日志仍在使用，请稍后再次清除以完成清理") }
        Ok(())
    }
    pub fn remove_all_roots(&self) -> Result<()> {
        let mut db = self.connection();
        let tx = db.transaction()?;
        let busy: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM roots WHERE status IN ('pending','indexing'))",
            [], |row| row.get(0),
        )?;
        if busy { bail!("正在索引，请等待当前任务完成后再移除所有文件夹") }
        tx.execute_batch("DELETE FROM chunk_fts;
            DELETE FROM documents;
            DELETE FROM index_issues;
            DELETE FROM roots;
            UPDATE metadata SET version=version+1;")?;
        tx.commit()?;
        self.reclaim_storage(&db)
    }
    pub fn clear_all_data(&self) -> Result<()> {
        let mut db = self.connection();
        db.execute_batch("PRAGMA secure_delete=ON;")?;
        let tx = db.transaction()?;
        let busy: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM roots WHERE status IN ('pending','indexing'))",
            [], |row| row.get(0),
        )?;
        if busy { bail!("正在索引，请等待当前任务完成后再清除所有数据") }
        tx.execute_batch("DELETE FROM conversations;
            DELETE FROM index_history;
            DELETE FROM drafts;
            DELETE FROM preferences;
            DELETE FROM chunk_fts;
            DELETE FROM documents;
            DELETE FROM index_issues;
            DELETE FROM roots;
            UPDATE metadata SET version=version+1;")?;
        tx.commit()?;
        self.reclaim_storage(&db)
    }
    pub fn excluded_directories(&self) -> Result<Vec<String>> {
        let db = self.connection();
        let mut query = db.prepare("SELECT path FROM root_exclusions ORDER BY path")?;
        let paths = query.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(paths)
    }
    fn path_excluded(db: &Connection, path: &str) -> Result<bool> {
        Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM root_exclusions e WHERE e.path=?1 OR instr(?1,e.path || ?2)=1 OR EXISTS(SELECT 1 FROM conversions cv WHERE cv.output=?1 AND (cv.source=e.path OR instr(cv.source,e.path || ?2)=1)))",
            params![path,std::path::MAIN_SEPARATOR.to_string()], |row| row.get(0))?)
    }
    pub fn is_excluded(&self, path: &Path) -> Result<bool> {
        Self::path_excluded(&self.connection(), &path_text(path)?)
    }
    pub fn authorize(&self, path: &Path) -> Result<PathBuf> {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("路径不可用：{}", path.display()))?;
        let allowed = {
            let connection = self.connection();
            let mut query = connection.prepare_cached("SELECT path FROM roots")?;
            let paths = query.query_map([], |r| r.get::<_, String>(0))?;
            let mut allowed = false;
            for path in paths {
                allowed |= canonical.starts_with(Path::new(&path?));
            }
            allowed
        };
        if !allowed {
            bail!("该路径不在已添加的文件夹内")
        };
        if self.is_excluded(&canonical)? { bail!("此路径已从知识库移除，请先重新添加") }
        Ok(canonical)
    }
    pub fn add_root(&self, path: &Path) -> Result<String> {
        let path = path.canonicalize().context("无法打开这个文件夹")?;
        if !path.is_dir() && !path.is_file() { bail!("请选择文件或文件夹") }
        let roots = self.roots()?;
        if let Some(r) = roots.iter().find(|r| path.starts_with(&r.path)) {
            let value = path_text(&path)?;
            let mut db = self.connection();
            let tx = db.transaction()?;
            tx.execute("DELETE FROM root_exclusions WHERE path=?", [&value])?;
            if Self::path_excluded(&tx, &value)? { bail!("请先重新添加已移除的上级目录") }
            tx.commit()?;
            return Ok(r.path.clone());
        };
        if !path.is_dir() {
            bail!("请选择文件夹")
        };
        let value = path_text(&path)?;
        let name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();
        let mut c = self.connection();
        let tx = c.transaction()?;
        tx.execute(
            "INSERT INTO roots(path,name) VALUES(?,?)",
            params![value, name],
        )?;
        for r in roots
            .iter()
            .filter(|r| Path::new(&r.path).starts_with(&path))
        {
            tx.execute(
                "UPDATE documents SET root=? WHERE root=?",
                params![value, r.path],
            )?;
            tx.execute("UPDATE index_issues SET root=? WHERE root=?", params![value, r.path])?;
            tx.execute("UPDATE root_exclusions SET root=? WHERE root=?", params![value, r.path])?;
            tx.execute("DELETE FROM roots WHERE path=?", [&r.path])?;
        }
        tx.commit()?;
        Ok(value)
    }
    pub fn mark_root_pending(&self, root: &str) -> Result<()> {
        self.connection().execute("UPDATE roots SET status='pending',error=NULL WHERE path=?", [root])?;
        Ok(())
    }
    pub fn remove_path(&self, path: &Path) -> Result<()> {
        let value = path_text(path)?;
        let roots = self.roots()?;
        if roots.iter().any(|root| root.path == value) { return self.remove_root(&value); }
        let root = roots.iter().find(|root| path.starts_with(&root.path)).context("此路径不在知识库中")?;
        if !path.is_dir() && !path.is_file() { bail!("只能移除知识库中的文件或文件夹") }
        let mut db = self.connection();
        let tx = db.transaction()?;
        if Self::path_excluded(&tx, &value)? { bail!("此路径已从知识库移除") }
        let prefix = format!("{}{}", value.trim_end_matches(std::path::MAIN_SEPARATOR), std::path::MAIN_SEPARATOR);
        tx.execute("DELETE FROM root_exclusions WHERE instr(path,?1)=1", [&prefix])?;
        tx.execute("INSERT INTO root_exclusions(path,root) VALUES(?,?)", params![value,root.path])?;
        Self::remove_indexed_under_tx(&tx, &root.path, path)?;
        tx.execute("UPDATE metadata SET version=version+1", [])?;
        tx.commit()?;
        drop(db);
        if root.status == "partial" { self.finish_indexing(&root.path)?; }
        Ok(())
    }
    pub fn remove_root(&self, path: &str) -> Result<()> {
        let mut c = self.connection();
        let tx = c.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM roots WHERE path=?)",
            [path],
            |r| r.get(0),
        )?;
        if !exists {
            bail!("只能移除已添加的顶层文件夹")
        };
        tx.execute("DELETE FROM chunk_fts WHERE rowid IN (SELECT c.id FROM chunks c JOIN documents d ON d.id=c.document_id WHERE d.root=?)",[path])?;
        tx.execute("DELETE FROM documents WHERE root=?", [path])?;
        tx.execute("DELETE FROM roots WHERE path=?", [path])?;
        tx.execute("UPDATE metadata SET version=version+1", [])?;
        tx.commit()?;
        Ok(())
    }
    pub fn entries(&self, path: &Path, show_hidden: bool) -> Result<Vec<Entry>> {
        let path = self.authorize(path)?;
        let mut entries = vec![];
        for item in fs::read_dir(path)? {
            let item = item?;
            let name = item.file_name().to_string_lossy().to_string();
            if !show_hidden && name.starts_with('.') {
                continue;
            };
            let ty = item.file_type()?;
            if ty.is_dir() && self.is_excluded(&item.path())? { continue; }
            if ty.is_dir() || ty.is_symlink() || ty.is_file() {
                entries.push(Entry {
                    path: path_text(&item.path())?,
                    name,
                    directory: ty.is_dir(),
                    symlink: ty.is_symlink(),
                })
            }
        }
        entries.sort_by(|a, b| {
            b.directory
                .cmp(&a.directory)
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    }
    pub fn document(&self, path: &Path) -> Result<Document> {
        Ok(self.matching_document(path, |_| true)?.unwrap())
    }
    pub(crate) fn matching_document(&self, path: &Path, matches: impl FnOnce(&str) -> bool) -> Result<Option<Document>> {
        let path = self.authorize(path)?;
        let path = if crate::docling::is_source(&path) {
            let output: Option<String> = self.connection().query_row("SELECT output FROM conversions WHERE source=? AND valid=1", [path_text(&path)?], |row| row.get(0)).optional()?;
            self.authorize(Path::new(&output.context("请启用其他格式并完成转换后查看文档")?))?
        } else { path };
        let raw = fs::read(&path)?;
        let body = crate::document_text::read_borrowed(&path, &raw)?;
        if !matches(&body) { return Ok(None); }
        let id = self
            .connection()
            .query_row(
                "SELECT id FROM documents WHERE path=?",
                [path_text(&path)?],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        Ok(Some(Document {
            id,
            path: path_text(&path)?,
            name: path.file_name().unwrap().to_string_lossy().into(),
            body: body.into_owned(),
            hash: hash(&raw),
        }))
    }
    pub fn files(&self) -> Result<Vec<FileInfo>> {
        let c = self.connection();
        let mut q = c.prepare("SELECT id,path,name FROM documents ORDER BY name,path")?;
        let rows = q
            .query_map([], |r| {
                Ok(FileInfo {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    name: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
    pub fn save_document(
        &self,
        path: &Path,
        content: &str,
        expected_hash: Option<&str>,
    ) -> Result<Document> {
        let parent = self.authorize(path.parent().context("缺少父文件夹")?)?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .context("文件名无效")?;
        let stem = name.split('.').next().unwrap().to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.ends_with(['1', '2', '3', '4', '5', '6', '7', '8', '9']));
        if name.is_empty()
            || reserved
            || name.contains(['/', '\\', ':', '\0', '?', '*', '<', '>', '|', '"'])
            || !text::is_markdown(path)
            || name.trim_end_matches([' ', '.']) != name
        {
            bail!("请输入有效的 .md 文件名")
        }
        let target = parent.join(name);
        let old = match expected_hash {
            Some(expected) => {
                let p = self.authorize(&target)?;
                let bytes = fs::read(p)?;
                if hash(&bytes) != expected {
                    bail!("文件已被其他程序修改。请先重新读取，当前编辑内容已保留。")
                };
                Some(bytes)
            }
            None => {
                if target.exists() {
                    bail!("同名文件已存在，请修改文件名")
                };
                None
            }
        };
        let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
        let mut output = normalized;
        let mut original_text = None;
        if let Some(bytes) = &old {
            let decoded = crate::document_text::decode(bytes)?;
            if decoded.body == content {
                return self.document(&target);
            }
            if decoded.body.contains("\r\n") {
                output = output.replace('\n', "\r\n")
            };
            original_text = Some(decoded);
        }
        let output = match original_text {
            Some(decoded) => decoded.encode(&output)?,
            None => output.into_bytes(),
        };
        if target
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            bail!("请在系统文件管理器中打开符号链接，应用不会替换链接")
        };
        let mut temp = tempfile::NamedTempFile::new_in(&parent)?;
        if old.is_some() {
            temp.as_file()
                .set_permissions(fs::metadata(&target)?.permissions())?;
        }
        temp.write_all(&output)?;
        temp.as_file().sync_all()?;
        if let Some(bytes) = &old {
            if hash(&fs::read(&target)?) != hash(bytes) {
                bail!("保存前检测到外部修改，未覆盖原文件")
            };
            temp.persist(&target).map_err(|e| e.error)?;
        } else {
            temp.persist_noclobber(&target).map_err(|e| e.error)?;
        }
        self.document(&target)
    }
    fn root_status(&self, path: &str, status: &str, error: Option<&str>) -> Result<()> {
        self.connection().execute(
            "UPDATE roots SET status=?,error=? WHERE path=?",
            params![status, error, path],
        )?;
        Ok(())
    }
    fn finish_indexing(&self, root: &str) -> Result<()> {
        let issues = {
            let db = self.connection();
            let mut query = db.prepare("SELECT path,error FROM index_issues WHERE root=? ORDER BY path")?;
            let rows = query.query_map([root], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        if issues.is_empty() {
            self.root_status(root, "ready", None)
        } else {
            let details = format!("未能索引 {} 个文件\n\n{}", issues.len(), issues.into_iter()
                .map(|(path, error)| format!("{path}\n{error}")).collect::<Vec<_>>().join("\n\n"));
            self.root_status(root, "partial", Some(&details))
        }
    }
    pub fn scan(&self, root: &str, progress: impl Fn(IndexProgress)) -> Result<usize> {
        if !self.begin_indexing(root)? { return Ok(0); }
        let started_at = now();
        self.indexing.begin(root);
        let result = self.indexing.checkpoint().and_then(|_| self.scan_inner(root, Path::new(root), |p| { self.indexing.progress(root, p.count, p.total); progress(p) }));
        match &result {
            Ok(_) => self.finish_indexing(root)?,
            Err(e) if !self.indexing.stopped() => self.root_status(root, "error", Some(&e.to_string()))?,
            Err(_) => {},
        };
        let saved = self.record_index_history(root, started_at);
        self.indexing.finish(root);
        saved?;
        result
    }
    pub fn update_paths(&self, root: &str, paths: &[PathBuf], progress: impl Fn(IndexProgress)) -> Result<usize> {
        if !self.begin_indexing(root)? { return Ok(0); }
        let started_at = now();
        self.indexing.begin(root);
        let result = self.indexing.checkpoint().and_then(|_| self.update_paths_inner(root, paths, |p| { self.indexing.progress(root, p.count, p.total); progress(p) }));
        match &result {
            Ok(_) => self.finish_indexing(root)?,
            Err(error) if !self.indexing.stopped() => self.root_status(root, "error", Some(&error.to_string()))?,
            Err(_) => {},
        }
        let saved = self.record_index_history(root, started_at);
        self.indexing.finish(root);
        saved?;
        result
    }
    fn record_index_history(&self, root: &str, started_at: i64) -> Result<()> {
        let task = self.indexing.task(root);
        let db = self.connection();
        let (name, status, error): (String, String, Option<String>) = db.query_row(
            "SELECT name,status,error FROM roots WHERE path=?", [&task.path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let status = if self.indexing.stopped() { "stopped" } else { &status };
        let mut files = self.indexing.log(root);
        let mut source_query = db.prepare("SELECT source FROM conversions WHERE output=?")?;
        for file in &mut files {
            if let Some(source) = source_query.query_row([&file.path], |row| row.get::<_, String>(0)).optional()? {
                file.path = source;
            }
        }
        let raw = files.iter().map(serde_json::to_string).collect::<serde_json::Result<Vec<_>>>()?.join("\n");
        let (completed, total) = self.indexing.indexed(root);
        db.execute("INSERT INTO index_history(path,name,started_at,finished_at,completed,total,status,error,file_log) VALUES(?,?,?,?,?,?,?,?,?)",
            params![task.path, name, started_at, now(), completed, total, status, error, raw])?;
        Ok(())
    }
    pub fn index_history_log(&self, id: i64) -> Result<sag_protocol::IndexHistoryLog> {
        let raw = self.connection().query_row("SELECT file_log FROM index_history WHERE id=?", [id], |row| row.get(0))?;
        Ok(sag_protocol::IndexHistoryLog { raw })
    }
    pub fn authorize_index_history_file(&self, id: i64, path: &str) -> Result<PathBuf> {
        let log = self.index_history_log(id)?;
        let mut found = false;
        if let Some(raw) = log.raw {
            for line in raw.lines() {
                let file: sag_protocol::IndexFileLog = serde_json::from_str(line)?;
                found |= file.path == path;
            }
        }
        if !found { bail!("文件不在该次索引记录中") }
        let path = self.authorize(Path::new(path))?;
        if !path.is_file() { bail!("原文件已不存在") }
        Ok(path)
    }
    pub fn index_history(&self) -> Result<Vec<sag_protocol::IndexHistory>> {
        let db = self.connection();
        let mut query = db.prepare("SELECT id,path,name,started_at,finished_at,completed,total,status,error FROM index_history ORDER BY id DESC LIMIT 100")?;
        let records = query.query_map([], |row| Ok(sag_protocol::IndexHistory {
            id: row.get(0)?, path: row.get(1)?, name: row.get(2)?, started_at: row.get(3)?,
            finished_at: row.get(4)?, completed: row.get(5)?, total: row.get(6)?, status: row.get(7)?, error: row.get(8)?,
        }))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(records)
    }
    pub fn clear_index_history(&self) -> Result<()> {
        self.connection().execute("DELETE FROM index_history", [])?;
        Ok(())
    }
    fn begin_indexing(&self, root: &str) -> Result<bool> {
        Ok(self.connection().execute(
            "UPDATE roots SET status='indexing',error=NULL WHERE path=?",
            [root],
        )? > 0)
    }
    fn update_paths_inner(&self, root: &str, paths: &[PathBuf], progress: impl Fn(IndexProgress)) -> Result<usize> {
        let settings = self.settings()?;
        if !Path::new(root).is_dir() {
            bail!("文件夹不可用，请检查磁盘或重新添加")
        }
        if paths.iter().any(|path| path.starts_with(root) && ignore_rules::is_rule_file(path)) {
            let directory = if paths.len() == 1 { paths[0].parent().unwrap() } else { Path::new(root) };
            return self.scan_inner(root, directory, progress);
        }
        let mut rules = crate::folder_index::Rules::new(Path::new(root), &settings)?;
        for path in paths.iter().filter(|path| path.starts_with(root) && !crate::docling::in_output(path)) {
            let options = rules.file(path)?;
            if options.convert_other_formats && (crate::docling::is_source(path) && crate::docling::is_selected(path, &options.conversion_formats) || path.is_dir() || !path.exists()) {
                return self.scan_inner(root, Path::new(root), progress);
            }
        }
        let mut ignore = ignore_rules::matcher(Path::new(root));
        enum Work { Directory(PathBuf, Vec<PathBuf>), File(PathBuf), Remove(PathBuf) }
        let mut work = Vec::new();
        let mut total = 0;
        let excluded = self.excluded_directories()?;
        for path in paths {
            if excluded.iter().any(|directory| path.starts_with(directory)) { continue; }
            let Ok(relative) = path.strip_prefix(root) else { continue; };
            if relative.components().any(|part| settings.excluded.iter().any(|name| part.as_os_str() == name.as_str())) { continue; }
            if text::in_hidden_directory(Path::new(root), path, false) {
                work.push(Work::Remove(path.clone()));
                total += 1;
                continue;
            }
            let metadata = match fs::symlink_metadata(path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    work.push(Work::Remove(path.clone()));
                    total += 1;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            if ignore_rules::is_ignored(&mut ignore, path, metadata.is_dir())?
                || metadata.file_type().is_symlink() || !path.canonicalize()?.starts_with(root)
                || text::in_hidden_directory(Path::new(root), path, metadata.is_dir()) {
                work.push(Work::Remove(path.clone()));
                total += 1;
            } else if metadata.is_dir() {
                let files = self.index_files(Path::new(root), path, &settings, &excluded, None)?;
                total += files.len().max(1);
                work.push(Work::Directory(path.clone(), files));
            } else if metadata.is_file() && Self::indexable_path(path, rules.file(path)?)
                && (!crate::docling::in_output(path) || self.conversion_outputs()?.contains(&path_text(path)?)) {
                work.push(Work::File(path.clone()));
                total += 1;
            } else {
                work.push(Work::Remove(path.clone()));
                total += 1;
            }
        }
        let mut count = 0;
        let mut completed = 0;
        progress(IndexProgress { count: 0, total });
        self.indexing.phase(root, "indexing", total);
        for item in work {
            self.indexing.checkpoint()?;
            match item {
                Work::Directory(path, files) => {
                    count += self.scan_directory(root, &path, &files, &settings, true, None, |p| progress(IndexProgress { count: completed + p.count, total }))?;
                    completed += files.len().max(1);
                }
                Work::File(path) => {
                    let result = self.indexing.file(root, &path, || self.prepare_file(&path, settings.retrieval.chunk_chars, true));
                    if let Some(file) = self.prepared_or_issue(root, &path, result)? {
                        let mut db = self.connection();
                        let tx = db.transaction()?;
                        Self::write_prepared(&tx, root, file)?;
                        tx.commit()?;
                    }
                    count += 1;
                    completed += 1;
                }
                Work::Remove(path) => {
                    count += self.remove_indexed_under(root, &path)?;
                    completed += 1;
                }
            }
            progress(IndexProgress { count: completed, total });
        }
        Ok(count)
    }
    fn remove_indexed_under(&self, root: &str, path: &Path) -> Result<usize> {
        let mut db = self.connection();
        let tx = db.transaction()?;
        let removed = Self::remove_indexed_under_tx(&tx, root, path)?;
        tx.commit()?;
        Ok(removed)
    }
    fn remove_indexed_under_tx(tx: &rusqlite::Transaction<'_>, root: &str, path: &Path) -> Result<usize> {
        let source = path_text(path)?;
        let prefix = format!("{}{}", source.trim_end_matches(std::path::MAIN_SEPARATOR), std::path::MAIN_SEPARATOR);
        let outputs = {
            let mut query = tx.prepare("SELECT output FROM conversions WHERE root=?1 AND (source=?2 OR instr(source,?3)=1)")?;
            let values = query.query_map(params![root,source,prefix], |row| row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            values
        };
        for output in outputs { Self::remove_indexed_under_tx(tx, root, Path::new(&output))?; }
        tx.execute("UPDATE conversions SET valid=0 WHERE root=?1 AND (source=?2 OR instr(source,?3)=1)", params![root,source,prefix])?;
        let path = source;
        let prefix = format!("{}{}", path.trim_end_matches(std::path::MAIN_SEPARATOR), std::path::MAIN_SEPARATOR);
        tx.execute("DELETE FROM chunk_fts WHERE rowid IN (SELECT c.id FROM chunks c JOIN documents d ON d.id=c.document_id WHERE d.root=?1 AND (d.path=?2 OR instr(d.path,?3)=1))", params![root,path,prefix])?;
        let removed = tx.execute("DELETE FROM documents WHERE root=?1 AND (path=?2 OR instr(path,?3)=1)", params![root,path,prefix])?;
        tx.execute("DELETE FROM index_issues WHERE root=?1 AND (path=?2 OR instr(path,?3)=1)", params![root,path,prefix])?;
        if removed > 0 { tx.execute("UPDATE metadata SET version=version+1", [])?; }
        Ok(removed)
    }
    fn scan_inner(&self, root: &str, directory: &Path, progress: impl Fn(IndexProgress)) -> Result<usize> {
        let settings = self.settings()?;
        if !Path::new(root).is_dir() {
            bail!("文件夹不可用，请检查磁盘或重新添加")
        };
        let excluded = self.excluded_directories()?;
        let files = self.index_files(Path::new(root), directory, &settings, &excluded, Some(false))?;
        let count = self.scan_directory(root, directory, &files, &settings, false, Some(false), &progress)?;
        let (sources, convert) = self.conversion_files(Path::new(root), directory, &settings, &excluded)?;
        let issues = if convert { self.convert_sources(root, directory, &sources, &settings, &progress)? } else { vec![] };
        let files = self.index_files(Path::new(root), directory, &settings, &excluded, Some(true))?;
        let converted_count = self.scan_directory(root, directory, &files, &settings, false, Some(true), progress)?;
        for (path, error) in issues {
            self.connection().execute("INSERT INTO index_issues(path,root,error) VALUES(?,?,?) ON CONFLICT(path) DO UPDATE SET root=excluded.root,error=excluded.error", params![path,root,error])?;
        }
        Ok(count + converted_count)
    }
    fn conversion_outputs(&self) -> Result<HashSet<String>> {
        let db = self.connection();
        let mut query = db.prepare("SELECT output FROM conversions WHERE valid=1")?;
        let rows = query.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
    pub(crate) fn indexable_path(path: &Path, settings: &FolderIndexOptions) -> bool {
        if crate::docling::in_output(path) {
            settings.convert_other_formats && text::is_markdown(path)
                && crate::docling::is_selected(&path.with_extension(""), &settings.conversion_formats)
        } else if let Some(extensions) = &settings.extensions {
            path.extension().and_then(|extension| extension.to_str())
                .is_some_and(|extension| extensions.iter().any(|selected| selected.eq_ignore_ascii_case(extension)))
        } else {
            text::is_indexable(path, &settings.file_types)
        }
    }
    fn conversion_files(&self, root: &Path, directory: &Path, settings: &Settings, excluded: &[String]) -> Result<(Vec<PathBuf>, bool)> {
        let excluded = excluded.to_vec();
        let names = settings.excluded.clone();
        let target = directory.to_path_buf();
        let mut rules = crate::folder_index::Rules::new(root, settings)?;
        let mut builder = ignore_rules::builder(root);
        builder.filter_entry(move |entry| entry.depth() == 0 ||
            (entry.path().starts_with(&target) || target.starts_with(entry.path()))
            && !crate::docling::in_output(entry.path())
            && !excluded.iter().any(|path| entry.path().starts_with(path))
            && !names.iter().any(|name| entry.file_name() == name.as_str())
            && !(entry.file_type().is_some_and(|kind| kind.is_dir()) && entry.file_name().as_encoded_bytes().starts_with(b".")));
        let mut files = vec![];
        for entry in builder.build() {
            self.indexing.checkpoint()?;
            let Some(entry) = self.walk_entry(root, entry)? else { continue };
            if let Some(error) = entry.error() { bail!("无法读取项目忽略规则：{error}") }
            if entry.file_type().is_some_and(|kind| kind.is_dir()) { rules.directory(entry.path())?; }
            if entry.path().starts_with(directory) && entry.file_type().is_some_and(|kind| kind.is_file())
                && crate::docling::is_source(entry.path()) && !crate::docling::is_pem_key(entry.path())?
                && rules.file(entry.path())?.convert_other_formats
                && crate::docling::is_selected(entry.path(), &rules.file(entry.path())?.conversion_formats) {
                files.push(entry.into_path());
            }
        }
        Ok((files, rules.converts_files()))
    }
    fn convert_sources(&self, root: &str, directory: &Path, sources: &[PathBuf], settings: &Settings, progress: &impl Fn(IndexProgress)) -> Result<Vec<(String,String)>> {
        let mut issues = vec![];
        let mut rules = crate::folder_index::Rules::new(Path::new(root), &self.settings()?)?;
        // Keep ownership beside the derived files so removing/re-adding a library
        // can reuse them without mistaking user files for disposable output.
        let metadata_path = crate::docling::output_path(Path::new(root), &Path::new(root).join(".ownership"))?
            .parent().unwrap().join(".sag-converted.json");
        if metadata_path.symlink_metadata().is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            bail!(".sag-converted 转换记录不能是符号链接")
        }
        if metadata_path.is_file() {
            let records: std::collections::BTreeMap<String,(String,String)> = serde_json::from_slice(&fs::read(&metadata_path)?)
                .context("无法读取 .sag-converted 转换记录")?;
            for (relative, (source_hash, output_hash)) in records {
                let source = Path::new(root).join(relative);
                let output = crate::docling::output_path(Path::new(root), &source)?;
                if !crate::docling::is_source(&source) {
                    bail!("转换记录包含无效的原文件路径")
                }
                self.connection().execute("INSERT OR IGNORE INTO conversions(source,root,source_hash,output,output_hash) VALUES(?,?,?,?,?)",
                    params![path_text(&source)?,root,source_hash,path_text(&output)?,output_hash])?;
            }
        }
        let sources_set: HashSet<_> = sources.iter().map(|path| path_text(path)).collect::<Result<_>>()?;
        let previous = {
            let db = self.connection();
            let scope = path_text(directory)?;
            let prefix = format!("{}{}", scope.trim_end_matches(std::path::MAIN_SEPARATOR), std::path::MAIN_SEPARATOR);
            let mut query = db.prepare("SELECT source,output,output_hash FROM conversions WHERE root=?1 AND (source=?2 OR instr(source,?3)=1)")?;
            let rows = query.query_map(params![root,scope,prefix], |row| Ok((row.get::<_, String>(0)?,row.get::<_, String>(1)?,row.get::<_, String>(2)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        for (source, output, output_hash) in previous {
            self.indexing.checkpoint()?;
            let options = rules.file(Path::new(&source))?;
            if sources_set.contains(&source) || !options.convert_other_formats || !crate::docling::is_selected(Path::new(&source), &options.conversion_formats) { continue; }
            let path = Path::new(&output);
            // Delete only our unchanged derivative; user-edited Markdown is preserved.
            if path.is_file() && crate::docling::fingerprint(path)? == output_hash {
                fs::remove_file(path)?;
            }
            self.connection().execute("DELETE FROM conversions WHERE source=?", [&source])?;
            self.remove_indexed(&output)?;
        }
        self.indexing.phase(root, "converting", sources.len());
        progress(IndexProgress { count: 0, total: sources.len() });
        let next = std::sync::atomic::AtomicUsize::new(0);
        thread::scope(|scope| -> Result<()> {
            let (tx, rx) = std::sync::mpsc::channel();
            let mut handles = Vec::new();
            for _ in 0..settings.conversion_workers.min(sources.len()) {
                let tx = tx.clone();
                let next = &next;
                handles.push(scope.spawn(move || -> Result<()> {
                    loop {
                        self.indexing.checkpoint()?;
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if index >= sources.len() { break; }
                        let source = &sources[index];
                        let source_text = path_text(source)?;
                        let mut converted = false;
                        let result = (|| -> Result<bool> {
                            let output = crate::docling::output_path(Path::new(root), source)?;
                            let source_hash = crate::docling::fingerprint(source)?;
                            let previous: Option<(String,String)> = self.connection().query_row(
                                "SELECT source_hash,output_hash FROM conversions WHERE source=?", [&source_text],
                                |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
                            if let Some((previous_source, previous_output)) = &previous {
                                if output.exists() {
                                    if previous_source == &source_hash {
                                        self.connection().execute("UPDATE conversions SET valid=1 WHERE source=?", [&source_text])?;
                                        return Ok(true);
                                    }
                                    if crate::docling::fingerprint(&output)? != *previous_output {
                                        bail!("转换文件已被手动修改，未覆盖：{}", output.display())
                                    }
                                }
                            } else if output.exists() { bail!("转换位置已有文件，未覆盖：{}", output.display()) }
                            fs::create_dir_all(output.parent().unwrap())?;
                            // Keep the same parent as the final Markdown: Docling derives
                            // relative source links from directory depth, not the file name.
                            let staged = tempfile::Builder::new().prefix(".sag-pending-").suffix(".md")
                                .tempfile_in(output.parent().unwrap())?.into_temp_path();
                            self.indexing.file(root, source, || self.docling.convert_cancellable(source, &staged, || self.indexing.stopped()))?;
                            converted = true;
                            // Pause between conversion and acceptance without retaining an
                            // "active file" entry or a database lock.
                            self.indexing.checkpoint()?;
                            if let Err(error) = self.publish_converted_file(root, source, &source_hash, &output, staged,
                                previous.as_ref().map(|(_, hash)| hash.as_str()), settings.retrieval.chunk_chars) {
                                // Keep successful history stages stable; add a failed entry
                                // only when acceptance rejects an otherwise converted file.
                                self.indexing.file(root, source, || Err::<(), _>(error))?;
                            }
                            Ok(false)
                        })();
                        let preserve_previous = converted || result.as_ref().err().is_some_and(|error| error.is::<std::io::Error>());
                        tx.send((source_text, result, preserve_previous))?;
                    }
                    Ok(())
                }));
            }
            drop(tx);
            for (index, (source, result, preserve_previous)) in rx.into_iter().enumerate() {
                if let Err(error) = result {
                    if preserve_previous && error.is::<rusqlite::Error>() { return Err(error); }
                    if !preserve_previous {
                        // Backend failure still invalidates evidence for the changed source.
                        self.connection().execute("UPDATE conversions SET valid=0 WHERE source=?", [&source])?;
                    }
                    issues.push((source, error.to_string()));
                } else if result.unwrap() {
                    let output = crate::docling::output_path(Path::new(root), Path::new(&source))?;
                    if let Some(file) = self.prepare_indexed_file(root, &output, settings.retrieval.chunk_chars, false)? {
                        let mut db = self.connection();
                        let tx = db.transaction()?;
                        Self::write_prepared(&tx, root, file)?;
                        tx.commit()?;
                    }
                }
                progress(IndexProgress { count: index + 1, total: sources.len() });
            }
            for handle in handles { handle.join().expect("conversion worker panicked")?; }
            Ok(())
        })?;
        if metadata_path.parent().unwrap().is_dir() {
            let db = self.connection();
            let mut query = db.prepare("SELECT source,source_hash,output_hash FROM conversions WHERE root=?")?;
            let records = query.query_map([root], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?.into_iter().map(|(source, source_hash, output_hash)| {
                    Ok((Path::new(&source).strip_prefix(root)?.to_string_lossy().into_owned(), (source_hash, output_hash)))
                }).collect::<Result<std::collections::BTreeMap<_,_>>>()?;
            let mut file = tempfile::NamedTempFile::new_in(metadata_path.parent().unwrap())?;
            serde_json::to_writer(file.as_file_mut(), &records)?;
            file.persist(&metadata_path).map_err(|error| error.error)?;
        }
        Ok(issues)
    }
    fn publish_converted_file(&self, root: &str, source: &Path, source_hash: &str, output: &Path,
        staged: tempfile::TempPath, previous_hash: Option<&str>, max_chars: usize) -> Result<()> {
        let reserved = preparation_reservation(&staged);
        let permit = self.text_preparation.reserve(reserved, || self.indexing.stopped())?;
        let mut prepared = self.prepare_file_at(output, &staged, max_chars, true, reserved)?
            .context("转换文件准备未生成完整索引")?;
        prepared._permit = Some(permit);
        self.indexing.checkpoint()?;
        if crate::docling::fingerprint(source)? != source_hash {
            bail!("转换期间原文件发生变化，请重新索引")
        }
        let backup = if output.exists() {
            if previous_hash != Some(crate::docling::fingerprint(output)?.as_str()) {
                self.connection().execute("UPDATE conversions SET valid=0 WHERE source=?", [path_text(source)?])?;
                bail!("转换文件已被手动修改，未覆盖：{}", output.display())
            }
            let backup = tempfile::Builder::new().prefix(".sag-previous-").suffix(".md")
                .tempfile_in(output.parent().unwrap())?.into_temp_path();
            fs::copy(output, &backup)?;
            Some(backup)
        } else { None };
        let output_hash = prepared.fingerprint.clone();
        let mut published = false;
        let outcome = (|| -> Result<()> {
            let mut db = self.connection();
            let tx = db.transaction()?;
            Self::write_prepared(&tx, root, prepared)?;
            tx.execute("INSERT INTO conversions(source,root,source_hash,output,output_hash) VALUES(?,?,?,?,?) ON CONFLICT(source) DO UPDATE SET root=excluded.root,source_hash=excluded.source_hash,output=excluded.output,output_hash=excluded.output_hash,valid=1",
                params![path_text(source)?,root,source_hash,path_text(output)?,output_hash])?;
            tx.execute("DELETE FROM index_issues WHERE path=?", [path_text(source)?])?;
            if self.indexing.stopped() { bail!("索引已停止") }
            fs::rename(&staged, output)?;
            published = true;
            tx.commit()?;
            Ok(())
        })();
        if let Err(error) = outcome {
            if published {
                if let Some(backup) = backup {
                    if let Err(restore_error) = fs::rename(&backup, output) {
                        let recovery = backup.keep().map_err(|keep| keep.error)?;
                        return Err(error.context(format!("旧转换文件恢复失败：{restore_error}；备份保留于 {}", recovery.display())));
                    }
                } else {
                    fs::remove_file(output).context("未提交的转换文件清理失败")?;
                }
            }
            return Err(error);
        }
        Ok(())
    }
    /// Resolves one walk entry. A directory that cannot be listed (macOS privacy
    /// protection, foreign ownership, Windows ACLs) is recorded as an index issue
    /// and skipped so the rest of the folder still gets indexed.
    fn walk_entry(&self, root: &Path, entry: std::result::Result<ignore::DirEntry, ignore::Error>) -> Result<Option<ignore::DirEntry>> {
        let error = match entry {
            Ok(entry) => return Ok(Some(entry)),
            Err(error) => error,
        };
        let Some(io) = error.io_error() else { return Err(error.into()) };
        if !matches!(io.kind(), std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::NotFound) {
            return Err(error.into());
        }
        let path = match &error {
            ignore::Error::WithPath { path, .. } => path.clone(),
            ignore::Error::WithDepth { err, .. } => match err.as_ref() {
                ignore::Error::WithPath { path, .. } => path.clone(),
                _ => root.to_path_buf(),
            },
            _ => root.to_path_buf(),
        };
        eprintln!("跳过无法读取的目录：{}：{io}", path.display());
        self.connection().execute(
            "INSERT INTO index_issues(path,root,error) VALUES(?,?,?) ON CONFLICT(path) DO UPDATE SET root=excluded.root,error=excluded.error",
            params![path_text(&path)?, path_text(root)?, format!("无法读取目录：{io}")],
        )?;
        Ok(None)
    }
    fn index_files(&self, root: &Path, directory: &Path, settings: &Settings, excluded: &[String], converted_only: Option<bool>) -> Result<Vec<PathBuf>> {
        let converted = if converted_only == Some(false) { HashSet::new() } else { self.conversion_outputs()? };
        let mut rules = crate::folder_index::Rules::new(root, settings)?;
        let converted_target = crate::docling::output_path(root, &directory.join(".scope"))?.parent().unwrap().to_path_buf();
        let output_target = converted_target.clone();
        let target = directory.to_path_buf();
        let excluded = excluded.to_vec();
        let names = settings.excluded.clone();
        let mut builder = ignore_rules::builder(root);
        builder.filter_entry(move |entry| {
            let path = entry.path();
            (if converted_only == Some(true) { path.starts_with(&output_target) || output_target.starts_with(path) }
             else { path.starts_with(&target) || target.starts_with(path) || path.starts_with(&output_target) || output_target.starts_with(path) })
                && (converted_only != Some(false) || !crate::docling::in_output(path))
                && !excluded.iter().any(|removed| path.starts_with(removed))
                && (entry.depth() == 0 || !(entry.file_type().is_some_and(|ty| ty.is_dir())
                    && entry.file_name().as_encoded_bytes().starts_with(b".")
                    && entry.file_name() != crate::docling::OUTPUT_DIRECTORY)
                    && !names.iter().any(|name| entry.file_name() == name.as_str()))
        });
        let mut files = Vec::new();
        for entry in builder.build() {
            self.indexing.checkpoint()?;
            let Some(entry) = self.walk_entry(root, entry)? else { continue };
            if let Some(error) = entry.error() { bail!("无法读取项目忽略规则：{error}") }
            if (entry.path().starts_with(directory) || entry.path().starts_with(&converted_target)) && entry.file_type().is_some_and(|ty| ty.is_file())
                && converted_only.is_none_or(|converted| converted == crate::docling::in_output(entry.path()))
                && Self::indexable_path(entry.path(), rules.file(entry.path())?)
                && (!crate::docling::in_output(entry.path()) || converted.contains(&path_text(entry.path())?)) {
                files.push(entry.into_path());
            }
        }
        Ok(files)
    }
    fn scan_directory(&self, root: &str, directory: &Path, files: &[PathBuf], settings: &Settings, verify: bool, converted_only: Option<bool>, progress: impl Fn(IndexProgress)) -> Result<usize> {
        let mut seen = HashSet::new();
        let mut count = 0;
        let total = files.len();
        let report = converted_only != Some(true) || total > 0;
        if report {
            self.indexing.phase(root, match converted_only { Some(false) => "indexing_native", Some(true) => "indexing_converted", None => "indexing" }, total);
            progress(IndexProgress { count, total });
        }
        let mut next = 0;
        while next < files.len() {
            self.indexing.checkpoint()?;
            // Reserve the complete window before starting it. Acquiring per-result permits
            // in workers could deadlock ordered writes behind a later completed result.
            let mut window = Vec::new();
            let mut reserved = 0;
            while next < files.len() && window.len() < settings.index_workers {
                let bytes = preparation_reservation(&files[next]);
                if reserved + bytes > TEXT_PREPARATION_BUDGET { break; }
                window.push((&files[next], bytes));
                reserved += bytes;
                next += 1;
            }
            let _permit = self.text_preparation.reserve(reserved, || self.indexing.stopped())?;
            let max_chars = settings.retrieval.chunk_chars;
            thread::scope(|scope| -> Result<()> {
                let mut handles = Vec::new();
                for (path, bytes) in window {
                    seen.insert(path_text(path)?);
                    handles.push(scope.spawn(move || {
                        let result = self.indexing.file(root, path, || self.prepare_file_reserved(path, max_chars, verify, bytes));
                        self.prepared_or_issue(root, path, result)
                    }));
                }
                for handle in handles {
                    let prepared = handle.join().expect("index worker panicked")?;
                    self.indexing.checkpoint()?;
                    if let Some(file) = prepared {
                        let mut db = self.connection();
                        let tx = db.transaction()?;
                        Self::write_prepared(&tx, root, file)?;
                        tx.commit()?;
                    }
                    count += 1;
                    if count % 20 == 0 { progress(IndexProgress { count, total }); }
                }
                Ok(())
            })?;
        }
        let indexed = {
            let c = self.connection();
            let directory = path_text(directory)?;
            let prefix = format!("{}{}", directory.trim_end_matches(std::path::MAIN_SEPARATOR), std::path::MAIN_SEPARATOR);
            let converted_directory = crate::docling::output_path(Path::new(root), &Path::new(&directory).join(".scope"))?.parent().unwrap().to_path_buf();
            let converted_prefix = format!("{}{}", converted_directory.display(), std::path::MAIN_SEPARATOR);
            let mut q = c.prepare("SELECT path FROM documents WHERE root=?1 AND (path=?2 OR instr(path,?3)=1 OR instr(path,?4)=1) UNION SELECT path FROM index_issues WHERE root=?1 AND (path=?2 OR instr(path,?3)=1 OR instr(path,?4)=1)")?;
            let rows = q
                .query_map(params![root,directory,prefix,converted_prefix], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        for p in indexed {
            self.indexing.checkpoint()?;
            if converted_only.is_some_and(|converted| converted != crate::docling::in_output(Path::new(&p))) { continue; }
            // An issue recorded for a directory the walk could not enter stays until the
            // directory becomes readable again or disappears.
            if Path::new(&p).is_dir() && fs::read_dir(&p).is_err() { continue; }
            if !seen.contains(&p) {
                self.remove_indexed(&p)?
            }
        }
        if report { progress(IndexProgress { count, total }); }
        Ok(count)
    }
    pub fn index_file(&self, root: &str, path: &Path, max_chars: usize) -> Result<bool> {
        let mut ignore = ignore_rules::matcher(Path::new(root));
        let mut rules = crate::folder_index::Rules::new(Path::new(root), &self.settings()?)?;
        if ignore_rules::is_ignored(&mut ignore, path, false)? || text::in_hidden_directory(Path::new(root), path, false)
            || !Self::indexable_path(path, rules.file(path)?) {
            self.remove_indexed(&path_text(path)?)?;
            return Ok(false);
        }
        let Some(file) = self.prepare_file(path, max_chars, false)? else {
            return Ok(false);
        };
        let mut db = self.connection();
        let tx = db.transaction()?;
        let changed = Self::write_prepared(&tx, root, file)?;
        tx.commit()?;
        Ok(changed)
    }
    fn prepare_file(&self, path: &Path, max_chars: usize, verify: bool) -> Result<Option<PreparedFile>> {
        let reserved = preparation_reservation(path);
        let permit = self.text_preparation.reserve(reserved, || self.indexing.stopped())?;
        let mut prepared = self.prepare_file_reserved(path, max_chars, verify, reserved)?;
        if let Some(file) = &mut prepared { file._permit = Some(permit); }
        Ok(prepared)
    }
    fn prepare_file_reserved(&self, path: &Path, max_chars: usize, verify: bool, reserved: usize) -> Result<Option<PreparedFile>> {
        self.prepare_file_at(path, path, max_chars, verify, reserved)
    }
    fn prepare_file_at(&self, path: &Path, read_path: &Path, max_chars: usize, verify: bool, reserved: usize) -> Result<Option<PreparedFile>> {
        let path_string = path_text(path)?;
        let metadata = fs::metadata(read_path)?;
        let mtime = metadata.modified()?.duration_since(UNIX_EPOCH)?.as_nanos() as i64;
        let size = metadata.len() as i64;
        let previous: Option<(i64, i64, String, bool)> = self.connection().query_row(
            "SELECT mtime,size,hash,EXISTS(SELECT 1 FROM index_issues WHERE path=documents.path) FROM documents WHERE path=?",
            [&path_string], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).optional()?;
        let unchanged = previous.as_ref().is_some_and(|(modified, length, hash, issue)|
            *modified == mtime && *length == size && !hash.is_empty() && !issue);
        if path == read_path && unchanged && !verify {
            return Ok(None);
        }
        if metadata.len() > MAX_TEXT_INPUT_BYTES as u64 {
            return Err(text::ResourceLimit("原生文本文件不得超过 16 MiB".into()).into());
        }
        let admitted_input = (reserved.saturating_sub(16 * 1024 * 1024) / 16).min(MAX_TEXT_INPUT_BYTES);
        // A growing file must not turn a small reservation into a large allocation.
        // The largest reservation is capped at the full budget, so keep its explicit input cap.
        let admitted_input = if reserved == TEXT_PREPARATION_BUDGET { MAX_TEXT_INPUT_BYTES } else { admitted_input };
        if metadata.len() > admitted_input as u64 {
            return Err(text::ResourceLimit("文件在准备期间增大，请重新索引以重新分配预算".into()).into());
        }
        // Bound the read too: the file may grow after metadata was checked.
        let mut bytes = Vec::with_capacity(size as usize);
        fs::File::open(read_path)?.take(admitted_input as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > admitted_input {
            return Err(text::ResourceLimit("文件在准备期间增大，请重新索引以重新分配预算".into()).into());
        }
        let fingerprint = hash(&bytes);
        if path == read_path && previous.as_ref().is_some_and(|(_, _, hash, _)| hash == &fingerprint) {
            let mut db = self.connection();
            let tx = db.transaction()?;
            tx.execute(
                "UPDATE documents SET mtime=?,size=? WHERE path=? AND hash=?",
                params![mtime, size, path_string, fingerprint])?;
            tx.execute("DELETE FROM index_issues WHERE path=?", [&path_string])?;
            tx.commit()?;
            return Ok(None);
        }
        let body = body(path, &bytes)?;
        if body.len() > MAX_DECODED_TEXT_BYTES {
            return Err(text::ResourceLimit("解码后正文不得超过 32 MiB".into()).into());
        }
        self.indexing.checkpoint()?;
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let title = text::index_text(&name);
        let prepared_limit = MAX_PREPARED_TEXT_BYTES.min(reserved / 2);
        let source_chunks = text::chunks_for_index(&body, max_chars, path, prepared_limit / 2)?;
        let mut used = 0usize;
        let mut chunks = Vec::with_capacity(source_chunks.len());
        for chunk in source_chunks {
            self.indexing.checkpoint()?;
            let prepared = PreparedChunk {
                heading: text::index_text(&chunk.heading),
                content: text::index_text(&chunk.content),
                chunk,
            };
            used += prepared.heading.capacity() + prepared.content.capacity() + prepared.chunk.heading.capacity()
                + prepared.chunk.content.capacity() + std::mem::size_of::<PreparedChunk>() * 2
                + prepared.chunk.spans.capacity() * std::mem::size_of::<SourceSpan>()
                + prepared.chunk.ranges.capacity() * std::mem::size_of::<(usize, usize)>();
            if used > prepared_limit {
                return Err(text::ResourceLimit("分块和分词结果超过单文件准备预算".into()).into());
            }
            chunks.push(prepared);
        }
        let mut vocabulary = crate::sag2_v10::VocabularyBuilder::new(prepared_limit / 2);
        vocabulary.extend(&text::indexable_text(&body, path))?;
        for chunk in &chunks {
            self.indexing.checkpoint()?;
            vocabulary.extend(&chunk.chunk.content)?;
        }
        let retrieval_terms = vocabulary.finish();
        let explore_terms = text::explore_summary_bounded(path, chunks.iter().map(|chunk| (chunk.chunk.heading.as_str(), chunk.chunk.content.as_str())), prepared_limit / 2)?;
        Ok(Some(PreparedFile {
            _permit: None,
            path: path_string,
            name,
            fingerprint,
            mtime,
            size,
            title,
            chunks,
            explore_terms,
            retrieval_terms,
        }))
    }
    fn prepare_indexed_file(&self, root: &str, path: &Path, max_chars: usize, verify: bool) -> Result<Option<PreparedFile>> {
        self.prepared_or_issue(root, path, self.prepare_file(path, max_chars, verify))
    }
    fn prepared_or_issue(&self, root: &str, path: &Path, result: Result<Option<PreparedFile>>) -> Result<Option<PreparedFile>> {
        match result {
            Ok(file) => Ok(file),
            Err(error) if error.is::<std::io::Error>() || error.is::<crate::document_text::DecodeError>() || error.is::<text::ResourceLimit>() => {
                let path = path_text(path)?;
                let reason = error.to_string();
                let mut db = self.connection();
                let tx = db.transaction()?;
                if Self::path_excluded(&tx, &path)? { return Ok(None); }
                // A bad update must not erase the previous complete index. Hydration still
                // verifies the source hash, so stale evidence cannot be returned as current.
                tx.execute("INSERT INTO index_issues(path,root,error) VALUES(?,?,?) ON CONFLICT(path) DO UPDATE SET root=excluded.root,error=excluded.error", params![path,root,reason])?;
                tx.commit()?;
                Ok(None)
            }
            Err(error) => Err(error.context(format!("无法索引文件：{}", path.display()))),
        }
    }
    fn write_prepared(
        tx: &rusqlite::Transaction<'_>,
        root: &str,
        file: PreparedFile,
    ) -> Result<bool> {
        let registered: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM roots WHERE path=?)",
            [root],
            |r| r.get(0),
        )?;
        if !registered {
            bail!("索引任务已取消：文件夹已移除")
        }
        if Self::path_excluded(tx, &file.path)? { return Ok(false); }
        tx.execute("DELETE FROM index_issues WHERE path=?", [&file.path])?;
        let previous: Option<(i64, String)> = tx
            .query_row(
                "SELECT id,hash FROM documents WHERE path=?",
                [&file.path],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if previous
            .as_ref()
            .is_some_and(|(_, h)| h == &file.fingerprint)
        {
            tx.execute(
                "UPDATE documents SET mtime=?,size=? WHERE path=?",
                params![file.mtime, file.size, file.path],
            )?;
            return Ok(false);
        }
        let id = if let Some((id, _)) = previous {
            tx.execute(
                "DELETE FROM chunk_fts WHERE rowid IN (SELECT id FROM chunks WHERE document_id=?)",
                [id],
            )?;
            tx.execute("DELETE FROM chunks WHERE document_id=?", [id])?;
            tx.execute(
                "UPDATE documents SET root=?,hash=?,mtime=?,name=?,size=? WHERE id=?",
                params![root, file.fingerprint, file.mtime, file.name, file.size, id],
            )?;
            id
        } else {
            tx.execute(
                "INSERT INTO documents(root,path,name,hash,mtime,size) VALUES(?,?,?,?,?,?)",
                params![
                    root,
                    file.path,
                    file.name,
                    file.fingerprint,
                    file.mtime,
                    file.size
                ],
            )?;
            tx.last_insert_rowid()
        };
        tx.execute("INSERT INTO retrieval_vocabulary(document_id,terms) VALUES(?1,?2) ON CONFLICT(document_id) DO UPDATE SET terms=excluded.terms",
            params![id, serde_json::to_string(&file.retrieval_terms)?])?;
        tx.execute("INSERT INTO explore_summaries(document_id,hash,algorithm,terms) VALUES(?1,?2,?3,?4) ON CONFLICT(document_id) DO UPDATE SET hash=excluded.hash,algorithm=excluded.algorithm,terms=excluded.terms",
            params![id, file.fingerprint, text::EXPLORE_SUMMARY_VERSION, serde_json::to_string(&file.explore_terms)?])?;
        for prepared in file.chunks {
            let chunk = prepared.chunk;
            let start = chunk.ranges.first().unwrap().0;
            let end = chunk.ranges.last().unwrap().1;
            tx.execute("INSERT INTO chunks(document_id,heading,ranges,start_line,end_line,spans) VALUES(?,?,?,?,?,?)",
                params![id,chunk.heading,serde_json::to_string(&chunk.ranges)?,start,end,serde_json::to_string(&chunk.spans)?])?;
            let chunk_id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO chunk_fts(rowid,title,heading,body) VALUES(?,?,?,?)",
                params![chunk_id, file.title, prepared.heading, prepared.content],
            )?;
        }
        tx.execute("UPDATE metadata SET version=version+1", [])?;
        Ok(true)
    }
    pub fn verify_root(&self, root: &str) -> Result<()> {
        self.connection()
            .execute("UPDATE documents SET mtime=0 WHERE root=?", [root])?;
        Ok(())
    }
    pub fn invalidate_index(&self) -> Result<()> {
        let mut c = self.connection();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM chunk_fts", [])?;
        tx.execute("DELETE FROM chunks", [])?;
        tx.execute("UPDATE documents SET hash=''", [])?;
        tx.execute("UPDATE metadata SET version=version+1", [])?;
        tx.commit()?;
        Ok(())
    }
    pub fn remove_indexed(&self, path: &str) -> Result<()> {
        let mut c = self.connection();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM chunk_fts WHERE rowid IN (SELECT c.id FROM chunks c JOIN documents d ON d.id=c.document_id WHERE d.path=?)",[path])?;
        let removed = tx.execute("DELETE FROM documents WHERE path=?", [path])?;
        tx.execute("DELETE FROM index_issues WHERE path=?", [path])?;
        if removed > 0 { tx.execute("UPDATE metadata SET version=version+1", [])?; }
        tx.commit()?;
        Ok(())
    }
    pub fn search_files(
        &self,
        query: &str,
        scope: &[String],
        limit: usize,
    ) -> Result<Vec<Candidate>> {
        Ok(self.search_files_page(query, scope, 1, limit)?.0)
    }
    pub fn search_files_page(
        &self,
        query: &str,
        scope: &[String],
        page: usize,
        page_size: usize,
    ) -> Result<(Vec<Candidate>, SearchPagination)> {
        let expression = text::match_query(query);
        if expression.is_empty() {
            return Ok((vec![], SearchPagination { total: 0, page: 1, page_size }));
        }
        let mut db = self.connection();
        let connection = db.transaction()?;
        let scope = serde_json::to_string(scope)?;
        let total: usize = connection.query_row("SELECT COUNT(DISTINCT c.document_id)
            FROM chunk_fts JOIN chunks c ON c.id=chunk_fts.rowid JOIN documents d ON d.id=c.document_id
            WHERE chunk_fts MATCH ?1 AND (json_array_length(?2)=0 OR EXISTS(
                SELECT 1 FROM json_each(?2) s WHERE (instr(replace(d.path,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1 OR EXISTS(SELECT 1 FROM conversions cv WHERE cv.output=d.path AND cv.valid=1 AND instr(replace(cv.source,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1))
            ))", params![expression, scope], |r| r.get(0))?;
        let page = page.min(total.div_ceil(page_size).max(1));
        let mut statement=connection.prepare("WITH matched AS MATERIALIZED (
            SELECT c.id,c.document_id,bm25(chunk_fts,2,2,1) AS score
            FROM chunk_fts JOIN chunks c ON c.id=chunk_fts.rowid JOIN documents d ON d.id=c.document_id
            WHERE chunk_fts MATCH ?1 AND (json_array_length(?2)=0 OR EXISTS(
                SELECT 1 FROM json_each(?2) s WHERE (instr(replace(d.path,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1 OR EXISTS(SELECT 1 FROM conversions cv WHERE cv.output=d.path AND cv.valid=1 AND instr(replace(cv.source,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1))
            ))
        ), ranked AS (
            SELECT id,score,row_number() OVER (PARTITION BY document_id ORDER BY score,id) AS position FROM matched
        ), top_hits AS MATERIALIZED (
            SELECT id,score FROM ranked WHERE position=1 ORDER BY score,id LIMIT ?3 OFFSET ?4
        )
        SELECT c.id,d.id,d.path,d.name,c.heading,c.start_line,c.end_line,c.ranges,r.score,d.hash,c.spans
        FROM top_hits r JOIN chunks c ON c.id=r.id JOIN documents d ON d.id=c.document_id
        ORDER BY r.score,c.id")?;
        let mut rows = statement.query(params![
            expression,
            scope,
            page_size as i64,
            ((page - 1) * page_size) as i64
        ])?;
        let mut results = vec![];
        let mut documents = HashMap::new();
        while let Some(row) = rows.next()? {
            if let Some(candidate) = candidate_from_row(row, results.len(), &mut documents)? {
                results.push(candidate);
            }
        }
        Ok((results, SearchPagination { total, page, page_size }))
    }
    pub fn candidate_query(
        &self,
        expression: &str,
        required: Option<&str>,
        scope: &[String],
        references: &[String],
        excluded: &[i64],
        limit: usize,
    ) -> Result<Vec<Candidate>> {
        let mut excluded = excluded.to_vec();
        excluded.sort_unstable();
        self.candidate_query_on(0, CandidateQuery {
            expression: expression.to_owned(),
            required: required.map(str::to_owned),
            scope: scope.to_vec(),
            references: references.to_vec(),
            excluded,
            limit,
        })
    }

    pub fn candidate_queries(
        &self,
        expression: &str,
        required: &[String],
        scope: &[String],
        references: &[String],
        excluded: &[i64],
        limit: usize,
    ) -> Result<Vec<Vec<Candidate>>> {
        let mut excluded = excluded.to_vec();
        excluded.sort_unstable();
        let workers = required.len().min(QUERY_WORKERS);
        let batches = thread::scope(|threads| {
            let handles: Vec<_> = (0..workers).map(|worker| {
                let excluded = &excluded;
                threads.spawn(move || -> Result<Vec<_>> {
                    (worker..required.len()).step_by(workers).map(|position| {
                        let rows = self.candidate_query_on(worker, CandidateQuery {
                            expression: expression.to_owned(),
                            required: Some(required[position].clone()),
                            scope: scope.to_vec(),
                            references: references.to_vec(),
                            excluded: excluded.clone(),
                            limit,
                        })?;
                        Ok((position, rows))
                    }).collect()
                })
            }).collect();
            handles.into_iter().map(|handle| {
                handle.join().expect("BM25 query worker panicked")
            }).collect::<Result<Vec<_>>>()
        })?;
        let mut results = vec![vec![]; required.len()];
        for batch in batches {
            for (position, rows) in batch {
                results[position] = rows;
            }
        }
        Ok(results)
    }

    fn candidate_query_on(&self, reader: usize, key: CandidateQuery) -> Result<Vec<Candidate>> {
        let mut connection = self.query_readers[reader].lock().expect("query reader mutex poisoned");
        let transaction = connection.transaction()?;
        // The version and candidates come from the same SQLite read snapshot.
        let version = transaction.query_row("SELECT version FROM metadata WHERE id=1", [], |row| row.get(0))?;
        let cached = self.query_cache.lock().expect("query cache mutex poisoned").get(version, &key);
        let metadata = if let Some(rows) = cached {
            rows
        } else {
            let stats = self.rank_statistics(&transaction, version, &key.expression)?;
            let rows = if stats.is_empty() { vec![] } else {
                let documents = self.scope_document_ids(&transaction, version, &key.scope, &key.references)?;
                Self::query_candidate_metadata(&transaction, &key, &stats, documents.as_deref())?
            };
            self.query_cache.lock().expect("query cache mutex poisoned").insert(version, key, rows.clone());
            rows
        };
        transaction.commit()?;
        drop(connection);
        let mut documents = HashMap::new();
        let mut hydrated = Vec::with_capacity(metadata.len());
        for candidate in metadata {
            if let Some(candidate) = hydrate_candidate(candidate, &mut documents)? {
                hydrated.push(candidate);
            }
        }
        Ok(hydrated)
    }

    fn rank_statistics(&self, connection: &Connection, version: i64, expression: &str) -> Result<Vec<u8>> {
        let mut cache = self.query_cache.lock().expect("query cache mutex poisoned");
        if cache.version == Some(version) {
            if let Some(stats) = cache.rank_stats.get(expression) { return Ok(stats.clone()); }
        }
        let stats = connection.query_row(
            "SELECT sag_stats(chunk_fts) FROM chunk_fts WHERE chunk_fts MATCH ? LIMIT 1",
            [expression], |row| row.get::<_, Vec<u8>>(0),
        ).optional()?.unwrap_or_default();
        if cache.version == Some(version) {
            cache.rank_stats.insert(expression.to_owned(), stats.clone());
            if cache.rank_stats.len() > QUERY_CACHE_ENTRIES { cache.rank_stats.shift_remove_index(0); }
        }
        Ok(stats)
    }

    fn scope_document_ids(&self, connection: &Connection, version: i64, scope: &[String], references: &[String]) -> Result<Option<Arc<str>>> {
        if scope.is_empty() { return Ok(None); }
        let key = CorpusScope { scope: scope.to_vec(), references: references.to_vec() };
        {
            let mut cache = self.query_cache.lock().expect("query cache mutex poisoned");
            if cache.refresh(version) {
                if let Some(documents) = cache.scope_documents.shift_remove(&key) {
                    cache.scope_documents.insert(key, documents.clone());
                    return Ok(documents);
                }
            }
        }
        let mut query = connection.prepare("SELECT d.id FROM documents d
            WHERE (d.path IN (SELECT value FROM json_each(?2)) OR d.path IN (SELECT output FROM conversions WHERE valid=1 AND source IN (SELECT value FROM json_each(?2)))) OR EXISTS(
                SELECT 1 FROM json_each(?1) s
                WHERE (instr(replace(d.path,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1 OR EXISTS(SELECT 1 FROM conversions cv WHERE cv.output=d.path AND cv.valid=1 AND instr(replace(cv.source,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1))
            ) ORDER BY d.id")?;
        let ids = query.query_map(params![serde_json::to_string(scope)?, serde_json::to_string(references)?], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let total: usize = connection.query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))?;
        // A scope covering every indexed document uses the same query as all files.
        let documents = if ids.len() == total { None } else { Some(Arc::<str>::from(serde_json::to_string(&ids)?)) };
        let mut cache = self.query_cache.lock().expect("query cache mutex poisoned");
        if cache.version == Some(version) {
            cache.scope_documents.insert(key, documents.clone());
            if cache.scope_documents.len() > CORPUS_CACHE_ENTRIES { cache.scope_documents.shift_remove_index(0); }
        }
        Ok(documents)
    }

    fn query_candidate_metadata(c: &Connection, key: &CandidateQuery, stats: &[u8], documents: Option<&str>) -> Result<Vec<Candidate>> {
        let sql = "WITH top_hits AS MATERIALIZED (
                SELECT rowid,sag_bm25(chunk_fts,?5,2,2,1) AS score FROM chunk_fts
                WHERE chunk_fts MATCH ?1
                AND rowid NOT IN (SELECT value FROM json_each(?2))
                AND (?3 IS NULL OR +rowid IN (
                    SELECT id FROM chunks WHERE document_id IN (SELECT value FROM json_each(?3))
                ))
                ORDER BY score,rowid LIMIT ?4
            )
            SELECT c.id,d.id,d.path,d.name,c.heading,c.start_line,c.end_line,c.ranges,h.score,d.hash,c.spans
            FROM top_hits h JOIN chunks c ON c.id=h.rowid JOIN documents d ON d.id=c.document_id
            ORDER BY h.score,c.id";
        let expression = key.required.as_ref().map_or_else(|| key.expression.clone(), |required| format!("({}) AND ({required})", key.expression));
        let mut values: Vec<rusqlite::types::Value> = vec![
            expression.into(),
            serde_json::to_string(&key.excluded)?.into(),
            documents.map(str::to_owned).into(),
            (key.limit as i64).into(),
            stats.to_vec().into(),
        ];
        let read = |sql: &str, values: &[rusqlite::types::Value]| -> Result<Vec<Candidate>> {
            let mut query = c.prepare(sql)?;
            let mut rows = query.query(rusqlite::params_from_iter(values))?;
            let mut result = vec![];
            while let Some(row) = rows.next()? { result.push(candidate_metadata(row, result.len())?); }
            Ok(result)
        };
        if key.required.is_none() && key.limit >= 100 {
            static PHRASES: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r#""(?:""|[^"])*""#).unwrap());
            let phrases: Vec<_> = PHRASES.find_iter(&key.expression).map(|m| m.as_str()).collect();
            let idfs: Vec<_> = stats.chunks_exact(8).skip(1).map(|bytes| f64::from_ne_bytes(bytes.try_into().unwrap())).collect();
            if phrases.len() >= 8 && phrases.len() == idfs.len() {
                let mut selected: Vec<_> = (0..idfs.len()).collect();
                selected.sort_by(|&a, &b| idfs[b].total_cmp(&idfs[a]).then(a.cmp(&b)));
                selected.truncate((idfs.len() as f64 * 0.55).ceil() as usize);
                let subset = selected.iter().map(|&i| phrases[i]).collect::<Vec<_>>().join(" OR ");
                let bound = 2.2 * idfs.iter().enumerate().filter(|(i, _)| !selected.contains(i)).map(|(_, idf)| idf).sum::<f64>();
                let restricted = sql.replace("ORDER BY score,rowid LIMIT ?4", "AND +rowid IN (SELECT rowid FROM chunk_fts WHERE chunk_fts MATCH ?6) ORDER BY score,rowid LIMIT ?4");
                values.push(subset.into());
                let possible = read(&restricted, &values)?;
                values.pop();
                if possible.len() == key.limit && -possible.last().unwrap().bm25 > bound { return Ok(possible); }
            }
        }
        read(sql, &values)
    }

    pub fn has_candidate_match(&self, expression: &str, scope: &[String], references: &[String], excluded: &[i64]) -> Result<bool> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let version = transaction.query_row("SELECT version FROM metadata WHERE id=1", [], |row| row.get(0))?;
        let documents = self.scope_document_ids(&transaction, version, scope, references)?;
        let found = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM chunk_fts WHERE chunk_fts MATCH ?1
                AND rowid NOT IN (SELECT value FROM json_each(?2))
                AND (?3 IS NULL OR +rowid IN (
                    SELECT id FROM chunks WHERE document_id IN (SELECT value FROM json_each(?3))
                )) LIMIT 1)",
            params![expression, serde_json::to_string(excluded)?, documents.as_deref()],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        Ok(found)
    }
    pub fn reference_candidates(
        &self,
        references: &[String],
        limit: usize,
    ) -> Result<Vec<Candidate>> {
        let c = self.connection();
        let mut q=c.prepare("SELECT c.id,d.id,d.path,d.name,c.heading,c.start_line,c.end_line,c.ranges,0.0,d.hash,c.spans FROM chunks c JOIN documents d ON d.id=c.document_id WHERE (d.path IN (SELECT value FROM json_each(?1)) OR d.path IN (SELECT output FROM conversions WHERE valid=1 AND source IN (SELECT value FROM json_each(?1)))) ORDER BY c.id LIMIT ?2")?;
        let mut rows = q.query(params![serde_json::to_string(references)?, limit as i64])?;
        let mut result = vec![];
        let mut documents = HashMap::new();
        while let Some(row) = rows.next()? {
            if let Some(candidate) = candidate_from_row(row, result.len(), &mut documents)? {
                result.push(candidate);
            }
        }
        Ok(result)
    }
    pub(crate) fn retrieval_statistics(
        &self,
        scope: &[String],
        references: &[String],
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<Arc<crate::sag2_v10::Corpus>> {
        let version = self.version()?;
        let key = CorpusScope { scope: scope.to_vec(), references: references.to_vec() };
        {
            let mut cache = self.query_cache.lock().expect("query cache mutex poisoned");
            if cache.refresh(version) {
                if let Some(corpus) = cache.corpora.shift_remove(&key) {
                    cache.corpora.insert(key, corpus.clone());
                    return Ok(corpus);
                }
            }
        }
        let rows = {
            let db = self.connection();
            let mut query = db.prepare("SELECT d.id,d.name,d.path,d.hash,v.terms,CASE WHEN v.document_id IS NULL THEN (SELECT json_group_array(json(c.spans)) FROM chunks c WHERE c.document_id=d.id) END FROM documents d LEFT JOIN retrieval_vocabulary v ON v.document_id=d.id WHERE EXISTS(SELECT 1 FROM chunks c WHERE c.document_id=d.id) AND (json_array_length(?1)=0 OR (d.path IN (SELECT value FROM json_each(?2)) OR d.path IN (SELECT output FROM conversions WHERE valid=1 AND source IN (SELECT value FROM json_each(?2)))) OR EXISTS(SELECT 1 FROM json_each(?1) s WHERE (instr(replace(d.path,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1 OR EXISTS(SELECT 1 FROM conversions cv WHERE cv.output=d.path AND cv.valid=1 AND instr(replace(cv.source,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1)))) ORDER BY d.id")?;
            let rows = query.query_map(params![serde_json::to_string(scope)?, serde_json::to_string(references)?], |r| {
                Ok((r.get::<_, i64>(0)?,r.get::<_, String>(1)?,r.get::<_, String>(2)?,r.get::<_, String>(3)?,r.get::<_, Option<String>>(4)?,r.get::<_, Option<String>>(5)?))
            })?.collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        let mut documents = Vec::with_capacity(rows.len());
        let mut missing = Vec::new();
        for (id, title, path, expected, saved, spans) in rows {
            if cancel.is_cancelled() { bail!("已停止") }
            let terms = if let Some(saved) = saved {
                serde_json::from_str(&saved)?
            } else {
                // Upgrade an existing index once; new and changed files save this during indexing.
                // A file that is unreadable or changed since indexing is left out of the statistics
                // instead of failing the query; the next index update restores it.
                let Some(source) = read_indexed_source(&path, &expected) else { continue };
                let mut terms = crate::sag2_v10::vocabulary(&text::indexable_text(&source, Path::new(&path)));
                let passages: Vec<Vec<SourceSpan>> = serde_json::from_str(&spans.unwrap())?;
                for spans in passages {
                    terms.extend(crate::sag2_v10::vocabulary(&text::source_content(&source, &spans)));
                }
                missing.push((id, serde_json::to_string(&terms)?));
                terms
            };
            documents.push((title, terms));
        }
        if cancel.is_cancelled() { bail!("已停止") }
        let corpus = Arc::new(crate::sag2_v10::Corpus::from_vocabulary(documents)?);
        {
            let mut db = self.connection();
            let tx = db.transaction()?;
            let current: i64 = tx.query_row("SELECT version FROM metadata WHERE id=1", [], |r| r.get(0))?;
            if current != version { bail!("索引已更新，请重新查询") }
            for (id, terms) in missing {
                tx.execute("INSERT INTO retrieval_vocabulary(document_id,terms) VALUES(?,?) ON CONFLICT(document_id) DO UPDATE SET terms=excluded.terms", params![id, terms])?;
            }
            tx.commit()?;
        }
        let mut cache = self.query_cache.lock().expect("query cache mutex poisoned");
        if cache.refresh(version) {
            cache.corpora.insert(key, corpus.clone());
            if cache.corpora.len() > CORPUS_CACHE_ENTRIES { cache.corpora.shift_remove_index(0); }
        }
        Ok(corpus)
    }

    pub fn retrieval_corpus(&self, scope: &[String], references: &[String]) -> Result<Vec<(String, String)>> {
        let rows: Vec<(String, String, String, String)> = {
            let db = self.connection();
            let mut query = db.prepare("SELECT d.name,d.path,d.hash,(SELECT json_group_array(json(c.spans)) FROM chunks c WHERE c.document_id=d.id) FROM documents d WHERE EXISTS(SELECT 1 FROM chunks c WHERE c.document_id=d.id) AND (json_array_length(?1)=0 OR (d.path IN (SELECT value FROM json_each(?2)) OR d.path IN (SELECT output FROM conversions WHERE valid=1 AND source IN (SELECT value FROM json_each(?2)))) OR EXISTS(SELECT 1 FROM json_each(?1) s WHERE (instr(replace(d.path,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1 OR EXISTS(SELECT 1 FROM conversions cv WHERE cv.output=d.path AND cv.valid=1 AND instr(replace(cv.source,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1)))) ORDER BY d.id")?;
            let values = query.query_map(params![serde_json::to_string(scope)?, serde_json::to_string(references)?], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
            values
        };
        rows.into_iter().filter_map(|(title,path,expected,spans)| {
            let source = read_indexed_source(&path, &expected)?;
            Some((|| -> Result<(String, String)> {
            let passages: Vec<Vec<SourceSpan>> = serde_json::from_str(&spans)?;
            let mut vocabulary = text::indexable_text(&source, Path::new(&path)).into_owned();
            // Count each document once, including the exact words in its indexed passages.
            // Partial source words can differ from whole-document words in saved indexes.
            for spans in passages {
                vocabulary.push('\n');
                vocabulary.push_str(&text::source_content(&source, &spans));
            }
            Ok((title, vocabulary))
            })())
        }).collect()
    }

    pub fn frequencies(&self, terms: &[String]) -> Result<HashMap<String, usize>> {
        let workers = terms.len().min(QUERY_WORKERS);
        let batches = thread::scope(|threads| {
            let handles: Vec<_> = (0..workers).map(|worker| threads.spawn(move || -> Result<Vec<(String, usize)>> {
                let mut connection = self.query_readers[worker].lock().expect("query reader mutex poisoned");
                let transaction = connection.transaction()?;
                let version: i64 = transaction.query_row("SELECT version FROM metadata WHERE id=1", [], |row| row.get(0))?;
                let mut results = vec![];
                for term in terms.iter().skip(worker).step_by(workers) {
                    let cached = {
                        let mut cache = self.query_cache.lock().expect("query cache mutex poisoned");
                        if cache.refresh(version) { cache.frequencies.get(term).copied() } else { None }
                    };
                    let count = match cached {
                        Some(count) => count,
                        None => {
                            let count = transaction.query_row("SELECT COUNT(*) FROM chunk_fts WHERE chunk_fts MATCH ?", [format!("body : {}", text::quote(term))], |row| row.get::<_, i64>(0))? as usize;
                            let mut cache = self.query_cache.lock().expect("query cache mutex poisoned");
                            if cache.version == Some(version) { cache.frequencies.insert(term.clone(), count); }
                            count
                        }
                    };
                    results.push((term.clone(), count));
                }
                transaction.commit()?;
                Ok(results)
            })).collect();
            handles.into_iter().map(|handle| handle.join().expect("frequency worker panicked")).collect::<Result<Vec<_>>>()
        })?;
        Ok(batches.into_iter().flatten().collect())
    }
    pub fn frequency(&self, term: &str) -> Result<usize> {
        Ok(self.frequencies(&[term.to_owned()])?[term])
    }
    pub fn chunk_count(&self) -> Result<usize> {
        Ok(self
            .connection()
            .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get::<_, i64>(0))?
            as usize)
    }
    pub fn conversations(&self) -> Result<Vec<Conversation>> {
        let c = self.connection();
        let mut q=c.prepare("SELECT id,title,messages,updated,deleted FROM conversations WHERE deleted=0 ORDER BY updated DESC")?;
        let mut rows = q.query([])?;
        let mut out = vec![];
        while let Some(r) = rows.next()? {
            out.push(Conversation {
                id: r.get(0)?,
                title: r.get(1)?,
                messages: serde_json::from_str(&r.get::<_, String>(2)?)?,
                updated: r.get(3)?,
                deleted: r.get(4)?,
            });
        }
        Ok(out)
    }
    pub fn put_conversation(&self, c: &Conversation) -> Result<()> {
        self.connection().execute("INSERT INTO conversations VALUES(?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET messages=excluded.messages,updated=excluded.updated",params![c.id,c.title,serde_json::to_string(&c.messages)?,c.updated,c.deleted])?;
        Ok(())
    }
    pub fn clear_conversations(&self) -> Result<()> {
        let connection = self.connection();
        connection.execute_batch("PRAGMA secure_delete=ON; DELETE FROM conversations;")?;
        let busy: i64 = connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        if busy != 0 {
            bail!("对话已删除，但数据库日志仍在使用，请稍后再次清空以完成清理")
        }
        Ok(())
    }
    pub fn edit_conversation(
        &self,
        id: &str,
        title: Option<&str>,
        deleted: Option<bool>,
    ) -> Result<()> {
        let c = self.connection();
        if let Some(t) = title {
            if t.trim().is_empty() {
                bail!("名称不能为空")
            };
            c.execute(
                "UPDATE conversations SET title=? WHERE id=?",
                params![t.trim(), id],
            )?;
        }
        if let Some(d) = deleted {
            c.execute(
                "UPDATE conversations SET deleted=? WHERE id=?",
                params![d, id],
            )?;
        }
        Ok(())
    }
    pub fn replace_messages(&self, id: &str, messages: Vec<Message>) -> Result<Conversation> {
        let mut conversation = self
            .conversations()?
            .into_iter()
            .find(|c| c.id == id)
            .context("对话不存在")?;
        conversation.messages = messages;
        conversation.updated = now();
        self.put_conversation(&conversation)?;
        Ok(conversation)
    }
    pub fn put_draft(&self, draft: &Draft) -> Result<()> {
        self.connection().execute(
            "INSERT INTO drafts VALUES(?,?) ON CONFLICT(path) DO UPDATE SET value=excluded.value",
            params![draft.path, serde_json::to_string(draft)?],
        )?;
        Ok(())
    }
    pub fn drafts(&self) -> Result<Vec<Draft>> {
        let c = self.connection();
        let mut q = c.prepare("SELECT value FROM drafts")?;
        let values = q
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        values
            .into_iter()
            .map(|s| Ok(serde_json::from_str(&s)?))
            .collect()
    }
    pub fn delete_draft(&self, path: &str) -> Result<()> {
        self.connection()
            .execute("DELETE FROM drafts WHERE path=?", [path])?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, Store, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let notes = dir.path().join("notes");
        fs::create_dir(&notes).unwrap();
        let db = Store::open(&dir.path().join("app/db.sqlite")).unwrap();
        db.add_root(&notes).unwrap();
        let notes = notes.canonicalize().unwrap();
        (dir, db, notes)
    }
    #[cfg(unix)]
    fn converted_text_fixture() -> (tempfile::TempDir, Store, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let (dir, store, root) = setup();
        let home = dir.path().join("app/docling");
        fs::create_dir_all(home.join("bin")).unwrap();
        let binary = home.join("bin/docling");
        fs::write(&binary, r#"#!/bin/sh
set -eu
source="$1"
shift
output=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = '--output-file' ]; then shift; output="$1"; fi
  shift
done
cat "$source" > "$output"
"#).unwrap();
        fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(home.join("ready"), "2.140.0").unwrap();
        let mut settings = store.settings().unwrap();
        settings.convert_other_formats = true;
        store.save_settings(&settings).unwrap();
        (dir, store, root)
    }

    #[cfg(unix)]
    #[test]
    fn converted_text_resource_failure_preserves_readable_previous_markdown_and_index() {
        let (_dir, store, root) = converted_text_fixture();
        fs::create_dir(root.join("nested")).unwrap();
        let source = root.join("nested/report.docx");
        fs::write(&source, "previousconvertedevidence.").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let output = crate::docling::output_path(&root, &source).unwrap();
        let previous = fs::read(&output).unwrap();
        assert!(std::str::from_utf8(&previous).unwrap().contains("../../nested/report.docx"));
        let previous_hash = hash(&previous);
        fs::write(&source, "unbrokenword ".repeat(25_000)).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(store.roots().unwrap()[0].status, "partial");
        assert!(fs::read(&output).unwrap() == previous, "a rejected replacement must not overwrite the source of the usable index");
        let saved: String = store.connection().query_row("SELECT hash FROM documents WHERE path=?", [output.to_str().unwrap()], |row| row.get(0)).unwrap();
        assert_eq!(saved, previous_hash);
        let candidates = store.search_files("previousconvertedevidence", &[source.to_str().unwrap().to_owned()], 10).unwrap();
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].content.contains("previousconvertedevidence"));
        assert_eq!(candidates[0].path, output.to_str().unwrap());
        assert_eq!(store.document(&source).unwrap().body.as_bytes(), previous);
        assert_eq!(*store.text_preparation.used.lock().unwrap(), 0);
        let names: Vec<_> = fs::read_dir(output.parent().unwrap()).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["report.docx.md"]);
    }

    #[cfg(unix)]
    #[test]
    fn converted_text_database_failure_rolls_back_markdown_and_index_together() {
        let (_dir, store, root) = converted_text_fixture();
        let source = root.join("report.docx");
        fs::write(&source, "previousconvertedevidence.").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let output = crate::docling::output_path(&root, &source).unwrap();
        let previous = fs::read(&output).unwrap();
        let previous_hash = hash(&previous);
        fs::write(&source, "replacementconvertedevidence.").unwrap();
        store.connection().execute_batch("CREATE TRIGGER reject_converted BEFORE INSERT ON chunks BEGIN SELECT RAISE(ABORT,'controlled converted write failure'); END;").unwrap();
        assert!(store.scan(root.to_str().unwrap(), |_| {}).is_err());
        assert_eq!(fs::read(&output).unwrap(), previous);
        let saved: String = store.connection().query_row("SELECT hash FROM documents WHERE path=?", [output.to_str().unwrap()], |row| row.get(0)).unwrap();
        assert_eq!(saved, previous_hash);
        assert_eq!(store.search_files("previousconvertedevidence", &[], 10).unwrap().len(), 1);
        assert_eq!(*store.text_preparation.used.lock().unwrap(), 0);
        store.connection().execute_batch("DROP TRIGGER reject_converted;").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(store.search_files("replacementconvertedevidence", &[], 10).unwrap().len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn converted_text_commit_failure_restores_the_published_markdown() {
        let (_dir, store, root) = converted_text_fixture();
        let source = root.join("report.docx");
        fs::write(&source, "previousconvertedevidence.").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let output = crate::docling::output_path(&root, &source).unwrap();
        let previous = fs::read(&output).unwrap();
        let previous_hash = hash(&previous);
        fs::write(&source, "replacementconvertedevidence.").unwrap();
        store.connection().execute_batch("CREATE TABLE deferred_gate(value TEXT REFERENCES roots(path) DEFERRABLE INITIALLY DEFERRED);
            CREATE TRIGGER reject_conversion_commit AFTER UPDATE ON conversions BEGIN INSERT INTO deferred_gate VALUES('controlled-missing-root'); END;").unwrap();
        assert!(store.scan(root.to_str().unwrap(), |_| {}).is_err());
        assert!(fs::read(&output).unwrap() == previous);
        let saved: String = store.connection().query_row("SELECT hash FROM documents WHERE path=?", [output.to_str().unwrap()], |row| row.get(0)).unwrap();
        assert_eq!(saved, previous_hash);
        assert_eq!(store.search_files("previousconvertedevidence", &[source.to_str().unwrap().to_owned()], 10).unwrap().len(), 1);
        assert_eq!(*store.text_preparation.used.lock().unwrap(), 0);
        assert!(fs::read_dir(output.parent().unwrap()).unwrap().all(|entry| !entry.unwrap().file_name().to_string_lossy().starts_with(".sag-p")));
    }

    #[cfg(unix)]
    #[test]
    fn converted_text_cancel_during_paused_acceptance_removes_staging_and_keeps_old_markdown() {
        let (dir, store, root) = converted_text_fixture();
        let source = root.join("report.docx");
        fs::write(&source, "previousconvertedevidence.").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let output = crate::docling::output_path(&root, &source).unwrap();
        let previous = fs::read(&output).unwrap();
        let bin = dir.path().join("app/docling/bin");
        fs::write(bin.join("docling"), r#"#!/bin/sh
set -eu
source="$1"
shift
output=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = '--output-file' ]; then shift; output="$1"; fi
  shift
done
cat "$source" > "$output"
touch "$(dirname "$0")/started"
while [ ! -f "$(dirname "$0")/release" ]; do sleep 0.02; done
"#).unwrap();
        fs::write(&source, "replacementconvertedevidence.").unwrap();
        thread::scope(|scope| {
            let task = scope.spawn(|| store.scan(root.to_str().unwrap(), |_| {}));
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !bin.join("started").exists() {
                assert!(std::time::Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
            store.indexing.set_paused(true);
            fs::write(bin.join("release"), "").unwrap();
            loop {
                let staged = fs::read_dir(output.parent().unwrap()).unwrap().filter_map(|entry| {
                    let path = entry.ok()?.path();
                    path.file_name()?.to_string_lossy().starts_with(".sag-pending-").then_some(path)
                }).any(|path| fs::read_to_string(path).is_ok_and(|text| text.contains("replacementconvertedevidence")));
                if staged { break; }
                assert!(std::time::Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
            assert!(fs::read(&output).unwrap() == previous);
            assert_eq!(store.search_files("previousconvertedevidence", &[], 10).unwrap().len(), 1);
            store.indexing.stop();
            assert!(task.join().unwrap().is_err());
        });
        assert!(fs::read(&output).unwrap() == previous);
        assert_eq!(*store.text_preparation.used.lock().unwrap(), 0);
        assert!(fs::read_dir(output.parent().unwrap()).unwrap().all(|entry| !entry.unwrap().file_name().to_string_lossy().starts_with(".sag-p")));
    }

    #[test]
    fn converted_text_legacy_media_manifest_reuses_transcripts_without_a_media_runtime() {
        let (dir, store, root) = setup();
        let mut settings = store.settings().unwrap();
        settings.convert_other_formats = true;
        store.save_settings(&settings).unwrap();
        let mut records = std::collections::BTreeMap::new();
        for (relative, evidence) in [("recording.mp3", "audiocachedevidence."), ("recording.mp4", "videocachedevidence.")] {
            let source = root.join(relative);
            fs::write(&source, "old media original").unwrap();
            let output = crate::docling::output_path(&root, &source).unwrap();
            fs::create_dir_all(output.parent().unwrap()).unwrap();
            fs::write(&output, evidence).unwrap();
            records.insert(relative, (crate::docling::fingerprint(&source).unwrap(), crate::docling::fingerprint(&output).unwrap()));
        }
        let metadata = root.join(".sag-converted/.sag-converted.json");
        fs::write(&metadata, serde_json::to_vec(&records).unwrap()).unwrap();
        let root_text = root.to_str().unwrap();
        // Both first import and remove/re-add must reuse owned transcripts without a converter.
        for _ in 0..2 {
            store.scan(root_text, |_| {}).unwrap();
            assert_eq!(store.connection().query_row("SELECT COUNT(*) FROM conversions WHERE valid=1", [], |row| row.get::<_, usize>(0)).unwrap(), 2);
            let saved: std::collections::BTreeMap<String, (String, String)> = serde_json::from_slice(&fs::read(&metadata).unwrap()).unwrap();
            for (relative, evidence) in [("recording.mp3", "audiocachedevidence"), ("recording.mp4", "videocachedevidence")] {
                let source = root.join(relative);
                let output = crate::docling::output_path(&root, &source).unwrap();
                assert_eq!(fs::read_to_string(&source).unwrap(), "old media original");
                assert_eq!(fs::read_to_string(&output).unwrap(), format!("{evidence}."));
                assert_eq!(&saved[relative], &records[relative]);
                let candidates = store.search_files(evidence, &[source.to_string_lossy().into_owned()], 5).unwrap();
                assert_eq!(candidates.len(), 1);
                assert!(candidates[0].content.contains(evidence));
            }
            store.remove_root(root_text).unwrap();
            store.add_root(&root).unwrap();
        }
        assert!(!dir.path().join("app/docling").exists(), "scanning cached media must never install a component");
        // Media support must never bypass traversal or unknown-extension validation.
        for relative in ["../outside.mp3", "unsupported.xyz"] {
            let invalid = std::collections::BTreeMap::from([(relative, ("source", "output"))]);
            fs::write(&metadata, serde_json::to_vec(&invalid).unwrap()).unwrap();
            assert!(store.scan(root_text, |_| {}).is_err());
            assert_eq!(fs::read_to_string(root.join(".sag-converted/recording.mp3.md")).unwrap(), "audiocachedevidence.");
        }
    }

    #[test]
    fn removed_file_stays_excluded_and_can_be_restored_without_changing_local_data() {
        let (dir, store, root) = setup();
        let removed = root.join("removed.txt");
        let kept = root.join("kept.txt");
        fs::write(&removed, "sharedmarker removedcontent").unwrap();
        fs::write(&kept, "sharedmarker keptcontent").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let scoped = store.search_files("sharedmarker", &[path_text(&removed).unwrap()], 10).unwrap();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].path, path_text(&removed).unwrap());
        store.remove_path(&removed).unwrap();
        assert_eq!(fs::read_to_string(&removed).unwrap(), "sharedmarker removedcontent");
        assert!(store.document(&removed).is_err());
        assert!(store.entries(&root, true).unwrap().iter().any(|entry| entry.path == path_text(&removed).unwrap()));
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        store.update_paths(root.to_str().unwrap(), &[removed.clone()], |_| {}).unwrap();
        let matches = store.search_files("sharedmarker", &[], 10).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, path_text(&kept).unwrap());
        let reopened = Store::open(&dir.path().join("app/db.sqlite")).unwrap();
        assert!(reopened.is_excluded(&removed).unwrap());
        assert_eq!(reopened.add_root(&removed).unwrap(), path_text(&root).unwrap());
        reopened.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(reopened.search_files("sharedmarker", &[], 10).unwrap().len(), 2);
        assert_eq!(reopened.roots().unwrap().len(), 1);
        assert_eq!(fs::read_to_string(&removed).unwrap(), "sharedmarker removedcontent");
    }
    #[test]
    fn remove_all_roots_clears_indexes_and_preserves_files_history_and_settings() {
        let (_dir, store, root) = setup();
        let source = root.join("source.txt");
        fs::write(&source, "uniquetoken original document").unwrap();
        assert!(store.remove_all_roots().is_err());
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let expression = text::match_query("uniquetoken");
        assert_eq!(store.candidate_query(&expression, None, &[], &[], &[], 10).unwrap().len(), 1);
        store.put_conversation(&Conversation {id:"kept".into(), title:"history".into(), messages:vec![], updated:now(), deleted:false}).unwrap();
        store.put_draft(&Draft {path:source.to_str().unwrap().into(), name:"source.txt".into(), body:"unsaved draft".into(), hash:"original".into(), is_new:false}).unwrap();
        let settings = serde_json::to_value(store.settings().unwrap()).unwrap();
        store.remove_all_roots().unwrap();
        assert!(store.roots().unwrap().is_empty());
        assert!(store.files().unwrap().is_empty());
        assert_eq!(store.chunk_count().unwrap(), 0);
        assert!(store.candidate_query(&expression, None, &[], &[], &[], 10).unwrap().is_empty());
        assert_eq!(store.conversations().unwrap()[0].id, "kept");
        assert_eq!(store.drafts().unwrap()[0].body, "unsaved draft");
        assert_eq!(serde_json::to_value(store.settings().unwrap()).unwrap(), settings);
        assert_eq!(fs::read_to_string(&source).unwrap(), "uniquetoken original document");
        // A file event queued before removal cannot recreate an index.
        assert_eq!(store.update_paths(root.to_str().unwrap(), &[source.clone()], |_| {}).unwrap(), 0);
        assert_eq!(store.scan(root.to_str().unwrap(), |_| {}).unwrap(), 0);
        store.add_root(&root).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(store.search_files("uniquetoken", &[], 10).unwrap().len(), 1);
        store.root_status(root.to_str().unwrap(), "indexing", None).unwrap();
        assert!(store.remove_all_roots().is_err());
        assert_eq!(store.files().unwrap().len(), 1);
    }
    #[test]
    fn clear_all_data_resets_settings_and_drafts_and_preserves_sources() {
        let (_dir, store, root) = setup();
        let source = root.join("source.txt");
        fs::write(&source, "original source text").unwrap();
        store.put_conversation(&Conversation { id:"hidden".into(),title:"history".into(),messages:vec![],updated:now(),deleted:true }).unwrap();
        let mut settings = store.settings().unwrap();
        settings.answer.model = "custom-model".into();
        settings.answer.models = vec!["custom-model".into()];
        store.save_settings(&settings).unwrap();
        store.put_draft(&Draft {path:source.to_str().unwrap().into(),name:"source.txt".into(),body:"unsaved draft".into(),hash:"original".into(),is_new:false}).unwrap();
        assert!(store.clear_all_data().is_err());
        let count: i64 = store.connection().query_row("SELECT count(*) FROM conversations", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "indexing failure must not delete conversations");
        assert_eq!(store.settings().unwrap().answer.model, "custom-model");
        assert_eq!(store.drafts().unwrap().len(), 1);
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        store.clear_all_data().unwrap();
        assert!(store.roots().unwrap().is_empty());
        assert!(store.files().unwrap().is_empty());
        assert_eq!(store.chunk_count().unwrap(), 0);
        let count: i64 = store.connection().query_row("SELECT count(*) FROM conversations", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
        assert_eq!(serde_json::to_value(store.settings().unwrap()).unwrap(), serde_json::to_value(Settings::default()).unwrap());
        assert!(store.drafts().unwrap().is_empty());
        assert_eq!(fs::read_to_string(&source).unwrap(), "original source text");
    }
    #[test]
    fn clearing_files_reclaims_sqlite_and_wal_space_without_changing_local_files() {
        for clear_everything in [false, true] {
            let (dir, store, root) = setup();
            let source = root.join("原始文档.md");
            let contents: String = (0..12_000).map(|i| format!("\n\n第 {i} 项记录。evidence{i} describes the original project requirements, release process and access permissions." )).collect();
            fs::write(&source, contents.as_bytes()).unwrap();
            let attachment = root.join("原始附件.bin");
            fs::write(&attachment, [0, 255, 1, 128, 42]).unwrap();
            let originals = [(source.clone(), hash(&fs::read(&source).unwrap())), (attachment.clone(), hash(&fs::read(&attachment).unwrap()))];
            store.scan(root.to_str().unwrap(), |_| {}).unwrap();
            assert!(!store.search_files("evidence42", &[], 5).unwrap().is_empty());
            store.put_conversation(&Conversation {id:"kept".into(),title:"history".into(),messages:vec![],updated:now(),deleted:false}).unwrap();
            let database = dir.path().join("app/db.sqlite");
            let wal = dir.path().join("app/db.sqlite-wal");
            let checkpoint: i64 = store.connection().query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0)).unwrap();
            assert_eq!(checkpoint, 0);
            let before = fs::metadata(&database).unwrap().len();
            assert!(before > 1_000_000, "fixture must allocate a substantial index: {before}");
            if clear_everything { store.clear_all_data().unwrap(); } else { store.remove_all_roots().unwrap(); }
            let after = fs::metadata(&database).unwrap().len();
            let wal_after = fs::metadata(&wal).unwrap().len();
            eprintln!("clear_all_data={clear_everything}: SQLite {before} -> {after} bytes, WAL {wal_after} bytes");
            assert!(after < before / 4, "clearing must return disk space immediately: {before} -> {after}");
            assert_eq!(wal_after, 0, "clearing must truncate the WAL while the application remains open");
            assert!(store.files().unwrap().is_empty());
            assert_eq!(store.chunk_count().unwrap(), 0);
            assert_eq!(store.conversations().unwrap().len(), if clear_everything { 0 } else { 1 });
            for (path, fingerprint) in &originals { assert_eq!(hash(&fs::read(path).unwrap()), *fingerprint); }
            let integrity: String = store.connection().query_row("PRAGMA integrity_check", [], |row| row.get(0)).unwrap();
            assert_eq!(integrity, "ok");
            store.add_root(&root).unwrap();
            store.scan(root.to_str().unwrap(), |_| {}).unwrap();
            assert!(!store.search_files("evidence42", &[], 5).unwrap().is_empty());
        }
    }

    #[test]
    fn converted_media_missing_profile_preserves_owned_and_hand_edited_markdown() {
        let (dir, store, root) = setup();
        let mut settings = store.settings().unwrap();
        settings.convert_other_formats = true;
        store.save_settings(&settings).unwrap();
        let source = root.join("recording.mp3");
        fs::write(&source, "original media").unwrap();
        let output = crate::docling::output_path(&root, &source).unwrap();
        fs::create_dir_all(output.parent().unwrap()).unwrap();
        fs::write(&output, "ownedtranscriptevidence.").unwrap();
        let records = std::collections::BTreeMap::from([("recording.mp3", (crate::docling::fingerprint(&source).unwrap(), crate::docling::fingerprint(&output).unwrap()))]);
        let ownership = output.parent().unwrap().join(".sag-converted.json");
        let previous_records = serde_json::to_vec(&records).unwrap();
        fs::write(&ownership, &previous_records).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(store.search_files("ownedtranscriptevidence", &[], 5).unwrap().len(), 1);
        fs::write(&source, "changed media").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(fs::read_to_string(&output).unwrap(), "ownedtranscriptevidence.");
        assert_eq!(fs::read(&ownership).unwrap(), previous_records);
        assert!(store.search_files("ownedtranscriptevidence", &[], 5).unwrap().is_empty(), "a previous transcript must not stand in for a changed source");
        fs::write(&output, "my hand edited transcript").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(fs::read_to_string(&output).unwrap(), "my hand edited transcript");
        assert_eq!(fs::read(&ownership).unwrap(), previous_records);
        assert!(store.search_files("hand edited transcript", &[], 5).unwrap().is_empty());
        assert!(!dir.path().join("app/docling").exists(), "failed media scans must never install a component");
    }

    #[test]
    fn new_settings_use_302_jev_and_keep_saved_provider() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.sqlite");
        let store = Store::open(&path).unwrap();
        let mut settings = store.settings().unwrap();
        assert_eq!(settings.jev.url, "https://api.302ai.cn/typesafeai/v1");
        assert_eq!(settings.jev.model, "jev-latest");
        assert_eq!(crate::models::endpoint(&settings.jev, true), "https://api.302ai.cn/typesafeai/v1/systemone");
        assert!(!settings.jev.has_key);

        settings.jev.url = "https://openrouter.ai/api/alpha/decisions".into();
        settings.jev.model = "typesafe/jev-1.13".into();
        store.save_settings(&settings).unwrap();
        drop(store);
        let saved = Store::open(&path).unwrap().settings().unwrap();
        assert_eq!(saved.jev.url, settings.jev.url);
        assert_eq!(saved.jev.model, settings.jev.model);
    }
    #[test]
    fn native_ranking_matches_sqlite_with_pruning_filters_and_new_versions() {
        let (_dir, store, root) = setup();
        let rare = ["rareone", "raretwo", "rarethree", "rarefour", "rarefive"];
        let common = ["commonone", "commontwo", "commonthree", "commonfour"];
        for i in 0..160 {
            let body = format!("{} {}", common.join(" "), if i < 120 { rare.join(" ").repeat(8) } else { String::new() });
            fs::write(root.join(format!("{i}.txt")), body).unwrap();
        }
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let expression = rare.iter().chain(common.iter()).map(|term| text::quote(term)).collect::<Vec<_>>().join(" OR ");
        for required in [None, Some("\"rareone\"")] {
            let actual = store.candidate_query(&expression, required, &[], &[], &[1, 3], 100).unwrap();
            let db = store.connection();
            let sql = format!("SELECT rowid,bm25(chunk_fts,2,2,1) AS score FROM chunk_fts WHERE chunk_fts MATCH ?1 AND rowid NOT IN (1,3) {} ORDER BY score,rowid LIMIT 100", if required.is_some() { "AND +rowid IN (SELECT rowid FROM chunk_fts WHERE chunk_fts MATCH ?2)" } else { "" });
            let mut statement = db.prepare(&sql).unwrap();
            let values: Vec<&str> = if let Some(term) = required { vec![&expression, term] } else { vec![&expression] };
            let expected: Vec<(i64, f64)> = statement.query_map(rusqlite::params_from_iter(values), |row| Ok((row.get(0)?, row.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
            assert_eq!(actual.iter().map(|row| (row.id, row.bm25)).collect::<Vec<_>>(), expected);
        }
        assert_eq!(store.frequency("rareone").unwrap(), 120);
        let subset = vec![path_text(&root.join("130.txt")).unwrap()];
        assert!(!store.has_candidate_match("\"rareone\"", &subset, &[], &[]).unwrap());
        assert!(store.has_candidate_match("\"rareone\"", &subset, &[path_text(&root.join("0.txt")).unwrap()], &[]).unwrap());
        fs::write(root.join("130.txt"), "rareone commonone").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(store.frequency("rareone").unwrap(), 121);
        assert!(store.has_candidate_match("\"rareone\"", &subset, &[], &[]).unwrap());
    }

    #[test]
    fn v10_startup_rebuilds_changed_chunk_defaults_and_preserves_files() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("upgrade.sqlite");
        let files = dir.path().join("files");
        std::fs::create_dir(&files).unwrap();
        let source = files.join("note.txt");
        std::fs::write(&source,"upgrade evidence ".repeat(400)).unwrap();
        let store = Store::open(&database).unwrap();
        let root = store.add_root(&files).unwrap();
        store.scan(&root, |_| {}).unwrap();
        let mut old = serde_json::to_value(Settings::default()).unwrap();
        old.as_object_mut().unwrap().remove("retrieval_version");
        old["retrieval"] = serde_json::json!({"candidate_k":50,"seed_k":10,"max_hops":1,"expansion_term_limit":20,"top_k":10,"chunk_chars":1000});
        store.connection().execute("INSERT INTO preferences VALUES(1,?)", [old.to_string()]).unwrap();
        drop(store);
        let reopened = Store::open(&database).unwrap();
        assert_eq!(reopened.settings().unwrap().retrieval.chunk_chars,512);
        assert_eq!(reopened.chunk_count().unwrap(),0);
        assert!(source.is_file());
        reopened.scan(&root, |_| {}).unwrap();
        let count = reopened.chunk_count().unwrap();
        assert!(count>0);
        drop(reopened);
        let stable = Store::open(&database).unwrap();
        assert_eq!(stable.chunk_count().unwrap(),count);
    }

    #[test]
    fn chunk_layout_upgrade_rebuilds_old_indexes_without_changing_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.sqlite");
        let root = dir.path().join("files");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("note.md"), "# Keep\nOriginal source remains.").unwrap();
        let store = Store::open(&path).unwrap();
        let root_key = store.add_root(&root).unwrap();
        store.scan(&root_key, |_| {}).unwrap();
        let mut settings = store.settings().unwrap();
        settings.retrieval.chunk_chars = 900;
        store.save_settings(&settings).unwrap();
        store.connection().pragma_update(None, "user_version", 6).unwrap();
        drop(store);
        let upgraded = Store::open(&path).unwrap();
        assert_eq!(upgraded.chunk_count().unwrap(), 0);
        assert_eq!(upgraded.roots().unwrap()[0].status, "pending");
        assert_eq!(upgraded.settings().unwrap().retrieval.chunk_chars, 900);
        upgraded.scan(&root_key, |_| {}).unwrap();
        assert_eq!(upgraded.search_files("Original", &[], 5).unwrap().len(), 1);
        drop(upgraded);
        assert_eq!(Store::open(&path).unwrap().chunk_count().unwrap(), 1);
    }

    #[test]
    fn v10_migrates_old_defaults_and_preserves_custom_settings() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("settings.sqlite")).unwrap();
        let mut old = serde_json::to_value(Settings::default()).unwrap();
        old.as_object_mut().unwrap().remove("retrieval_version");
        old["jev"]["url"] = serde_json::json!("https://existing.example/decisions");
        old["retrieval"] = serde_json::json!({"candidate_k":50,"seed_k":10,"max_hops":1,"expansion_term_limit":20,"top_k":10,"chunk_chars":1000});
        store.connection().execute("INSERT INTO preferences VALUES(1,?)", [old.to_string()]).unwrap();
        let upgraded = store.settings().unwrap();
        assert_eq!(upgraded.retrieval_version, 10);
        assert_eq!(upgraded.retrieval.candidate_k, 50);
        assert_eq!(upgraded.retrieval.max_hops, 1);
        assert_eq!(upgraded.jev.url, "https://existing.example/decisions");
        old["retrieval"]["candidate_k"] = serde_json::json!(70);
        store.connection().execute("UPDATE preferences SET value=? WHERE id=1", [old.to_string()]).unwrap();
        assert_eq!(store.settings().unwrap().retrieval.candidate_k, 70);
        assert_eq!(store.settings().unwrap().retrieval.max_hops, 1);
    }

    #[test]
    fn interface_language_persists_and_old_settings_default_to_chinese() {
        let (_dir, store, _) = setup();
        let mut settings = store.settings().unwrap();
        assert_eq!(settings.language, "zh-CN");
        settings.language = "en".into();
        store.save_settings(&settings).unwrap();
        assert_eq!(store.settings().unwrap().language, "en");
        settings.language = "unsupported".into();
        assert!(store.save_settings(&settings).is_err());
        assert_eq!(store.settings().unwrap().language, "en");
        let mut legacy = serde_json::to_value(Settings::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("language");
        let restored: Settings = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.language, "zh-CN");
    }
    #[test]
    fn search_summary_reference_count_defaults_to_ten_and_has_no_fixed_maximum() {
        let (dir, store, _) = setup();
        let mut settings = store.settings().unwrap();
        assert_eq!(settings.search_summary_sources, 10);
        for count in [1, 100, 100_000] {
            settings.search_summary_sources = count;
            store.save_settings(&settings).unwrap();
            assert_eq!(store.settings().unwrap().search_summary_sources, count);
        }
        let reopened = Store::open(&dir.path().join("app/db.sqlite")).unwrap();
        assert_eq!(reopened.settings().unwrap().search_summary_sources, 100_000);
        settings.search_summary_sources = 0;
        assert!(store.save_settings(&settings).is_err());
        let restored: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(restored.search_summary_sources, 10);
    }
    #[test]
    fn context_budget_above_128000_persists_without_an_upper_limit() {
        let (dir, store, _) = setup();
        let mut settings = store.settings().unwrap();
        for budget in [131_072, 200_000, 1_048_576] {
            settings.answer_context_tokens = budget;
            store.save_settings(&settings).unwrap();
            assert_eq!(store.settings().unwrap().answer_context_tokens, budget);
        }
        let reopened = Store::open(&dir.path().join("app/db.sqlite")).unwrap();
        assert_eq!(reopened.settings().unwrap().answer_context_tokens, 1_048_576);
        settings.answer_context_tokens = 4095;
        assert!(store.save_settings(&settings).is_err());
    }
    #[test]
    fn model_profiles_and_tool_limit_persist_and_validate() {
        let (_dir, store, _) = setup();
        let mut settings = store.settings().unwrap();
        assert_eq!(settings.max_tool_calls, 10);
        settings.mcp_servers.push(McpServer {
            id: "wiki".into(),
            name: "Wiki".into(),
            url: "https://example.com/mcp".into(),
            ..Default::default()
        });
        settings.model_profiles.insert(
            "first".into(),
            ModelProfile {
                system_prompt: "中文".into(),
                reasoning_effort: Some(ReasoningEffort::Low),
                sag2_enabled: false,
                mcp_servers: vec!["wiki".into()],
                mcp_tools: [("wiki".into(), vec!["read_wiki_structure".into()])].into(),
            },
        );
        settings
            .model_profiles
            .insert("second".into(), ModelProfile::default());
        settings.max_tool_calls = 7;
        store.save_settings(&settings).unwrap();
        let saved = store.settings().unwrap();
        assert_eq!(saved.max_tool_calls, 7);
        assert_eq!(saved.model_profiles["first"].system_prompt, "中文");
        assert_eq!(
            saved.model_profiles["first"].reasoning_effort,
            Some(ReasoningEffort::Low)
        );
        assert!(!saved.model_profiles["first"].sag2_enabled);
        assert!(saved.model_profiles["second"].sag2_enabled);
        assert!(saved.model_profiles["second"].mcp_servers.is_empty());
        assert_eq!(
            saved.model_profiles["first"].mcp_tools["wiki"],
            vec!["read_wiki_structure"]
        );
        settings.max_tool_calls = 0;
        assert!(store.save_settings(&settings).is_err());
        settings.max_tool_calls = 3;
        settings.mcp_servers.clear();
        assert!(store.save_settings(&settings).is_err());
        assert_eq!(store.settings().unwrap().max_tool_calls, 7);
    }
    #[test]
    fn clearing_conversations_removes_hidden_history_and_database_log_bytes() {
        let (dir, store, _) = setup();
        let secret = "PURGE_HISTORY_PAYLOAD_7c9d4e";
        let message: Message = serde_json::from_value(serde_json::json!({
            "question": secret, "answer": secret, "reasoning": secret, "evidence": [],
            "model":"test", "scope":[], "references":[], "status":"complete",
            "error":secret, "retrieval_question":secret,
            "tool_calls":[{"id":"call", "query":secret, "source_count":0,"result":{"text":secret}}]
        })).unwrap();
        for (id, deleted) in [("visible", false), ("hidden", true)] {
            store.put_conversation(&Conversation {id:id.into(),title:secret.into(),messages:vec![message.clone()],updated:now(),deleted}).unwrap();
        }
        let settings = store.settings().unwrap();
        let roots = store.roots().unwrap();
        store.clear_conversations().unwrap();
        assert!(store.conversations().unwrap().is_empty());
        assert_eq!(store.connection().query_row("SELECT COUNT(*) FROM conversations",[],|row|row.get::<_,i64>(0)).unwrap(),0);
        for file in ["db.sqlite", "db.sqlite-wal"] {
            let path = dir.path().join("app").join(file);
            if path.exists() { assert!(!fs::read(path).unwrap().windows(secret.len()).any(|bytes|bytes==secret.as_bytes())); }
        }
        assert_eq!(store.roots().unwrap().len(), roots.len());
        assert_eq!(store.settings().unwrap().answer.model, settings.answer.model);
    }

    #[test]
    fn mcp_call_details_survive_history_storage() {
        let (_dir, store, _) = setup();
        let message:Message=serde_json::from_value(serde_json::json!({"question":"test","answer":"failed","evidence":[],"model":"test","scope":[],"references":[],"status":"complete","error":null,"retrieval_question":"test","tool_calls":[{"id":"tool-1","query":"{\"path\":\"sample.txt\"}","source_count":0,"server_name":"local","tool_name":"read","is_error":true,"denied":false,"result":{"isError":true,"content":[{"type":"text","text":"Missing: sample.txt"}]},"error":"Missing: sample.txt"}]})).unwrap();
        store
            .put_conversation(&Conversation {
                id: "mcp-history".into(),
                title: "MCP history".into(),
                messages: vec![message],
                updated: now(),
                deleted: false,
            })
            .unwrap();
        let history = store.conversations().unwrap();
        let record = &history[0].messages[0].tool_calls[0];
        assert_eq!(record.error.as_deref(), Some("Missing: sample.txt"));
        assert_eq!(
            record.result.as_ref().unwrap()["content"][0]["text"],
            "Missing: sample.txt"
        );
        let legacy: ToolCallRecord = serde_json::from_value(
            serde_json::json!({"id":"old","query":"query","source_count":0}),
        )
        .unwrap();
        assert!(legacy.result.is_none());
        assert!(legacy.error.is_none());
    }
    #[test]
    fn delete_and_restore_message_keeps_the_rest_of_the_conversation() {
        let (_dir, store, _) = setup();
        let message = Message {
            created_at: Some(1_792_368_300_000),
            question: "第一问".into(),
            answer: "第一答".into(),
            reasoning: String::new(),
            reasoning_blocks: vec![],
            evidence: vec![],
            model: "test".into(),
            scope: vec![],
            references: vec![],
            status: "complete".into(),
            error: None,
            retrieval_question: "第一问".into(),
            tool_calls: vec![],
            context: None,
        };
        let mut next = message.clone();
        next.question = "第二问".into();
        let original = Conversation {
            id: "message-test".into(),
            title: "保留的标题".into(),
            messages: vec![message, next.clone()],
            updated: now(),
            deleted: false,
        };
        store.put_conversation(&original).unwrap();
        let edited = store.replace_messages(&original.id, vec![next]).unwrap();
        assert_eq!(edited.title, original.title);
        assert_eq!(edited.messages.len(), 1);
        assert_eq!(edited.messages[0].question, "第二问");
        store
            .replace_messages(&original.id, original.messages)
            .unwrap();
        let restored = store.conversations().unwrap();
        assert_eq!(restored[0].messages.len(), 2);
        assert_eq!(restored[0].messages[0].answer, "第一答");
        assert_eq!(restored[0].messages[0].created_at, Some(1_792_368_300_000));
        assert!(store.replace_messages("missing", vec![]).is_err());
    }
    #[test]
    fn remove_never_deletes_source() {
        let (_d, s, n) = setup();
        let file = n.join("one.md");
        fs::write(&file, "# Heading\n\nRetained content").unwrap();
        s.scan(n.to_str().unwrap(), |_| {}).unwrap();
        s.remove_root(n.to_str().unwrap()).unwrap();
        assert_eq!(
            fs::read_to_string(file).unwrap(),
            "# Heading\n\nRetained content"
        );
        assert_eq!(s.chunk_count().unwrap(), 0);
    }
    #[test]
    fn preserves_no_change_bytes_and_crlf() {
        let (_d, s, n) = setup();
        let f = n.join("x.md");
        let bytes = b"\xef\xbb\xbf# Hello\r\n\r\nText\r\n";
        fs::write(&f, bytes).unwrap();
        let d = s.document(&f).unwrap();
        s.save_document(&f, &d.body, Some(&d.hash)).unwrap();
        assert_eq!(fs::read(&f).unwrap(), bytes);
        s.save_document(&f, "# Hello\n\nChanged\n", Some(&d.hash))
            .unwrap();
        assert_eq!(
            fs::read(&f).unwrap(),
            b"\xef\xbb\xbf# Hello\r\n\r\nChanged\r\n"
        );
    }
    #[test]
    fn conflict_and_duplicate_preserve_file() {
        let (_d, s, n) = setup();
        let f = n.join("x.md");
        fs::write(&f, "old").unwrap();
        let d = s.document(&f).unwrap();
        fs::write(&f, "external").unwrap();
        assert!(s.save_document(&f, "mine", Some(&d.hash)).is_err());
        assert!(s.save_document(&f, "mine", None).is_err());
        assert_eq!(fs::read_to_string(&f).unwrap(), "external");
    }
    #[test]
    fn cannot_read_sibling_prefix_or_save_outside() {
        let (d, s, n) = setup();
        let outside = d.path().join("notes2");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("x.md"), "private").unwrap();
        assert!(s.document(&outside.join("x.md")).is_err());
        assert!(s.save_document(&outside.join("new.md"), "x", None).is_err());
        assert!(s.save_document(&n.join("../escape.md"), "x", None).is_err());
    }
    #[test]
    fn parent_add_deduplicates_roots() {
        let (d, s, n) = setup();
        s.add_root(d.path()).unwrap();
        assert_eq!(s.roots().unwrap().len(), 1);
        s.add_root(&n).unwrap();
        assert_eq!(s.roots().unwrap().len(), 1);
    }
    #[test]
    fn range_filter_before_limit() {
        let (_d, s, n) = setup();
        let a = n.join("a");
        let b = n.join("b");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        fs::write(a.join("one.md"), "unique matching term").unwrap();
        fs::write(b.join("two.md"), "unique matching term").unwrap();
        s.scan(n.to_str().unwrap(), |_| {}).unwrap();
        let hits = s
            .candidate_query(
                &text::match_query("unique"),
                None,
                &[path_text(&b).unwrap()],
                &[],
                &[],
                1,
            )
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert!(Path::new(&hits[0].path).starts_with(b));
    }
    #[test]
    fn local_search_returns_files_not_repeated_chunks_and_keeps_scope() {
        let (_d, s, n) = setup();
        let a = n.join("a");
        let b = n.join("b");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        fs::write(a.join("many.md"), "octx alpha data\n\n".repeat(100)).unwrap();
        fs::write(a.join("single.md"), "octx beta").unwrap();
        fs::write(b.join("outside.md"), "octx").unwrap();
        for path in [a.join("many.md"), a.join("single.md"), b.join("outside.md")] {
            s.index_file(n.to_str().unwrap(), &path, 100).unwrap();
        }
        let hits = s
            .search_files("octx", &[path_text(&a).unwrap()], 100)
            .unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|c| Path::new(&c.path).starts_with(&a)));
        assert!(hits[0].bm25 <= hits[1].bm25);
        assert_eq!(
            s.search_files("octx", &[path_text(&b).unwrap()], 1)
                .unwrap()
                .len(),
            1
        );
        assert!(s.search_files("", &[], 100).unwrap().is_empty());
    }
    #[test]
    fn fast_search_paginates_all_files_in_stable_order_and_keeps_scope() {
        let (_dir, store, root) = setup();
        let selected = root.join("selected");
        let other = root.join("other");
        fs::create_dir(&selected).unwrap();
        fs::create_dir(&other).unwrap();
        let content = "paginationmarker repeated matching paragraph.\n\n".repeat(8);
        for index in 0..235 {
            let path = if index < 123 { &selected } else { &other }.join(format!("{index:03}.md"));
            fs::write(&path, &content).unwrap();
            store.index_file(root.to_str().unwrap(), &path, 100).unwrap();
        }
        let expected=store.search_files("paginationmarker",&[],300).unwrap();
        assert_eq!(expected.len(),235);
        let mut actual=vec![];
        for page in 1..=12 {
            let (items,meta)=store.search_files_page("paginationmarker",&[],page,20).unwrap();
            assert_eq!((meta.total,meta.page,meta.page_size),(235,page,20));
            assert_eq!(items.len(),if page==12 {15} else {20});
            actual.extend(items);
        }
        assert_eq!(actual.iter().map(|c|c.id).collect::<Vec<_>>(),expected.iter().map(|c|c.id).collect::<Vec<_>>());
        assert_eq!(actual.iter().map(|c|&c.path).collect::<HashSet<_>>().len(),235);
        let (scoped,meta)=store.search_files_page("paginationmarker",&[path_text(&selected).unwrap()],7,20).unwrap();
        assert_eq!((meta.total,meta.page,scoped.len()),(123,7,3));
        assert!(scoped.iter().all(|c|Path::new(&c.path).starts_with(&selected)));
        let (last,meta)=store.search_files_page("paginationmarker",&[],99,20).unwrap();
        assert_eq!((meta.page,last.len()),(12,15));
        for c in &actual {assert_eq!(fs::read_to_string(&c.path).unwrap(),content);}
        let (empty,meta)=store.search_files_page("nohitmarker",&[],2,20).unwrap();
        assert!(empty.is_empty());assert_eq!((meta.total,meta.page),(0,1));
    }
    #[test]
    fn incremental_scan_and_explicit_recheck() {
        let (_d, s, n) = setup();
        let f = n.join("a.md");
        fs::write(&f, "first unique").unwrap();
        let root = n.to_str().unwrap();
        assert!(s.index_file(root, &f, 2200).unwrap());
        let version = s.version().unwrap();
        assert!(!s.index_file(root, &f, 2200).unwrap());
        s.verify_root(root).unwrap();
        assert!(!s.index_file(root, &f, 2200).unwrap());
        assert_eq!(s.version().unwrap(), version);
        fs::write(&f, "second different content").unwrap();
        assert!(s.index_file(root, &f, 2200).unwrap());
        assert!(s.version().unwrap() > version);
    }
    #[test]
    fn scan_synchronizes_modified_renamed_and_deleted_files() {
        let (_dir, store, root) = setup();
        let changed = root.join("changed.md");
        let untouched = root.join("untouched.txt");
        fs::write(&changed, "quartzbefore").unwrap();
        fs::write(&untouched, "Original unchanged content.\r\n").unwrap();
        let original = fs::read(&untouched).unwrap();
        let root_path = root.to_str().unwrap();
        store.scan(root_path, |_| {}).unwrap();
        let untouched_id = store.document(&untouched).unwrap().id;
        assert_eq!(store.search_files("quartzbefore", &[], 10).unwrap().len(), 1);

        fs::write(&changed, "berylafter with updated content").unwrap();
        store.scan(root_path, |_| {}).unwrap();
        assert!(store.search_files("quartzbefore", &[], 10).unwrap().is_empty());
        assert_eq!(store.search_files("berylafter", &[], 10).unwrap().len(), 1);

        let directory = root.join("nested");
        fs::create_dir(&directory).unwrap();
        let renamed = directory.join("renamed.md");
        fs::rename(&changed, &renamed).unwrap();
        store.scan(root_path, |_| {}).unwrap();
        let hits = store.search_files("berylafter", &[], 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, path_text(&renamed).unwrap());
        assert!(!store.files().unwrap().iter().any(|file| file.path == path_text(&changed).unwrap()));

        fs::remove_file(&renamed).unwrap();
        store.scan(root_path, |_| {}).unwrap();
        assert!(store.search_files("berylafter", &[], 10).unwrap().is_empty());
        assert_eq!(store.files().unwrap().len(), 1);
        assert_eq!(store.document(&untouched).unwrap().id, untouched_id);
        assert_eq!(fs::read(&untouched).unwrap(), original);
    }
    #[test]
    fn references_include_files_without_keyword_overlap() {
        let (_d, s, n) = setup();
        let f = n.join("a.md");
        fs::write(&f, "Only a completely different subject.").unwrap();
        s.scan(n.to_str().unwrap(), |_| {}).unwrap();
        let refs = vec![path_text(&f).unwrap()];
        let hits = s.reference_candidates(&refs, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, refs[0]);
    }

    #[test]
    fn saved_partial_passages_have_ranking_statistics() {
        let (_dir, store, root) = setup();
        let path = root.join("partial.md");
        let source = "嵌套文件夹内容列表的第一行提供完整信息。";
        fs::write(&path, source).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        // A saved pre-fix index can start inside a continuous word.
        let span = text::Lines::new(source).span(source, "嵌套".len(), source.len());
        store.connection().execute("UPDATE chunks SET spans=?", [serde_json::to_string(&vec![span]).unwrap()]).unwrap();
        let mut candidates = store.reference_candidates(&[path_text(&path).unwrap()], 10).unwrap();
        assert_eq!(candidates[0].content, "文件夹内容列表的第一行提供完整信息。");
        candidates[0].score = 0.9;
        let corpus = crate::sag2_v10::Corpus::new(&store.retrieval_corpus(&[], &[]).unwrap()).unwrap();
        corpus.diversify(&mut candidates);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].score, 0.9);
    }

    #[test]
    fn retrieval_statistics_preserve_ranking_and_scope_and_upgrade_existing_indexes() {
        let (dir, store, root) = setup();
        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        let path = a.join("Café.md");
        fs::write(&path, "# 配置 → 检索\n嵌套文件夹内容列表的第一行提供完整信息。\nStraße CAFÉ alpha evidence.").unwrap();
        fs::write(b.join("other.md"), "alpha shared evidence. beta unique information.").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let cancel = tokio_util::sync::CancellationToken::new();
        for scope in [vec![], vec![path_text(&a).unwrap()], vec![path_text(&b).unwrap()]] {
            let original = crate::sag2_v10::Corpus::new(&store.retrieval_corpus(&scope, &[]).unwrap()).unwrap();
            let cached = store.retrieval_statistics(&scope, &[], &cancel).unwrap();
            let mut expected = store.candidate_query(&text::match_query("alpha"), None, &scope, &[], &[], 100).unwrap();
            for candidate in &mut expected { candidate.score = 0.8; }
            let mut actual = expected.clone();
            original.diversify(&mut expected);
            cached.diversify(&mut actual);
            assert_eq!(serde_json::to_value(actual).unwrap(), serde_json::to_value(expected).unwrap());
        }
        // Simulate a pre-upgrade index, including a passage beginning inside a word.
        let source = fs::read_to_string(&path).unwrap();
        let start = source.find("文件夹").unwrap();
        let span = text::Lines::new(&source).span(&source, start, source.len());
        store.connection().execute("UPDATE chunks SET spans=? WHERE document_id=(SELECT id FROM documents WHERE path=?)", params![serde_json::to_string(&vec![span]).unwrap(), path_text(&path).unwrap()]).unwrap();
        store.connection().execute("DELETE FROM retrieval_vocabulary", []).unwrap();
        drop(store);
        let database = dir.path().join("app/db.sqlite");
        let store = Store::open(&database).unwrap();
        let corpus = store.retrieval_statistics(&[], &[], &cancel).unwrap();
        let mut candidates = store.reference_candidates(&[path_text(&path).unwrap()], 10).unwrap();
        candidates[0].score = 0.9;
        corpus.diversify(&mut candidates);
        let document_count: usize = store.connection().query_row("SELECT COUNT(*) FROM retrieval_vocabulary", [], |r| r.get(0)).unwrap();
        assert_eq!(document_count, 2);
        // The backfilled statistics must also survive a restart without original file reads.
        drop(store);
        fs::remove_file(&path).unwrap();
        let reopened = Store::open(&database).unwrap();
        reopened.retrieval_statistics(&[], &[], &cancel).unwrap();
    }

    #[test]
    fn plain_text_sentences_use_the_chunk_target() {
        let (_dir, store, root) = setup();
        let path = root.join("sentences.txt");
        fs::write(&path, "每个完整句子都需要保留。".repeat(100)).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let candidates = store.reference_candidates(&[path_text(&path).unwrap()], 100).unwrap();
        assert!(candidates.len() > 2);
        assert!(candidates.iter().all(|candidate| candidate.content.ends_with('。') && candidate.content.encode_utf16().count() <= 512));
    }

    #[test]
    fn source_code_and_partial_lines_round_trip_without_writes() {
        let (_d, s, n) = setup();
        let mut settings = s.settings().unwrap();
        settings.file_types.code = true;
        s.save_settings(&settings).unwrap();
        let source = format!(
            "# Python comment\r\n{}\r\nsecret_value = 1234\r\n",
            "variable = '文本'; ".repeat(600)
        );
        let f = n.join("module.py");
        fs::write(&f, &source).unwrap();
        s.scan(n.to_str().unwrap(), |_| {}).unwrap();
        assert!(s
            .entries(&n, true)
            .unwrap()
            .iter()
            .any(|e| e.name == "module.py"));
        let items = s
            .reference_candidates(&[path_text(&f).unwrap()], 100)
            .unwrap();
        assert!(items.len() > 2);
        for c in items {
            assert!(!c.source_spans.is_empty());
            assert_eq!(
                c.content,
                c.source_spans
                    .iter()
                    .map(|sp| &source[sp.start_byte..sp.end_byte])
                    .collect::<Vec<_>>()
                    .join("\n\n")
            );
        }
        assert!(s
            .save_document(&f, "replacement", Some(&hash(source.as_bytes())))
            .is_err());
        assert_eq!(fs::read_to_string(f).unwrap(), source);
    }
    #[test]
    fn empty_documents_remain_visible() {
        let (_d, s, n) = setup();
        fs::write(n.join("blank.md"), "").unwrap();
        s.scan(n.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(s.files().unwrap().len(), 1);
        assert_eq!(s.chunk_count().unwrap(), 0);
    }

    #[test]
    fn parallel_scan_preserves_rows_ranking_and_incremental_updates() {
        let (dir, serial, root) = setup();
        for i in 0..40 {
            fs::write(
                root.join(format!("{i:02}.md")),
                format!(
                    "# 文档 {i}\n\nAlpha evidence 中文。\n\n{}",
                    "Other facts. ".repeat(i + 1)
                ),
            )
            .unwrap();
        }
        let parallel = Store::open(&dir.path().join("parallel.sqlite")).unwrap();
        let root = root.to_str().unwrap();
        parallel.add_root(Path::new(root)).unwrap();
        let mut settings = Settings::default();
        settings.index_workers = 1;
        serial.save_settings(&settings).unwrap();
        settings.index_workers = 8;
        parallel.save_settings(&settings).unwrap();
        assert_eq!(serial.scan(root, |_| {}).unwrap(), 40);
        assert_eq!(parallel.scan(root, |_| {}).unwrap(), 40);
        for query in [
            "SELECT json_array(id,document_id,heading,ranges,start_line,end_line,spans) FROM chunks ORDER BY id",
            "SELECT json_array(term,col,doc,cnt) FROM chunk_vocab ORDER BY term,col",
        ] {
            let rows = |store: &Store| -> Vec<String> {
                store.connection().prepare(query).unwrap()
                    .query_map([], |row| row.get(0)).unwrap()
                    .collect::<rusqlite::Result<_>>().unwrap()
            };
            assert_eq!(rows(&serial), rows(&parallel));
        }
        let query = text::match_query("Alpha 中文 evidence");
        let left = serial
            .candidate_query(&query, None, &[], &[], &[], 100)
            .unwrap();
        let right = parallel
            .candidate_query(&query, None, &[], &[], &[], 100)
            .unwrap();
        assert_eq!(
            serde_json::to_value(left).unwrap(),
            serde_json::to_value(right).unwrap()
        );
        let version = parallel.version().unwrap();
        settings.index_workers = 14;
        parallel.save_settings(&settings).unwrap();
        parallel.scan(root, |_| {}).unwrap();
        assert_eq!(parallel.version().unwrap(), version);
        fs::write(Path::new(root).join("00.md"), "Changed alpha evidence").unwrap();
        parallel.scan(root, |_| {}).unwrap();
        assert_eq!(parallel.version().unwrap(), version + 1);
    }

    #[test]
    fn bounded_text_scan_commits_completed_files_before_preparing_the_whole_batch() {
        let (_dir, store, root) = setup();
        let mut settings = store.settings().unwrap();
        settings.index_workers = 1;
        store.save_settings(&settings).unwrap();
        for i in 0..40 {
            fs::write(root.join(format!("{i:02}.txt")), "incremental evidence.").unwrap();
        }
        let observed = std::cell::Cell::new(false);
        store.scan(root.to_str().unwrap(), |progress| {
            if progress.total == 40 && progress.count == 20 {
                let written: usize = store.connection().query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0)).unwrap();
                assert_eq!(written, 20, "completed files must be published incrementally, not as a 256-file batch");
                observed.set(true);
            }
        }).unwrap();
        assert!(observed.get());
    }

    #[test]
    fn bounded_text_resource_failure_preserves_the_previous_index_and_reports_an_issue() {
        let (_dir, store, root) = setup();
        let path = root.join("large.txt");
        fs::write(&path, "previous evidence marker.").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let before: (i64, String, usize) = store.connection().query_row(
            "SELECT id,hash,(SELECT COUNT(*) FROM chunks WHERE document_id=documents.id) FROM documents WHERE path=?",
            [path.to_str().unwrap()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).unwrap();
        // A modest synthetic input exercises the long-line/tokenizer limit without an OOM stress test.
        fs::write(&path, "unbrokenword ".repeat(25_000)).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let state = &store.roots().unwrap()[0];
        assert_eq!(state.status, "partial");
        assert!(state.error.as_ref().unwrap().contains("资源上限"));
        let after: (i64, String, usize) = store.connection().query_row(
            "SELECT id,hash,(SELECT COUNT(*) FROM chunks WHERE document_id=documents.id) FROM documents WHERE path=?",
            [path.to_str().unwrap()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).unwrap();
        assert_eq!(after, before);
        fs::write(&path, "recovered bounded evidence.").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(store.roots().unwrap()[0].status, "ready");
        assert_eq!(store.search_files("recovered", &[], 10).unwrap().len(), 1);
    }

    #[test]
    fn bounded_text_resource_failure_is_recorded_as_failed_in_history() {
        let (_dir, store, root) = setup();
        let path = root.join("long.txt");
        fs::write(&path, "unbrokenword ".repeat(25_000)).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let id = store.index_history().unwrap()[0].id;
        let raw = store.index_history_log(id).unwrap().raw.unwrap();
        let logs: Vec<sag_protocol::IndexFileLog> = raw.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].status, "failed");
        assert!(logs[0].error.as_ref().unwrap().contains("资源上限"));
    }

    #[test]
    fn bounded_text_decode_failure_preserves_the_previous_index() {
        let (_dir, store, root) = setup();
        let path = root.join("broken.txt");
        fs::write(&path, "previous evidence marker.").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let before: (String, usize) = store.connection().query_row(
            "SELECT hash,(SELECT COUNT(*) FROM chunks WHERE document_id=documents.id) FROM documents WHERE path=?",
            [path.to_str().unwrap()], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let after: Option<(String, usize)> = store.connection().query_row(
            "SELECT hash,(SELECT COUNT(*) FROM chunks WHERE document_id=documents.id) FROM documents WHERE path=?",
            [path.to_str().unwrap()], |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().unwrap();
        assert_eq!(after, Some(before));
        assert_eq!(store.roots().unwrap()[0].status, "partial");
    }

    #[test]
    fn bounded_text_restoring_the_previous_source_clears_the_failure_issue() {
        let (_dir, store, root) = setup();
        let path = root.join("restored.txt");
        fs::write(&path, "original restore marker.").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(store.roots().unwrap()[0].status, "partial");
        fs::write(&path, "original restore marker.").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(store.roots().unwrap()[0].status, "ready");
        assert_eq!(store.search_files("restore", &[], 10).unwrap().len(), 1);
    }

    #[test]
    fn bounded_text_input_cap_rejects_a_sparse_file_before_reading_it() {
        let (_dir, store, root) = setup();
        let path = root.join("too-large.txt");
        fs::File::create(&path).unwrap().set_len(MAX_TEXT_INPUT_BYTES as u64 + 1).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(store.roots().unwrap()[0].status, "partial");
        assert!(store.roots().unwrap()[0].error.as_ref().unwrap().contains("16 MiB"));
        assert_eq!(store.chunk_count().unwrap(), 0);
        assert_eq!(*store.text_preparation.used.lock().unwrap(), 0);
    }

    #[test]
    fn bounded_text_cancel_releases_the_window_and_keeps_only_committed_files() {
        let (_dir, store, root) = setup();
        let mut settings = store.settings().unwrap();
        settings.index_workers = 1;
        store.save_settings(&settings).unwrap();
        for i in 0..40 {
            fs::write(root.join(format!("{i:02}.txt")), "cancel bounded evidence.").unwrap();
        }
        let result = store.scan(root.to_str().unwrap(), |progress| {
            if progress.total == 40 && progress.count == 20 { store.indexing.stop(); }
        });
        assert!(result.unwrap_err().to_string().contains("停止"));
        let written: usize = store.connection().query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0)).unwrap();
        assert_eq!(written, 20);
        assert_eq!(*store.text_preparation.used.lock().unwrap(), 0);
        let _permit = store.text_preparation.reserve(TEXT_PREPARATION_BUDGET, || false).unwrap();
    }

    #[test]
    fn bounded_text_writer_error_rolls_back_a_document_and_releases_the_budget() {
        let (_dir, store, root) = setup();
        let path = root.join("old.txt");
        fs::write(&path, "previous evidence.").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let before: (String, i64) = store.connection().query_row(
            "SELECT hash,(SELECT id FROM chunks WHERE document_id=documents.id LIMIT 1) FROM documents WHERE path=?",
            [path.to_str().unwrap()], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        fs::write(&path, "replacement evidence.").unwrap();
        store.connection().execute_batch("CREATE TRIGGER reject_chunk BEFORE INSERT ON chunks BEGIN SELECT RAISE(ABORT,'controlled write failure'); END;").unwrap();
        assert!(store.scan(root.to_str().unwrap(), |_| {}).is_err());
        let after: (String, i64) = store.connection().query_row(
            "SELECT hash,(SELECT id FROM chunks WHERE document_id=documents.id LIMIT 1) FROM documents WHERE path=?",
            [path.to_str().unwrap()], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(after, before);
        assert_eq!(*store.text_preparation.used.lock().unwrap(), 0);
        store.connection().execute_batch("DROP TRIGGER reject_chunk;").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(store.search_files("replacement", &[], 10).unwrap().len(), 1);
    }

    #[test]
    fn bounded_text_budget_wait_is_cancellable_and_permits_release_on_error() {
        let budget = PreparationBudget::new(10);
        let first = budget.reserve(7, || false).unwrap();
        let result = budget.reserve(4, || true);
        assert!(result.is_err());
        assert_eq!(*budget.used.lock().unwrap(), 7);
        drop(first);
        let second = budget.reserve(10, || false).unwrap();
        assert_eq!(*budget.used.lock().unwrap(), 10);
        drop(second);
        assert_eq!(*budget.used.lock().unwrap(), 0);
    }

    #[test]
    fn parallel_queries_preserve_every_score_order_and_cache_only_metadata() {
        let (_dir, store, root) = setup();
        for i in 0..32 {
            fs::write(root.join(format!("{i:02}.txt")), format!(
                "anchor alpha {} marker{}", if i % 2 == 0 { "beta" } else { "gamma" }, i % 17
            )).unwrap();
        }
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let terms: Vec<String> = ["alpha".to_owned(), "beta".to_owned(), "gamma".to_owned()]
            .into_iter().chain((0..17).map(|i| format!("marker{i}")))
            .map(|term| text::quote(&term)).collect();
        let expression = terms.join(" OR ");
        let expected: Vec<_> = terms.iter().map(|term| {
            store.candidate_query(&expression, Some(term), &[], &[], &[2, 1], 10).unwrap()
        }).collect();
        store.query_cache.lock().unwrap().entries.clear();
        let actual = store.candidate_queries(&expression, &terms, &[], &[], &[1, 2], 10).unwrap();
        assert_eq!(serde_json::to_value(&actual).unwrap(), serde_json::to_value(&expected).unwrap());
        let hits_before = store.query_cache.lock().unwrap().hits;
        let repeated = store.candidate_queries(&expression, &terms, &[], &[], &[2, 1], 10).unwrap();
        assert_eq!(serde_json::to_value(repeated).unwrap(), serde_json::to_value(expected).unwrap());
        let cache = store.query_cache.lock().unwrap();
        assert_eq!(cache.hits - hits_before, terms.len());
        assert!(cache.entries.values().flatten().all(|candidate| candidate.content.is_empty()));
        drop(cache);
        assert!(store.candidate_queries("\"unterminated", &terms, &[], &[], &[], 10).is_err());
    }

    #[test]
    fn query_cache_distinguishes_scope_references_required_terms_and_limit() {
        let (_dir, store, root) = setup();
        let first = root.join("first.txt");
        let second = root.join("second.txt");
        fs::write(&first, "alpha common").unwrap();
        fs::write(&second, "beta common").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let expression = text::match_query("alpha beta common");
        let scope = vec![path_text(&first).unwrap()];
        let references = vec![path_text(&second).unwrap()];
        let all = store.candidate_query(&expression, None, &[], &[], &[], 100).unwrap();
        let one = store.candidate_query(&expression, None, &scope, &[], &[], 100).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].path, scope[0]);
        assert_eq!(store.candidate_query(&expression, None, &scope, &references, &[], 100).unwrap().len(), 2);
        assert_eq!(store.candidate_query(&expression, None, &[], &[], &[], 1).unwrap().len(), 1);
        assert_eq!(store.candidate_query(&expression, Some("\"alpha\""), &[], &[], &[], 100).unwrap().len(), 1);
        assert_eq!(store.candidate_query(&expression, None, &[], &[], &[all[0].id], 100).unwrap().len(), 1);
        let misses = store.query_cache.lock().unwrap().misses;
        let mut copy = store.candidate_query(&expression, None, &[], &[], &[], 100).unwrap();
        copy[0].bm25 = 12345.;
        assert_eq!(serde_json::to_value(store.candidate_query(&expression, None, &[], &[], &[], 100).unwrap()).unwrap(), serde_json::to_value(all).unwrap());
        assert_eq!(store.query_cache.lock().unwrap().misses, misses);
    }

    #[test]
    fn scoped_queries_keep_directory_boundaries_and_refresh_after_index_changes() {
        let (dir, store, root) = setup();
        let first = root.join("first.txt");
        fs::write(&first, "alpha evidence").unwrap();
        let root_path = path_text(&root).unwrap();
        store.scan(&root_path, |_| {}).unwrap();
        let scope = vec![root_path.clone()];
        let expression = text::match_query("alpha");
        let scoped = store.candidate_query(&expression, None, &scope, &[], &[], 10).unwrap();
        let all = store.candidate_query(&expression, None, &[], &[], &[], 10).unwrap();
        assert_eq!(serde_json::to_value(&scoped).unwrap(), serde_json::to_value(&all).unwrap());

        let sibling = root.with_file_name(format!("{}-other", root.file_name().unwrap().to_string_lossy()));
        fs::create_dir(&sibling).unwrap();
        let second = sibling.join("second.txt");
        fs::write(&second, "alpha outside evidence").unwrap();
        let writer = Store::open(&dir.path().join("app/db.sqlite")).unwrap();
        let sibling_path = writer.add_root(&sibling).unwrap();
        writer.scan(&sibling_path, |_| {}).unwrap();
        let only_first = store.candidate_query(&expression, None, &scope, &[], &[], 10).unwrap();
        assert_eq!(only_first.len(), 1);
        assert_eq!(only_first[0].path, path_text(&first).unwrap());
        let references = vec![path_text(&second).unwrap()];
        assert_eq!(store.candidate_query(&expression, None, &scope, &references, &[], 10).unwrap().len(), 2);

        let added = root.join("added.txt");
        fs::write(&added, "alpha added evidence").unwrap();
        writer.scan(&root_path, |_| {}).unwrap();
        assert_eq!(store.candidate_query(&expression, None, &scope, &[], &[], 10).unwrap().len(), 2);
        writer.remove_indexed(&path_text(&first).unwrap()).unwrap();
        let terms = vec![expression.clone()];
        let remaining = store.candidate_queries(&expression, &terms, &scope, &[], &[], 10).unwrap();
        assert_eq!(remaining[0].len(), 1);
        assert_eq!(remaining[0][0].path, path_text(&added).unwrap());
        let missing = vec![path_text(&root.join("missing")).unwrap()];
        assert!(!store.has_candidate_match(&expression, &missing, &[], &[]).unwrap());
        assert!(store.candidate_query(&expression, None, &missing, &[], &[], 10).unwrap().is_empty());
    }

    #[test]
    fn query_cache_refreshes_after_another_store_updates_or_removes_documents() {
        let (dir, store, root) = setup();
        let source = root.join("source.txt");
        fs::write(&source, "alpha original source").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let expression = text::match_query("alpha");
        assert_eq!(store.candidate_query(&expression, None, &[], &[], &[], 100).unwrap().len(), 1);
        fs::write(&source, "beta replacement source").unwrap();
        // Even a cache hit must validate the original file; a changed file is left out.
        assert!(store.candidate_query(&expression, None, &[], &[], &[], 100).unwrap().is_empty());
        let writer = Store::open(&dir.path().join("app/db.sqlite")).unwrap();
        writer.update_paths(root.to_str().unwrap(), &[source.clone()], |_| {}).unwrap();
        assert!(store.candidate_query(&expression, None, &[], &[], &[], 100).unwrap().is_empty());
        let beta = text::match_query("beta");
        assert_eq!(store.candidate_query(&beta, None, &[], &[], &[], 100).unwrap()[0].content, "beta replacement source");
        writer.remove_indexed(source.to_str().unwrap()).unwrap();
        assert!(store.candidate_query(&beta, None, &[], &[], &[], 100).unwrap().is_empty());
    }

    #[test]
    fn query_cache_is_bounded_and_older_readers_cannot_replace_newer_generation() {
        let mut cache = QueryCache::default();
        let mut key = CandidateQuery { expression: "alpha".into(), required: None,
            scope: vec![], references: vec![], excluded: vec![], limit: 0 };
        for limit in 0..=QUERY_CACHE_ENTRIES {
            key.limit = limit;
            assert!(cache.get(1, &key).is_none());
            cache.insert(1, key.clone(), vec![]);
        }
        assert_eq!(cache.entries.len(), QUERY_CACHE_ENTRIES);
        key.limit = 0;
        assert!(!cache.entries.contains_key(&key));
        key.limit = 1;
        assert!(cache.get(1, &key).is_some());
        key.limit = QUERY_CACHE_ENTRIES + 1;
        cache.insert(1, key.clone(), vec![]);
        key.limit = 2;
        assert!(!cache.entries.contains_key(&key));
        cache.get(2, &key);
        cache.insert(2, key.clone(), vec![]);
        assert!(cache.get(1, &key).is_none());
        cache.insert(1, key, vec![]);
        assert_eq!(cache.version, Some(2));
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn unchanged_events_skip_text_preparation_and_refresh_file_metadata() {
        let (_dir, store, root) = setup();
        let path = root.join("unchanged.md");
        fs::write(&path, "# Guide\nStable searchable content.").unwrap();
        store.index_file(root.to_str().unwrap(), &path, 512).unwrap();
        let version = store.version().unwrap();
        let modified = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        fs::File::options().write(true).open(&path).unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified)).unwrap();
        assert!(store.prepare_file(&path, 512, true).unwrap().is_none());
        let saved: i64 = store.connection().query_row(
            "SELECT mtime FROM documents WHERE path=?", [path_text(&path).unwrap()], |row| row.get(0),
        ).unwrap();
        assert_eq!(saved, 1_700_000_000_000_000_000);
        store.update_paths(root.to_str().unwrap(), &[path], |_| {}).unwrap();
        assert_eq!(store.version().unwrap(), version);
        assert_eq!(store.search_files("searchable", &[], 10).unwrap().len(), 1);
    }

    #[test]
    fn manual_reindex_checks_all_roots_and_preserves_unchanged_chunks() {
        let (dir, store, root) = setup();
        let other = dir.path().join("other");
        fs::create_dir(&other).unwrap();
        let other = other.canonicalize().unwrap();
        store.add_root(&other).unwrap();
        let stable = root.join("stable.md");
        let changed = other.join("changed.md");
        fs::write(&stable, "stablemarker original text").unwrap();
        fs::write(&changed, "beforemarker").unwrap();
        for path in [&root, &other] { store.scan(path.to_str().unwrap(), |_| {}).unwrap(); }
        let stable_chunk = store.search_files("stablemarker", &[], 10).unwrap()[0].id;
        let modified = fs::metadata(&changed).unwrap().modified().unwrap();
        fs::write(&changed, "afterxmarker").unwrap();
        std::fs::File::options().write(true).open(&changed).unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified)).unwrap();
        let roots = store.prepare_reindex().unwrap();
        assert_eq!(roots.len(), 2);
        assert!(store.roots().unwrap().iter().all(|root| root.status == "pending"));
        assert!(store.prepare_reindex().is_err());
        assert_eq!(store.search_files("stablemarker", &[], 10).unwrap()[0].id, stable_chunk);
        for (index, root) in roots.into_iter().enumerate() {
            crate::indexer::apply_request(&store, crate::indexer::IndexRequest::Scan { root, verify: true }, |_| {}).unwrap();
            if index == 0 { assert!(store.prepare_reindex().is_err()); }
        }
        assert!(store.roots().unwrap().iter().all(|root| root.status == "ready"));
        assert_eq!(store.search_files("stablemarker", &[], 10).unwrap()[0].id, stable_chunk);
        assert_eq!(store.search_files("afterxmarker", &[], 10).unwrap().len(), 1);
        assert_eq!(fs::read_to_string(&stable).unwrap(), "stablemarker original text");
        assert_eq!(fs::read_to_string(&changed).unwrap(), "afterxmarker");
        assert_eq!(store.prepare_reindex().unwrap().len(), 2);
    }

    #[test]
    fn file_type_choices_default_to_documents_and_update_only_selected_indexes() {
        let (dir, store, root) = setup();
        let root_path = root.to_str().unwrap();
        let defaults: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(defaults.file_types, FileTypes { markdown: true, text: true, code: false, config: false });
        let partial: Settings = serde_json::from_str(r#"{"file_types":{"code":true}}"#).unwrap();
        assert!(partial.file_types.code);
        assert!(partial.file_types.markdown && partial.file_types.text && !partial.file_types.config);

        let names = ["notes.md", "notes.rst", "main.py", "page.HTML", "Makefile", "settings.json"];
        for name in names {
            fs::write(root.join(name), format!("filetypemarker {name}")).unwrap();
        }
        store.scan(root_path, |_| {}).unwrap();
        assert_eq!(store.search_files("filetypemarker", &[], 20).unwrap().len(), 2);
        let note_id = store.document(&root.join("notes.md")).unwrap().id;
        let mut settings = store.settings().unwrap();
        settings.file_types.code = true;
        settings.file_types.config = true;
        store.save_settings(&settings).unwrap();
        store.scan(root_path, |_| {}).unwrap();
        assert_eq!(store.search_files("filetypemarker", &[], 20).unwrap().len(), 6);
        settings.file_types.code = false;
        store.save_settings(&settings).unwrap();
        store.scan(root_path, |_| {}).unwrap();
        assert_eq!(store.search_files("filetypemarker", &[], 20).unwrap().len(), 3);
        assert_eq!(store.document(&root.join("notes.md")).unwrap().id, note_id);
        assert_eq!(store.entries(&root, true).unwrap().len(), 6);
        for name in ["main.py", "page.HTML", "Makefile"] {
            assert!(!store.index_file(root_path, &root.join(name), 1000).unwrap());
            store.update_paths(root_path, &[root.join(name)], |_| {}).unwrap();
        }
        assert_eq!(store.files().unwrap().len(), 3);
        drop(store);
        let store = Store::open(&dir.path().join("app/db.sqlite")).unwrap();
        assert!(!store.settings().unwrap().file_types.code);
        assert!(store.settings().unwrap().file_types.config);
        settings.file_types.code = true;
        store.save_settings(&settings).unwrap();
        store.scan(root_path, |_| {}).unwrap();
        assert_eq!(store.search_files("filetypemarker", &[], 20).unwrap().len(), 6);
        assert_eq!(store.document(&root.join("notes.md")).unwrap().id, note_id);
        for name in names {
            assert_eq!(fs::read_to_string(root.join(name)).unwrap(), format!("filetypemarker {name}"));
        }
        settings.file_types = FileTypes { markdown: false, text: false, code: false, config: false };
        store.save_settings(&settings).unwrap();
        store.scan(root_path, |_| {}).unwrap();
        assert!(store.files().unwrap().is_empty());
        assert_eq!(store.entries(&root, true).unwrap().len(), 6);
    }

    #[test]
    fn index_worker_setting_defaults_persists_and_validates() {
        let (_dir, store, _) = setup();
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.index_workers, 8);
        settings.index_workers = 14;
        store.save_settings(&settings).unwrap();
        assert_eq!(store.settings().unwrap().index_workers, 14);
        for workers in [0, 65] {
            settings.index_workers = workers;
            assert!(store.save_settings(&settings).is_err());
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn rtf_search_uses_visible_text_and_exact_citations_without_changing_source() {
        let (_dir, store, root) = setup();
        let path = root.join("中文资料.RTF");
        let bytes = br"{\rtf1\ansi\ansicpg1252\uc1{\fonttbl{\f0 metadataonlyfont;}}{\info{\title metadataonlytitle}}\f0\b Release\b0\par \u26143?\u27827? rtfmarker LYRA-73492\par caf\'e9 \u-10179?\u-8704?}";
        fs::write(&path, bytes).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let results = store.search_files("rtfmarker", &[], 10).unwrap();
        assert_eq!(results.len(), 1);
        let doc = store.document(&path).unwrap();
        assert_eq!(doc.body, "Release\n星河 rtfmarker LYRA-73492\ncafé 😀");
        let hit = &results[0];
        assert_eq!(hit.content, hit.source_spans.iter().map(|s| &doc.body[s.start_byte..s.end_byte]).collect::<Vec<_>>().join("\n\n"));
        assert!(store.search_files("metadataonlytitle metadataonlyfont", &[], 10).unwrap().is_empty());
        assert!(store.save_document(&path, "do not write", Some(&doc.hash)).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn long_rtf_citations_and_incremental_changes_use_the_same_body() {
        let (_dir, store, root) = setup();
        let path = root.join("long.rtf");
        let original = format!("章节一\n{} rtfdeepmarker RELEASE-73492 {}\n章节二", "长段落😀。".repeat(1200), "结尾文字。".repeat(200));
        let encoded = original.encode_utf16().map(|unit| {
            if unit == 10 { "\\par\n".to_owned() } else { format!("\\u{}?", unit as i16) }
        }).collect::<String>();
        let bytes = format!("{{\\rtf1\\ansi\\uc1 {encoded}}}");
        fs::write(&path, &bytes).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let doc = store.document(&path).unwrap();
        assert_eq!(doc.body, original);
        let results = store.search_files("rtfdeepmarker", &[], 10).unwrap();
        assert_eq!(results.len(), 1);
        let hit = &results[0];
        assert!(hit.content.contains("RELEASE-73492"));
        for span in &hit.source_spans {
            assert_eq!(&doc.body[span.start_byte..span.end_byte], hit.content);
            assert_eq!(span.start_line, 2);
            let line_start = doc.body.find('\n').unwrap() + 1;
            assert_eq!(span.start_column, doc.body[line_start..span.start_byte].chars().count() + 1);
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), bytes);

        fs::write(&path, br"{\rtf1\ansi updatedrtfmarker}").unwrap();
        store.update_paths(root.to_str().unwrap(), &[path.clone()], |_| {}).unwrap();
        assert!(store.search_files("rtfdeepmarker", &[], 10).unwrap().is_empty());
        assert_eq!(store.search_files("updatedrtfmarker", &[], 10).unwrap().len(), 1);
        let renamed = root.join("renamed.rtf");
        fs::rename(&path, &renamed).unwrap();
        store.update_paths(root.to_str().unwrap(), &[path, renamed.clone()], |_| {}).unwrap();
        assert_eq!(store.search_files("updatedrtfmarker", &[], 10).unwrap()[0].path, renamed.to_str().unwrap());
        fs::remove_file(&renamed).unwrap();
        store.update_paths(root.to_str().unwrap(), &[renamed], |_| {}).unwrap();
        assert!(store.search_files("updatedrtfmarker", &[], 10).unwrap().is_empty());
    }

    #[test]
    fn encoded_documents_keep_search_previews_and_citations_aligned() {
        let (_dir, store, root) = setup();
        let cases = [
            (encoding_rs::UTF_16LE, "", "中文文档与原文引用，支持长段落。", true),
            (encoding_rs::UTF_16BE, "", "中文文档与原文引用，支持长段落。", true),
            (encoding_rs::GB18030, "", "中文文档与原文引用，支持长段落。", false),
            (encoding_rs::BIG5, "# coding: big5\n", "中文文件與原文引用，支援長段落。", false),
            (encoding_rs::SHIFT_JIS, "# coding: shift_jis\n", "日本語の文章と引用を正確に表示します。", false),
            (encoding_rs::WINDOWS_1252, "<meta charset=windows-1252>\n", "Résumé café déjà vu. ", false),
        ];
        for (i, (encoding, header, sentence, bom)) in cases.into_iter().enumerate() {
            let content = format!("{header}\n{}\n\nencodingmarker{i} {}", sentence.repeat(300), sentence.repeat(10));
            let bytes = if encoding == encoding_rs::UTF_16LE {
                [vec![0xff, 0xfe], content.encode_utf16().flat_map(u16::to_le_bytes).collect()].concat()
            } else if encoding == encoding_rs::UTF_16BE {
                [vec![0xfe, 0xff], content.encode_utf16().flat_map(u16::to_be_bytes).collect()].concat()
            } else {
                let (bytes, _, errors) = encoding.encode(&content);
                assert!(!errors);
                assert!(!bom);
                bytes.into_owned()
            };
            let path = root.join(format!("encoded-{i}.txt"));
            fs::write(&path, &bytes).unwrap();
            store.scan(root.to_str().unwrap(), |_| {}).unwrap();
            assert_eq!(store.document(&path).unwrap().body, content, "{}", encoding.name());
            let results = store.search_files(&format!("encodingmarker{i}"), &[], 10).unwrap();
            let hit = results.iter().find(|hit| hit.path == path.to_str().unwrap()).unwrap();
            let cited = hit.source_spans.iter().map(|span| &content[span.start_byte..span.end_byte]).collect::<Vec<_>>().join("\n\n");
            assert_eq!(hit.content, cited);
            assert!(hit.content.contains(&format!("encodingmarker{i}")));
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn saving_encoded_markdown_preserves_encoding_and_rejects_unmappable_characters() {
        let (_dir, store, root) = setup();
        for (i, encoding) in [encoding_rs::UTF_16LE, encoding_rs::UTF_16BE, encoding_rs::BIG5].into_iter().enumerate() {
            let content = "# coding: big5\r\n\r\n原文內容\r\n";
            let bytes = if encoding == encoding_rs::UTF_16LE {
                [vec![0xff, 0xfe], content.encode_utf16().flat_map(u16::to_le_bytes).collect()].concat()
            } else if encoding == encoding_rs::UTF_16BE {
                [vec![0xfe, 0xff], content.encode_utf16().flat_map(u16::to_be_bytes).collect()].concat()
            } else {
                encoding.encode(content).0.into_owned()
            };
            let path = root.join(format!("encoded-{i}.md"));
            fs::write(&path, &bytes).unwrap();
            let document = store.document(&path).unwrap();
            store.save_document(&path, content, Some(&document.hash)).unwrap();
            assert_eq!(fs::read(&path).unwrap(), bytes);
            let edited = content.replace("原文內容", "追加內容");
            let saved = store.save_document(&path, &edited, Some(&document.hash)).unwrap();
            assert_eq!(saved.body, edited);
            let output = fs::read(&path).unwrap();
            if encoding == encoding_rs::BIG5 {
                assert_eq!(output, encoding.encode(&edited).0.as_ref());
                assert!(store.save_document(&path, "# coding: big5\n😀", Some(&saved.hash)).is_err());
                assert_eq!(fs::read(&path).unwrap(), output);
            } else {
                assert_eq!(&output[..2], &bytes[..2]);
            }
        }
    }

    #[test]
    fn progress_counts_only_files_in_the_current_index_task() {
        let (_dir, store, root) = setup();
        let a = root.join("a.txt");
        let removed = root.join("removed.txt");
        fs::write(&a, "original").unwrap();
        fs::write(&removed, "removed").unwrap();
        let updates = std::cell::RefCell::new(Vec::new());
        store.scan(root.to_str().unwrap(), |p| updates.borrow_mut().push(p)).unwrap();
        assert_eq!(updates.borrow().first(), Some(&IndexProgress { count: 0, total: 2 }));
        assert_eq!(updates.borrow().last(), Some(&IndexProgress { count: 2, total: 2 }));
        fs::write(&a, "changedmarker").unwrap();
        fs::remove_file(&removed).unwrap();
        let sub = root.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("first.txt"), "first").unwrap();
        fs::write(sub.join("second.txt"), "second").unwrap();
        fs::write(root.join("unrelated.txt"), "unrelatedmarker").unwrap();
        updates.borrow_mut().clear();
        store.update_paths(root.to_str().unwrap(), &[a, removed, sub], |p| updates.borrow_mut().push(p)).unwrap();
        let updates = updates.into_inner();
        assert_eq!(updates.first(), Some(&IndexProgress { count: 0, total: 4 }));
        assert_eq!(updates.last(), Some(&IndexProgress { count: 4, total: 4 }));
        assert!(updates.windows(2).all(|p| p[0].count <= p[1].count && p[0].total == p[1].total));
        assert!(store.search_files("unrelatedmarker", &[], 10).unwrap().is_empty());
    }

    #[test]
    fn hidden_subdirectories_are_skipped_in_scans_and_path_updates() {
        let (_dir, store, root) = setup();
        let hidden = root.join(".cache/nested");
        fs::create_dir_all(&hidden).unwrap();
        let hidden_file = hidden.join("secret.md");
        fs::write(&hidden_file, "hiddenmarker").unwrap();
        fs::write(root.join("visible.md"), "visiblemarker").unwrap();
        fs::write(root.join(".note.md"), "dotfilemarker").unwrap();
        let mut settings = store.settings().unwrap();
        settings.show_hidden = true;
        store.save_settings(&settings).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert!(store.search_files("hiddenmarker", &[], 10).unwrap().is_empty());
        assert_eq!(store.search_files("visiblemarker", &[], 10).unwrap().len(), 1);
        assert_eq!(store.search_files("dotfilemarker", &[], 10).unwrap().len(), 1);
        store.update_paths(root.to_str().unwrap(), &[hidden_file, root.join(".cache")], |_| {}).unwrap();
        assert!(store.search_files("hiddenmarker", &[], 10).unwrap().is_empty());
        let visible = root.join("visible");
        fs::rename(root.join(".cache"), &visible).unwrap();
        store.update_paths(root.to_str().unwrap(), &[root.join(".cache"), visible.clone()], |_| {}).unwrap();
        assert_eq!(store.search_files("hiddenmarker", &[], 10).unwrap().len(), 1);
        fs::rename(&visible, root.join(".cache")).unwrap();
        store.update_paths(root.to_str().unwrap(), &[visible, root.join(".cache")], |_| {}).unwrap();
        assert!(store.search_files("hiddenmarker", &[], 10).unwrap().is_empty());
    }

    #[test]
    fn adding_a_parent_removes_previously_indexed_hidden_subdirectories() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("files");
        let hidden = parent.join(".hidden");
        fs::create_dir_all(&hidden).unwrap();
        let source = hidden.join("source.md");
        fs::write(&source, "legacyhiddenmarker").unwrap();
        fs::write(hidden.join("broken.txt"), [0xff, 0xfe, 0x00]).unwrap();
        let store = Store::open(&dir.path().join("app/index.sqlite")).unwrap();
        let explicit_root = store.add_root(&hidden).unwrap();
        store.scan(&explicit_root, |_| {}).unwrap();
        assert_eq!(store.search_files("legacyhiddenmarker", &[], 10).unwrap().len(), 1);
        assert_eq!(store.roots().unwrap()[0].status, "partial");
        let parent_root = store.add_root(&parent).unwrap();
        store.scan(&parent_root, |_| {}).unwrap();
        assert!(store.search_files("legacyhiddenmarker", &[], 10).unwrap().is_empty());
        assert_eq!(store.roots().unwrap()[0].status, "ready");
        assert!(store.roots().unwrap()[0].error.is_none());
        assert_eq!(fs::read_to_string(source).unwrap(), "legacyhiddenmarker");
    }

    #[test]
    fn parallel_scan_continues_after_invalid_document_and_tracks_recovery() {
        let (_dir, store, root) = setup();
        let path = root.join("broken.txt");
        let bytes = [0xff, 0xfe, 0x00];
        fs::write(&path, bytes).unwrap();
        for i in 0..270 {
            fs::write(root.join(format!("valid-{i:03}.txt")), format!("indexvalidmarker document {i}")).unwrap();
        }
        assert!(store.scan(root.to_str().unwrap(), |_| {}).is_ok());
        let state = &store.roots().unwrap()[0];
        assert_eq!(state.status, "partial");
        assert!(state.error.as_ref().unwrap().contains(path.to_str().unwrap()));
        assert!(state.error.as_ref().unwrap().contains("编码"));
        assert_eq!(store.search_files("indexvalidmarker", &[], 300).unwrap().len(), 270);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let other = root.join("later.txt");
        fs::write(&other, "latermarker").unwrap();
        store.update_paths(root.to_str().unwrap(), &[other], |_| {}).unwrap();
        assert_eq!(store.roots().unwrap()[0].status, "partial");
        fs::write(&path, "recoveredmarker").unwrap();
        store.update_paths(root.to_str().unwrap(), &[path.clone()], |_| {}).unwrap();
        assert_eq!(store.roots().unwrap()[0].status, "ready");
        assert_eq!(store.search_files("recoveredmarker", &[], 10).unwrap().len(), 1);
        fs::write(&path, bytes).unwrap();
        store.update_paths(root.to_str().unwrap(), &[path.clone()], |_| {}).unwrap();
        assert_eq!(store.roots().unwrap()[0].status, "partial");
        // A changed file is left out of results instead of failing the whole search.
        assert!(store.search_files("recoveredmarker", &[], 10).unwrap().is_empty());
        fs::remove_file(&path).unwrap();
        store.update_paths(root.to_str().unwrap(), &[path], |_| {}).unwrap();
        assert_eq!(store.roots().unwrap()[0].status, "ready");
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_subdirectory_is_reported_and_the_rest_of_the_folder_is_indexed() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 { return; }
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store = Store::open(&temp.path().join("index.sqlite")).unwrap();
        let root_text = store.add_root(&root).unwrap();
        fs::write(root.join("open.md"), "readablemarker").unwrap();
        let locked = root.join("locked");
        fs::create_dir(&locked).unwrap();
        fs::write(locked.join("inside.md"), "lockedmarker").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let result = store.scan(&root_text, |_| {});
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        result.unwrap();
        assert_eq!(store.search_files("readablemarker", &[], 10).unwrap().len(), 1);
        let state = &store.roots().unwrap()[0];
        assert_eq!(state.status, "partial");
        assert!(state.error.as_ref().unwrap().contains("locked"), "{:?}", state.error);
        // Once the directory becomes readable again a rescan clears the issue.
        store.scan(&root_text, |_| {}).unwrap();
        assert_eq!(store.roots().unwrap()[0].status, "ready");
        assert_eq!(store.search_files("lockedmarker", &[], 10).unwrap().len(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn an_unreadable_sagignore_never_widens_what_gets_indexed() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 { return; }
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store = Store::open(&temp.path().join("index.sqlite")).unwrap();
        let root_text = store.add_root(&root).unwrap();
        let private = root.join("private");
        fs::create_dir(&private).unwrap();
        fs::write(private.join("salary.md"), "salarymarker").unwrap();
        fs::write(private.join("notes.md"), "notesmarker").unwrap();
        let rules = private.join(".sagignore");
        fs::write(&rules, "salary.md\n").unwrap();
        store.scan(&root_text, |_| {}).unwrap();
        assert!(store.search_files("salarymarker", &[], 10).unwrap().is_empty());
        assert_eq!(store.search_files("notesmarker", &[], 10).unwrap().len(), 1);
        // The rules file becomes unreadable while the directory stays readable.
        fs::set_permissions(&rules, fs::Permissions::from_mode(0o000)).unwrap();
        let full = store.scan(&root_text, |_| {});
        let partial = store.update_paths(&root_text, &[private.join("salary.md")], |_| {});
        fs::set_permissions(&rules, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(full.is_err(), "a full scan must fail closed when a rules file cannot be read");
        assert!(partial.is_err(), "a targeted update must fail closed when a rules file cannot be read");
        assert!(store.search_files("salarymarker", &[], 10).unwrap().is_empty());
    }
    #[test]
    fn file_index_issues_persist_and_clear_when_the_directory_is_excluded() {
        let (dir, store, root) = setup();
        let mut settings = store.settings().unwrap();
        settings.file_types.code = true;
        store.save_settings(&settings).unwrap();
        let dependency = root.join("dependencies");
        fs::create_dir(&dependency).unwrap();
        let file = dependency.join("fixture.py");
        fs::write(&file, [0xff, 0xfe, 0x00]).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        drop(store);
        let store = Store::open(&dir.path().join("app/db.sqlite")).unwrap();
        assert!(store.roots().unwrap()[0].error.as_ref().unwrap().contains(file.to_str().unwrap()));
        let mut settings = store.settings().unwrap();
        settings.excluded.push("dependencies".into());
        store.save_settings(&settings).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(store.roots().unwrap()[0].status, "ready");
        assert!(store.roots().unwrap()[0].error.is_none());
        assert_eq!(fs::read(file).unwrap(), [0xff, 0xfe, 0x00]);
    }

    #[test]
    fn database_errors_still_fail_the_indexing_task() {
        let (_dir, store, root) = setup();
        fs::write(root.join("valid.txt"), "valid text").unwrap();
        store.connection().execute("DROP TABLE documents", []).unwrap();
        assert!(store.scan(root.to_str().unwrap(), |_| {}).is_err());
        let db = store.connection();
        let status: String = db.query_row("SELECT status FROM roots", [], |row| row.get(0)).unwrap();
        assert_eq!(status, "error");
        let issues: i64 = db.query_row("SELECT COUNT(*) FROM index_issues", [], |row| row.get(0)).unwrap();
        assert_eq!(issues, 0);
    }

    #[test]
    fn external_index_does_not_store_original_or_tokenized_text() {
        let (_dir, store, root) = setup();
        let file = root.join("source.txt");
        let source = "External original text with searchable evidence.";
        fs::write(&file, source).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let db = store.connection();
        let has_body: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('documents') WHERE name='body')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!has_body);
        let stored_text: Option<String> = db
            .query_row("SELECT body FROM chunk_fts LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored_text, None);
        drop(db);
        let hits = store
            .candidate_query(&text::match_query("searchable"), None, &[], &[], &[], 100)
            .unwrap();
        assert_eq!(hits[0].content, source);
    }

    #[test]
    fn html_image_data_never_enters_search_or_retrieval_vocabulary() {
        let (_dir, store, root) = setup();
        let mut settings = store.settings().unwrap();
        settings.file_types.code = true;
        store.save_settings(&settings).unwrap();
        let path = root.join("page.html");
        let source = "<h1>beforemarker</h1><img src=\"data:image/png;base64,imagepayloadmarker\" alt=\"图像说明\"><p>aftermarker 正常正文</p>";
        fs::write(&path, source).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert!(store.search_files("imagepayloadmarker", &[], 10).unwrap().is_empty());
        for query in ["beforemarker", "aftermarker", "正常正文", "图像说明"] {
            let hits = store.candidate_query(&text::match_query(query), None, &[], &[], &[], 10).unwrap();
            assert!(!hits.is_empty(), "missing {query}");
            assert!(hits.iter().all(|hit| !hit.content.contains("imagepayloadmarker")));
        }
        let vocabulary: String = store.connection().query_row("SELECT terms FROM retrieval_vocabulary", [], |row| row.get(0)).unwrap();
        assert!(!vocabulary.contains("imagepayloadmarker"));
        assert!(store.retrieval_corpus(&[], &[]).unwrap().iter().all(|(_, text)| !text.contains("imagepayloadmarker")));
        store.connection().execute("DELETE FROM retrieval_vocabulary", []).unwrap();
        store.retrieval_statistics(&[], &[], &tokio_util::sync::CancellationToken::new()).unwrap();
        let vocabulary: String = store.connection().query_row("SELECT terms FROM retrieval_vocabulary", [], |row| row.get(0)).unwrap();
        assert!(!vocabulary.contains("imagepayloadmarker"));
        assert_eq!(store.document(&path).unwrap().body, source);
        assert_eq!(fs::read_to_string(path).unwrap(), source);
    }

    #[test]
    fn embedded_resource_upgrade_reindexes_documents_once() {
        let (dir, store, root) = setup();
        let old_path = root.join("page.txt");
        let html_path = root.join("page.html");
        fs::write(&old_path, "<p>bodymarker</p><img src='data:image/png;base64,imagepayloadmarker'>").unwrap();
        fs::write(root.join("keep.txt"), "unaffectedmarker").unwrap();
        let mut settings = store.settings().unwrap();
        settings.file_types.code = true;
        settings.retrieval.chunk_chars = 900;
        store.save_settings(&settings).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(store.search_files("imagepayloadmarker", &[], 10).unwrap().len(), 1);
        let keep_id = store.search_files("unaffectedmarker", &[], 10).unwrap()[0].id;
        fs::rename(&old_path, &html_path).unwrap();
        store.connection().execute("UPDATE documents SET path=?,name='page.html' WHERE path=?", params![path_text(&html_path).unwrap(),path_text(&old_path).unwrap()]).unwrap();
        store.connection().pragma_update(None, "user_version", 7).unwrap();
        drop(store);
        let upgraded = Store::open(&dir.path().join("app/db.sqlite")).unwrap();
        assert!(upgraded.search_files("imagepayloadmarker", &[], 10).unwrap().is_empty());
        assert_eq!(upgraded.search_files("unaffectedmarker", &[], 10).unwrap()[0].id, keep_id);
        assert_eq!(upgraded.chunk_count().unwrap(), 1);
        assert_eq!(upgraded.settings().unwrap().retrieval.chunk_chars, 900);
        assert_eq!(upgraded.roots().unwrap()[0].status, "pending");
        upgraded.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(upgraded.search_files("bodymarker", &[], 10).unwrap().len(), 1);
        assert!(upgraded.search_files("imagepayloadmarker", &[], 10).unwrap().is_empty());
        let count = upgraded.chunk_count().unwrap();
        drop(upgraded);
        let reopened = Store::open(&dir.path().join("app/db.sqlite")).unwrap();
        assert_eq!(reopened.chunk_count().unwrap(), count);
        assert_eq!(reopened.roots().unwrap()[0].status, "ready");
    }

    #[test]
    fn markdown_resource_upgrade_clears_old_payloads_and_keeps_original_files() {
        let (dir, store, root) = setup();
        let old_path = root.join("legacy.txt");
        let markdown_path = root.join("legacy.md");
        let body = "正文marker\n<svg><text>svgpayloadmarker</text></svg>\n![图片说明](data:image/png;base64,imagepayloadmarker)\n[附件](data:application/pdf;base64,attachmentpayloadmarker)";
        fs::write(&old_path, body).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(store.search_files("imagepayloadmarker", &[], 10).unwrap().len(), 1);
        fs::rename(&old_path, &markdown_path).unwrap();
        store.connection().execute("UPDATE documents SET path=?,name='legacy.md' WHERE path=?", params![path_text(&markdown_path).unwrap(),path_text(&old_path).unwrap()]).unwrap();
        store.connection().pragma_update(None, "user_version", 8).unwrap();
        drop(store);
        let upgraded = Store::open(&dir.path().join("app/db.sqlite")).unwrap();
        upgraded.scan(root.to_str().unwrap(), |_| {}).unwrap();
        for marker in ["imagepayloadmarker", "svgpayloadmarker", "attachmentpayloadmarker"] {
            assert!(upgraded.search_files(marker, &[], 10).unwrap().is_empty(), "retained {marker}");
            assert!(upgraded.candidate_query(&text::match_query(marker), None, &[], &[], &[], 10).unwrap().is_empty());
        }
        assert_eq!(upgraded.search_files("正文marker", &[], 10).unwrap().len(), 1);
        assert_eq!(fs::read_to_string(&markdown_path).unwrap(), body);
        let count = upgraded.chunk_count().unwrap();
        drop(upgraded);
        let reopened = Store::open(&dir.path().join("app/db.sqlite")).unwrap();
        assert_eq!(reopened.chunk_count().unwrap(), count);
        assert_eq!(reopened.roots().unwrap()[0].status, "ready");
    }

    #[test]
    fn external_index_checks_source_changes_and_reads_only_selected_files() {
        let (_dir, store, root) = setup();
        let best = root.join("best.txt");
        let other = root.join("other.txt");
        fs::write(&best, "needle needle needle").unwrap();
        fs::write(&other, format!("needle {}", "filler ".repeat(50))).unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let expression = text::match_query("needle");
        assert_eq!(
            store
                .candidate_query(&expression, None, &[], &[], &[], 1)
                .unwrap()[0]
                .path,
            path_text(&best).unwrap()
        );
        fs::remove_file(&other).unwrap();
        assert_eq!(
            store
                .candidate_query(&expression, None, &[], &[], &[], 1)
                .unwrap()
                .len(),
            1
        );
        // The removed file drops out of a wider query without failing it.
        assert_eq!(
            store
                .candidate_query(&expression, None, &[], &[], &[], 100)
                .unwrap()
                .len(),
            1
        );
        fs::write(&best, "modified needle").unwrap();
        assert!(store
            .candidate_query(&expression, None, &[], &[], &[], 1)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn legacy_index_migration_preserves_user_data_and_reindexes_sources() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("notes");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let file = root.join("legacy.txt");
        let source = "Legacy document with migration evidence.";
        fs::write(&file, source).unwrap();
        let path = dir.path().join("legacy.sqlite");
        let old = Connection::open(&path).unwrap();
        old.execute_batch(r#"CREATE TABLE roots(path TEXT PRIMARY KEY,name TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'pending',error TEXT);
            CREATE TABLE documents(id INTEGER PRIMARY KEY AUTOINCREMENT,root TEXT NOT NULL,path TEXT UNIQUE NOT NULL,name TEXT NOT NULL,body TEXT NOT NULL,hash TEXT NOT NULL,mtime INTEGER NOT NULL,size INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE preferences(id INTEGER PRIMARY KEY CHECK(id=1),value TEXT NOT NULL);
            INSERT INTO preferences VALUES(1,'{"language":"en"}');
            CREATE TABLE conversations(id TEXT PRIMARY KEY,title TEXT NOT NULL,messages TEXT NOT NULL,updated INTEGER NOT NULL,deleted INTEGER NOT NULL DEFAULT 0);
            INSERT INTO conversations VALUES('kept','Saved conversation','[]',1,0);
            CREATE TABLE drafts(path TEXT PRIMARY KEY,value TEXT NOT NULL);
            PRAGMA user_version=3;"#).unwrap();
        old.execute(
            "INSERT INTO roots VALUES(?,'notes','ready',NULL)",
            [path_text(&root).unwrap()],
        )
        .unwrap();
        old.execute(
            "INSERT INTO documents VALUES(1,?,?,'legacy.txt',?,?,0,0)",
            params![
                path_text(&root).unwrap(),
                path_text(&file).unwrap(),
                source,
                hash(source.as_bytes())
            ],
        )
        .unwrap();
        let draft = Draft {
            path: path_text(&file).unwrap(),
            name: "legacy.txt".into(),
            body: "Unsaved changes".into(),
            hash: hash(source.as_bytes()),
            is_new: false,
        };
        old.execute(
            "INSERT INTO drafts VALUES(?,?)",
            params![draft.path, serde_json::to_string(&draft).unwrap()],
        )
        .unwrap();
        drop(old);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.settings().unwrap().language, "en");
        assert_eq!(
            store.conversations().unwrap()[0].title,
            "Saved conversation"
        );
        assert_eq!(store.drafts().unwrap()[0].body, "Unsaved changes");
        assert_eq!(store.files().unwrap()[0].path, path_text(&file).unwrap());
        assert_eq!(store.roots().unwrap()[0].status, "pending");
        assert_eq!(store.chunk_count().unwrap(), 0);
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        assert_eq!(
            store
                .candidate_query(&text::match_query("migration"), None, &[], &[], &[], 100)
                .unwrap()[0]
                .content,
            source
        );
        assert_eq!(fs::read_to_string(file).unwrap(), source);
        drop(store);
        let reopened = Store::open(&path).unwrap();
        assert!(reopened.chunk_count().unwrap() > 0);
        assert_eq!(reopened.roots().unwrap()[0].status, "ready");
    }
}
