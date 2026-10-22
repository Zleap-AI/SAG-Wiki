use sag_core::{folder_index, store::Store};
use sag_protocol::types::{FileTypes, FolderIndexOptions};
use std::{fs, path::Path};

fn options(markdown: bool, code: bool) -> FolderIndexOptions {
    FolderIndexOptions { extensions: None, file_types: FileTypes { markdown, text: false, code, config: false }, convert_other_formats: false, conversion_formats: None }
}

#[test]
fn folders_keep_independent_types_and_manual_ignore_rules_across_scans() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    for root in [&a, &b] {
        fs::create_dir(root).unwrap();
        fs::write(root.join("readme.MD"), "documentation evidence").unwrap();
        fs::write(root.join("main.py"), "print('code evidence')").unwrap();
        fs::write(root.join("ignored.md"), "private ignored content").unwrap();
    }
    fs::write(a.join(".sagignore"), "# Custom rule\nignored.md\n").unwrap();
    folder_index::write(&a, &options(true, false)).unwrap();
    folder_index::write(&b, &options(false, true)).unwrap();
    let database = temp.path().join("index.sqlite");
    let store = Store::open(&database).unwrap();
    let a_root = store.add_root(&a).unwrap();
    let b_root = store.add_root(&b).unwrap();
    store.scan(&a_root, |_| {}).unwrap();
    store.scan(&b_root, |_| {}).unwrap();
    let files = store.files().unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().any(|file| Path::new(&file.path) == Path::new(&a_root).join("readme.MD").as_path()));
    assert!(files.iter().any(|file| Path::new(&file.path) == Path::new(&b_root).join("main.py").as_path()));
    fs::write(a.join("added.py"), "incremental code").unwrap();
    store.update_paths(&a_root, &[Path::new(&a_root).join("added.py")], |_| {}).unwrap();
    assert_eq!(store.files().unwrap().len(), 2);
    let old_b = files.iter().find(|file| file.path.starts_with(&b_root)).unwrap().clone();
    let rule_before = fs::read_to_string(a.join(".sagignore")).unwrap();
    store.save_folder_index_options(Path::new(&a_root), &options(false, true), true).unwrap();
    assert!(store.files().unwrap().iter().all(|file| !file.path.starts_with(&a_root)));
    assert_eq!(store.files().unwrap()[0].path, old_b.path);
    let rule_after = fs::read_to_string(a.join(".sagignore")).unwrap();
    assert!(rule_after.starts_with("# Custom rule\nignored.md\n"));
    assert_ne!(rule_before, rule_after);
    assert_eq!(rule_after.matches("# SAG Wiki index options: begin").count(), 1);
    store.scan(&a_root, |_| {}).unwrap();
    let files = store.files().unwrap();
    assert_eq!(files.len(), 3);
    assert!(!files.iter().any(|file| file.path.ends_with(".MD")));
    assert_eq!(fs::read_to_string(a.join("readme.MD")).unwrap(), "documentation evidence");
    drop(store);
    let store = Store::open(&database).unwrap();
    assert_eq!(store.folder_index_options(&a).unwrap(), options(false, true));
    store.scan(&a_root, |_| {}).unwrap();
    assert_eq!(store.files().unwrap().len(), 3);
}

#[test]
fn active_folders_and_files_cannot_erase_indexes_or_rewrite_rules() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir_all(root.join("child")).unwrap();
    fs::write(root.join(".sagignore"), "untouched.md\n").unwrap();
    let store = Store::open(&temp.path().join("index.sqlite")).unwrap();
    let path = store.add_root(&root).unwrap();
    assert!(store.save_folder_index_options(Path::new(&path), &options(true, false), true).is_err());
    store.scan(&path, |_| {}).unwrap();
    fs::write(root.join("note.md"), "source file").unwrap();
    assert!(store.save_folder_index_options(&root.join("note.md"), &options(true, false), true).is_err());
    assert_eq!(fs::read_to_string(root.join(".sagignore")).unwrap(), "untouched.md\n");
    assert!(!root.join("child").join(".sagignore").exists());
}

#[test]
fn malformed_managed_blocks_are_reported_without_overwriting_custom_rules() {
    let temp = tempfile::tempdir().unwrap();
    let original = "# custom\nprivate/\n# SAG Wiki index options: begin\n# broken";
    fs::write(temp.path().join(".sagignore"), original).unwrap();
    assert!(folder_index::read(temp.path()).is_err());
    assert!(folder_index::write(temp.path(), &options(true, false)).is_err());
    assert_eq!(fs::read_to_string(temp.path().join(".sagignore")).unwrap(), original);
}

