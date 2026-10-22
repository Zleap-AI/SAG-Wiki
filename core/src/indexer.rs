use crate::{
    store::Store,
    types::{Root, Settings},
};
use anyhow::Result;
use notify::{event::ModifyKind, Event, EventKind};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub enum IndexRequest {
    Scan { root: String, verify: bool },
    Paths { root: String, paths: Vec<PathBuf> },
}

pub fn add_root(
    store: &Store,
    watcher: &std::sync::Mutex<notify::RecommendedWatcher>,
    scan: &mpsc::Sender<IndexRequest>,
    path: &std::path::Path,
) -> Result<String> {
    use notify::Watcher;
    let mut watcher = watcher.lock().unwrap();
    let root = store.add_root(path)?;
    watcher.watch(std::path::Path::new(&root), notify::RecursiveMode::Recursive)?;
    store.mark_root_pending(&root)?;
    scan.send(IndexRequest::Scan { root: root.clone(), verify: false })?;
    Ok(root)
}

pub fn event_requests(event: &Event, roots: &[Root], settings: &Settings) -> Vec<IndexRequest> {
    if !settings.watch {
        return vec![];
    }
    if event.need_rescan() {
        return roots
            .iter()
            .filter(|root| {
                event.paths.is_empty() || event.paths.iter().any(|p| p.starts_with(&root.path))
            })
            .map(|root| IndexRequest::Scan {
                root: root.path.clone(),
                verify: true,
            })
            .collect();
    }
    if matches!(event.kind, EventKind::Access(_)) {
        return vec![];
    }
    roots
        .iter()
        .filter_map(|root| {
            let rules: Vec<_> = event.paths.iter().filter(|path| path.starts_with(&root.path) && crate::ignore_rules::is_rule_file(path)).cloned().collect();
            if !rules.is_empty() {
                return Some(IndexRequest::Paths { root: root.path.clone(), paths: rules });
            }
            let paths: Vec<_> = event
                .paths
                .iter()
                .filter(|path| {
                    // Directory permissions and attributes do not change indexed content.
                    !(matches!(event.kind, EventKind::Modify(ModifyKind::Metadata(_)))
                        && path.is_dir())
                        && !crate::text::in_hidden_directory(std::path::Path::new(&root.path), path, path.is_dir())
                        && path.strip_prefix(&root.path).is_ok_and(|relative| {
                            !relative.components().any(|part| {
                                settings
                                    .excluded
                                    .iter()
                                    .any(|name| part.as_os_str() == name.as_str())
                            })
                        })
                })
                .cloned()
                .collect();
            (!paths.is_empty()).then(|| IndexRequest::Paths {
                root: root.path.clone(),
                paths,
            })
        })
        .collect()
}

pub fn coalesce(requests: impl IntoIterator<Item = IndexRequest>) -> Vec<IndexRequest> {
    let mut roots = BTreeMap::<String, (Option<bool>, BTreeSet<PathBuf>)>::new();
    for request in requests {
        match request {
            IndexRequest::Scan { root, verify } => {
                let state = roots.entry(root).or_default();
                state.0 = Some(state.0.unwrap_or(false) || verify);
                state.1.clear();
            }
            IndexRequest::Paths { root, paths } => {
                let state = roots.entry(root).or_default();
                if state.0.is_none() {
                    state.1.extend(paths);
                }
            }
        }
    }
    roots
        .into_iter()
        .map(|(root, (scan, paths))| match scan {
            Some(verify) => IndexRequest::Scan { root, verify },
            None => {
                let mut minimal: Vec<PathBuf> = vec![];
                for path in paths {
                    if !minimal.iter().any(|parent| path.starts_with(parent)) {
                        minimal.push(path);
                    }
                }
                IndexRequest::Paths {
                    root,
                    paths: minimal,
                }
            }
        })
        .collect()
}

pub fn receive_batch(
    receiver: &mpsc::Receiver<IndexRequest>,
    first: IndexRequest,
) -> Vec<IndexRequest> {
    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut requests = vec![first];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(300))) {
            Ok(request) => requests.push(request),
            Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    coalesce(requests)
}

