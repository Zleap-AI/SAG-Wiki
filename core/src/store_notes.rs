//! Note bookkeeping inside the index: parsed anchors, their current state,
//! and re-verification whenever a cited file is indexed or removed.
use crate::{
    notes::{self, NoteHeader},
    store::{hash, now, path_text, Store},
};
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use sag_protocol::types::{NoteDetail, NoteSource, NoteSourceState, NoteSourceStatus, NoteSummary, SourceSpan};
use std::{fs, path::Path};

pub(crate) const SCHEMA: &str = "
 CREATE TABLE IF NOT EXISTS notes(document_id INTEGER PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,path TEXT NOT NULL,note_hash TEXT NOT NULL,question TEXT NOT NULL,asked_at INTEGER,model TEXT NOT NULL DEFAULT '',scope TEXT NOT NULL DEFAULT '[]',parse_error TEXT,checked_at INTEGER);
 CREATE INDEX IF NOT EXISTS notes_path ON notes(path);
 CREATE TABLE IF NOT EXISTS note_sources(note_id INTEGER NOT NULL REFERENCES notes(document_id) ON DELETE CASCADE,id INTEGER NOT NULL,path TEXT NOT NULL,display_path TEXT,hash TEXT NOT NULL,start_byte INTEGER NOT NULL,end_byte INTEGER NOT NULL,start_line INTEGER NOT NULL,end_line INTEGER NOT NULL,text TEXT NOT NULL,state TEXT NOT NULL DEFAULT 'unverified',reason TEXT,current_hash TEXT,current_start_byte INTEGER,current_end_byte INTEGER,current_start_line INTEGER,current_end_line INTEGER,similarity REAL,checked_at INTEGER,PRIMARY KEY(note_id,id));
 CREATE INDEX IF NOT EXISTS note_sources_path ON note_sources(path);
";

fn state_text(state: NoteSourceState) -> &'static str {
    match state {
        NoteSourceState::Intact => "intact",
        NoteSourceState::Moved => "moved",
        NoteSourceState::Changed => "changed",
        NoteSourceState::Missing => "missing",
        NoteSourceState::Unverified => "unverified",
    }
}
fn state_from(text: &str) -> NoteSourceState {
    match text {
        "intact" => NoteSourceState::Intact,
        "moved" => NoteSourceState::Moved,
        "changed" => NoteSourceState::Changed,
        "missing" => NoteSourceState::Missing,
        _ => NoteSourceState::Unverified,
    }
}

/// Outcome of checking one anchor against the current file content.
struct Check {
    state: NoteSourceState,
    reason: Option<String>,
    current_hash: Option<String>,
    current: Option<SourceSpan>,
    similarity: Option<f64>,
}

/// Reads the file ourselves: the index may lag the watcher, and a note's
/// state must follow the bytes on disk, not the last indexed snapshot.
fn check_source(source: &NoteSource, authorized: bool) -> Check {
    if !authorized {
        return Check { state: NoteSourceState::Unverified, reason: Some("来源不在知识库中".into()), current_hash: None, current: None, similarity: None };
    }
    let raw = match fs::read(&source.path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Check { state: NoteSourceState::Missing, reason: Some("文件已删除".into()), current_hash: None, current: None, similarity: None }
        }
        Err(error) => {
            return Check { state: NoteSourceState::Unverified, reason: Some(format!("无法读取文件：{error}")), current_hash: None, current: None, similarity: None }
        }
    };
    let current_hash = hash(&raw);
    if current_hash == source.hash {
        return Check {
            state: NoteSourceState::Intact,
            reason: None,
            current_hash: Some(current_hash),
            current: Some(SourceSpan { start_byte: source.start_byte, end_byte: source.end_byte, start_line: source.start_line, start_column: 1, end_line: source.end_line, end_column: 1 }),
            similarity: Some(1.),
        };
    }
    let body = match crate::document_text::read(Path::new(&source.path), &raw) {
        Ok(body) => body,
        Err(error) => {
            return Check { state: NoteSourceState::Unverified, reason: Some(error.to_string()), current_hash: Some(current_hash), current: None, similarity: None }
        }
    };
    let relocation = notes::relocate(source, &body);
    let current = (relocation.state != NoteSourceState::Missing)
        .then(|| notes::span_of(&body, relocation.start_byte, relocation.end_byte));
    let reason = match relocation.state {
        NoteSourceState::Missing => Some("当前文件中找不到引用的段落".into()),
        _ => None,
    };
    Check { state: relocation.state, reason, current_hash: Some(current_hash), current, similarity: relocation.similarity }
}