#[test]
fn nested_options_inherit_and_override_without_reindexing_siblings() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("project");
    for relative in ["child", "child/deeper", "sibling", "child-other"] {
        fs::create_dir_all(folder.join(relative)).unwrap();
        fs::write(folder.join(relative).join("note.md"), "documentation evidence").unwrap();
        fs::write(folder.join(relative).join("main.py"), "print('code evidence')").unwrap();
    }
    folder_index::write(&folder, &options(true, false)).unwrap();
    folder_index::write(&folder.join("child").join("deeper"), &options(true, false)).unwrap();
    fs::write(folder.join("child").join(".sagignore"), "# handwritten rule\nignored.py\n").unwrap();
    fs::write(folder.join("child").join("ignored.py"), "ignored content").unwrap();
    let database = temp.path().join("index.sqlite");
    let store = Store::open(&database).unwrap();
    let root = store.add_root(&folder).unwrap();
    let child = Path::new(&root).join("child");
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.folder_index_options(&child).unwrap(), options(true, false));
    assert_eq!(store.files().unwrap().len(), 4);
    let sibling = Path::new(&root).join("sibling").join("note.md");
    // A sibling read would now fail; a scoped reindex must not inspect it.
    fs::write(&sibling, [0xff, 0xfe, 0x00]).unwrap();
    let returned_root = store.save_folder_index_options(&child, &options(false, true), true).unwrap();
    assert_eq!(returned_root, root);
    assert_eq!(store.files().unwrap().len(), 2);
    let event = notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any)).add_path(child.join(".sagignore"));
    let requests = sag_core::indexer::event_requests(&event, &store.roots().unwrap(), &store.settings().unwrap());
    assert!(matches!(&requests[..], [sag_core::indexer::IndexRequest::Paths { paths, .. }] if paths == &vec![child.join(".sagignore")]));
    for request in requests { sag_core::indexer::apply_request(&store, request, |_| {}).unwrap(); }
    assert_eq!(store.roots().unwrap()[0].status, "ready");
    let files = store.files().unwrap();
    assert_eq!(files.len(), 4);
    assert!(files.iter().any(|file| Path::new(&file.path).ends_with(Path::new("child").join("main.py"))));
    assert!(!files.iter().any(|file| Path::new(&file.path).ends_with(Path::new("child").join("note.md")) || Path::new(&file.path).ends_with("ignored.py")));
    assert!(files.iter().any(|file| Path::new(&file.path).ends_with(Path::new("child").join("deeper").join("note.md"))));
    assert!(files.iter().any(|file| Path::new(&file.path).ends_with(Path::new("child-other").join("note.md"))));
    assert!(fs::read_to_string(child.join(".sagignore")).unwrap().starts_with("# handwritten rule\nignored.py\n"));
    assert_eq!(store.folder_index_options(&child.join("deeper")).unwrap(), options(true, false));
    fs::write(child.join("added.py"), "incremental evidence").unwrap();
    fs::write(child.join("added.md"), "must stay unindexed").unwrap();
    store.update_paths(&root, &[child.join("added.py"), child.join("added.md")], |_| {}).unwrap();
    assert_eq!(store.files().unwrap().len(), 5);
    fs::write(&sibling, "documentation evidence").unwrap();
    drop(store);
    let store = Store::open(&database).unwrap();
    assert_eq!(store.folder_index_options(&child).unwrap(), options(false, true));
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.files().unwrap().len(), 5);
    // Parent changes must retain explicit child and grandchild overrides.
    folder_index::write(Path::new(&root), &options(false, false)).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.files().unwrap().len(), 3);
    assert_eq!(store.folder_index_options(&Path::new(&root).join("sibling")).unwrap(), options(false, false));
    assert_eq!(fs::read_to_string(child.join("note.md")).unwrap(), "documentation evidence");
}

#[test]
fn extension_selection_applies_to_incremental_indexing_with_code_disabled_globally() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("main.PY"), "initial python evidence").unwrap();
    fs::write(project.join("note.md"), "excluded markdown evidence").unwrap();
    let mut selected = options(false, false);
    selected.extensions = Some(vec!["py".into()]);
    folder_index::write(&project, &selected).unwrap();
    let store = Store::open(&temp.path().join("index.sqlite")).unwrap();
    assert!(!store.settings().unwrap().file_types.code);
    let root = store.add_root(&project).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.files().unwrap().len(), 1);
    let python = Path::new(&root).join("added.py");
    let javascript = Path::new(&root).join("added.js");
    fs::write(&python, "incremental python evidence").unwrap();
    fs::write(&javascript, "excluded javascript evidence").unwrap();
    store.update_paths(&root, &[python.clone(), javascript], |_| {}).unwrap();
    let files = store.files().unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|file| file.path.to_lowercase().ends_with(".py")));
    fs::write(&python, "updated python evidence").unwrap();
    store.update_paths(&root, &[python.clone()], |_| {}).unwrap();
    assert_eq!(store.document(&python).unwrap().body, "updated python evidence");
}