pub fn apply_request(
    store: &Store,
    request: IndexRequest,
    progress: impl Fn(crate::types::IndexProgress),
) -> Result<usize> {
    let root = match &request {
        IndexRequest::Scan { root, .. } | IndexRequest::Paths { root, .. } => root,
    };
    if !store
        .roots()?
        .iter()
        .any(|registered| &registered.path == root)
    {
        return Ok(0);
    }
    match request {
        IndexRequest::Scan { root, verify } => {
            if verify {
                store.verify_root(&root)?;
            }
            store.scan(&root, progress)
        }
        IndexRequest::Paths { root, paths } => store.update_paths(&root, &paths, progress),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{
        AccessKind, CreateKind, DataChange, Flag, ModifyKind, RemoveKind, RenameMode,
    };
    use notify::{RecursiveMode, Watcher};
    use std::fs;

    fn apply(store: &Store, event: Event) -> Result<()> {
        let requests = event_requests(&event, &store.roots()?, &store.settings()?);
        for request in coalesce(requests) {
            apply_request(store, request, |_| {})?;
        }
        Ok(())
    }

    #[test]
    fn file_events_update_only_the_changed_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("files");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let store = Store::open(&dir.path().join("index.sqlite")).unwrap();
        let mut settings = store.settings().unwrap();
        settings.file_types.code = true;
        store.save_settings(&settings).unwrap();
        let root_text = store.add_root(&root).unwrap();
        let unrelated = root.join("unrelated.txt");
        fs::write(&unrelated, "untouched content").unwrap();
        store.scan(&root_text, |_| {}).unwrap();
        // Reading this unrelated source would fail and expose a whole-root scan.
        fs::write(&unrelated, [0xff, 0xfe]).unwrap();
        let file = root.join("changed.md");
        fs::write(&file, "alphaone").unwrap();
        apply(
            &store,
            Event::new(EventKind::Create(CreateKind::File)).add_path(file.clone()),
        )
        .unwrap();
        assert_eq!(store.search_files("alphaone", &[], 10).unwrap().len(), 1);
        let mtime = fs::metadata(&file).unwrap().modified().unwrap();
        fs::write(&file, "betatwos").unwrap();
        fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(mtime))
            .unwrap();
        apply(
            &store,
            Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
                .add_path(file.clone()),
        )
        .unwrap();
        assert!(store.search_files("alphaone", &[], 10).unwrap().is_empty());
        assert_eq!(store.search_files("betatwos", &[], 10).unwrap().len(), 1);
        let renamed = root.join("renamed.py");
        fs::rename(&file, &renamed).unwrap();
        apply(
            &store,
            Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                .add_path(file)
                .add_path(renamed.clone()),
        )
        .unwrap();
        assert_eq!(
            store.search_files("betatwos", &[], 10).unwrap()[0].path,
            renamed.to_str().unwrap()
        );
        fs::remove_file(&renamed).unwrap();
        apply(
            &store,
            Event::new(EventKind::Remove(RemoveKind::File)).add_path(renamed),
        )
        .unwrap();
        assert!(store.search_files("betatwos", &[], 10).unwrap().is_empty());
        assert_eq!(store.files().unwrap().len(), 1);
        assert_eq!(fs::read(unrelated).unwrap(), [0xff, 0xfe]);
    }

    #[test]
    fn directory_moves_and_atomic_saves_update_only_the_affected_subtree() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store = Store::open(&temp.path().join("index.sqlite")).unwrap();
        let mut settings = store.settings().unwrap();
        settings.file_types.code = true;
        store.save_settings(&settings).unwrap();
        let root_text = store.add_root(&root).unwrap();
        let folder = root.join("folder");
        let sibling = root.join("folder-other");
        fs::create_dir_all(folder.join("nested")).unwrap();
        fs::create_dir(&sibling).unwrap();
        fs::write(folder.join("nested/code.rs"), "subtreetoken").unwrap();
        fs::write(sibling.join("keep.txt"), "siblingtoken").unwrap();
        store.scan(&root_text, |_| {}).unwrap();
        fs::write(sibling.join("keep.txt"), [0xff, 0xfe]).unwrap();

        let moved = root.join("moved");
        fs::rename(&folder, &moved).unwrap();
        apply(
            &store,
            Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From))).add_path(folder),
        )
        .unwrap();
        apply(
            &store,
            Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::To))).add_path(moved.clone()),
        )
        .unwrap();
        assert_eq!(
            store.search_files("subtreetoken", &[], 10).unwrap()[0].path,
            moved.join("nested/code.rs").to_str().unwrap()
        );

        let target = moved.join("nested/code.rs");
        let replacement = moved.join("nested/.save-temp");
        fs::write(&replacement, "replacementtoken").unwrap();
        fs::rename(&replacement, &target).unwrap();
        apply(
            &store,
            Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                .add_path(replacement)
                .add_path(target),
        )
        .unwrap();
        assert!(store
            .search_files("subtreetoken", &[], 10)
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .search_files("replacementtoken", &[], 10)
                .unwrap()
                .len(),
            1
        );

        fs::remove_dir_all(&moved).unwrap();
        apply(
            &store,
            Event::new(EventKind::Remove(RemoveKind::Folder)).add_path(moved),
        )
        .unwrap();
        assert_eq!(store.files().unwrap().len(), 1);
        assert_eq!(
            store.files().unwrap()[0].path,
            sibling.join("keep.txt").to_str().unwrap()
        );
    }

    #[test]
    fn hidden_directory_events_do_not_schedule_indexing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let store = Store::open(&root.join("index.sqlite")).unwrap();
        store.add_root(&root).unwrap();
        let hidden = root.join(".cache");
        fs::create_dir(&hidden).unwrap();
        for (kind, path) in [
            (EventKind::Create(CreateKind::Folder), hidden.clone()),
            (EventKind::Create(CreateKind::File), hidden.join("file.md")),
            (EventKind::Modify(ModifyKind::Data(DataChange::Content)), hidden.join("nested/file.md")),
        ] {
            assert!(event_requests(&Event::new(kind).add_path(path), &store.roots().unwrap(), &store.settings().unwrap()).is_empty());
        }
        assert_eq!(event_requests(&Event::new(EventKind::Create(CreateKind::File)).add_path(root.join(".note.md")), &store.roots().unwrap(), &store.settings().unwrap()).len(), 1);
    }

    #[test]
    fn event_filtering_and_batching_preserve_scope() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store = Store::open(&root.join("index.sqlite")).unwrap();
        let root_text = store.add_root(&root).unwrap();
        let roots = store.roots().unwrap();
        let settings = store.settings().unwrap();
        let file = root.join("file.md");
        let excluded = root.join("node_modules/file.md");
        assert!(event_requests(
            &Event::new(EventKind::Access(AccessKind::Read)).add_path(file.clone()),
            &roots,
            &settings
        )
        .is_empty());
        assert!(event_requests(
            &Event::new(EventKind::Create(CreateKind::File)).add_path(excluded),
            &roots,
            &settings
        )
        .is_empty());
        assert!(event_requests(
            &Event::new(EventKind::Create(CreateKind::File))
                .add_path(root.with_extension("outside").join("file.md")),
            &roots,
            &settings
        )
        .is_empty());
        let mut disabled = settings.clone();
        disabled.watch = false;
        assert!(event_requests(
            &Event::new(EventKind::Create(CreateKind::File)).add_path(file.clone()),
            &roots,
            &disabled
        )
        .is_empty());

        let request = || IndexRequest::Paths {
            root: root_text.clone(),
            paths: vec![
                file.clone(),
                file.clone(),
                root.join("folder/child.md"),
                root.join("folder"),
            ],
        };
        let batch = coalesce([request(), request()]);
        assert!(
            matches!(&batch[..], [IndexRequest::Paths { paths, .. }] if paths == &vec![file.clone(), root.join("folder")])
        );
        let mut requests = event_requests(
            &Event::new(EventKind::Other).set_flag(Flag::Rescan),
            &roots,
            &settings,
        );
        requests.push(request());
        assert!(matches!(
            &coalesce(requests)[..],
            [IndexRequest::Scan { verify: true, .. }]
        ));
        store.remove_root(&root_text).unwrap();
        assert_eq!(apply_request(&store, request(), |_| {}).unwrap(), 0);
        assert!(store.roots().unwrap().is_empty());
    }

    #[test]
    fn unavailable_root_keeps_its_existing_index() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("files");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let store = Store::open(&temp.path().join("index.sqlite")).unwrap();
        let root_text = store.add_root(&root).unwrap();
        let file = root.join("keep.md");
        fs::write(&file, "kepttoken").unwrap();
        store.scan(&root_text, |_| {}).unwrap();
        fs::rename(&root, temp.path().join("unavailable")).unwrap();
        assert!(apply(
            &store,
            Event::new(EventKind::Remove(RemoveKind::Folder)).add_path(root)
        )
        .is_err());
        assert_eq!(store.files().unwrap().len(), 1);
        assert_eq!(store.roots().unwrap()[0].status, "error");
    }

    #[test]
    fn coalesced_directory_events_still_verify_changed_content() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store = Store::open(&root.join("index.sqlite")).unwrap();
        let root_text = store.add_root(&root).unwrap();
        let folder = root.join("folder");
        fs::create_dir(&folder).unwrap();
        let file = folder.join("file.md");
        fs::write(&file, "alphaone").unwrap();
        store.scan(&root_text, |_| {}).unwrap();
        let mtime = fs::metadata(&file).unwrap().modified().unwrap();
        fs::write(&file, "betatwos").unwrap();
        fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(mtime))
            .unwrap();
        let batch = coalesce([IndexRequest::Paths {
            root: root_text,
            paths: vec![folder, file],
        }]);
        for request in batch {
            apply_request(&store, request, |_| {}).unwrap();
        }
        assert!(store.search_files("alphaone", &[], 10).unwrap().is_empty());
        assert_eq!(store.search_files("betatwos", &[], 10).unwrap().len(), 1);
        let metadata = Event::new(EventKind::Modify(ModifyKind::Metadata(
            notify::event::MetadataKind::Permissions,
        )))
        .add_path(root);
        assert!(event_requests(
            &metadata,
            &store.roots().unwrap(),
            &store.settings().unwrap()
        )
        .is_empty());
    }

    // Exercise the real OS watcher with a private directory and database.
    #[test]
    fn native_notifications_keep_the_index_current_without_root_scans() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("files");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let store = Store::open(&temp.path().join("index.sqlite")).unwrap();
        let mut settings = store.settings().unwrap();
        settings.file_types.code = true;
        store.save_settings(&settings).unwrap();
        let root_text = store.add_root(&root).unwrap();
        let unrelated = root.join("unrelated.txt");
        fs::write(&unrelated, "untouched content").unwrap();
        store.scan(&root_text, |_| {}).unwrap();
        fs::write(&unrelated, [0xff, 0xfe]).unwrap();
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            tx.send(event).expect("watcher test receiver closed");
        })
        .unwrap();
        watcher.watch(&root, RecursiveMode::Recursive).unwrap();
        let sync = || {
            let mut requests = vec![];
            let first: notify::Result<Event> = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("no filesystem notification");
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut next = Some(first);
            while let Some(event) = next {
                let event = event.unwrap();
                eprintln!("native event: {event:?}");
                requests.extend(event_requests(
                    &event,
                    &store.roots().unwrap(),
                    &store.settings().unwrap(),
                ));
                assert!(
                    Instant::now() < deadline,
                    "filesystem events did not settle"
                );
                next = match rx.recv_timeout(Duration::from_millis(400)) {
                    Ok(event) => Some(event),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(error) => panic!("watcher disconnected: {error}"),
                };
            }
            assert!(!requests.is_empty(), "no applicable change paths");
            for request in coalesce(requests) {
                assert!(matches!(request, IndexRequest::Paths { .. }));
                apply_request(&store, request, |_| {}).unwrap();
            }
        };
        let file = root.join("new.md");
        fs::write(&file, "nativetoken").unwrap();
        sync();
        assert_eq!(store.search_files("nativetoken", &[], 10).unwrap().len(), 1);
        fs::write(&file, "modifiedtoken").unwrap();
        sync();
        assert!(store
            .search_files("nativetoken", &[], 10)
            .unwrap()
            .is_empty());
        assert_eq!(
            store.search_files("modifiedtoken", &[], 10).unwrap().len(),
            1
        );
        let replacement = root.join(".save-temp");
        fs::write(&replacement, "atomictoken").unwrap();
        fs::rename(&replacement, &file).unwrap();
        sync();
        assert_eq!(store.search_files("atomictoken", &[], 10).unwrap().len(), 1);
        let folder = root.join("nested");
        fs::create_dir(&folder).unwrap();
        let renamed = folder.join("renamed.rs");
        fs::rename(&file, &renamed).unwrap();
        sync();
        assert_eq!(
            store.search_files("atomictoken", &[], 10).unwrap()[0].path,
            renamed.to_str().unwrap()
        );
        let moved = root.join("moved");
        fs::rename(&folder, &moved).unwrap();
        sync();
        assert_eq!(
            store.search_files("atomictoken", &[], 10).unwrap()[0].path,
            moved.join("renamed.rs").to_str().unwrap()
        );
        fs::remove_dir_all(moved).unwrap();
        sync();
        assert!(store
            .search_files("atomictoken", &[], 10)
            .unwrap()
            .is_empty());
        assert_eq!(store.files().unwrap().len(), 1);
        assert_eq!(fs::read(unrelated).unwrap(), [0xff, 0xfe]);
    }
}
