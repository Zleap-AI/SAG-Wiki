//! Offline migration, used by the desktop client after stopping the shared service.
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection};
use std::{fs, io::Write, path::{Path, PathBuf}};

pub fn migrate(source: &Path, destination: &Path, location_file: &Path) -> Result<()> {
    ensure!(destination.is_absolute(), "请选择绝对路径");
    let source_real = source.canonicalize().context("无法读取原数据目录")?;
    let destination = destination.canonicalize().context("请选择已存在的空文件夹")?;
    ensure!(!destination.starts_with(&source_real) && !source_real.starts_with(&destination), "新旧数据目录不能相同或互相包含");
    ensure!(fs::read_dir(&destination)?.next().is_none(), "请选择空文件夹，目标目录已有内容");
    let _instance = crate::cli_service::lock_instance(&source_real)?;
    let parent = destination.parent().context("不能使用磁盘根目录")?;
    let staged = tempfile::tempdir_in(parent).context("无法写入目标位置")?;
    let sources = [source.to_path_buf(), source_real.clone()];
    for entry in walkdir::WalkDir::new(&source_real).min_depth(1) {
        let entry = entry.context("无法读取待迁移文件")?;
        let relative = entry.path().strip_prefix(&source_real)?;
        if entry.depth() == 1 && matches!(relative.to_str(), Some("sag-service.json" | "cli-endpoint.json" | "cli-instance.lock" | "cli-prepare.lock")) {
            continue;
        }
        let target = staged.path().join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir(&target)?;
        } else if entry.file_type().is_symlink() {
            let link = fs::read_link(entry.path())?;
            // Resolve relative links against their old parent before relocating;
            // a link outside the data directory must keep pointing outside it.
            let link = crate::notes::normalize(&entry.path().parent().unwrap().join(link));
            let link = relocate(&link, &sources, &destination).unwrap_or(link);
            #[cfg(unix)]
            std::os::unix::fs::symlink(&link, &target)?;
            #[cfg(windows)]
            if entry.path().is_dir() {
                std::os::windows::fs::symlink_dir(&link, &target)?;
            } else {
                std::os::windows::fs::symlink_file(&link, &target)?;
            }
        } else {
            ensure!(entry.file_type().is_file(), "无法迁移特殊文件：{}", entry.path().display());
            fs::copy(entry.path(), &target).with_context(|| format!("无法复制 {}", entry.path().display()))?;
        }
    }
    rewrite_database(&staged.path().join("sag.sqlite"), &sources, &destination)?;
    // Prepare the locator before publishing anything. The original stays usable on every error.
    fs::create_dir_all(location_file.parent().context("无效的位置配置路径")?)?;
    let mut locator = tempfile::NamedTempFile::new_in(location_file.parent().unwrap())?;
    locator.write_all(&serde_json::to_vec(&destination)?)?;
    locator.as_file().sync_all()?;
    // remove_dir only succeeds if the folder is still empty.
    fs::remove_dir(&destination).context("目标目录已被修改，请重新选择空文件夹")?;
    fs::rename(staged.path(), &destination).context("无法发布迁移后的数据目录；原数据仍保留")?;
    locator.persist(location_file).map_err(|e| e.error)
        .context("无法保存新位置；原数据仍在使用，新目录保留已复制的数据，请重新选择空文件夹")?;
    Ok(())
}

fn relocate(path: &Path, sources: &[PathBuf], destination: &Path) -> Option<PathBuf> {
    sources.iter().find_map(|source| path.strip_prefix(source).ok().map(|suffix| destination.join(suffix)))
}

fn rewrite_json(value: &mut serde_json::Value, sources: &[PathBuf], destination: &Path) {
    match value {
        serde_json::Value::String(text) => {
            if let Some(path) = relocate(Path::new(text), sources, destination) {
                *text = path.to_string_lossy().into_owned();
            }
        }
        serde_json::Value::Array(items) => for item in items { rewrite_json(item, sources, destination); },
        serde_json::Value::Object(items) => for item in items.values_mut() { rewrite_json(item, sources, destination); },
        _ => {}
    }
}

