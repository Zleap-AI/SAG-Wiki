use sag_core::store::Store;
use sag_protocol::types::{ConversionStorage, Draft};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn new_profiles_store_conversions_outside_libraries_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("data/index.sqlite");
    let library = dir.path().join("library");
    fs::create_dir(&library).unwrap();
    let source = library.join("report.rtf");
    fs::write(&source, br"{\rtf1\ansi conversionstoragedefault}").unwrap();
    let store = Store::open(&db).unwrap();
    assert_eq!(
        store.settings().unwrap().conversion_storage,
        ConversionStorage::AppData
    );
    let root = store.add_root(&library).unwrap();
    store.scan(&root, |_| {}).unwrap();
    drop(store);
    let store = Store::open(&db).unwrap();
    assert_eq!(
        store.settings().unwrap().conversion_storage,
        ConversionStorage::AppData
    );
    let results = store
        .search_files("conversionstoragedefault", &[root], 10)
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(Path::new(&results[0].path).starts_with(db.parent().unwrap().canonicalize().unwrap()));
    assert!(!library.join(".sag-converted").exists());
    assert_eq!(
        results[0].source_path.as_deref(),
        source.canonicalize().unwrap().to_str()
    );
}

#[test]
fn migrates_a_legacy_ocr_result_without_reparsing_and_preserves_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("data/index.sqlite");
    let library = dir.path().join("library");
    let custom = dir.path().join("custom");
    fs::create_dir_all(library.join(".sag-converted/scans")).unwrap();
    fs::create_dir(library.join("scans")).unwrap();
    fs::create_dir(&custom).unwrap();
    let source = library.join("scans/report.pdf");
    // The original has no searchable text. There is no OCR component in this profile.
    let original = include_bytes!("fixtures/anydoc/pdf-handmade-scanned.pdf");
    fs::write(&source, original).unwrap();
    let old = library.join(".sag-converted/scans/report.pdf.md");
    let photo = ".sag-pdf-parts/.sag-mineru-assets/package/images/figure.png";
    let image = old.parent().unwrap().join(photo);
    fs::create_dir_all(image.parent().unwrap()).unwrap();
    fs::write(&image, b"fixture figure").unwrap();
    let body = format!("migratedocrpassage\n\n![Figure]({photo})");
    fs::write(&old, &body).unwrap();
    let store = Store::open(&db).unwrap();
    let current = store.settings().unwrap();
    let mut legacy = serde_json::to_value(&current).unwrap();
    legacy.as_object_mut().unwrap().remove("conversion_storage");
    drop(store);
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection
        .execute(
            "UPDATE preferences SET value=? WHERE id=1",
            [legacy.to_string()],
        )
        .unwrap();
    drop(connection);
    let store = Store::open(&db).unwrap();
    let mut settings = store.settings().unwrap();
    assert_eq!(settings.conversion_storage, ConversionStorage::Library);
    let root = store.add_root(&library).unwrap();
    let source = source.canonicalize().unwrap();
    let old = old.canonicalize().unwrap();
    let source_hash = format!(
        "pdf-content-v1:segments-50:anydoc-0.2.4:adapter-1:{}",
        sag_core::store::hash(original)
    );
    let ownership = serde_json::json!({"scans/report.pdf":[source_hash,sag_core::store::hash(body.as_bytes())]});
    fs::write(
        library.join(".sag-converted/.sag-converted.json"),
        ownership.to_string(),
    )
    .unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert!(store.index_status().unwrap().conversion_states.is_empty());
    let found = store
        .search_files("migratedocrpassage", std::slice::from_ref(&root), 10)
        .unwrap();
    assert_eq!(found.len(), 1);
    let document_id = found[0].document_id;
    let edited = format!("{body}\n\nmanualannotation");
    store
        .save_document(&old, &edited, Some(&sag_core::store::hash(body.as_bytes())))
        .unwrap();
    store.index_file(&root, &old, 512).unwrap();
    store
        .put_draft(&Draft {
            path: old.to_str().unwrap().into(),
            name: "report.pdf.md".into(),
            body: "unsaved annotation".into(),
            hash: sag_core::store::hash(edited.as_bytes()),
            is_new: false,
        })
        .unwrap();
    let blocked = library.join("scans");
    settings.conversion_storage = ConversionStorage::Custom {
        directory: blocked.to_string_lossy().into_owned(),
    };
    assert!(store.save_settings(&settings).is_err());
    assert_eq!(
        store.settings().unwrap().conversion_storage,
        ConversionStorage::Library
    );
    assert_eq!(fs::read_to_string(&old).unwrap(), edited);
    let mut previous = old.clone();
    for (storage, base) in [
        (
            ConversionStorage::AppData,
            db.parent().unwrap().to_path_buf(),
        ),
        (
            ConversionStorage::Custom {
                directory: custom.to_string_lossy().into_owned(),
            },
            custom.clone(),
        ),
        (ConversionStorage::Library, library.clone()),
    ] {
        settings.conversion_storage = storage;
        store.save_settings(&settings).unwrap();
        assert!(!previous.exists());
        let found = store
            .search_files("manualannotation", std::slice::from_ref(&root), 10)
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].document_id, document_id);
        assert_eq!(found[0].source_path.as_deref(), source.to_str());
        let output = PathBuf::from(&found[0].path);
        let draft = store.drafts().unwrap().pop().unwrap();
        assert_eq!(Path::new(&draft.path), output);
        assert_eq!(draft.body, "unsaved annotation");
        assert!(output.starts_with(base.canonicalize().unwrap()));
        let document = store.document(&source).unwrap();
        assert_eq!(document.body, edited);
        assert_eq!(
            store
                .authorize(Path::new(document.source_path.as_ref().unwrap()))
                .unwrap(),
            source
        );
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(
            fs::read(output.parent().unwrap().join(photo)).unwrap(),
            b"fixture figure"
        );
        store.scan(&root, |_| {}).unwrap();
        assert!(store.index_status().unwrap().conversion_states.is_empty());
        assert_eq!(store.document(&source).unwrap().body, edited);
        previous = output;
    }
}
