use crate::{text, types::*};
use anyhow::{bail, Context, Result};
use indexmap::IndexMap;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    collections::{hash_map::Entry as MapEntry, HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
use walkdir::WalkDir;
struct PreparedChunk {
    chunk: text::Chunk,
    heading: String,
    content: String,
}
struct PreparedFile {
    path: String,
    name: String,
    fingerprint: String,
    mtime: i64,
    size: i64,
    title: String,
    chunks: Vec<PreparedChunk>,
    explore_terms: Vec<DocumentTerm>,
}
pub struct Store {
    db: Mutex<Connection>,
    query_readers: Vec<Mutex<Connection>>,
    query_cache: Mutex<QueryCache>,
}
const QUERY_WORKERS: usize = 8;
const QUERY_CACHE_ENTRIES: usize = 256;

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
}

impl QueryCache {
    fn refresh(&mut self, version: i64) -> bool {
        if self.version.is_some_and(|current| version < current) { return false; }
        if self.version != Some(version) {
            self.entries.clear();
            self.rank_stats.clear();
            self.frequencies.clear();
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
    documents: &mut HashMap<i64, String>,
) -> Result<Candidate> {
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

fn hydrate_candidate(
    mut candidate: Candidate,
    documents: &mut HashMap<i64, String>,
) -> Result<Candidate> {
    let path = &candidate.path;
    let source = match documents.entry(candidate.document_id) {
        MapEntry::Occupied(entry) => entry.into_mut(),
        MapEntry::Vacant(entry) => {
            let raw = fs::read(&path).with_context(|| format!("无法读取原文件：{path}"))?;
            if hash(&raw) != candidate.hash {
                bail!("文件已修改，请刷新索引：{path}")
            }
            entry.insert(body(Path::new(path), &raw)?)
        }
    };
    let lines: Vec<&str> = source
        .split('\n')
        .map(|l| l.trim_end_matches('\r'))
        .collect();
    candidate.content = if !candidate.source_spans.is_empty() {
        candidate.source_spans
            .iter()
            .map(|s| &source[s.start_byte..s.end_byte])
            .collect::<Vec<_>>()
            .join("\n\n")
    } else {
        candidate.source_ranges
            .iter()
            .map(|(s, e)| lines[s - 1..*e].join("\n"))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    Ok(candidate)
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
 CREATE TABLE IF NOT EXISTS explore_summaries(document_id INTEGER PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,hash TEXT NOT NULL,algorithm INTEGER NOT NULL,terms TEXT NOT NULL);
 INSERT OR IGNORE INTO metadata VALUES(1,0);
")?;
        let has_size: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('documents') WHERE name='size')",
            [],
            |r| r.get(0),
        )?;
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
        db.pragma_update(None, "user_version", 4)?;
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
            db: Mutex::new(db),
            query_readers,
            query_cache: Mutex::new(QueryCache::default()),
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
    pub fn save_settings(&self, s: &Settings) -> Result<()> {
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
        let previous = self.settings()?;
        let mut db = self.connection();
        let transaction = db.transaction()?;
        transaction.execute("INSERT INTO preferences VALUES(1,?) ON CONFLICT(id) DO UPDATE SET value=excluded.value",[serde_json::to_string(s)?])?;
        if previous.file_types != s.file_types || previous.excluded != s.excluded {
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
    pub fn excluded_directories(&self) -> Result<Vec<String>> {
        let db = self.connection();
        let mut query = db.prepare("SELECT path FROM root_exclusions ORDER BY path")?;
        let paths = query.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(paths)
    }
    fn path_excluded(db: &Connection, path: &str) -> Result<bool> {
        Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM root_exclusions WHERE path=?1 OR instr(?1,path || ?2)=1)",
            params![path,std::path::MAIN_SEPARATOR.to_string()], |row| row.get(0))?)
    }
    pub fn is_excluded(&self, path: &Path) -> Result<bool> {
        Self::path_excluded(&self.connection(), &path_text(path)?)
    }
    pub fn authorize(&self, path: &Path) -> Result<PathBuf> {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("路径不可用：{}", path.display()))?;
        if !self
            .roots()?
            .iter()
            .any(|r| canonical.starts_with(Path::new(&r.path)))
        {
            bail!("该路径不在已添加的文件夹内")
        };
        if self.is_excluded(&canonical)? { bail!("此目录已从知识库移除，请先重新添加") }
        Ok(canonical)
    }
    pub fn add_root(&self, path: &Path) -> Result<String> {
        let path = path.canonicalize().context("无法打开这个文件夹")?;
        if !path.is_dir() {
            bail!("请选择文件夹")
        };
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
        let root = roots.iter().find(|root| path.starts_with(&root.path)).context("此文件夹不在知识库中")?;
        if !path.is_dir() { bail!("只能移除知识库中的文件夹") }
        let mut db = self.connection();
        let tx = db.transaction()?;
        if Self::path_excluded(&tx, &value)? { bail!("此文件夹已从知识库移除") }
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
            if self.is_excluded(&item.path())? { continue; }
            if !show_hidden && name.starts_with('.') {
                continue;
            };
            let ty = item.file_type()?;
            if ty.is_dir() || ty.is_symlink() || text::is_supported(&item.path()) {
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
        let path = self.authorize(path)?;
        let raw = fs::read(&path)?;
        let id = self
            .connection()
            .query_row(
                "SELECT id FROM documents WHERE path=?",
                [path_text(&path)?],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        Ok(Document {
            id,
            path: path_text(&path)?,
            name: path.file_name().unwrap().to_string_lossy().into(),
            body: body(&path, &raw)?,
            hash: hash(&raw),
        })
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
        self.root_status(root, "indexing", None)?;
        let result = self.scan_inner(root, progress);
        match &result {
            Ok(_) => self.finish_indexing(root)?,
            Err(e) => self.root_status(root, "error", Some(&e.to_string()))?,
        };
        result
    }
    pub fn update_paths(&self, root: &str, paths: &[PathBuf], progress: impl Fn(IndexProgress)) -> Result<usize> {
        self.root_status(root, "indexing", None)?;
        let result = self.update_paths_inner(root, paths, progress);
        match &result {
            Ok(_) => self.finish_indexing(root)?,
            Err(error) => self.root_status(root, "error", Some(&error.to_string()))?,
        }
        result
    }
    fn update_paths_inner(&self, root: &str, paths: &[PathBuf], progress: impl Fn(IndexProgress)) -> Result<usize> {
        let settings = self.settings()?;
        if !Path::new(root).is_dir() {
            bail!("文件夹不可用，请检查磁盘或重新添加")
        }
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
            if metadata.file_type().is_symlink() || !path.canonicalize()?.starts_with(root)
                || text::in_hidden_directory(Path::new(root), path, metadata.is_dir()) {
                work.push(Work::Remove(path.clone()));
                total += 1;
            } else if metadata.is_dir() {
                let files = Self::index_files(path, &settings, &excluded)?;
                total += files.len().max(1);
                work.push(Work::Directory(path.clone(), files));
            } else if metadata.is_file() && text::is_indexable(path, &settings.file_types) {
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
        for item in work {
            match item {
                Work::Directory(path, files) => {
                    count += self.scan_directory(root, &path, &files, &settings, true, |p| progress(IndexProgress { count: completed + p.count, total }))?;
                    completed += files.len().max(1);
                }
                Work::File(path) => {
                    if let Some(file) = self.prepare_indexed_file(root, &path, settings.retrieval.chunk_chars, true)? {
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
        let path = path_text(path)?;
        let prefix = format!("{}{}", path.trim_end_matches(std::path::MAIN_SEPARATOR), std::path::MAIN_SEPARATOR);
        tx.execute("DELETE FROM chunk_fts WHERE rowid IN (SELECT c.id FROM chunks c JOIN documents d ON d.id=c.document_id WHERE d.root=?1 AND (d.path=?2 OR instr(d.path,?3)=1))", params![root,path,prefix])?;
        let removed = tx.execute("DELETE FROM documents WHERE root=?1 AND (path=?2 OR instr(path,?3)=1)", params![root,path,prefix])?;
        tx.execute("DELETE FROM index_issues WHERE root=?1 AND (path=?2 OR instr(path,?3)=1)", params![root,path,prefix])?;
        if removed > 0 { tx.execute("UPDATE metadata SET version=version+1", [])?; }
        Ok(removed)
    }
    fn scan_inner(&self, root: &str, progress: impl Fn(IndexProgress)) -> Result<usize> {
        let settings = self.settings()?;
        if !Path::new(root).is_dir() {
            bail!("文件夹不可用，请检查磁盘或重新添加")
        };
        let files = Self::index_files(Path::new(root), &settings, &self.excluded_directories()?)?;
        self.scan_directory(root, Path::new(root), &files, &settings, false, progress)
    }
    fn index_files(directory: &Path, settings: &Settings, excluded: &[String]) -> Result<Vec<PathBuf>> {
        WalkDir::new(directory)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !excluded.iter().any(|path| e.path().starts_with(path)) && (e.depth() == 0 || (
                !(e.file_type().is_dir() && e.file_name().as_encoded_bytes().starts_with(b"."))
                    && !settings.excluded.iter().any(|x| e.file_name() == x.as_str()))))
            .filter_map(|entry| match entry {
                Ok(entry) if entry.file_type().is_file() && text::is_indexable(entry.path(), &settings.file_types) => Some(Ok(entry.into_path())),
                Ok(_) => None,
                Err(error) => Some(Err(anyhow::Error::from(error))),
            })
            .collect()
    }
    fn scan_directory(&self, root: &str, directory: &Path, files: &[PathBuf], settings: &Settings, verify: bool, progress: impl Fn(IndexProgress)) -> Result<usize> {
        let mut seen = HashSet::new();
        let mut count = 0;
        let total = files.len();
        progress(IndexProgress { count, total });
        for batch in files.chunks(256) {
            for path in batch {
                seen.insert(path_text(path)?);
            }
            let max_chars = settings.retrieval.chunk_chars;
            let prepared = thread::scope(|scope| -> Result<Vec<Option<PreparedFile>>> {
                let mut handles = Vec::new();
                for paths in batch.chunks(batch.len().div_ceil(settings.index_workers)) {
                    handles.push(scope.spawn(move || {
                        paths
                            .iter()
                            .map(|path| self.prepare_indexed_file(root, path, max_chars, verify))
                            .collect::<Result<Vec<_>>>()
                    }));
                }
                let mut prepared = Vec::with_capacity(batch.len());
                for handle in handles {
                    prepared.extend(handle.join().expect("index worker panicked")?);
                }
                Ok(prepared)
            })?;
            {
                let mut db = self.connection();
                let tx = db.transaction()?;
                for file in prepared.into_iter().flatten() {
                    Self::write_prepared(&tx, root, file)?;
                }
                tx.commit()?;
            }
            for _ in batch {
                count += 1;
                if count % 20 == 0 {
                    progress(IndexProgress { count, total });
                }
            }
        }
        let indexed = {
            let c = self.connection();
            let directory = path_text(directory)?;
            let prefix = format!("{}{}", directory.trim_end_matches(std::path::MAIN_SEPARATOR), std::path::MAIN_SEPARATOR);
            let mut q = c.prepare("SELECT path FROM documents WHERE root=?1 AND (path=?2 OR instr(path,?3)=1) UNION SELECT path FROM index_issues WHERE root=?1 AND (path=?2 OR instr(path,?3)=1)")?;
            let rows = q
                .query_map(params![root,directory,prefix], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        for p in indexed {
            if !seen.contains(&p) {
                self.remove_indexed(&p)?
            }
        }
        progress(IndexProgress { count, total });
        Ok(count)
    }
    pub fn index_file(&self, root: &str, path: &Path, max_chars: usize) -> Result<bool> {
        if text::in_hidden_directory(Path::new(root), path, false)
            || !text::is_indexable(path, &self.settings()?.file_types) {
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
        let path_string = path_text(path)?;
        let metadata = fs::metadata(path)?;
        let mtime = metadata.modified()?.duration_since(UNIX_EPOCH)?.as_nanos() as i64;
        let size = metadata.len() as i64;
        let unchanged: bool = self.connection().query_row(
            "SELECT EXISTS(SELECT 1 FROM documents WHERE path=? AND mtime=? AND size=? AND hash<>'')",
            params![path_string, mtime, size], |r| r.get(0))?;
        if unchanged && !verify {
            return Ok(None);
        }
        let bytes = fs::read(path)?;
        let fingerprint = hash(&bytes);
        let body = body(path, &bytes)?;
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let title = text::index_text(&name);
        let chunks: Vec<PreparedChunk> = text::chunks_for_file(&body, max_chars, text::is_markdown(path))
            .into_iter()
            .map(|chunk| PreparedChunk {
                heading: text::index_text(&chunk.heading),
                content: text::index_text(&chunk.content),
                chunk,
            })
            .collect();
        let explore_terms = text::explore_summary(path, chunks.iter().map(|chunk| (chunk.chunk.heading.as_str(), chunk.chunk.content.as_str())));
        Ok(Some(PreparedFile {
            path: path_string,
            name,
            fingerprint,
            mtime,
            size,
            title,
            chunks,
            explore_terms,
        }))
    }
    fn prepare_indexed_file(&self, root: &str, path: &Path, max_chars: usize, verify: bool) -> Result<Option<PreparedFile>> {
        match self.prepare_file(path, max_chars, verify) {
            Ok(file) => Ok(file),
            Err(error) if error.is::<std::io::Error>() || error.is::<crate::document_text::DecodeError>() => {
                let path = path_text(path)?;
                let reason = error.to_string();
                let mut db = self.connection();
                let tx = db.transaction()?;
                if Self::path_excluded(&tx, &path)? { return Ok(None); }
                tx.execute("DELETE FROM chunk_fts WHERE rowid IN (SELECT c.id FROM chunks c JOIN documents d ON d.id=c.document_id WHERE d.path=?)", [&path])?;
                let removed = tx.execute("DELETE FROM documents WHERE path=?", [&path])?;
                if removed > 0 { tx.execute("UPDATE metadata SET version=version+1", [])?; }
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
                SELECT 1 FROM json_each(?2) s WHERE instr(replace(d.path,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1
            ))", params![expression, scope], |r| r.get(0))?;
        let page = page.min(total.div_ceil(page_size).max(1));
        let mut statement=connection.prepare("WITH matched AS MATERIALIZED (
            SELECT c.id,c.document_id,bm25(chunk_fts,2,2,1) AS score
            FROM chunk_fts JOIN chunks c ON c.id=chunk_fts.rowid JOIN documents d ON d.id=c.document_id
            WHERE chunk_fts MATCH ?1 AND (json_array_length(?2)=0 OR EXISTS(
                SELECT 1 FROM json_each(?2) s WHERE instr(replace(d.path,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1
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
            results.push(candidate_from_row(row, results.len(), &mut documents)?);
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
            let rows = if stats.is_empty() { vec![] } else { Self::query_candidate_metadata(&transaction, &key, &stats)? };
            self.query_cache.lock().expect("query cache mutex poisoned").insert(version, key, rows.clone());
            rows
        };
        transaction.commit()?;
        drop(connection);
        let mut documents = HashMap::new();
        metadata.into_iter().map(|candidate| hydrate_candidate(candidate, &mut documents)).collect()
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

    fn query_candidate_metadata(c: &Connection, key: &CandidateQuery, stats: &[u8]) -> Result<Vec<Candidate>> {
        let sql = "WITH top_hits AS MATERIALIZED (
                SELECT rowid,sag_bm25(chunk_fts,?6,2,2,1) AS score FROM chunk_fts
                WHERE chunk_fts MATCH ?1
                AND rowid NOT IN (SELECT value FROM json_each(?2))
                AND (json_array_length(?3)=0 OR +rowid IN (
                    SELECT c.id FROM chunks c JOIN documents d ON d.id=c.document_id
                    WHERE d.path IN (SELECT value FROM json_each(?4)) OR EXISTS(
                        SELECT 1 FROM json_each(?3) s
                        WHERE instr(replace(d.path,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1
                    )
                ))
                ORDER BY score,rowid LIMIT ?5
            )
            SELECT c.id,d.id,d.path,d.name,c.heading,c.start_line,c.end_line,c.ranges,h.score,d.hash,c.spans
            FROM top_hits h JOIN chunks c ON c.id=h.rowid JOIN documents d ON d.id=c.document_id
            ORDER BY h.score,c.id";
        let expression = key.required.as_ref().map_or_else(|| key.expression.clone(), |required| format!("({}) AND ({required})", key.expression));
        let mut values: Vec<rusqlite::types::Value> = vec![
            expression.into(),
            serde_json::to_string(&key.excluded)?.into(),
            serde_json::to_string(&key.scope)?.into(),
            serde_json::to_string(&key.references)?.into(),
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
                let restricted = sql.replace("ORDER BY score,rowid LIMIT ?5", "AND +rowid IN (SELECT rowid FROM chunk_fts WHERE chunk_fts MATCH ?7) ORDER BY score,rowid LIMIT ?5");
                values.push(subset.into());
                let possible = read(&restricted, &values)?;
                values.pop();
                if possible.len() == key.limit && -possible.last().unwrap().bm25 > bound { return Ok(possible); }
            }
        }
        read(sql, &values)
    }

    pub fn has_candidate_match(&self, expression: &str, scope: &[String], references: &[String], excluded: &[i64]) -> Result<bool> {
        Ok(self.connection().query_row(
            "SELECT EXISTS(SELECT 1 FROM chunk_fts WHERE chunk_fts MATCH ?1
                AND rowid NOT IN (SELECT value FROM json_each(?2))
                AND (json_array_length(?3)=0 OR +rowid IN (
                    SELECT c.id FROM chunks c JOIN documents d ON d.id=c.document_id
                    WHERE d.path IN (SELECT value FROM json_each(?4)) OR EXISTS(
                        SELECT 1 FROM json_each(?3) s
                        WHERE instr(replace(d.path,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1
                    )
                )) LIMIT 1)",
            params![expression, serde_json::to_string(excluded)?, serde_json::to_string(scope)?, serde_json::to_string(references)?],
            |row| row.get(0),
        )?)
    }
    pub fn reference_candidates(
        &self,
        references: &[String],
        limit: usize,
    ) -> Result<Vec<Candidate>> {
        let c = self.connection();
        let mut q=c.prepare("SELECT c.id,d.id,d.path,d.name,c.heading,c.start_line,c.end_line,c.ranges,0.0,d.hash,c.spans FROM chunks c JOIN documents d ON d.id=c.document_id WHERE d.path IN (SELECT value FROM json_each(?1)) ORDER BY c.id LIMIT ?2")?;
        let mut rows = q.query(params![serde_json::to_string(references)?, limit as i64])?;
        let mut result = vec![];
        let mut documents = HashMap::new();
        while let Some(row) = rows.next()? {
            result.push(candidate_from_row(row, result.len(), &mut documents)?);
        }
        Ok(result)
    }
    pub fn retrieval_corpus(&self, scope: &[String], references: &[String]) -> Result<Vec<(String, String)>> {
        let rows: Vec<(String, String, String)> = {
            let db = self.connection();
            let mut query = db.prepare("SELECT d.name,d.path,d.hash FROM documents d WHERE EXISTS(SELECT 1 FROM chunks c WHERE c.document_id=d.id) AND (json_array_length(?1)=0 OR d.path IN (SELECT value FROM json_each(?2)) OR EXISTS(SELECT 1 FROM json_each(?1) s WHERE instr(replace(d.path,char(92),'/')||'/',rtrim(replace(s.value,char(92),'/'),'/')||'/')=1)) ORDER BY d.id")?;
            let values = query.query_map(params![serde_json::to_string(scope)?, serde_json::to_string(references)?], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
            values
        };
        rows.into_iter().map(|(title,path,expected)| {
            let raw = fs::read(&path).with_context(|| format!("无法读取原文件：{path}"))?;
            if hash(&raw) != expected { bail!("文件已修改，请刷新索引：{path}") }
            Ok((title, body(Path::new(&path), &raw)?))
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
        assert_eq!(reopened.settings().unwrap().retrieval.chunk_chars,2200);
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
        assert_eq!(upgraded.retrieval.candidate_k, 100);
        assert_eq!(upgraded.retrieval.max_hops, 2);
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
            question: "第一问".into(),
            answer: "第一答".into(),
            reasoning: String::new(),
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
    fn source_code_and_partial_lines_round_trip_without_writes() {
        let (_d, s, n) = setup();
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
            assert!(c.content.encode_utf16().count() <= s.settings().unwrap().retrieval.chunk_chars);
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
    fn query_cache_refreshes_after_another_store_updates_or_removes_documents() {
        let (dir, store, root) = setup();
        let source = root.join("source.txt");
        fs::write(&source, "alpha original source").unwrap();
        store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        let expression = text::match_query("alpha");
        assert_eq!(store.candidate_query(&expression, None, &[], &[], &[], 100).unwrap().len(), 1);
        fs::write(&source, "beta replacement source").unwrap();
        // Even a cache hit must validate the original file.
        assert!(store.candidate_query(&expression, None, &[], &[], &[], 100).is_err());
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
    fn file_type_choices_default_to_all_and_update_only_selected_indexes() {
        let (dir, store, root) = setup();
        let root_path = root.to_str().unwrap();
        let defaults: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(defaults.file_types, FileTypes::default());
        let partial: Settings = serde_json::from_str(r#"{"file_types":{"code":false}}"#).unwrap();
        assert!(!partial.file_types.code);
        assert!(partial.file_types.markdown && partial.file_types.text && partial.file_types.config);

        let names = ["notes.md", "notes.rst", "main.py", "page.HTML", "Makefile", "settings.json"];
        for name in names {
            fs::write(root.join(name), format!("filetypemarker {name}")).unwrap();
        }
        store.scan(root_path, |_| {}).unwrap();
        assert_eq!(store.search_files("filetypemarker", &[], 20).unwrap().len(), 6);
        let note_id = store.document(&root.join("notes.md")).unwrap().id;
        let mut settings = store.settings().unwrap();
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
        assert!(hit.content.encode_utf16().count() <= store.settings().unwrap().retrieval.chunk_chars);
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
        assert!(store.settings().unwrap().show_hidden);
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
        assert!(store.search_files("recoveredmarker", &[], 10).unwrap().is_empty());
        fs::remove_file(&path).unwrap();
        store.update_paths(root.to_str().unwrap(), &[path], |_| {}).unwrap();
        assert_eq!(store.roots().unwrap()[0].status, "ready");
    }

    #[test]
    fn file_index_issues_persist_and_clear_when_the_directory_is_excluded() {
        let (dir, store, root) = setup();
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
        assert!(store
            .candidate_query(&expression, None, &[], &[], &[], 100)
            .is_err());
        fs::write(&best, "modified needle").unwrap();
        assert!(store
            .candidate_query(&expression, None, &[], &[], &[], 1)
            .is_err());
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
