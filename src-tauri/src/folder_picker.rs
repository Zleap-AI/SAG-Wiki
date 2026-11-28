use crate::{error, CommandResult};
use serde::Serialize;
use std::path::{Path, PathBuf};

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

/// A shortcut in the picker's location menu: a common folder or a mounted volume.
#[derive(Debug, PartialEq, Serialize)]
pub struct Location {
    kind: &'static str,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Serialize)]
pub struct FolderLocations {
    places: Vec<Location>,
    volumes: Vec<Location>,
}

fn location(kind: &'static str, path: &Path, name: Option<String>) -> Location {
    Location {
        kind,
        path: path.to_string_lossy().into_owned(),
        name,
    }
}

fn places() -> Vec<Location> {
    let mut places: Vec<Location> = Vec::new();
    for (kind, path) in [
        ("home", dirs::home_dir()),
        ("desktop", dirs::desktop_dir()),
        ("documents", dirs::document_dir()),
        ("downloads", dirs::download_dir()),
    ] {
        // Unconfigured XDG folders fall back to the home directory on Linux.
        if let Some(path) = path.filter(|path| path.is_dir()) {
            let path = location(kind, &path, None);
            if !places.iter().any(|place| place.path == path.path) {
                places.push(path);
            }
        }
    }
    places
}

/// Typing only a drive letter means its root, not the drive's current directory.
fn requested_path(path: &str) -> PathBuf {
    let bytes = path.as_bytes();
    if cfg!(windows) && bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return PathBuf::from(format!("{path}\\"));
    }
    PathBuf::from(path)
}

/// Mount points directly below `base` (external disks, disk images, network shares).
#[cfg(unix)]
fn mounted_children(
    base: &Path,
    is_mount: impl Fn(&Path, &Path) -> bool,
) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(base) else {
        return Vec::new();
    };
    let mut children: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            (!name.starts_with('.')
                && entry.file_type().is_ok_and(|kind| kind.is_dir())
                && is_mount(base, &path))
            .then_some((name, path))
        })
        .collect();
    children.sort_by_key(|(name, _)| name.to_lowercase());
    children
}

#[cfg(unix)]
fn on_other_device(base: &Path, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(base), std::fs::metadata(path)) {
        (Ok(base), Ok(path)) => base.dev() != path.dev(),
        _ => false,
    }
}

/// macOS lists every volume in /Volumes; the one linking to / is the startup disk.
#[cfg(all(unix, any(test, target_os = "macos")))]
fn macos_volumes(base: &Path, is_mount: impl Fn(&Path, &Path) -> bool) -> Vec<Location> {
    let startup = std::fs::read_dir(base).ok().and_then(|entries| {
        entries.flatten().find_map(|entry| {
            let target = std::fs::read_link(entry.path()).ok()?;
            (target == Path::new("/")).then(|| entry.file_name().to_string_lossy().into_owned())
        })
    });
    let mut volumes = vec![location("system", Path::new("/"), startup)];
    volumes.extend(
        mounted_children(base, is_mount)
            .into_iter()
            .map(|(name, path)| location("volume", &path, Some(name))),
    );
    volumes
}

#[cfg(all(unix, any(test, not(target_os = "macos"))))]
fn linux_volumes(bases: &[PathBuf], is_mount: impl Fn(&Path, &Path) -> bool) -> Vec<Location> {
    let mut volumes = vec![location("system", Path::new("/"), None)];
    for base in bases {
        for (name, path) in mounted_children(base, &is_mount) {
            if !volumes.iter().any(|volume| Path::new(&volume.path) == path) {
                volumes.push(location("volume", &path, Some(name)));
            }
        }
    }
    volumes
}

