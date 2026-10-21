use sag_core::store::{FileAccess, FileAccessMode, Store};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[test]
fn native_policy_applies_to_indexing_search_editor_and_revocation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("files");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("public.md"), "nativepermissionmarker public").unwrap();
    std::fs::write(root.join("private.md"), "nativepermissionmarker private").unwrap();
    let revoked = Arc::new(AtomicBool::new(false));
    let revoked_policy = revoked.clone();
    let policy: FileAccess = Arc::new(move |path, write| Ok(write == FileAccessMode::Read && !path.ends_with("private.md") && !revoked_policy.load(Ordering::SeqCst)));
    let store = Store::open_with_file_access(&temp.path().join("data/sag.sqlite"), Some(policy)).unwrap();
    let root = store.add_root(&root).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.files().unwrap().len(), 1);
    let results = store.search_files("nativepermissionmarker", &[], 10).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("public"));
    let path = std::path::Path::new(&root).join("public.md");
    assert!(store.save_document(&path, "overwrite", None).is_err());
    assert!(store.document(&std::path::Path::new(&root).join("private.md")).is_err());
    revoked.store(true, Ordering::SeqCst);
    assert!(store.search_files("nativepermissionmarker", &[], 10).is_err());
    assert!(std::fs::read_to_string(path).unwrap().contains("public"));
}