fn rewrite_database(path: &Path, sources: &[PathBuf], destination: &Path) -> Result<()> {
    // Do not create an empty database if the original is missing or corrupt.
    let mut db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let tx = db.transaction()?;
    for (table, column) in [
        ("roots", "path"), ("root_watch_errors", "root"),
        ("root_exclusions", "path"), ("root_exclusions", "root"),
        ("index_issues", "path"), ("index_issues", "root"), ("index_history", "path"),
        ("chat_imports", "path"), ("conversions", "source"), ("conversions", "root"), ("conversions", "output"),
        ("conversion_states", "source"), ("conversion_states", "root"),
        ("conversion_failures", "source"), ("conversion_failures", "root"),
        ("documents", "path"), ("documents", "root"), ("notes", "path"),
        ("note_sources", "path"), ("note_sources", "display_path"), ("drafts", "path"),
    ] {
        let mut query = tx.prepare(&format!("SELECT rowid,{column} FROM {table} WHERE {column} IS NOT NULL"))?;
        let rows = query.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, text) in rows {
            if let Some(path) = relocate(Path::new(&text), sources, destination) {
                tx.execute(&format!("UPDATE {table} SET {column}=?1 WHERE rowid=?2"), params![path.to_str().context("目录路径不是有效 UTF-8")?, id])?;
            }
        }
    }
    for (table, column) in [("preferences", "value"), ("conversations", "messages"), ("drafts", "value"), ("notes", "scope")] {
        let mut query = tx.prepare(&format!("SELECT rowid,{column} FROM {table}"))?;
        let rows = query.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, text) in rows {
            let mut value = serde_json::from_str(&text).with_context(|| format!("{table}.{column} 中的 JSON 无效"))?;
            rewrite_json(&mut value, sources, destination);
            let updated = serde_json::to_string(&value)?;
            if updated != text {
                tx.execute(&format!("UPDATE {table} SET {column}=?1 WHERE rowid=?2"), params![updated, id])?;
            }
        }
    }
    tx.execute("UPDATE metadata SET version=version+1", [])?;
    let invalid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)", [], |r| r.get(0))?;
    ensure!(!invalid, "迁移后数据库引用校验失败");
    tx.commit()?;
    let check: String = db.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    ensure!(check == "ok", "迁移后数据库校验失败：{check}");
    db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
    db.close().map_err(|(_, e)| e)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    #[test]
    fn migration_preserves_content_and_rewrites_only_internal_references() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("old");
        let destination = temp.path().join("new");
        fs::create_dir(&destination).unwrap();
        let location = temp.path().join("config/location.json");
        let store = Store::open(&source.join("sag.sqlite")).unwrap();
        let root = temp.path().join("documents").to_str().unwrap().to_owned();
        let external = format!("{root}/report.pdf");
        let converted = source.join("converted/report.md");
        let internal = converted.to_str().unwrap();
        fs::create_dir(source.join("converted")).unwrap();
        fs::write(&converted, "# 转换正文").unwrap();
        fs::write(source.join("credentials.json"), br#"{"answer":"keep-secret"}"#).unwrap();
        fs::write(source.join("sag-service.json"), "stale endpoint").unwrap();
        let db = store.connection();
        db.execute("INSERT INTO roots(path,name) VALUES(?1,'docs')", [&root]).unwrap();
        db.execute("INSERT INTO conversions(source,root,source_hash,output,output_hash) VALUES(?1,?2,'hash',?3,'out')", params![external, root, internal]).unwrap();
        db.execute("INSERT INTO documents(root,path,name,hash,mtime) VALUES(?1,?2,'report','hash',0)", params![root, internal]).unwrap();
        let messages = serde_json::json!([{"text":"history", "citations":[{"path":internal},{"path":external}]}]);
        db.execute("INSERT INTO conversations(id,title,messages,updated) VALUES('chat','history',?1,0)", [messages.to_string()]).unwrap();
        db.execute("INSERT INTO drafts(path,value) VALUES(?1,?2)", params![internal, serde_json::json!({"path":internal,"text":"unsaved draft"}).to_string()]).unwrap();
        db.execute("INSERT OR REPLACE INTO preferences(id,value) VALUES(1,?1)", [serde_json::json!({"notes_folder":source.join("notes"),"external":root}).to_string()]).unwrap();
        drop(db);
        drop(store);
        let original = fs::read(source.join("sag.sqlite")).unwrap();
        migrate(&source, &destination, &location).unwrap();
        let destination = destination.canonicalize().unwrap();
        assert_eq!(serde_json::from_slice::<PathBuf>(&fs::read(location).unwrap()).unwrap(), destination);
        assert_eq!(fs::read(source.join("sag.sqlite")).unwrap(), original);
        assert_eq!(fs::read_to_string(destination.join("credentials.json")).unwrap(), r#"{"answer":"keep-secret"}"#);
        assert_eq!(fs::read_to_string(destination.join("converted/report.md")).unwrap(), "# 转换正文");
        assert!(!destination.join("sag-service.json").exists());
        assert!(!destination.join("cli-instance.lock").exists());
        let db = Connection::open(destination.join("sag.sqlite")).unwrap();
        let (output, original_source): (String, String) = db.query_row("SELECT output,source FROM conversions", [], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(Path::new(&output), destination.join("converted/report.md"));
        assert_eq!(original_source, external);
        let text: String = db.query_row("SELECT messages FROM conversations", [], |r| r.get(0)).unwrap();
        let messages: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(messages[0]["text"], "history");
        assert_eq!(messages[0]["citations"][0]["path"], output);
        assert_eq!(messages[0]["citations"][1]["path"], external);
        let (path, draft): (String, String) = db.query_row("SELECT path,value FROM drafts", [], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(path, output);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&draft).unwrap()["path"], output);
        let settings: String = db.query_row("SELECT value FROM preferences", [], |r| r.get(0)).unwrap();
        let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
        assert_eq!(settings["notes_folder"], destination.join("notes").to_str().unwrap());
        assert_eq!(settings["external"], root);
    }

    #[test]
    fn rejects_nonempty_overlapping_and_active_directories_without_switching() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("old");
        fs::create_dir(&source).unwrap();
        let destination = temp.path().join("new");
        fs::create_dir(&destination).unwrap();
        let location = temp.path().join("location.json");
        fs::write(&location, b"unchanged").unwrap();
        fs::write(destination.join("keep.txt"), "keep").unwrap();
        assert!(migrate(&source, &destination, &location).unwrap_err().to_string().contains("空文件夹"));
        assert_eq!(fs::read_to_string(destination.join("keep.txt")).unwrap(), "keep");
        assert!(migrate(&source, &source, &location).is_err());
        let nested = source.join("nested");
        fs::create_dir(&nested).unwrap();
        assert!(migrate(&source, &nested, &location).is_err());
        assert!(migrate(&source, temp.path(), &location).is_err());
        fs::remove_file(destination.join("keep.txt")).unwrap();
        let lock = crate::cli_service::lock_instance(&source).unwrap();
        assert!(migrate(&source, &destination, &location).unwrap_err().to_string().contains("正在使用"));
        drop(lock);
        // A missing/corrupt database must never publish an empty replacement.
        fs::write(source.join("sag.sqlite"), "broken").unwrap();
        assert!(migrate(&source, &destination, &location).is_err());
        assert!(fs::read_dir(destination).unwrap().next().is_none());
        assert_eq!(fs::read(location).unwrap(), b"unchanged");
        assert_eq!(fs::read_to_string(source.join("sag.sqlite")).unwrap(), "broken");
    }

    #[cfg(unix)]
    #[test]
    fn unix_migration_preserves_executables_and_relocates_links_across_repeated_moves() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("旧数据 old");
        let destination = temp.path().join("新数据 new");
        let final_destination = temp.path().join("再次迁移 final");
        fs::create_dir(&destination).unwrap();
        fs::create_dir(&final_destination).unwrap();
        let location = temp.path().join("config/location.json");
        drop(Store::open(&source.join("sag.sqlite")).unwrap());
        let tools = source.join("tools");
        fs::create_dir(&tools).unwrap();
        fs::write(tools.join("python"), "#!/bin/sh\nprintf 'runtime-ok'\n").unwrap();
        fs::set_permissions(tools.join("python"), fs::Permissions::from_mode(0o755)).unwrap();
        symlink("python", tools.join("python3")).unwrap();
        fs::write(temp.path().join("external.txt"), "external document").unwrap();
        symlink("../../external.txt", tools.join("external")).unwrap();
        let alias = temp.path().join("source-alias");
        symlink(&source, &alias).unwrap();
        // macOS temporary paths may also have /var and /private/var aliases.
        let db = Connection::open(source.join("sag.sqlite")).unwrap();
        db.execute("INSERT OR REPLACE INTO preferences(id,value) VALUES(1,?1)",
            [serde_json::json!({"notes_folder":alias.join("notes")}).to_string()]).unwrap();
        drop(db);
        migrate(&alias, &destination, &location).unwrap();
        for directory in [&destination, &final_destination] {
            if directory == &final_destination {
                migrate(&destination, directory, &location).unwrap();
            }
            let directory = directory.canonicalize().unwrap();
            let executable = directory.join("tools/python3");
            assert_eq!(fs::read_link(&executable).unwrap(), directory.join("tools/python"));
            assert_eq!(fs::metadata(&executable).unwrap().permissions().mode() & 0o777, 0o755);
            let output = std::process::Command::new(&executable).output().unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, b"runtime-ok");
            assert_eq!(fs::read_to_string(directory.join("tools/external")).unwrap(), "external document");
            let db = Connection::open(directory.join("sag.sqlite")).unwrap();
            let settings: String = db.query_row("SELECT value FROM preferences", [], |r| r.get(0)).unwrap();
            let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
            assert_eq!(settings["notes_folder"], directory.join("notes").to_str().unwrap());
            assert_eq!(serde_json::from_slice::<PathBuf>(&fs::read(&location).unwrap()).unwrap(), directory);
        }
        assert_eq!(fs::read_link(tools.join("python3")).unwrap(), PathBuf::from("python"));
        assert_eq!(fs::read_to_string(tools.join("external")).unwrap(), "external document");
    }
}
