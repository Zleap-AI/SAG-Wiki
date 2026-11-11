use sag_core::store::Store;
use std::{fs, path::Path};

const DOCX: &str = "tests/fixtures/anydoc/docx-text.docx";

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

/// A routine scan of an unchanged tree must not read every source again: the
/// stored mtime and size stand in for the content hash.
#[cfg(unix)]
#[test]
fn unchanged_sources_are_not_read_again() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    fs::create_dir(&root).unwrap();
    let docx = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(DOCX)).unwrap();
    let source = root.join("report.docx");
    fs::write(&source, &docx).unwrap();
    let store = Store::open(&temp.path().join("data/index.sqlite")).unwrap();
    let registered = store.add_root(&root).unwrap();

    store.scan(&registered, |_| {}).unwrap();
    let first = store.roots().unwrap().remove(0);
    assert_eq!(first.status, "ready", "{:?}", first.error);
    assert!(root.join(".sag-converted/report.docx.md").is_file());

    // Still stat-able, but any attempt to read or hash it fails.
    set_mode(&source, 0o000);
    store.scan(&registered, |_| {}).unwrap();
    let second = store.roots().unwrap().remove(0);
    assert_eq!(second.status, "ready", "unchanged source was read again: {:?}", second.error);

    // Verification (re-index) ignores the shortcut and must read the file.
    store.verify_root(&registered).unwrap();
    store.scan(&registered, |_| {}).unwrap();
    let verified = store.roots().unwrap().remove(0);
    assert_ne!(verified.status, "ready", "verify must hash the source again");
    set_mode(&source, 0o644);

    // A modified source is hashed and converted again.
    fs::write(&source, [docx.as_slice(), b"\nchanged"].concat()).unwrap();
    store.scan(&registered, |_| {}).unwrap();
    let edited = store.roots().unwrap().remove(0);
    assert_eq!(edited.status, "ready", "{:?}", edited.error);
}
