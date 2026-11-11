use sag_core::store::Store;
use std::{fs, path::Path};

const SCANNED: &str = "tests/fixtures/anydoc/pdf-handmade-scanned.pdf";

fn attempted(store: &Store, name: &str) -> Vec<bool> {
    store.index_history().unwrap().into_iter().rev().map(|entry| {
        store.index_history_log(entry.id).unwrap().raw.unwrap_or_default().contains(name)
    }).collect()
}

#[test]
fn files_failing_for_a_missing_component_are_not_reconverted_on_later_scans() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    fs::create_dir(&root).unwrap();
    let scanned = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(SCANNED)).unwrap();
    fs::write(root.join("scan.pdf"), &scanned).unwrap();
    let store = Store::open(&temp.path().join("data/index.sqlite")).unwrap();
    let registered = store.add_root(&root).unwrap();

    store.scan(&registered, |_| {}).unwrap();
    let first = store.roots().unwrap().remove(0);
    assert_eq!(first.status, "partial");
    let error = first.error.as_deref().unwrap();
    assert!(error.contains("scan.pdf") && error.contains("OCR"), "{error}");

    store.scan(&registered, |_| {}).unwrap();
    let second = store.roots().unwrap().remove(0);
    assert_eq!(second.status, "partial");
    assert_eq!(second.error, first.error, "the recorded failure keeps the original message");
    assert_eq!(attempted(&store, "scan.pdf"), vec![true, false], "second scan must not run the converter again");

    fs::write(root.join("scan.pdf"), [scanned.as_slice(), b"\n%changed"].concat()).unwrap();
    store.scan(&registered, |_| {}).unwrap();
    assert_eq!(attempted(&store, "scan.pdf"), vec![true, false, true], "a changed source is converted again");

    fs::remove_file(root.join("scan.pdf")).unwrap();
    store.scan(&registered, |_| {}).unwrap();
    assert_eq!(store.roots().unwrap().remove(0).status, "ready");
}
