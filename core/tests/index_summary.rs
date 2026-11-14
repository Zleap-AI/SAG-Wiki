use sag_core::store::Store;
use serde_json::{json, Value};
use std::{fs, path::Path};

fn latest_summary(store: &Store) -> Value {
    serde_json::to_value(&store.index_history().unwrap()[0]).unwrap()["summary"].clone()
}

fn chunk_ids(store: &Store) -> Vec<i64> {
    store.connection().prepare("SELECT id FROM chunks ORDER BY id").unwrap()
        .query_map([], |row| row.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

fn fixture() -> (tempfile::TempDir, Store, String) {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("library");
    fs::create_dir(&library).unwrap();
    fs::write(library.join("a.md"), "stablemarker unchanged source").unwrap();
    fs::write(library.join("b.txt"), "beforemarker original source").unwrap();
    let store = Store::open(&directory.path().join("data/sag.sqlite")).unwrap();
    let root = store.add_root(&library).unwrap();
    (directory, store, root)
}

#[test]
fn reopened_unchanged_scan_reports_reuse_without_rebuilding_chunks() {
    let (directory, store, root) = fixture();
    store.scan(&root, |_| {}).unwrap();
    let ids = chunk_ids(&store);
    let version = store.version().unwrap();
    drop(store);
    let reopened = Store::open(&directory.path().join("data/sag.sqlite")).unwrap();
    let observed = std::cell::RefCell::new(Value::Null);
    reopened.scan(&root, |_| {
        *observed.borrow_mut() = serde_json::to_value(&reopened.indexing.status().tasks[0]).unwrap()["summary"].clone();
    }).unwrap();
    let expected = json!({"checked":2,"reused":2,"updated":0,"failed":0});
    assert_eq!(observed.into_inner(), expected, "live checks must distinguish reuse from real index updates");
    assert_eq!(latest_summary(&reopened), expected);
    assert_eq!(chunk_ids(&reopened), ids);
    assert_eq!(reopened.version().unwrap(), version);
}

#[test]
fn scan_reports_only_changed_document_as_updated() {
    let (_directory, store, root) = fixture();
    store.scan(&root, |_| {}).unwrap();
    let stable_id = store.search_files("stablemarker", &[], 5).unwrap()[0].id;
    fs::write(Path::new(&root).join("b.txt"), "aftermarker updated source text").unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(latest_summary(&store), json!({"checked":2,"reused":1,"updated":1,"failed":0}));
    assert_eq!(store.search_files("stablemarker", &[], 5).unwrap()[0].id, stable_id);
    assert_eq!(store.search_files("aftermarker", &[], 5).unwrap().len(), 1);
    assert!(store.search_files("beforemarker", &[], 5).unwrap().is_empty());
}

#[test]
fn unreadable_text_is_failed_instead_of_reported_as_reused() {
    let (_directory, store, root) = fixture();
    fs::write(Path::new(&root).join("broken.txt"), [0xff, 0xfe, 0x00, 0xd8]).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(latest_summary(&store), json!({"checked":3,"reused":0,"updated":2,"failed":1}));
    assert_eq!(store.roots().unwrap()[0].status, "partial");
}

#[test]
fn old_history_does_not_invent_zero_update_counts() {
    let (_directory, store, root) = fixture();
    store.connection().execute("INSERT INTO index_history(path,name,started_at,finished_at,completed,total,status) VALUES(?,'Library',1,2,20,20,'ready')", [&root]).unwrap();
    assert!(latest_summary(&store).is_null());
}

#[test]
fn converted_document_is_counted_once_and_final_scan_does_not_hide_its_update() {
    let (_directory, store, root) = fixture();
    fs::remove_file(Path::new(&root).join("a.md")).unwrap();
    fs::remove_file(Path::new(&root).join("b.txt")).unwrap();
    fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/anydoc/docx-text.docx"), Path::new(&root).join("report.docx")).unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(latest_summary(&store), json!({"checked":1,"reused":0,"updated":1,"failed":0}));
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(latest_summary(&store), json!({"checked":1,"reused":1,"updated":0,"failed":0}));
}

#[test]
fn removed_document_is_reported_as_an_actual_index_update() {
    let (_directory, store, root) = fixture();
    store.scan(&root, |_| {}).unwrap();
    fs::remove_file(Path::new(&root).join("b.txt")).unwrap();
    store.scan_basic(&root, |_| {}).unwrap();
    assert_eq!(latest_summary(&store), json!({"checked":2,"reused":1,"updated":1,"failed":0}));
    assert!(store.search_files("beforemarker", &[], 5).unwrap().is_empty());
}

#[test]
fn watcher_directory_removal_counts_each_removed_index_target() {
    let (_directory, store, root) = fixture();
    let child = Path::new(&root).join("child");
    fs::create_dir(&child).unwrap();
    fs::write(child.join("one.md"), "childmarker first file").unwrap();
    fs::write(child.join("two.txt"), "childmarker second file").unwrap();
    store.scan(&root, |_| {}).unwrap();
    fs::remove_dir_all(&child).unwrap();
    store.update_paths_basic(&root, &[child], |_| {}).unwrap();
    assert_eq!(latest_summary(&store), json!({"checked":2,"reused":0,"updated":2,"failed":0}));
    assert!(store.search_files("childmarker", &[], 5).unwrap().is_empty());
    assert_eq!(store.search_files("stablemarker", &[], 5).unwrap().len(), 1);
}

#[test]
fn remembered_conversion_failure_is_reported_on_desktop_startup_check() {
    let (directory, store, root) = fixture();
    fs::remove_file(Path::new(&root).join("a.md")).unwrap();
    fs::remove_file(Path::new(&root).join("b.txt")).unwrap();
    fs::write(Path::new(&root).join("broken.pdf"), b"not a parseable PDF").unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.roots().unwrap()[0].status, "partial");
    drop(store);
    let reopened = Store::open(&directory.path().join("data/sag.sqlite")).unwrap();
    reopened.scan_basic(&root, |_| {}).unwrap();
    assert_eq!(latest_summary(&reopened), json!({"checked":1,"reused":0,"updated":0,"failed":1}));
    assert_eq!(reopened.roots().unwrap()[0].status, "partial");
}

#[test]
fn published_basic_text_stays_counted_as_updated_when_enhancement_fails() {
    let (_directory, store, root) = fixture();
    fs::remove_file(Path::new(&root).join("a.md")).unwrap();
    fs::remove_file(Path::new(&root).join("b.txt")).unwrap();
    fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/anydoc/pdf-handmade-mixed.pdf"), Path::new(&root).join("mixed.pdf")).unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert!(!store.files().unwrap().is_empty(), "basic text was actually published before the unavailable enhancement failed");
    assert_eq!(store.roots().unwrap()[0].status, "partial");
    assert_eq!(latest_summary(&store), json!({"checked":1,"reused":0,"updated":1,"failed":1}));
    store.scan_basic(&root, |_| {}).unwrap();
    assert_eq!(latest_summary(&store), json!({"checked":1,"reused":0,"updated":0,"failed":1}), "a cached failure must not look like a successful reuse of its preserved basic output");
}

#[test]
fn invalidating_stale_converted_index_does_not_clear_the_conversion_failure() {
    let (_directory, store, root) = fixture();
    fs::remove_file(Path::new(&root).join("a.md")).unwrap();
    fs::remove_file(Path::new(&root).join("b.txt")).unwrap();
    let source = Path::new(&root).join("report.docx");
    fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/anydoc/docx-text.docx"), &source).unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert!(!store.files().unwrap().is_empty());
    fs::write(&source, "changed source is no longer a valid DOCX").unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert!(store.files().unwrap().is_empty(), "stale evidence must be removed when source changes");
    assert_eq!(latest_summary(&store), json!({"checked":1,"reused":0,"updated":1,"failed":1}));
}
