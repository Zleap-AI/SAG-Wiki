use sag_core::store::Store;
use std::fs;

#[test]
fn file_browser_lists_unindexed_files_and_empty_folders_without_indexing_them() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("资料");
    fs::create_dir(&root).unwrap();
    for name in ["图片", "视频", "空目录", ".hidden"] { fs::create_dir(root.join(name)).unwrap(); }
    let files = [("正文.md", "searchable document"), ("cover.png", "image fixture"), ("clip.mp4", "video fixture"), ("archive.zip", "archive fixture")];
    for (name, body) in files { fs::write(root.join(name), body).unwrap(); }
    let store = Store::open(&temp.path().join("data/sag.sqlite")).unwrap();
    let registered = store.add_root(&root).unwrap();
    store.scan(&registered, |_| {}).unwrap();
    assert_eq!(store.files().unwrap().len(), 1);
    assert!(!store.settings().unwrap().show_hidden);
    let entries = store.entries(&root, store.settings().unwrap().show_hidden).unwrap();
    for name in ["图片", "视频", "空目录", "正文.md", "cover.png", "clip.mp4", "archive.zip"] {
        assert!(entries.iter().any(|entry| entry.name == name), "missing {name}");
    }
    assert!(!entries.iter().any(|entry| entry.name == ".hidden"));
    let mut settings = store.settings().unwrap();
    settings.show_hidden = true;
    store.save_settings(&settings).unwrap();
    let reopened = Store::open(&temp.path().join("data/sag.sqlite")).unwrap();
    assert!(reopened.settings().unwrap().show_hidden);
    assert!(reopened.entries(&root, reopened.settings().unwrap().show_hidden).unwrap().iter().any(|entry| entry.name == ".hidden"));
    for (name, body) in files { assert_eq!(fs::read_to_string(root.join(name)).unwrap(), body); }
    assert_eq!(store.files().unwrap().len(), 1);
}

#[test]
fn removing_an_indexed_file_keeps_it_browsable_and_excluded_from_search() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("资料");
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let original = root.join("正文.md");
    fs::write(&original, "retainedfilemarker original content").unwrap();
    let store = Store::open(&temp.path().join("data/sag.sqlite")).unwrap();
    let registered = store.add_root(&root).unwrap();
    store.scan(&registered, |_| {}).unwrap();
    assert_eq!(store.files().unwrap().len(), 1);
    store.remove_path(&original).unwrap();
    assert!(store.entries(&root, false).unwrap().iter().any(|entry| entry.name == "正文.md"));
    assert!(store.files().unwrap().is_empty());
    assert!(store.search_files("retainedfilemarker", &[], 10).unwrap().is_empty());
    store.scan(&registered, |_| {}).unwrap();
    assert!(store.files().unwrap().is_empty());
    assert_eq!(fs::read_to_string(&original).unwrap(), "retainedfilemarker original content");
    store.add_root(&original).unwrap();
    store.scan(&registered, |_| {}).unwrap();
    assert_eq!(store.files().unwrap().len(), 1);
    assert_eq!(fs::read_to_string(&original).unwrap(), "retainedfilemarker original content");
}
