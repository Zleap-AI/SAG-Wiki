use crate::{error, rpc, AppState, CommandResult};
use sag_protocol::{types::Root, Operation};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

pub fn prepare_default_library(documents: &Path) -> CommandResult<PathBuf> {
    let root = documents.join("sag-wiki");
    std::fs::create_dir_all(root.join("clippings")).map_err(error)?;
    Ok(root)
}

#[derive(Serialize)]
pub struct ClipperSettings {
    packages: Vec<String>,
}

fn package(app: &tauri::AppHandle, browser: &str) -> CommandResult<PathBuf> {
    let filename = match browser {
        "chrome" => "sag-web-clipper-chrome.zip",
        "safari" => "sag-web-clipper-safari.dmg",
        _ => return Err("不支持的浏览器".into()),
    };
    let base = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    } else {
        app.path().resource_dir().map_err(error)?
    };
    Ok(base.join("clipper").join(filename))
}

#[tauri::command]
pub fn get_web_clipper(state: State<Arc<AppState>>) -> CommandResult<ClipperSettings> {
    let mut packages = Vec::new();
    for browser in ["chrome", "safari"] {
        if package(&state.app, browser)?.is_file() {
            packages.push(browser.to_string());
        }
    }
    Ok(ClipperSettings { packages })
}

#[tauri::command]
pub async fn add_web_clipper_folder(
    state: State<'_, Arc<AppState>>,
) -> CommandResult<Option<String>> {
    let app = state.app.clone();
    let selected = tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app.dialog().file();
        if let Some(documents) = dirs::document_dir() {
            let folder = documents.join("sag-wiki/clippings");
            dialog = dialog.set_directory(if folder.is_dir() { folder } else { documents });
        }
        dialog.blocking_pick_folder()
    })
    .await
    .map_err(error)?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected
        .into_path()
        .map_err(error)?
        .canonicalize()
        .map_err(error)?;
    let roots: Vec<Root> =
        serde_json::from_value(rpc(&state, Operation::ListRoots).await?).map_err(error)?;
    if !roots.iter().any(|root| path.starts_with(&root.path)) {
        rpc(
            &state,
            Operation::AddRoot {
                path: path.display().to_string(),
            },
        )
        .await?;
    }
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
pub async fn export_web_clipper(
    browser: String,
    state: State<'_, Arc<AppState>>,
) -> CommandResult<Option<String>> {
    let source = package(&state.app, &browser)?;
    if !source.is_file() {
        return Err("当前版本未附带此浏览器的扩展安装包".into());
    }
    let app = state.app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_file_name(source.file_name().unwrap().to_string_lossy())
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(error)?;
        std::fs::copy(source, &path).map_err(error)?;
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(error)?
}

#[cfg(test)]
mod tests {
    #[test]
    fn creates_the_default_library_without_changing_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = super::prepare_default_library(temp.path()).unwrap();
        assert_eq!(root, temp.path().join("sag-wiki"));
        assert!(root.join("clippings").is_dir());
        let note = root.join("clippings/existing.md");
        std::fs::write(&note, "keep").unwrap();
        super::prepare_default_library(temp.path()).unwrap();
        assert_eq!(std::fs::read_to_string(note).unwrap(), "keep");
    }
}
