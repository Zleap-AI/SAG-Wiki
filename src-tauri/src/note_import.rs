use crate::{error, rpc, AppState, CommandResult};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{types::ValueRef, Connection, OpenFlags};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::State;
use tauri_plugin_dialog::DialogExt;

#[derive(Default)]
pub struct Imports(Mutex<HashMap<u64, Import>>);
struct Import {
    staging: tempfile::TempDir,
    source: Option<PathBuf>,
    destination: PathBuf,
    published: bool,
}
#[derive(Serialize)]
pub struct Prepared {
    session: u64,
    destination: String,
    folders: Vec<Value>,
}

fn relative(path: &str) -> CommandResult<&Path> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("导入文件路径无效".into());
    }
    Ok(path)
}
fn query(connection: &Connection, sql: &str, params: Vec<Value>) -> CommandResult<Vec<Value>> {
    let mut statement = connection.prepare(sql).map_err(error)?;
    if !sql
        .split_whitespace()
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("SELECT"))
        || !statement.readonly()
    {
        return Err("只允许读取备忘录数据库".into());
    }
    let names: Vec<String> = statement
        .column_names()
        .iter()
        .map(|name| name.to_string())
        .collect();
    let values: Vec<rusqlite::types::Value> = params
        .into_iter()
        .map(|value| match value {
            Value::Null => Ok(rusqlite::types::Value::Null),
            Value::String(value) => Ok(rusqlite::types::Value::Text(value)),
            Value::Number(value) => value
                .as_i64()
                .map(rusqlite::types::Value::Integer)
                .or_else(|| value.as_f64().map(rusqlite::types::Value::Real))
                .ok_or("Invalid SQL number"),
            _ => Err("Invalid SQL parameter"),
        })
        .collect::<Result<_, _>>()
        .map_err(error)?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(values), |row| {
            let mut result = serde_json::Map::new();
            for (index, name) in names.iter().enumerate() {
                let value = match row.get_ref(index)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(v) => json!(v),
                    ValueRef::Real(v) => json!(v),
                    ValueRef::Text(v) => json!(String::from_utf8_lossy(v)),
                    ValueRef::Blob(v) => json!(STANDARD.encode(v)),
                };
                result.entry(name.clone()).or_insert(value);
            }
            Ok(Value::Object(result))
        })
        .map_err(error)?;
    rows.collect::<Result<_, _>>().map_err(error)
}

#[tauri::command]
pub async fn choose_note_import_folder(
    state: State<'_, Arc<AppState>>,
) -> CommandResult<Option<String>> {
    let app = state.app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_folder()
            .map(|file| {
                file.into_path()
                    .map(|path| path.to_string_lossy().into_owned())
                    .map_err(error)
            })
            .transpose()
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub fn open_full_disk_access() -> CommandResult<()> {
    #[cfg(target_os = "macos")]
    {
        crate::open_browser_url(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Apple Notes 导入仅支持 macOS".into())
    }
}

fn check_notes_access(path: &Path) -> CommandResult<()> {
    fs::File::open(path).map(|_| ()).map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            "APPLE_NOTES_PERMISSION_REQUIRED".into()
        } else {
            format!("无法读取 Apple Notes 数据文件：{error}")
        }
    })
}

