use rusqlite::{params, Connection};
use sag_core::store::{hash, Store};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

fn setup() -> (tempfile::TempDir, Store, PathBuf, String, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let db = temp.path().join("data/index.sqlite");
    let store = Store::open(&db).unwrap();
    let registered = store.add_root(&root).unwrap();
    (temp, store, root, registered, db)
}

fn old_output_time(path: &Path) -> SystemTime {
    let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(946_684_800);
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    fs::metadata(path).unwrap().modified().unwrap()
}

#[test]
fn newer_schema_is_rejected_before_database_or_journal_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("future.sqlite");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE future_payload(value TEXT); INSERT INTO future_payload VALUES('keep'); PRAGMA user_version=11;").unwrap();
    drop(db);
    let bytes = fs::read(&path).unwrap();
    let error = match Store::open(&path) {
        Ok(_) => panic!("a newer schema must not be opened or downgraded"),
        Err(error) => format!("{error:#}"),
    };
    assert!(error.contains("11") && error.contains("10"), "{error}");
    assert_eq!(
        fs::read(&path).unwrap(),
        bytes,
        "rejection must leave the database byte-for-byte unchanged"
    );
    assert!(!path.with_extension("sqlite-wal").exists());
    assert!(!path.with_extension("sqlite-shm").exists());
}

#[test]
fn unchanged_builtin_source_content_reuses_output_after_stat_change_and_reopen() {
    let (_temp, store, root, registered, db) = setup();
    let source = root.join("report.docx");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/anydoc/docx-text.docx"),
        &source,
    )
    .unwrap();
    store.scan(&registered, |_| {}).unwrap();
    let output = root.join(".sag-converted/report.docx.md");
    let bytes = fs::read(&output).unwrap();
    let modified = old_output_time(&output);
    drop(store);
    fs::File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000))
        .unwrap();
    for _ in 0..2 {
        let reopened = Store::open(&db).unwrap();
        reopened.scan(&registered, |_| {}).unwrap();
        assert_eq!(reopened.roots().unwrap()[0].status, "ready");
        assert_eq!(fs::read(&output).unwrap(), bytes);
        assert_eq!(
            fs::metadata(&output).unwrap().modified().unwrap(),
            modified,
            "unchanged content must not publish a fresh conversion"
        );
    }
}

fn seed_pdf(
    store: &Store,
    root: &Path,
    db: &Path,
    legacy_base: &str,
) -> (PathBuf, PathBuf, String) {
    let source = root.join("cached.pdf");
    // Conversion cannot succeed: reuse must come from the validated ownership record.
    fs::write(&source, b"already parsed PDF fixture").unwrap();
    let output = root.join(".sag-converted/cached.pdf.md");
    fs::create_dir_all(output.parent().unwrap()).unwrap();
    fs::write(&output, "cachedownershiptoken complete structured output").unwrap();
    let fingerprint = hash(&fs::read(&source).unwrap());
    let legacy_hash = format!("{legacy_base}:{fingerprint}");
    Connection::open(db).unwrap().execute(
        "INSERT INTO conversions(source,root,source_hash,output,output_hash,valid) VALUES(?1,?2,?3,?4,?5,1)",
        params![source.to_str().unwrap(), root.to_str().unwrap(), legacy_hash, output.to_str().unwrap(), hash(&fs::read(&output).unwrap())],
    ).unwrap();
    assert_eq!(store.roots().unwrap().len(), 1);
    (source, output, legacy_hash)
}

