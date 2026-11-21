//! Offline migration, used by the desktop client after stopping the shared service.
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection};
use std::{fs, io::Write, path::{Path, PathBuf}};

pub fn migrate(source: &Path, destination: &Path, location_file: &Path) -> Result<()> {
    ensure!(destination.is_absolute(), "请选择绝对路径");
    let source_real = source.canonicalize().context("无法读取原数据目录")?;
    let destination = destination.canonicalize().context("请选择已存在的空文件夹")?;
    ensure!(!location_file.starts_with(&source_real), "保存位置的配置不能位于待迁移目录内");
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
        if relative == Path::new("http-api.json") {
            ensure!(entry.file_type().is_file(), "远程服务配置必须是普通文件");
            crate::http_api::copy_relocated_config(
                entry.path(), &target, |path| relocate(path, &sources, &destination),
            )?;
            continue;
        }
        if relative == Path::new("credentials.json") {
            ensure!(entry.file_type().is_file(), "密钥配置必须是普通文件");
            crate::http_api::write_private(&target, &fs::read(entry.path())?)?;
            continue;
        }
        if entry.file_type().is_dir() {
            fs::create_dir(&target)?;
        } else if entry.file_type().is_symlink() {
            let original = fs::read_link(entry.path())?;
            let resolved = crate::notes::normalize(&entry.path().parent().unwrap().join(&original));
            let link = match relocate(&resolved, &sources, &destination) {
                Some(link) if original.is_relative() && crate::notes::normalize(
                    &destination.join(relative).parent().unwrap().join(&original),
                ) == link => original,
                Some(link) => link,
                None => resolved,
            };
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
    let database = staged.path().join("sag.sqlite");
    ensure!(database.symlink_metadata()?.is_file(), "应用数据库必须是普通文件");
    let mut conversion_paths = Vec::new();
    let db = Connection::open_with_flags(&database, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let settings: String = db.query_row("SELECT value FROM preferences WHERE id=1", [], |row| row.get(0))?;
    let settings: sag_protocol::types::Settings = serde_json::from_str(&settings)?;
    let output_base = match settings.conversion_storage {
        sag_protocol::types::ConversionStorage::AppData => Some(source_real.clone()),
        sag_protocol::types::ConversionStorage::Custom { directory } => Some(PathBuf::from(directory)),
        sag_protocol::types::ConversionStorage::Library => None,
    };
    if let Some(base) = output_base {
        let output = base.join(crate::docling::OUTPUT_DIRECTORY);
        if let (Some(staged_output), Some(new_output)) = (
            relocate(&output, &sources, staged.path()),
            relocate(&output, &sources, &destination),
        ) {
            let mut query = db.prepare("SELECT path FROM roots")?;
            let paths = query.query_map([], |row| row.get::<_, String>(0))?;
            for path in paths {
                let path = PathBuf::from(path?);
                let Some(relocated) = relocate(&path, &sources, &destination) else { continue };
                let hash = crate::store::hash(path.as_os_str().as_encoded_bytes());
                let new_hash = crate::store::hash(relocated.as_os_str().as_encoded_bytes());
                let old = staged_output.join(&hash);
                if old.try_exists()? {
                    let new = staged_output.join(&new_hash);
                    ensure!(!new.try_exists()?, "目标转换目录已有内容：{}", new.display());
                    fs::rename(&old, new)?;
                }
                conversion_paths.push((output.join(hash), new_output.join(new_hash)));
            }
        }
    }
    drop(db);
    rewrite_database(&database, &sources, &destination, &conversion_paths)?;
    // remove_dir only succeeds if the folder is still empty.
    fs::remove_dir(&destination).context("目标目录已被修改，请重新选择空文件夹")?;
    fs::rename(staged.path(), &destination).context("无法发布迁移后的数据目录；原数据仍保留")?;
    switch_location(Some(&source_real), &destination, location_file)
        .context("无法启用新位置；原数据仍保留，新目录保留已复制的数据")
}

pub fn recover(destination: &Path, location_file: &Path) -> Result<()> {
    let previous = match sag_protocol::read_data_directory_location(location_file) {
        Ok(previous) => previous.filter(|path| path.is_absolute()),
        Err(error) if error.downcast_ref::<serde_json::Error>().is_some() => None,
        Err(error) => return Err(error),
    };
    let destination = destination.canonicalize().context("无法打开所选数据目录")?;
    let database = destination.join("sag.sqlite");
    let db = Connection::open_with_flags(&database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .context("请选择包含现有 SAG 数据库的文件夹")?;
    let recognized: bool = db.query_row(
        "SELECT COUNT(*)=2 FROM sqlite_master WHERE type='table' AND name IN ('roots','metadata')",
        [], |row| row.get(0),
    )?;
    ensure!(recognized, "请选择包含现有 SAG 数据库的文件夹");
    let check: String = db.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    ensure!(check == "ok", "所选数据目录的数据库校验失败：{check}");
    switch_location(previous.as_deref(), &destination, location_file)
}

fn switch_location(source: Option<&Path>, destination: &Path, location_file: &Path) -> Result<()> {
    let parent = location_file.parent().context("无效的位置配置路径")?;
    fs::create_dir_all(parent)?;
    let mut locator = tempfile::NamedTempFile::new_in(parent)?;
    locator.write_all(&serde_json::to_vec(destination)?)?;
    locator.as_file().sync_all()?;
    let save = || locator.persist(location_file).map(|_| ()).map_err(|e| e.error.into());
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    if let Some(source) = source.filter(|source| *source != destination) {
        return crate::api_autostart::move_registration(source, destination, &std::env::current_exe()?, save);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = source;
    save()
}

fn relocate(path: &Path, sources: &[PathBuf], destination: &Path) -> Option<PathBuf> {
    sources.iter().find_map(|source| path.strip_prefix(source).ok().map(|suffix| destination.join(suffix)))
}

fn rewrite_paths(value: &mut serde_json::Value, relocate: &impl Fn(&Path) -> Option<PathBuf>) {
    match value {
        serde_json::Value::String(text) => {
            if let Some(path) = relocate(Path::new(text)) {
                *text = path.to_string_lossy().into_owned();
            }
        }
        serde_json::Value::Array(items) => for item in items { rewrite_paths(item, relocate); },
        _ => {}
    }
}

fn rewrite_database(path: &Path, sources: &[PathBuf], destination: &Path, outputs: &[(PathBuf, PathBuf)]) -> Result<()> {
    let remap = |path: &Path| {
        outputs.iter().find_map(|(from, to)| path.strip_prefix(from).ok().map(|suffix| to.join(suffix)))
            .or_else(|| relocate(path, sources, destination))
    };
    let mut db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let tx = db.transaction()?;
    tx.execute_batch("PRAGMA defer_foreign_keys=ON")?;
    for (table, column) in [
        ("roots", "path"), ("root_watch_errors", "root"),
        ("root_exclusions", "path"), ("root_exclusions", "root"),
        ("index_issues", "path"), ("index_issues", "root"), ("index_history", "path"),
        ("chat_imports", "path"), ("chat_import_schedules", "source_root"), ("conversions", "source"), ("conversions", "root"), ("conversions", "output"),
        ("conversion_states", "source"), ("conversion_states", "root"),
        ("conversion_failures", "source"), ("conversion_failures", "root"),
        ("documents", "path"), ("documents", "root"), ("notes", "path"),
        ("note_sources", "path"), ("note_sources", "display_path"), ("drafts", "path"),
    ] {
        if table == "chat_import_schedules" && !tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='chat_import_schedules')", [], |row| row.get::<_, bool>(0),
        )? { continue; }
        let mut query = tx.prepare(&format!("SELECT rowid,{column} FROM {table} WHERE {column} IS NOT NULL"))?;
        let rows = query.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, text) in rows {
            if let Some(path) = remap(Path::new(&text)) {
                tx.execute(&format!("UPDATE {table} SET {column}=?1 WHERE rowid=?2"), params![path.to_str().context("目录路径不是有效 UTF-8")?, id])?;
            }
        }
    }
    for (table, column) in [("preferences", "value"), ("conversations", "messages"), ("drafts", "value"), ("notes", "scope"), ("online_sources", "config"), ("online_items", "files")] {
        if matches!(table, "online_sources" | "online_items") && !tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)", [table], |row| row.get::<_, bool>(0),
        )? { continue; }
        let mut query = tx.prepare(&format!("SELECT rowid,{column} FROM {table}"))?;
        let rows = query.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, text) in rows {
            let mut value: serde_json::Value = serde_json::from_str(&text).with_context(|| format!("{table}.{column} 中的 JSON 无效"))?;
            match table {
                "preferences" => {
                    for field in ["chat_import_folder", "excluded"] {
                        if let Some(path) = value.get_mut(field) { rewrite_paths(path, &remap); }
                    }
                    for (container, field) in [("notes", "folder"), ("conversion_storage", "directory")] {
                        if let Some(path) = value.get_mut(container).and_then(|value| value.get_mut(field)) {
                            rewrite_paths(path, &remap);
                        }
                    }
                }
                "conversations" => if let Some(messages) = value.as_array_mut() {
                    for message in messages {
                        for field in ["scope", "references", "note_path"] {
                            if let Some(path) = message.get_mut(field) { rewrite_paths(path, &remap); }
                        }
                        if let Some(evidence) = message.get_mut("evidence").and_then(serde_json::Value::as_array_mut) {
                            for item in evidence {
                                for field in ["path", "source_path"] {
                                    if let Some(path) = item.get_mut(field) { rewrite_paths(path, &remap); }
                                }
                            }
                        }
                    }
                },
                "drafts" => if let Some(path) = value.get_mut("path") { rewrite_paths(path, &remap); },
                "online_sources" => if let Some(path) = value.get_mut("directory") { rewrite_paths(path, &remap); },
                "online_items" => if let Some(files) = value.as_array_mut() {
                    for file in files {
                        if let Some(path) = file.get_mut("path") { rewrite_paths(path, &remap); }
                    }
                },
                _ => rewrite_paths(&mut value, &remap),
            }
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
        fs::create_dir(source.join("docs")).unwrap();
        let internal_root = source.join("docs").canonicalize().unwrap();
        let internal_pdf = internal_root.join("book.pdf");
        let cache_name = crate::store::hash(internal_root.as_os_str().as_encoded_bytes());
        let cached = source.canonicalize().unwrap().join(crate::docling::OUTPUT_DIRECTORY).join(cache_name).join("book.pdf.md");
        fs::create_dir_all(cached.parent().unwrap()).unwrap();
        fs::write(&cached, "# 已转换").unwrap();
        fs::create_dir(&root).unwrap();
        fs::write(&internal_pdf, "PDF fixture").unwrap();
        let external_root = Path::new(&root).canonicalize().unwrap();
        fs::write(&converted, "# 转换正文").unwrap();
        fs::write(source.join("credentials.json"), br#"{"answer":"keep-secret"}"#).unwrap();
        let api = serde_json::json!({"enabled":false,"host":"127.0.0.1","port":24681,"keys":[
            {"id":"scoped","name":"Project","prefix":"fixture","created_at":0,"digest":[1,2,3],
                "scope":{"type":"folders","directories":[
                    {"path":internal_root,"canonical":internal_root},
                    {"path":external_root,"canonical":external_root}
                ]}},
            {"id":"global","name":"Existing","prefix":"fixture","created_at":0,"digest":[4,5,6],"scope":{"type":"all"}}
        ]});
        let api_bytes = serde_json::to_vec(&api).unwrap();
        fs::write(source.join("http-api.json"), &api_bytes).unwrap();
        fs::write(source.join("sag-service.json"), "stale endpoint").unwrap();
        let db = store.connection();
        db.execute("INSERT INTO roots(path,name) VALUES(?1,'docs')", [&root]).unwrap();
        db.execute("INSERT INTO roots(path,name) VALUES(?1,'archive')", [internal_root.to_str().unwrap()]).unwrap();
        db.execute("INSERT INTO conversions(source,root,source_hash,output,output_hash) VALUES(?1,?2,'hash',?3,'out')",
            params![internal_pdf.to_str().unwrap(), internal_root.to_str().unwrap(), cached.to_str().unwrap()]).unwrap();
        db.execute("INSERT INTO conversions(source,root,source_hash,output,output_hash) VALUES(?1,?2,'hash',?3,'out')", params![external, root, internal]).unwrap();
        db.execute("INSERT INTO documents(root,path,name,hash,mtime) VALUES(?1,?2,'report','hash',0)", params![root, internal]).unwrap();
        let messages = serde_json::json!([{"answer":internal, "evidence":[{"path":internal},{"path":external}]}]);
        db.execute("INSERT INTO conversations(id,title,messages,updated) VALUES('chat','history',?1,0)", [messages.to_string()]).unwrap();
        db.execute("INSERT INTO drafts(path,value) VALUES(?1,?2)", params![internal, serde_json::json!({"path":internal,"text":"unsaved draft"}).to_string()]).unwrap();
        db.execute("INSERT OR REPLACE INTO preferences(id,value) VALUES(1,?1)", [serde_json::json!({
            "notes":{"folder":source.join("notes")},"external":root,"conversion_storage":{"mode":"app_data"}
        }).to_string()]).unwrap();
        db.execute("INSERT INTO online_sources(id,config,next_at) VALUES('feed',?1,42)", [serde_json::json!({
            "directory": internal_root, "name": internal
        }).to_string()]).unwrap();
        db.execute("INSERT INTO online_items(source,id,version,files) VALUES('feed','entry','1',?1)", [serde_json::json!([
            {"path": internal_pdf, "hash": "same-content"},
            {"path": external, "hash": "external-content"}
        ]).to_string()]).unwrap();
        drop(db);
        drop(store);
        let original = fs::read(source.join("sag.sqlite")).unwrap();
        migrate(&source, &destination, &location).unwrap();
        let destination = destination.canonicalize().unwrap();
        assert_eq!(serde_json::from_slice::<PathBuf>(&fs::read(location).unwrap()).unwrap(), destination);
        assert_eq!(fs::read(source.join("sag.sqlite")).unwrap(), original);
        assert_eq!(fs::read(source.join("http-api.json")).unwrap(), api_bytes);
        assert_eq!(fs::read_to_string(destination.join("credentials.json")).unwrap(), r#"{"answer":"keep-secret"}"#);
        assert_eq!(fs::read_to_string(destination.join("converted/report.md")).unwrap(), "# 转换正文");
        assert!(!destination.join("sag-service.json").exists());
        assert!(!destination.join("cli-instance.lock").exists());
        let db = Connection::open(destination.join("sag.sqlite")).unwrap();
        let (output, original_source): (String, String) = db.query_row("SELECT output,source FROM conversions WHERE source=?1", [&external], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(Path::new(&output), destination.join("converted/report.md"));
        assert_eq!(original_source, external);
        let text: String = db.query_row("SELECT messages FROM conversations", [], |r| r.get(0)).unwrap();
        let messages: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(messages[0]["answer"], internal);
        assert_eq!(messages[0]["evidence"][0]["path"], output);
        assert_eq!(messages[0]["evidence"][1]["path"], external);
        let (path, draft): (String, String) = db.query_row("SELECT path,value FROM drafts", [], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(path, output);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&draft).unwrap()["path"], output);
        let settings: String = db.query_row("SELECT value FROM preferences", [], |r| r.get(0)).unwrap();
        let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
        assert_eq!(settings["notes"]["folder"], destination.join("notes").to_str().unwrap());
        assert_eq!(settings["external"], root);
        let relocated_root = destination.join("docs");
        let (config, next_at): (String, i64) = db.query_row("SELECT config,next_at FROM online_sources WHERE id='feed'", [], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&config).unwrap(), serde_json::json!({
            "directory": relocated_root, "name": internal
        }));
        assert_eq!(next_at, 42);
        let files: String = db.query_row("SELECT files FROM online_items WHERE source='feed'", [], |r| r.get(0)).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&files).unwrap(), serde_json::json!([
            {"path": relocated_root.join("book.pdf"), "hash": "same-content"},
            {"path": external, "hash": "external-content"}
        ]));
        let migrated_api: serde_json::Value = serde_json::from_slice(&fs::read(destination.join("http-api.json")).unwrap()).unwrap();
        let mut expected_api = api;
        expected_api["web_allow_anonymous"] = serde_json::json!(false);
        expected_api["keys"][0]["scope"]["directories"][0] = serde_json::json!({
            "path":relocated_root, "canonical":relocated_root,
        });
        assert_eq!(migrated_api, expected_api);
        let (stored, output): (String, String) = db.query_row(
            "SELECT root,output FROM conversions WHERE source=?1",
            [relocated_root.join("book.pdf").to_str().unwrap()], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(Path::new(&stored), relocated_root);
        let new_hash = crate::store::hash(relocated_root.as_os_str().as_encoded_bytes());
        let relocated_output = destination.join(crate::docling::OUTPUT_DIRECTORY).join(new_hash).join("book.pdf.md");
        assert_eq!(Path::new(&output), relocated_output);
        assert_eq!(fs::read_to_string(relocated_output).unwrap(), "# 已转换");
        assert_eq!(fs::read_to_string(relocated_root.join("book.pdf")).unwrap(), "PDF fixture");
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
            [serde_json::json!({"notes":{"folder":alias.join("notes")}}).to_string()]).unwrap();
        drop(db);
        migrate(&alias, &destination, &location).unwrap();
        for directory in [&destination, &final_destination] {
            if directory == &final_destination {
                migrate(&destination, directory, &location).unwrap();
            }
            let directory = directory.canonicalize().unwrap();
            let executable = directory.join("tools/python3");
            assert_eq!(fs::read_link(&executable).unwrap(), PathBuf::from("python"));
            assert_eq!(fs::metadata(&executable).unwrap().permissions().mode() & 0o777, 0o755);
            let output = std::process::Command::new(&executable).output().unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, b"runtime-ok");
            assert_eq!(fs::read_to_string(directory.join("tools/external")).unwrap(), "external document");
            let db = Connection::open(directory.join("sag.sqlite")).unwrap();
            let settings: String = db.query_row("SELECT value FROM preferences", [], |r| r.get(0)).unwrap();
            let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
            assert_eq!(settings["notes"]["folder"], directory.join("notes").to_str().unwrap());
            assert_eq!(serde_json::from_slice::<PathBuf>(&fs::read(&location).unwrap()).unwrap(), directory);
        }
        assert_eq!(fs::read_link(tools.join("python3")).unwrap(), PathBuf::from("python"));
        assert_eq!(fs::read_to_string(tools.join("external")).unwrap(), "external document");
    }

    #[test]
    fn recovery_requires_an_existing_sag_database_and_preserves_the_unavailable_location_on_error() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing drive");
        let location = temp.path().join("location.json");
        fs::write(&location, serde_json::to_vec(&missing).unwrap()).unwrap();
        assert!(recover(temp.path(), &location).is_err());
        assert_eq!(sag_protocol::read_data_directory_location(&location).unwrap(), Some(missing));
        let backup = temp.path().join("backup");
        drop(Store::open(&backup.join("sag.sqlite")).unwrap());
        recover(&backup, &location).unwrap();
        assert_eq!(sag_protocol::read_data_directory_location(&location).unwrap(), Some(backup.canonicalize().unwrap()));
        fs::write(&location, "invalid JSON").unwrap();
        recover(&backup, &location).unwrap();
        assert_eq!(sag_protocol::read_data_directory_location(&location).unwrap(), Some(backup.canonicalize().unwrap()));
    }
}