fn write_check(tx: &Connection, note_id: i64, id: usize, check: &Check, at: i64) -> Result<()> {
    tx.execute(
        "UPDATE note_sources SET state=?3,reason=?4,current_hash=?5,current_start_byte=?6,current_end_byte=?7,current_start_line=?8,current_end_line=?9,similarity=?10,checked_at=?11 WHERE note_id=?1 AND id=?2",
        params![
            note_id, id as i64, state_text(check.state), check.reason, check.current_hash,
            check.current.as_ref().map(|s| s.start_byte as i64), check.current.as_ref().map(|s| s.end_byte as i64),
            check.current.as_ref().map(|s| s.start_line as i64), check.current.as_ref().map(|s| s.end_line as i64),
            check.similarity, at
        ],
    )?;
    Ok(())
}

impl Store {
    /// Whether `path` lies inside a registered root and is not excluded.
    fn note_path_authorized(db: &Connection, path: &str) -> Result<bool> {
        let roots: Vec<String> = db.prepare_cached("SELECT path FROM roots")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        if !roots.iter().any(|root| Path::new(path).starts_with(root)) {
            return Ok(false);
        }
        Ok(!Self::path_excluded(db, path)?)
    }

    /// Called inside the index transaction after a document's chunks are written.
    /// Registers or refreshes the note when the body is one, and re-checks every
    /// anchor pointing at this path using the body already in memory.
    pub(crate) fn index_note_tx(tx: &Transaction<'_>, document_id: i64, path: &str, fingerprint: &str, body: &str) -> Result<()> {
        let at = now();
        if notes::is_note(body) {
            let note_dir = Path::new(path).parent().context("笔记缺少父目录")?;
            match notes::parse(body, note_dir) {
                Ok(header) => Self::write_note_tx(tx, document_id, path, fingerprint, &header, at)?,
                Err(error) => {
                    tx.execute("INSERT INTO notes(document_id,path,note_hash,question,parse_error,checked_at) VALUES(?1,?2,?3,'',?4,?5) ON CONFLICT(document_id) DO UPDATE SET path=excluded.path,note_hash=excluded.note_hash,parse_error=excluded.parse_error,checked_at=excluded.checked_at",
                        params![document_id, path, fingerprint, error.to_string(), at])?;
                    tx.execute("DELETE FROM note_sources WHERE note_id=?", [document_id])?;
                }
            }
        } else {
            tx.execute("DELETE FROM notes WHERE document_id=?", [document_id])?;
        }
        // Anchors into this file: verify against the bytes that were just indexed.
        let anchors: Vec<(i64, i64, String, usize, usize, usize, usize, String)> = tx
            .prepare("SELECT note_id,id,hash,start_byte,end_byte,start_line,end_line,text FROM note_sources WHERE path=?")?
            .query_map([path], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? as usize, r.get::<_, i64>(4)? as usize, r.get::<_, i64>(5)? as usize, r.get::<_, i64>(6)? as usize, r.get(7)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (note_id, id, anchor_hash, start_byte, end_byte, start_line, end_line, text) in anchors {
            let source = NoteSource { id: id as usize, path: path.into(), display_path: None, hash: anchor_hash, start_byte, end_byte, start_line, end_line, text };
            let check = if fingerprint == source.hash {
                Check { state: NoteSourceState::Intact, reason: None, current_hash: Some(fingerprint.into()), current: Some(SourceSpan { start_byte, end_byte, start_line, start_column: 1, end_line, end_column: 1 }), similarity: Some(1.) }
            } else {
                let relocation = notes::relocate(&source, body);
                let current = (relocation.state != NoteSourceState::Missing).then(|| notes::span_of(body, relocation.start_byte, relocation.end_byte));
                let reason = (relocation.state == NoteSourceState::Missing).then(|| "当前文件中找不到引用的段落".to_string());
                Check { state: relocation.state, reason, current_hash: Some(fingerprint.into()), current, similarity: relocation.similarity }
            };
            write_check(tx, note_id, source.id, &check, at)?;
        }
        Ok(())
    }

    fn write_note_tx(tx: &Transaction<'_>, document_id: i64, path: &str, fingerprint: &str, header: &NoteHeader, at: i64) -> Result<()> {
        tx.execute("INSERT INTO notes(document_id,path,note_hash,question,asked_at,model,scope,parse_error,checked_at) VALUES(?1,?2,?3,?4,?5,?6,?7,NULL,?8) ON CONFLICT(document_id) DO UPDATE SET path=excluded.path,note_hash=excluded.note_hash,question=excluded.question,asked_at=excluded.asked_at,model=excluded.model,scope=excluded.scope,parse_error=NULL,checked_at=excluded.checked_at",
            params![document_id, path, fingerprint, header.question, header.asked_at, header.model, serde_json::to_string(&header.scope)?, at])?;
        tx.execute("DELETE FROM note_sources WHERE note_id=?", [document_id])?;
        for source in &header.sources {
            let authorized = Self::note_path_authorized(tx, &source.path)?;
            let check = check_source(source, authorized);
            tx.execute("INSERT INTO note_sources(note_id,id,path,display_path,hash,start_byte,end_byte,start_line,end_line,text) VALUES(?,?,?,?,?,?,?,?,?,?)",
                params![document_id, source.id as i64, source.path, source.display_path, source.hash, source.start_byte as i64, source.end_byte as i64, source.start_line as i64, source.end_line as i64, source.text])?;
            write_check(tx, document_id, source.id, &check, at)?;
        }
        Ok(())
    }

    /// Anchors into removed paths become missing; nothing is deleted.
    pub(crate) fn mark_note_sources_removed_tx(tx: &Connection, path: &str, under: bool) -> Result<()> {
        let at = now();
        if under {
            let prefix = format!("{}{}", path.trim_end_matches(std::path::MAIN_SEPARATOR), std::path::MAIN_SEPARATOR);
            tx.execute("UPDATE note_sources SET state='missing',reason='文件已从知识库移除',checked_at=?3 WHERE path=?1 OR instr(path,?2)=1", params![path, prefix, at])?;
        } else {
            tx.execute("UPDATE note_sources SET state='missing',reason='文件已删除',checked_at=?2 WHERE path=?1", params![path, at])?;
        }
        Ok(())
    }

    /// Re-checks every anchor of the given notes against the files on disk.
    pub fn verify_notes(&self, paths: &[String]) -> Result<Vec<NoteSummary>> {
        let mut out = vec![];
        for path in paths {
            let path = path_text(&self.authorize(Path::new(path))?)?;
            let mut db = self.connection();
            let tx = db.transaction()?;
            let note_id: Option<i64> = tx.query_row("SELECT document_id FROM notes WHERE path=?", [&path], |r| r.get(0)).optional()?;
            let Some(note_id) = note_id else { bail!("不是已索引的笔记：{path}") };
            let sources = Self::note_sources_tx(&tx, note_id)?;
            let at = now();
            for status in sources {
                let authorized = Self::note_path_authorized(&tx, &status.source.path)?;
                let check = check_source(&status.source, authorized);
                write_check(&tx, note_id, status.source.id, &check, at)?;
            }
            tx.execute("UPDATE notes SET checked_at=? WHERE document_id=?", params![at, note_id])?;
            tx.commit()?;
            drop(db);
            out.push(self.note_summary(&path)?.context("笔记已不存在")?);
        }
        Ok(out)
    }

    fn note_sources_tx(db: &Connection, note_id: i64) -> Result<Vec<NoteSourceStatus>> {
        let mut query = db.prepare("SELECT id,path,display_path,hash,start_byte,end_byte,start_line,end_line,text,state,reason,current_hash,current_start_byte,current_end_byte,current_start_line,current_end_line,similarity,checked_at FROM note_sources WHERE note_id=? ORDER BY id")?;
        let rows = query.query_map([note_id], |r| {
            let current = match (r.get::<_, Option<i64>>(12)?, r.get::<_, Option<i64>>(13)?, r.get::<_, Option<i64>>(14)?, r.get::<_, Option<i64>>(15)?) {
                (Some(sb), Some(eb), Some(sl), Some(el)) => Some(SourceSpan { start_byte: sb as usize, end_byte: eb as usize, start_line: sl as usize, start_column: 1, end_line: el as usize, end_column: 1 }),
                _ => None,
            };
            Ok(NoteSourceStatus {
                source: NoteSource { id: r.get::<_, i64>(0)? as usize, path: r.get(1)?, display_path: r.get(2)?, hash: r.get(3)?, start_byte: r.get::<_, i64>(4)? as usize, end_byte: r.get::<_, i64>(5)? as usize, start_line: r.get::<_, i64>(6)? as usize, end_line: r.get::<_, i64>(7)? as usize, text: r.get(8)? },
                state: state_from(&r.get::<_, String>(9)?),
                reason: r.get(10)?,
                current_hash: r.get(11)?,
                current: current,
                current_text: None,
                similarity: r.get(16)?,
                checked_at: r.get(17)?,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn summary_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(NoteSummary, i64)> {
        let path: String = r.get(1)?;
        let title = Path::new(&path).file_name().map(|n| n.to_string_lossy().trim_end_matches(".md").to_owned()).unwrap_or_default();
        let scope: Vec<String> = serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or_default();
        let max_state: Option<String> = r.get(8)?;
        let parse_error: Option<String> = r.get(6)?;
        let state = if parse_error.is_some() { NoteSourceState::Unverified } else { max_state.as_deref().map(state_from).unwrap_or(NoteSourceState::Intact) };
        Ok((NoteSummary {
            path, title,
            question: r.get(2)?,
            asked_at: r.get(3)?,
            model: r.get(4)?,
            scope,
            state,
            source_count: r.get::<_, i64>(9)? as usize,
            attention: r.get::<_, i64>(10)? as usize,
            checked_at: r.get(7)?,
            parse_error,
        }, r.get(0)?))
    }
    const SUMMARY_SQL: &'static str = "SELECT n.document_id,n.path,n.question,n.asked_at,n.model,n.scope,n.parse_error,n.checked_at,
        (SELECT s.state FROM note_sources s WHERE s.note_id=n.document_id ORDER BY CASE s.state WHEN 'missing' THEN 4 WHEN 'changed' THEN 3 WHEN 'unverified' THEN 2 WHEN 'moved' THEN 1 ELSE 0 END DESC LIMIT 1),
        (SELECT COUNT(*) FROM note_sources s WHERE s.note_id=n.document_id),
        (SELECT COUNT(*) FROM note_sources s WHERE s.note_id=n.document_id AND s.state IN ('missing','changed','unverified'))
        FROM notes n";

    pub fn note_summary(&self, path: &str) -> Result<Option<NoteSummary>> {
        let db = self.connection();
        Ok(db.query_row(&format!("{} WHERE n.path=?", Self::SUMMARY_SQL), [path], |r| Self::summary_row(r).map(|(s, _)| s)).optional()?)
    }
    pub fn notes(&self, scope: Option<&str>) -> Result<Vec<NoteSummary>> {
        let db = self.connection();
        let mut query = db.prepare(&format!("{} WHERE (?1 IS NULL OR instr(replace(n.path,char(92),'/')||'/',?1)=1) ORDER BY n.asked_at DESC,n.path", Self::SUMMARY_SQL))?;
        let prefix = scope.map(|s| format!("{}/", s.replace('\\', "/").trim_end_matches('/')));
        let rows = query.query_map([prefix], |r| Self::summary_row(r).map(|(s, _)| s))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
    pub fn note(&self, path: &str) -> Result<NoteDetail> {
        let path = path_text(&self.authorize(Path::new(path))?)?;
        let db = self.connection();
        let (summary, note_id) = db.query_row(&format!("{} WHERE n.path=?", Self::SUMMARY_SQL), [&path], Self::summary_row).optional()?.context("不是已索引的笔记，或索引尚未完成")?;
        let mut sources = Self::note_sources_tx(&db, note_id)?;
        drop(db);
        // The current passage is read lazily here so the table stays small.
        let mut cache: std::collections::HashMap<String, Option<String>> = Default::default();
        for status in &mut sources {
            let Some(span) = &status.current else { continue };
            let body = cache.entry(status.source.path.clone()).or_insert_with(|| {
                fs::read(&status.source.path).ok().and_then(|raw| crate::document_text::read(Path::new(&status.source.path), &raw).ok())
            });
            if let Some(body) = body {
                if span.end_byte <= body.len() && body.is_char_boundary(span.start_byte) && body.is_char_boundary(span.end_byte) {
                    status.current_text = Some(body[span.start_byte..span.end_byte].to_owned());
                }
            }
        }
        Ok(NoteDetail { summary, sources })
    }

    /// Notes whose state should be surfaced in the file tree, keyed by path.
    pub fn note_states(&self) -> Result<Vec<(String, NoteSourceState)>> {
        let db = self.connection();
        let mut query = db.prepare(Self::SUMMARY_SQL)?;
        let rows = query.query_map([], |r| Self::summary_row(r).map(|(s, _)| (s.path, s.state)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Writes a new note file and indexes it immediately.
    pub fn write_note(&self, target: &Path, body: &str, expected_hash: Option<&str>) -> Result<crate::types::Document> {
        let document = self.save_document(target, body, expected_hash)?;
        let root = self.roots()?.into_iter().find(|r| Path::new(&document.path).starts_with(&r.path)).context("目录未加入")?;
        let settings = self.settings()?;
        self.index_file(&root.path, Path::new(&document.path), settings.retrieval.chunk_chars)?;
        Ok(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::NoteHeader;
    use std::fs;

    fn setup() -> (tempfile::TempDir, Store, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("kb");
        fs::create_dir_all(root.join("notes")).unwrap();
        let store = Store::open(&dir.path().join("app/db.sqlite")).unwrap();
        let mut settings = store.settings().unwrap();
        settings.file_types.code = true;
        store.save_settings(&settings).unwrap();
        store.add_root(&root).unwrap();
        (dir, store, root.canonicalize().unwrap())
    }
    fn index(store: &Store, root: &Path, path: &Path) {
        store.index_file(root.to_str().unwrap(), path, 1000).unwrap();
    }
    const PASSAGE: &str = "fn check_permission(user: &User) -> bool {\n    user.roles.contains(&Role::Admin)\n}";

    fn write_source(root: &Path) -> std::path::PathBuf {
        let file = root.join("guard.rs");
        fs::write(&file, format!("// header\n{PASSAGE}\n// footer\n")).unwrap();
        file
    }
    fn note_for(root: &Path, file: &Path) -> std::path::PathBuf {
        let raw = fs::read(file).unwrap();
        let body = String::from_utf8(raw.clone()).unwrap();
        let start = body.find(PASSAGE).unwrap();
        let header = NoteHeader {
            question: "权限在哪里校验".into(), asked_at: Some(1_761_370_800_000), model: "m".into(), scope: vec![root.to_string_lossy().into_owned()],
            sources: vec![NoteSource { id: 1, path: file.to_string_lossy().into_owned(), display_path: None, hash: hash(&raw), start_byte: start, end_byte: start + PASSAGE.len(), start_line: 2, end_line: 4, text: PASSAGE.into() }],
        };
        let note = root.join("notes/权限.md");
        fs::write(&note, notes::render(&header, note.parent().unwrap(), "在 guard 里 [1]。", false)).unwrap();
        note
    }

    #[test]
    fn indexing_a_note_records_anchors_and_tracks_source_changes() {
        let (_dir, store, root) = setup();
        let file = write_source(&root);
        let note = note_for(&root, &file);
        index(&store, &root, &file);
        index(&store, &root, &note);
        let detail = store.note(note.to_str().unwrap()).unwrap();
        assert_eq!(detail.summary.state, NoteSourceState::Intact);
        assert_eq!(detail.sources[0].current_text.as_deref(), Some(PASSAGE));
        assert_eq!(detail.summary.question, "权限在哪里校验");

        // Insert lines above: moved, with the new line numbers.
        fs::write(&file, format!("// header\n{}\n{PASSAGE}\n// footer\n", "x\n".repeat(20))).unwrap();
        index(&store, &root, &file);
        let detail = store.note(note.to_str().unwrap()).unwrap();
        assert_eq!(detail.sources[0].state, NoteSourceState::Moved);
        assert_eq!(detail.sources[0].current.as_ref().unwrap().start_line, 23);
        assert_eq!(detail.summary.state, NoteSourceState::Moved);
        assert_eq!(detail.summary.attention, 0);

        // Edit the passage: changed.
        fs::write(&file, "fn check_permission(user: &User, action: Action) -> bool {\n    user.roles.contains(&Role::Admin) || action.is_public()\n}\n").unwrap();
        index(&store, &root, &file);
        let detail = store.note(note.to_str().unwrap()).unwrap();
        assert_eq!(detail.sources[0].state, NoteSourceState::Changed);
        assert!(detail.sources[0].current_text.as_deref().unwrap().contains("action"));
        assert_eq!(detail.summary.attention, 1);

        // Delete the file: missing. Restoring the original brings it back to intact.
        let original = format!("// header\n{PASSAGE}\n// footer\n");
        fs::remove_file(&file).unwrap();
        store.remove_indexed(file.to_str().unwrap()).unwrap();
        assert_eq!(store.note(note.to_str().unwrap()).unwrap().sources[0].state, NoteSourceState::Missing);
        fs::write(&file, original).unwrap();
        index(&store, &root, &file);
        assert_eq!(store.note(note.to_str().unwrap()).unwrap().summary.state, NoteSourceState::Intact);
        assert_eq!(store.notes(None).unwrap().len(), 1);
        assert_eq!(store.notes(Some(root.join("notes").to_str().unwrap())).unwrap().len(), 1);
        assert!(store.notes(Some(root.join("other").to_str().unwrap())).unwrap().is_empty());
    }

    #[test]
    fn verify_reads_disk_before_the_watcher_and_broken_notes_report_their_error() {
        let (_dir, store, root) = setup();
        let file = write_source(&root);
        let note = note_for(&root, &file);
        index(&store, &root, &file);
        index(&store, &root, &note);
        fs::write(&file, "nothing here\n").unwrap();
        let summaries = store.verify_notes(&[note.to_string_lossy().into_owned()]).unwrap();
        assert_eq!(summaries[0].state, NoteSourceState::Missing);
        assert_eq!(store.note(note.to_str().unwrap()).unwrap().sources[0].reason.as_deref(), Some("当前文件中找不到引用的段落"));

        let broken = root.join("notes/broken.md");
        fs::write(&broken, "---\nsag_note: 1\nsources:\n  - id: 1\n---\n").unwrap();
        index(&store, &root, &broken);
        let summary = store.note_summary(broken.to_str().unwrap()).unwrap().unwrap();
        assert_eq!(summary.state, NoteSourceState::Unverified);
        assert!(summary.parse_error.is_some());

        // A plain Markdown file is not a note; deleting a note cleans its rows.
        let plain = root.join("notes/plain.md");
        fs::write(&plain, "# hi\n").unwrap();
        index(&store, &root, &plain);
        assert_eq!(store.notes(None).unwrap().len(), 2);
        fs::remove_file(&note).unwrap();
        store.remove_indexed(note.to_str().unwrap()).unwrap();
        assert_eq!(store.notes(None).unwrap().len(), 1);
    }

    #[test]
    fn removing_the_source_root_marks_anchors_missing() {
        let (dir, store, root) = setup();
        let other = dir.path().join("other");
        fs::create_dir(&other).unwrap();
        store.add_root(&other).unwrap();
        let other = other.canonicalize().unwrap();
        let file = other.join("guard.rs");
        fs::write(&file, format!("// header\n{PASSAGE}\n// footer\n")).unwrap();
        let note = note_for(&root, &file);
        index(&store, &other, &file);
        index(&store, &root, &note);
        assert_eq!(store.note(note.to_str().unwrap()).unwrap().summary.state, NoteSourceState::Intact);
        store.remove_root(other.to_str().unwrap()).unwrap();
        let detail = store.note(note.to_str().unwrap()).unwrap();
        assert_eq!(detail.sources[0].state, NoteSourceState::Missing);
        assert_eq!(detail.sources[0].reason.as_deref(), Some("文件已从知识库移除"));
    }

    #[test]
    fn sources_outside_the_knowledge_base_stay_unverified() {
        let (dir, store, root) = setup();
        let outside = dir.path().join("outside.rs");
        fs::write(&outside, PASSAGE).unwrap();
        let raw = fs::read(&outside).unwrap();
        let header = NoteHeader { question: "q".into(), asked_at: None, model: String::new(), scope: vec![],
            sources: vec![NoteSource { id: 1, path: outside.to_string_lossy().into_owned(), display_path: None, hash: hash(&raw), start_byte: 0, end_byte: PASSAGE.len(), start_line: 1, end_line: 3, text: PASSAGE.into() }] };
        let note = root.join("notes/out.md");
        fs::write(&note, notes::render(&header, note.parent().unwrap(), "a [1]", true)).unwrap();
        index(&store, &root, &note);
        let detail = store.note(note.to_str().unwrap()).unwrap();
        assert_eq!(detail.sources[0].state, NoteSourceState::Unverified);
        assert_eq!(detail.sources[0].reason.as_deref(), Some("来源不在知识库中"));
    }
}