#[test]
fn compatible_pdf_legacy_key_is_adopted_without_rewriting_output_after_reopen() {
    let (_temp, store, root, registered, db) = setup();
    let (source, output, legacy_hash) = seed_pdf(
        &store,
        &root,
        &db,
        "pdf-segments-v1:50:2.140.0-documents.1:anydoc-0.2.4:adapter-1",
    );
    let modified = old_output_time(&output);
    drop(store);
    for _ in 0..2 {
        let reopened = Store::open(&db).unwrap();
        reopened.scan(&registered, |_| {}).unwrap();
        assert_eq!(reopened.roots().unwrap()[0].status, "ready");
        assert_eq!(fs::metadata(&output).unwrap().modified().unwrap(), modified);
        assert_eq!(
            reopened
                .search_files("cachedownershiptoken", &[], 5)
                .unwrap()
                .len(),
            1
        );
        let adopted: String = Connection::open(&db)
            .unwrap()
            .query_row(
                "SELECT source_hash FROM conversions WHERE source=?1",
                [source.to_str().unwrap()],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(
            adopted, legacy_hash,
            "compatible legacy ownership must move to the component-independent content contract"
        );
        assert!(!adopted.contains("2.140.0-documents.1"), "{adopted}");
    }
}

#[test]
fn compatible_pdf_adoption_rejects_edited_output_changed_source_and_unknown_parser_keys() {
    for variant in ["output", "source", "parser", "pre-segments", "settings"] {
        let (_temp, store, root, registered, db) = setup();
        let base = if variant == "parser" {
            "pdf-segments-v1:50:2.139.0-documents.1:anydoc-0.2.4:adapter-1"
        } else if variant == "pre-segments" {
            "anydoc-0.2.3:adapter-1"
        } else {
            "pdf-segments-v1:50:2.140.0-documents.1:anydoc-0.2.4:adapter-1"
        };
        let (source, output, legacy_hash) = seed_pdf(&store, &root, &db, base);
        if variant == "source" {
            fs::write(&source, "changed source content").unwrap();
        }
        if variant == "output" {
            fs::write(&output, "manual edits must survive").unwrap();
        }
        if variant == "settings" {
            let mut settings = store.settings().unwrap();
            settings.runtime.do_ocr = false;
            store.save_settings(&settings).unwrap();
        }
        let bytes = fs::read(&output).unwrap();
        store.scan(&registered, |_| {}).unwrap();
        assert_eq!(fs::read(&output).unwrap(), bytes, "{variant}");
        assert_eq!(
            store.roots().unwrap()[0].status,
            "partial",
            "{variant} must not be accepted as a cache hit"
        );
        let stored: String = Connection::open(&db)
            .unwrap()
            .query_row(
                "SELECT source_hash FROM conversions WHERE source=?1",
                [source.to_str().unwrap()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            stored, legacy_hash,
            "{variant} must not adopt an incompatible record"
        );
    }
}

#[test]
fn changed_builtin_source_replaces_only_its_own_output() {
    let (_temp, store, root, registered, _db) = setup();
    let fixture = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/anydoc/docx-text.docx"),
    )
    .unwrap();
    fs::write(root.join("kept.docx"), &fixture).unwrap();
    fs::write(root.join("changed.docx"), &fixture).unwrap();
    store.scan(&registered, |_| {}).unwrap();
    let kept = root.join(".sag-converted/kept.docx.md");
    let changed = root.join(".sag-converted/changed.docx.md");
    let kept_time = old_output_time(&kept);
    let changed_time = old_output_time(&changed);
    // DOCX allows trailing bytes: this changes source content without corrupting it.
    fs::write(
        root.join("changed.docx"),
        [fixture.as_slice(), b"\nchanged"].concat(),
    )
    .unwrap();
    store.scan(&registered, |_| {}).unwrap();
    assert_eq!(store.roots().unwrap()[0].status, "ready");
    assert_eq!(fs::metadata(&kept).unwrap().modified().unwrap(), kept_time);
    assert_ne!(
        fs::metadata(&changed).unwrap().modified().unwrap(),
        changed_time
    );
}

#[test]
fn metadata_only_builtin_reuse_preserves_manually_edited_output() {
    let (_temp, store, root, registered, _db) = setup();
    let source = root.join("report.docx");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/anydoc/docx-text.docx"),
        &source,
    )
    .unwrap();
    store.scan(&registered, |_| {}).unwrap();
    let output = root.join(".sag-converted/report.docx.md");
    fs::write(&output, "manual edits must survive").unwrap();
    fs::File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000))
        .unwrap();
    store.scan(&registered, |_| {}).unwrap();
    assert_eq!(
        fs::read_to_string(&output).unwrap(),
        "manual edits must survive"
    );
    assert_eq!(store.roots().unwrap()[0].status, "partial");
    assert!(store.roots().unwrap()[0]
        .error
        .as_ref()
        .unwrap()
        .contains("手动修改"));
}