#[tauri::command]
pub async fn prepare_note_import(
    source: String,
    destination: Option<String>,
    apple_path: Option<String>,
    imports: State<'_, Imports>,
) -> CommandResult<Prepared> {
    if !["notion", "evernote", "keep", "apple"].contains(&source.as_str()) {
        return Err("未知的导入来源".into());
    }
    if source == "apple" && !cfg!(target_os = "macos") {
        return Err("Apple Notes 导入仅支持 macOS".into());
    }
    let base = destination.map(PathBuf::from).unwrap_or_else(|| {
        dirs::document_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap())
            .join("sag-wiki")
            .join("notes")
    });
    let staging = tempfile::tempdir().map_err(error)?;
    let mut folders = vec![];
    let source_path = if source == "apple" {
        let path = apple_path.map(PathBuf::from).unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap()
                .join("Library/Group Containers/group.com.apple.notes")
        });
        let database_path = path.join("NoteStore.sqlite");
        check_notes_access(&database_path)?;
        let original = Connection::open_with_flags(database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(error)?;
        let mut snapshot = Connection::open(staging.path().join("notes.sqlite")).map_err(error)?;
        rusqlite::backup::Backup::new(&original, &mut snapshot)
            .map_err(error)?
            .run_to_completion(100, std::time::Duration::from_millis(10), None)
            .map_err(error)?;
        folders=query(&snapshot,"SELECT z_pk AS ID, ztitle2 AS NAME FROM ziccloudsyncingobject WHERE z_ent=(SELECT z_ent FROM z_primarykey WHERE z_name='ICFolder') AND ztitle2 IS NOT NULL AND (zfoldertype IS NULL OR zfoldertype NOT IN (1,3)) ORDER BY ztitle2",vec![])?;
        Some(path.canonicalize().map_err(error)?)
    } else {
        None
    };
    let session = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(error)?
        .as_micros() as u64;
    let name = match source.as_str() {
        "notion" => "Notion",
        "evernote" => "Evernote",
        "keep" => "Google Keep",
        _ => "Apple Notes",
    };
    let destination = base.join(name);
    let result = Prepared {
        session,
        destination: destination.to_string_lossy().into_owned(),
        folders,
    };
    imports.0.lock().unwrap().insert(
        session,
        Import {
            staging,
            source: source_path,
            destination,
            published: false,
        },
    );
    Ok(result)
}
#[tauri::command]
pub async fn query_apple_notes(
    session: u64,
    sql: String,
    params: Vec<Value>,
    imports: State<'_, Imports>,
) -> CommandResult<Vec<Value>> {
    let entries = imports.0.lock().unwrap();
    let import = entries.get(&session).ok_or("导入已关闭")?;
    if import.source.is_none() {
        return Err("此导入没有备忘录数据库".into());
    }
    let connection = Connection::open_with_flags(
        import.staging.path().join("notes.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(error)?;
    query(&connection, &sql, params)
}
#[tauri::command]
pub async fn read_apple_attachment(
    session: u64,
    account: String,
    path: String,
    imports: State<'_, Imports>,
) -> CommandResult<String> {
    let entries = imports.0.lock().unwrap();
    let import = entries.get(&session).ok_or("导入已关闭")?;
    let root = import.source.as_ref().ok_or("此导入没有备忘录附件")?;
    let path = relative(&path)?;
    let account = relative(&account)?;
    for candidate in [
        root.join("Accounts").join(account).join(path),
        root.join(path),
    ] {
        match candidate.canonicalize() {
            Ok(resolved) => {
                if !resolved.starts_with(root) {
                    return Err("附件位于备忘录文件夹之外".into());
                }
                return fs::read(resolved)
                    .map(|bytes| STANDARD.encode(bytes))
                    .map_err(error);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(error(e)),
        }
    }
    Err(format!("找不到附件：{}", path.display()))
}
#[tauri::command]
pub async fn write_note_import_file(
    session: u64,
    path: String,
    data: String,
    imports: State<'_, Imports>,
) -> CommandResult<()> {
    let entries = imports.0.lock().unwrap();
    let import = entries.get(&session).ok_or("导入已关闭")?;
    let output = import.staging.path().join("output").join(relative(&path)?);
    fs::create_dir_all(output.parent().unwrap()).map_err(error)?;
    let bytes = STANDARD.decode(data).map_err(error)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(output)
        .map_err(error)?;
    file.write_all(&bytes).map_err(error)
}
#[derive(Serialize)]
pub struct ExistingFile {
    path: String,
    data: String,
}

fn read_existing(
    root: &Path,
    directory: &Path,
    files: &mut Vec<ExistingFile>,
) -> CommandResult<()> {
    for entry in fs::read_dir(directory).map_err(error)? {
        let entry = entry.map_err(error)?;
        let kind = entry.file_type().map_err(error)?;
        if kind.is_symlink() {
            return Err("导入目录包含符号链接，请选择其他位置".into());
        }
        if kind.is_dir() {
            read_existing(root, &entry.path(), files)?;
        } else if kind.is_file() {
            files.push(ExistingFile {
                path: entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                data: STANDARD.encode(fs::read(entry.path()).map_err(error)?),
            });
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn read_note_import_files(
    session: u64,
    imports: State<'_, Imports>,
) -> CommandResult<Vec<ExistingFile>> {
    let entries = imports.0.lock().unwrap();
    let import = entries.get(&session).ok_or("导入已关闭")?;
    let mut files = Vec::new();
    if import.destination.try_exists().map_err(error)? {
        if fs::symlink_metadata(&import.destination)
            .map_err(error)?
            .file_type()
            .is_symlink()
        {
            return Err("导入目录包含符号链接，请选择其他位置".into());
        }
        read_existing(&import.destination, &import.destination, &mut files)?;
    }
    Ok(files)
}
fn merge_output(from: &Path, to: &Path) -> CommandResult<()> {
    if to.try_exists().map_err(error)?
        && fs::symlink_metadata(to)
            .map_err(error)?
            .file_type()
            .is_symlink()
    {
        return Err("导入目录包含符号链接，请选择其他位置".into());
    }
    fs::create_dir_all(to).map_err(error)?;
    for entry in fs::read_dir(from).map_err(error)? {
        let entry = entry.map_err(error)?;
        let dest = to.join(entry.file_name());
        if entry.file_type().map_err(error)?.is_dir() {
            merge_output(&entry.path(), &dest)?;
        } else {
            let bytes = fs::read(entry.path()).map_err(error)?;
            if dest.try_exists().map_err(error)? {
                if fs::symlink_metadata(&dest)
                    .map_err(error)?
                    .file_type()
                    .is_symlink()
                    || fs::read(&dest).map_err(error)? != bytes
                {
                    return Err(format!("文件已更改，请重新导入：{}", dest.display()));
                }
            } else {
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&dest)
                    .map_err(error)?
                    .write_all(&bytes)
                    .map_err(error)?;
            }
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn finish_note_import(
    session: u64,
    imports: State<'_, Imports>,
    state: State<'_, Arc<AppState>>,
) -> CommandResult<String> {
    let (destination, has_output) = {
        let mut entries = imports.0.lock().unwrap();
        let import = entries.get_mut(&session).ok_or("导入已关闭")?;
        let output = import.staging.path().join("output");
        if output.is_dir() && !import.published {
            merge_output(&output, &import.destination)?;
            import.published = true;
        }
        (import.destination.clone(), output.is_dir())
    };
    if has_output {
        let roots: Vec<sag_protocol::types::Root> =
            serde_json::from_value(rpc(&state, sag_protocol::Operation::ListRoots).await?)
                .map_err(error)?;
        if let Some(root) = roots
            .iter()
            .find(|root| destination.starts_with(&root.path))
        {
            rpc(
                &state,
                sag_protocol::Operation::RefreshRoot {
                    path: root.path.clone(),
                },
            )
            .await?;
        } else {
            rpc(
                &state,
                sag_protocol::Operation::AddRoot {
                    path: destination.to_string_lossy().into_owned(),
                },
            )
            .await?;
        }
    }
    imports.0.lock().unwrap().remove(&session);
    Ok(destination.to_string_lossy().into_owned())
}
#[tauri::command]
pub fn close_note_import(session: u64, imports: State<'_, Imports>) {
    imports.0.lock().unwrap().remove(&session);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::Manager;
    #[test]
    fn repeated_import_merges_without_overwriting_notes() {
        let input = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        fs::write(input.path().join("note.md"), "original").unwrap();
        merge_output(input.path(), target.path()).unwrap();
        merge_output(input.path(), target.path()).unwrap();
        fs::write(input.path().join("second.md"), "second").unwrap();
        merge_output(input.path(), target.path()).unwrap();
        assert_eq!(
            fs::read_to_string(target.path().join("second.md")).unwrap(),
            "second"
        );
        fs::write(target.path().join("note.md"), "user edit").unwrap();
        assert!(merge_output(input.path(), target.path()).is_err());
        assert_eq!(
            fs::read_to_string(target.path().join("note.md")).unwrap(),
            "user edit"
        );
    }
    #[test]
    fn writes_markdown_and_binary_into_isolated_staging() {
        tauri::async_runtime::block_on(async {
            let app = tauri::test::mock_builder()
                .manage(Imports::default())
                .build(tauri::test::mock_context(tauri::test::noop_assets()))
                .unwrap();
            let prepared = prepare_note_import("notion".into(), None, None, app.state())
                .await
                .unwrap();
            let default_folder = dirs::document_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap())
                .join("sag-wiki")
                .join("notes");
            assert_eq!(
                Path::new(&prepared.destination),
                default_folder.join("Notion").as_path()
            );

            write_note_import_file(
                prepared.session,
                "Notes/test.md".into(),
                STANDARD.encode("# test"),
                app.state(),
            )
            .await
            .unwrap();
            write_note_import_file(
                prepared.session,
                "image.png".into(),
                STANDARD.encode([0, 1, 255]),
                app.state(),
            )
            .await
            .unwrap();
            assert!(write_note_import_file(
                prepared.session,
                "../escape.md".into(),
                STANDARD.encode("bad"),
                app.state()
            )
            .await
            .is_err());
            {
                let imports = app.state::<Imports>();
                let entries = imports.0.lock().unwrap();
                let path = entries[&prepared.session].staging.path().join("output");
                assert_eq!(fs::read(path.join("image.png")).unwrap(), vec![0, 1, 255]);
                assert_eq!(
                    fs::read_to_string(path.join("Notes/test.md")).unwrap(),
                    "# test"
                );
            }
            close_note_import(prepared.session, app.state());
            assert!(app.state::<Imports>().0.lock().unwrap().is_empty());
        });
    }

    #[test]
    fn rejects_paths_outside_destination() {
        for path in ["../x", "/tmp/x", "a/../../x", ""] {
            assert!(relative(path).is_err())
        }
        assert!(relative("folder/note.md").is_ok());
    }
    #[test]
    fn apple_database_preserves_column_names_and_snapshot() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("../../tests/fixtures/note-import/apple.sql"))
            .unwrap();
        let result = query(
            &db,
            "SELECT hex(ZDATA) AS zhexdata FROM zicnotedata",
            vec![],
        )
        .unwrap();
        assert!(result[0]["zhexdata"].as_str().unwrap().starts_with("1F8B"));
        db.execute(
            "UPDATE ziccloudsyncingobject SET ZISPASSWORDPROTECTED=1 WHERE Z_ENT=3",
            [],
        )
        .unwrap();
        let locked=query(&db,"SELECT * FROM (SELECT *, NULL AS ZISPASSWORDPROTECTED FROM ziccloudsyncingobject) WHERE Z_ENT=3",vec![]).unwrap();
        assert_eq!(locked[0]["ZISPASSWORDPROTECTED"], 1);

        let mut snapshot = Connection::open_in_memory().unwrap();
        rusqlite::backup::Backup::new(&db, &mut snapshot)
            .unwrap()
            .run_to_completion(100, std::time::Duration::from_millis(1), None)
            .unwrap();
        assert_eq!(
            query(
                &snapshot,
                "SELECT ZTITLE1 FROM ziccloudsyncingobject WHERE Z_ENT=3",
                vec![]
            )
            .unwrap()[0]["ZTITLE1"],
            "Test note"
        );
    }

    #[test]
    fn reads_sql_without_mutating() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE notes(title TEXT);INSERT INTO notes VALUES ('hello')")
            .unwrap();
        assert_eq!(
            query(
                &db,
                "SELECT title FROM notes WHERE title=?",
                vec![json!("hello")]
            )
            .unwrap()[0]["title"],
            "hello"
        );
        assert!(query(&db, "DELETE FROM notes", vec![]).is_err());
    }
}
