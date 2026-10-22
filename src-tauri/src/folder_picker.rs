use crate::{error, CommandResult};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct Folder {
    name: String,
    path: String,
}

#[derive(Serialize)]
pub struct FolderListing {
    path: String,
    parent: Option<String>,
    home: String,
    folders: Vec<Folder>,
}

fn list(path: Option<String>, show_hidden: bool) -> CommandResult<FolderListing> {
    let home = dirs::home_dir().ok_or("无法获取用户目录")?;
    let path = path.map(PathBuf::from).unwrap_or_else(|| home.clone());
    let path = path.canonicalize().map_err(error)?;
    let mut folders = Vec::new();
    for entry in std::fs::read_dir(&path).map_err(error)? {
        let entry = entry.map_err(error)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if (!show_hidden && name.starts_with('.')) || !entry.file_type().map_err(error)?.is_dir() {
            continue;
        }
        folders.push(Folder {
            name,
            path: entry.path().to_string_lossy().into_owned(),
        });
    }
    folders.sort_by_key(|folder| folder.name.to_lowercase());
    Ok(FolderListing {
        parent: path
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned()),
        path: path.to_string_lossy().into_owned(),
        home: home.to_string_lossy().into_owned(),
        folders,
    })
}

#[tauri::command]
pub async fn browse_folders(
    path: Option<String>,
    show_hidden: bool,
) -> CommandResult<FolderListing> {
    tauri::async_runtime::spawn_blocking(move || list(path, show_hidden))
        .await
        .map_err(error)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_unindexed_folders_without_reading_file_contents() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("beta")).unwrap();
        std::fs::create_dir(temp.path().join("Alpha")).unwrap();
        std::fs::create_dir(temp.path().join(".hidden")).unwrap();
        std::fs::write(temp.path().join("notes.txt"), "not a folder").unwrap();
        let path = temp.path().to_string_lossy().into_owned();
        let result = list(Some(path.clone()), false).unwrap();
        assert_eq!(
            result
                .folders
                .iter()
                .map(|folder| folder.name.as_str())
                .collect::<Vec<_>>(),
            ["Alpha", "beta"]
        );
        assert_eq!(
            result.path,
            temp.path().canonicalize().unwrap().to_string_lossy()
        );
        assert!(result.parent.is_some());
        assert_eq!(list(Some(path), true).unwrap().folders.len(), 3);
    }

    #[test]
    fn reports_invalid_paths_instead_of_returning_an_empty_directory() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("file.txt");
        std::fs::write(&file, "content").unwrap();
        assert!(list(Some(file.to_string_lossy().into_owned()), false).is_err());
        assert!(list(
            Some(temp.path().join("missing").to_string_lossy().into_owned()),
            false
        )
        .is_err());
    }
}