#[cfg(target_os = "macos")]
fn volumes() -> Vec<Location> {
    macos_volumes(Path::new("/Volumes"), on_other_device)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn volumes() -> Vec<Location> {
    let mut bases = Vec::new();
    if let Ok(user) = std::env::var("USER") {
        bases.push(Path::new("/media").join(&user));
        bases.push(Path::new("/run/media").join(&user));
    }
    bases.extend([PathBuf::from("/media"), PathBuf::from("/mnt")]);
    linux_volumes(&bases, on_other_device)
}

#[cfg(windows)]
fn volumes() -> Vec<Location> {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };
    use windows::Win32::System::Diagnostics::Debug::{
        SetThreadErrorMode, SEM_FAILCRITICALERRORS, SEM_NOOPENFILEERRORBOX, THREAD_ERROR_MODE,
    };
    // GetDriveTypeW values; the constants live in a large unrelated windows-rs module.
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;
    const DRIVE_RAMDISK: u32 = 6;
    // Probing an empty card reader or optical drive must fail quietly, not
    // raise the system "insert a disk" dialog.
    let mut previous = THREAD_ERROR_MODE::default();
    let quiet = unsafe {
        SetThreadErrorMode(
            SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX,
            Some(&mut previous),
        )
    }
    .is_ok();
    let mask = unsafe { GetLogicalDrives() };
    let mut volumes = Vec::new();
    for index in 0..26u8 {
        if mask & (1 << index) == 0 {
            continue;
        }
        let root = format!("{}:\\", (b'A' + index) as char);
        let wide = HSTRING::from(root.as_str());
        let kind = match unsafe { GetDriveTypeW(&wide) } {
            DRIVE_FIXED | DRIVE_RAMDISK => "fixed",
            DRIVE_REMOVABLE => "removable",
            DRIVE_REMOTE => "network",
            DRIVE_CDROM => "optical",
            _ => continue,
        };
        // A disconnected network share can block for seconds; show its letter only.
        let name = if kind == "network" {
            None
        } else {
            let mut label = [0u16; 261];
            match unsafe { GetVolumeInformationW(&wide, Some(&mut label), None, None, None, None) } {
                Ok(()) => {
                    let end = label.iter().position(|&unit| unit == 0).unwrap_or(label.len());
                    Some(String::from_utf16_lossy(&label[..end])).filter(|name| !name.trim().is_empty())
                }
                // No media in the drive.
                Err(_) if kind != "fixed" => continue,
                Err(_) => None,
            }
        };
        volumes.push(location(kind, Path::new(&root), name));
    }
    if quiet {
        let _ = unsafe { SetThreadErrorMode(previous, None) };
    }
    volumes
}

#[tauri::command]
pub async fn folder_locations() -> CommandResult<FolderLocations> {
    tauri::async_runtime::spawn_blocking(|| FolderLocations {
        places: places(),
        volumes: volumes(),
    })
    .await
    .map_err(error)
}

fn list(path: Option<String>, show_hidden: bool) -> CommandResult<FolderListing> {
    let home = dirs::home_dir().ok_or("无法获取用户目录")?;
    let path = path
        .map(|path| requested_path(&path))
        .unwrap_or_else(|| home.clone());
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

    #[test]
    fn common_places_exist_and_are_listed_once() {
        let places = places();
        assert_eq!(places.first().map(|place| place.kind), Some("home"));
        for place in &places {
            assert!(Path::new(&place.path).is_dir());
            assert_eq!(places.iter().filter(|other| other.path == place.path).count(), 1);
        }
    }

    #[test]
    fn bare_drive_letters_open_the_drive_root() {
        if cfg!(windows) {
            assert_eq!(requested_path("D:"), PathBuf::from("D:\\"));
        } else {
            assert_eq!(requested_path("D:"), PathBuf::from("D:"));
        }
        assert_eq!(requested_path("D:\\data"), PathBuf::from("D:\\data"));
    }

    #[cfg(unix)]
    #[test]
    fn macos_lists_the_startup_disk_first_then_mounted_volumes() {
        let temp = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/", temp.path().join("Macintosh HD")).unwrap();
        std::fs::create_dir(temp.path().join("backup")).unwrap();
        std::fs::create_dir(temp.path().join("Archive")).unwrap();
        std::fs::create_dir(temp.path().join(".timemachine")).unwrap();
        std::fs::create_dir(temp.path().join("stale")).unwrap();
        std::fs::write(temp.path().join("note.txt"), "").unwrap();
        let volumes = macos_volumes(temp.path(), |_, path| !path.ends_with("stale"));
        assert_eq!(
            volumes,
            [
                location("system", Path::new("/"), Some("Macintosh HD".into())),
                location("volume", &temp.path().join("Archive"), Some("Archive".into())),
                location("volume", &temp.path().join("backup"), Some("backup".into())),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn linux_lists_root_and_mount_points_once() {
        let temp = tempfile::tempdir().unwrap();
        let media = temp.path().join("media");
        std::fs::create_dir_all(media.join("usb")).unwrap();
        std::fs::create_dir_all(temp.path().join("mnt/data")).unwrap();
        let volumes = linux_volumes(&[media.clone(), media.clone(), temp.path().join("mnt")], |_, _| true);
        assert_eq!(
            volumes,
            [
                location("system", Path::new("/"), None),
                location("volume", &media.join("usb"), Some("usb".into())),
                location("volume", &temp.path().join("mnt/data"), Some("data".into())),
            ]
        );
        // Plain directories on the same device are not mount points.
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("empty")).unwrap();
        assert_eq!(linux_volumes(&[temp.path().to_path_buf()], on_other_device).len(), 1);
    }
}
