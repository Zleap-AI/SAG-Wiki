use sag_core::store::Store;
use std::fs;

fn pdf(path: &std::path::Path, text: bool, mixed: bool) {
    use lopdf::{dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.5");
    let pages = doc.new_object_id();
    let font =
        doc.add_object(dictionary! {"Type"=>"Font", "Subtype"=>"Type1", "BaseFont"=>"Helvetica"});
    let stream = doc.add_object(Stream::new(
        dictionary! {},
        if text {
            b"BT /F1 12 Tf 20 30 Td (progressivetoken) Tj ET".to_vec()
        } else {
            vec![]
        },
    ));
    let resources = if text {
        dictionary! {"Font"=>dictionary!{"F1"=>font}}
    } else {
        dictionary! {}
    };
    let page = doc.add_object(dictionary!{"Type"=>"Page", "Parent"=>pages, "MediaBox"=>vec![0.into(),0.into(),200.into(),100.into()], "Contents"=>stream, "Resources"=>resources});
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! {"Type"=>"Pages", "Kids"=>vec![page.into()], "Count"=>1}),
    );
    if mixed {
        let blank = doc.add_object(dictionary!{"Type"=>"Page", "Parent"=>pages, "MediaBox"=>vec![0.into(),0.into(),200.into(),100.into()], "Resources"=>dictionary!{}});
        doc.objects.insert(
            pages,
            Object::Dictionary(
                dictionary! {"Type"=>"Pages", "Kids"=>vec![page.into(), blank.into()], "Count"=>2},
            ),
        );
    }
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages});
    doc.trailer.set("Root", catalog);
    doc.save(path).unwrap();
}

#[test]
fn basic_pdf_is_published_before_enhancement_and_survives_missing_component() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("files");
    fs::create_dir(&root).unwrap();
    pdf(&root.join("text.pdf"), true, true);
    let db = dir.path().join("index.sqlite");
    let store = Store::open(&db).unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    settings.conversion_formats = Some(vec!["pdf".into()]);
    store.save_settings(&settings).unwrap();
    let key = store.add_root(&root).unwrap();
    let basic_seen = std::cell::Cell::new(false);
    store
        .scan(&key, |_| {
            if !store
                .search_files("progressivetoken", &[], 10)
                .unwrap()
                .is_empty()
            {
                basic_seen.set(true);
            }
        })
        .unwrap();
    assert!(
        basic_seen.get(),
        "basic text must be searchable before full conversion completes"
    );
    let file = store.files().unwrap().remove(0);
    assert_eq!(
        serde_json::to_value(&file).unwrap()["index_status"],
        "enhancement_failed"
    );
    drop(store);
    let reopened = Store::open(&db).unwrap();
    assert!(!reopened
        .search_files("progressivetoken", &[], 10)
        .unwrap()
        .is_empty());
    assert_eq!(
        serde_json::to_value(&reopened.files().unwrap()[0]).unwrap()["index_status"],
        "enhancement_failed"
    );
    reopened.scan(&key, |_| {}).unwrap();
    assert!(
        !reopened
            .search_files("progressivetoken", &[], 10)
            .unwrap()
            .is_empty(),
        "repeated missing-component failures must preserve usable basic text"
    );
}

fn setup() -> (tempfile::TempDir, Store, std::path::PathBuf, String) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("files");
    fs::create_dir(&root).unwrap();
    let store = Store::open(&dir.path().join("index.sqlite")).unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    settings.conversion_formats = Some(vec!["pdf".into()]);
    store.save_settings(&settings).unwrap();
    let key = store.add_root(&root).unwrap();
    (dir, store, root, key)
}

#[test]
fn pure_scan_has_persistent_nonsearchable_status_and_failed_enhancement_is_honest() {
    let (dir, store, root, key) = setup();
    pdf(&root.join("scan.pdf"), false, true);
    store.scan_basic(&key, |_| {}).unwrap();
    assert!(store.files().unwrap().is_empty());
    let status = store.index_status().unwrap();
    assert_eq!(status.conversion_states[0].status, "awaiting_ocr");
    assert!(!status.conversion_states[0].searchable);
    drop(store);
    let reopened = Store::open(&dir.path().join("index.sqlite")).unwrap();
    reopened.enhance(&key, |_| {}).unwrap();
    let status = reopened.index_status().unwrap();
    assert_eq!(status.conversion_states[0].status, "enhancement_failed");
    assert!(!status.conversion_states[0].searchable);
}

#[test]
fn all_text_pdf_finishes_with_anydoc_without_ocr_component() {
    let (_dir, store, root, key) = setup();
    pdf(&root.join("text.pdf"), true, false);
    store.scan(&key, |_| {}).unwrap();
    let files = store.files().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].index_status.as_deref(), Some("complete"));
    assert!(store.index_status().unwrap().conversion_states.is_empty());
    assert_eq!(store.roots().unwrap()[0].status, "ready");
}

#[test]
fn changed_source_retracts_old_basic_text_when_replaced_with_scan() {
    let (_dir, store, root, key) = setup();
    let source = root.join("mixed.pdf");
    pdf(&source, true, true);
    store.scan_basic(&key, |_| {}).unwrap();
    assert!(!store
        .search_files("progressivetoken", &[], 10)
        .unwrap()
        .is_empty());
    pdf(&source, false, true);
    store.scan_basic(&key, |_| {}).unwrap();
    assert!(store
        .search_files("progressivetoken", &[], 10)
        .unwrap()
        .is_empty());
    assert!(!store.index_status().unwrap().conversion_states[0].searchable);
}

#[test]
fn basic_publication_resumes_after_restart() {
    let (dir, store, root, key) = setup();
    pdf(&root.join("mixed.pdf"), true, true);
    store.scan_basic(&key, |_| {}).unwrap();
    assert_eq!(
        store.files().unwrap()[0].index_status.as_deref(),
        Some("basic")
    );
    drop(store);
    let reopened = Store::open(&dir.path().join("index.sqlite")).unwrap();
    assert!(reopened.index_status().unwrap().conversion_states[0].searchable);
    reopened.enhance(&key, |_| {}).unwrap();
    assert_eq!(
        reopened.files().unwrap()[0].index_status.as_deref(),
        Some("enhancement_failed")
    );
}

#[cfg(unix)]
fn fake_docling(dir: &std::path::Path, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    let home = dir.join("docling");
    fs::create_dir_all(home.join("bin")).unwrap();
    let binary = home.join("bin/docling");
    fs::write(&binary, script).unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(home.join("ready"), "2.140.0").unwrap();
}

#[test]
#[cfg(unix)]
fn source_mutation_during_enhancement_retracts_basic_evidence_and_searchable_status() {
    let (dir, store, root, key) = setup();
    let source = root.join("mixed.pdf");
    pdf(&source, true, true);
    store.scan_basic(&key, |_| {}).unwrap();
    fake_docling(
        dir.path(),
        r#"#!/bin/sh
set -eu
source="$1"; shift
output=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = '--output-file' ]; then shift; output="$1"; fi
  shift
done
cp "$(dirname "$0")/replacement.pdf" "$source"
printf 'enhancedtoken\n' > "$output"
"#,
    );
    pdf(&dir.path().join("docling/bin/replacement.pdf"), false, true);
    store.enhance(&key, |_| {}).unwrap();
    assert!(store
        .search_files("progressivetoken", &[], 10)
        .unwrap()
        .is_empty());
    assert!(store
        .search_files("enhancedtoken", &[], 10)
        .unwrap()
        .is_empty());
    let state = store.index_status().unwrap().conversion_states.remove(0);
    assert_eq!(state.status, "enhancement_failed");
    assert!(!state.searchable);
}

#[test]
#[cfg(unix)]
fn enhancement_turn_preserves_unselected_basic_documents() {
    let (dir, store, root, key) = setup();
    pdf(&root.join("a.pdf"), true, true);
    pdf(&root.join("b.pdf"), true, true);
    store.scan_basic(&key, |_| {}).unwrap();
    assert_eq!(store.files().unwrap().len(), 2);
    fake_docling(
        dir.path(),
        r#"#!/bin/sh
set -eu
shift
output=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = '--output-file' ]; then shift; output="$1"; fi
  shift
done
printf 'enhancedtoken\n' > "$output"
"#,
    );
    store.enhance(&key, |_| {}).unwrap();
    let files = store.files().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(
        files
            .iter()
            .filter(|file| file.index_status.as_deref() == Some("complete"))
            .count(),
        1
    );
    assert_eq!(
        files
            .iter()
            .filter(|file| file.index_status.as_deref() == Some("basic"))
            .count(),
        1
    );
    let history = store.index_history().unwrap();
    assert_eq!(
        (history[0].completed, history[0].total),
        (1, 1),
        "enhancement history must report processed files"
    );
    store.enhance(&key, |_| {}).unwrap();
    assert!(store
        .files()
        .unwrap()
        .iter()
        .all(|file| file.index_status.as_deref() == Some("complete")));
    assert!(
        dir.path()
            .join("files/.sag-converted/.sag-converted.json")
            .is_file(),
        "ownership journal is persisted once enhancement queue drains"
    );
}

#[test]
fn removing_pending_scan_cancels_durable_enhancement() {
    let (_dir, store, root, key) = setup();
    let source = root.join("scan.pdf");
    pdf(&source, false, true);
    store.scan_basic(&key, |_| {}).unwrap();
    assert_eq!(store.index_status().unwrap().conversion_states.len(), 1);
    store.remove_path(&source.canonicalize().unwrap()).unwrap();
    assert!(store.index_status().unwrap().conversion_states.is_empty());
    store.enhance(&key, |_| {}).unwrap();
    assert!(store.files().unwrap().is_empty());
}

#[test]
#[cfg(unix)]
fn non_pdf_requeues_for_missing_output_and_parser_configuration_change() {
    let (dir, store, root, key) = setup();
    let mut settings = store.settings().unwrap();
    settings.conversion_formats = Some(vec!["eml".into()]);
    fake_docling(
        dir.path(),
        r#"#!/bin/sh
set -eu
shift
output=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = '--output-file' ]; then shift; output="$1"; fi
  shift
done
printf 'enhancedtoken\n' > "$output"
"#,
    );
    store.save_settings(&settings).unwrap();
    fs::write(root.join("mail.eml"), "Subject: mail\n\nmailbody").unwrap();
    store.scan_basic(&key, |_| {}).unwrap();
    store.enhance(&key, |_| {}).unwrap();
    let output = store.files().unwrap()[0].path.clone();
    fs::remove_file(&output).unwrap();
    store.scan_basic(&key, |_| {}).unwrap();
    assert_eq!(
        store.index_status().unwrap().conversion_states[0].status,
        "awaiting_conversion"
    );
    store.enhance(&key, |_| {}).unwrap();
    assert!(std::path::Path::new(&output).is_file());
    settings.runtime.do_ocr = false;
    store.save_settings(&settings).unwrap();
    store.scan_basic(&key, |_| {}).unwrap();
    assert_eq!(
        store.index_status().unwrap().conversion_states[0].status,
        "awaiting_conversion"
    );
}

#[test]
fn enhancement_requests_are_coalesced_after_foreground_work() {
    use sag_core::indexer::{coalesce, IndexRequest};
    let batch = coalesce([
        IndexRequest::Enhance { root: "a".into() },
        IndexRequest::Scan {
            root: "b".into(),
            verify: false,
        },
        IndexRequest::Enhance { root: "a".into() },
        IndexRequest::Paths {
            root: "c".into(),
            paths: vec!["c/file.txt".into()],
        },
    ]);
    assert_eq!(batch.len(), 3);
    assert!(matches!(batch[0], IndexRequest::Scan { .. }));
    assert!(matches!(batch[1], IndexRequest::Paths { .. }));
    assert!(matches!(batch[2], IndexRequest::Enhance { .. }));
}

#[test]
fn basic_pdf_keeps_original_page_markers_and_inherited_fonts() {
    let (_dir, store, root, key) = setup();
    let source = root.join("mixed.pdf");
    pdf(&source, true, true);
    let mut doc = lopdf::Document::load(&source).unwrap();
    let page = doc.get_pages()[&1];
    let parent = doc
        .get_object(page)
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Parent")
        .unwrap()
        .as_reference()
        .unwrap();
    let resources = doc
        .get_object_mut(page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .remove(b"Resources")
        .unwrap();
    let resources = doc.add_object(resources);
    doc.get_object_mut(parent)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Resources", resources);
    // The last page is an intentional empty page: only original page 1 may be indexed.
    doc.save(&source).unwrap();
    store.scan_basic(&key, |_| {}).unwrap();
    let file = store.files().unwrap().remove(0);
    let body = fs::read_to_string(file.path).unwrap();
    assert!(body.contains("source-pages:1-1"));
    assert!(!body.contains("source-pages:2-2"));
    assert!(body.contains("progressivetoken"));
}

#[test]
#[cfg(unix)]
fn cancelling_enhancement_keeps_previously_searchable_basic_text() {
    let (dir, store, root, key) = setup();
    pdf(&root.join("mixed.pdf"), true, true);
    store.scan_basic(&key, |_| {}).unwrap();
    fake_docling(
        dir.path(),
        r#"#!/bin/sh
set -eu
marker="$(dirname "$0")/started"
touch "$marker"
while [ ! -f "$(dirname "$0")/release" ]; do sleep 0.02; done
"#,
    );
    std::thread::scope(|scope| {
        let result = scope.spawn(|| store.enhance(&key, |_| {}));
        let marker = dir.path().join("docling/bin/started");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !marker.is_file() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let started = marker.is_file();
        store.indexing.cancel_root(&key);
        assert!(result.join().unwrap().is_err());
        assert!(
            started,
            "fixture parser must actually run before cancellation"
        );
    });
    assert!(!store
        .search_files("progressivetoken", &[], 10)
        .unwrap()
        .is_empty());
    assert!(store.index_status().unwrap().conversion_states[0].searchable);
}

#[test]
fn removing_and_readding_library_recovers_owned_pending_basic_output() {
    let (_dir, store, root, key) = setup();
    pdf(&root.join("mixed.pdf"), true, true);
    store.scan_basic(&key, |_| {}).unwrap();
    store.remove_root(&key).unwrap();
    let key = store.add_root(&root).unwrap();
    store.scan_basic(&key, |_| {}).unwrap();
    assert!(!store
        .search_files("progressivetoken", &[], 10)
        .unwrap()
        .is_empty());
    assert_eq!(
        store.files().unwrap()[0].index_status.as_deref(),
        Some("basic")
    );
}

#[test]
#[cfg(unix)]
fn partially_enhanced_library_recovers_completed_and_basic_outputs_after_readd() {
    let (dir, store, root, key) = setup();
    pdf(&root.join("a.pdf"), true, true);
    pdf(&root.join("b.pdf"), true, true);
    store.scan_basic(&key, |_| {}).unwrap();
    fake_docling(
        dir.path(),
        r#"#!/bin/sh
set -eu
shift
output=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = '--output-file' ]; then shift; output="$1"; fi
  shift
done
printf 'enhancedtoken\n' > "$output"
"#,
    );
    store.enhance(&key, |_| {}).unwrap();
    store.remove_root(&key).unwrap();
    let key = store.add_root(&root).unwrap();
    store.scan_basic(&key, |_| {}).unwrap();
    let files = store.files().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(
        files
            .iter()
            .filter(|file| file.index_status.as_deref() == Some("complete"))
            .count(),
        1
    );
    assert!(!store
        .search_files("enhancedtoken", &[], 10)
        .unwrap()
        .is_empty());
    assert!(!store
        .search_files("progressivetoken", &[], 10)
        .unwrap()
        .is_empty());
}

#[test]
fn enhancement_phase_and_progress_track_processed_file_counts() {
    let control = sag_core::index_status::IndexControl::default();
    control.begin("root");
    control.phase("root", "enhancing", 3);
    assert_eq!(control.indexed("root"), (0, 3));
    control.progress("root", 1, 3);
    assert_eq!(control.indexed("root"), (1, 3));
    control.progress("root", 3, 3);
    assert_eq!(control.indexed("root"), (3, 3));
}

#[test]
fn basic_text_preserves_spatial_word_gaps_for_full_text_search() {
    let (_dir, store, root, key) = setup();
    let source = root.join("spatial.pdf");
    pdf(&source, true, true);
    let mut doc = lopdf::Document::load(&source).unwrap();
    let page = doc.get_pages()[&1];
    let content = doc
        .get_object(page)
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.get_object_mut(content)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_plain_content(
            b"BT /F1 12 Tf 1 0 0 1 10 30 Tm (cluster) Tj 1 0 0 1 70 30 Tm (defects) Tj ET".to_vec(),
        );
    doc.save(&source).unwrap();
    store.scan_basic(&key, |_| {}).unwrap();
    assert_eq!(
        store.search_files("defects", &[], 10).unwrap().len(),
        1,
        "geometric spacing must preserve searchable word boundaries"
    );
}

#[test]
fn basic_pass_rejects_oversized_decoded_stream_before_positional_parser() {
    let (_dir, store, root, key) = setup();
    let source = root.join("compressed.pdf");
    pdf(&source, true, true);
    let mut doc = lopdf::Document::load(&source).unwrap();
    let page = doc.get_pages()[&1];
    let content = doc
        .get_object(page)
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_reference()
        .unwrap();
    let mut bytes = b"%".to_vec();
    bytes.extend(vec![b'a'; 2 * 1024 * 1024]);
    bytes.extend_from_slice(b"\nBT /F1 12 Tf 20 30 Td (boundedtoken) Tj ET");
    doc.get_object_mut(content)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_plain_content(bytes);
    doc.compress();
    doc.save(&source).unwrap();
    let mut settings = store.settings().unwrap();
    settings.runtime.conversion_output_mib = 1;
    settings.runtime.max_text_input_mib = 1;
    settings.runtime.max_decoded_text_mib = 1;
    store.save_settings(&settings).unwrap();
    store.scan_basic(&key, |_| {}).unwrap();
    assert!(store.files().unwrap().is_empty());
    let state = store.index_status().unwrap().conversion_states.remove(0);
    assert_eq!(state.status, "awaiting_conversion");
    assert!(state.error.unwrap_or_default().contains("1 MiB"), "the bounded preflight must reject decoded input rather than letting the positional parser run");
}

#[test]
fn basic_pass_bounds_form_streams_and_embedded_font_programs() {
    use lopdf::{dictionary, Stream};
    for form in [true, false] {
        let (_dir, store, root, key) = setup();
        let source = root.join("resource.pdf");
        pdf(&source, true, true);
        let mut doc = lopdf::Document::load(&source).unwrap();
        let page = doc.get_pages()[&1];
        let mut bytes = b"%".to_vec();
        bytes.extend(vec![b'a'; 2 * 1024 * 1024]);
        bytes.extend_from_slice(b"\nBT /F1 12 Tf 20 30 Td (boundedtoken) Tj ET");
        let resources = doc
            .get_object(page)
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Resources")
            .unwrap()
            .clone();
        if form {
            let stream = doc.add_object(Stream::new(dictionary!{"Type"=>"XObject", "Subtype"=>"Form", "BBox"=>vec![0.into(),0.into(),200.into(),100.into()], "Resources"=>resources},bytes));
            let contents = doc
                .get_object(page)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Contents")
                .unwrap()
                .as_reference()
                .unwrap();
            doc.get_object_mut(contents)
                .unwrap()
                .as_stream_mut()
                .unwrap()
                .set_plain_content(b"q /Fm1 Do Q".to_vec());
            doc.get_object_mut(page)
                .unwrap()
                .as_dict_mut()
                .unwrap()
                .get_mut(b"Resources")
                .unwrap()
                .as_dict_mut()
                .unwrap()
                .set("XObject", dictionary! {"Fm1"=>stream});
        } else {
            let font = resources
                .as_dict()
                .unwrap()
                .get(b"Font")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"F1")
                .unwrap()
                .as_reference()
                .unwrap();
            let stream = doc.add_object(Stream::new(dictionary! {}, bytes));
            let descriptor =
                doc.add_object(dictionary! {"Type"=>"FontDescriptor", "FontFile2"=>stream});
            doc.get_object_mut(font)
                .unwrap()
                .as_dict_mut()
                .unwrap()
                .set("FontDescriptor", descriptor);
        }
        doc.compress();
        doc.save(&source).unwrap();
        let mut settings = store.settings().unwrap();
        settings.runtime.conversion_output_mib = 1;
        settings.runtime.max_text_input_mib = 1;
        settings.runtime.max_decoded_text_mib = 1;
        store.save_settings(&settings).unwrap();
        store.scan_basic(&key, |_| {}).unwrap();
        assert!(
            store.files().unwrap().is_empty(),
            "resource budget must reject {} before positional parsing",
            if form {
                "Form streams"
            } else {
                "font programs"
            }
        );
        assert!(store.index_status().unwrap().conversion_states[0]
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("1 MiB"));
    }
}

#[test]
fn basic_resource_rejection_keeps_the_normal_anydoc_fallback_for_text_pdf() {
    let (_dir, store, root, key) = setup();
    let source = root.join("text.pdf");
    pdf(&source, true, false);
    let mut doc = lopdf::Document::load(&source).unwrap();
    // An unused stream is conservatively charged by basic preflight but has no
    // bearing on the existing structured text conversion.
    doc.add_object(lopdf::Stream::new(
        lopdf::dictionary! {},
        vec![b'x'; 2 * 1024 * 1024],
    ));
    doc.compress();
    doc.save(&source).unwrap();
    let mut settings = store.settings().unwrap();
    settings.runtime.conversion_output_mib = 1;
    settings.runtime.max_text_input_mib = 1;
    settings.runtime.max_decoded_text_mib = 1;
    store.save_settings(&settings).unwrap();
    store.scan_basic(&key, |_| {}).unwrap();
    assert_eq!(
        store.index_status().unwrap().conversion_states[0].status,
        "awaiting_conversion"
    );
    store.enhance(&key, |_| {}).unwrap();
    assert_eq!(
        store.files().unwrap()[0].index_status.as_deref(),
        Some("complete")
    );
    assert!(!store
        .search_files("progressivetoken", &[], 10)
        .unwrap()
        .is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn blocking_enhancement_does_not_block_new_files_in_same_or_other_root() {
    use sag_core::service::{EventSink, Service};
    use sag_protocol::Operation;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    fs::create_dir(&data).unwrap();
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    pdf(&first.join("a.pdf"), true, true);
    fake_docling(
        &data,
        r#"#!/bin/sh
set -eu
output=''
shift
while [ "$#" -gt 0 ]; do
  if [ "$1" = '--output-file' ]; then shift; output="$1"; fi
  shift
done
touch "$(dirname "$0")/started"
while [ ! -f "$(dirname "$0")/release" ]; do sleep 0.02; done
printf 'enhancedtoken\n' > "$output"
"#,
    );
    let seed = Store::open(&data.join("sag.sqlite")).unwrap();
    let mut settings = seed.settings().unwrap();
    settings.convert_other_formats = true;
    settings.conversion_formats = Some(vec!["pdf".into()]);
    settings.runtime.watch_debounce_ms = 50;
    settings.runtime.watch_max_delay_ms = 100;
    settings.runtime.conversion_job_mib = 64;
    seed.save_settings(&settings).unwrap();
    let root = seed.add_root(&first).unwrap();
    drop(seed);
    let service = Service::open(data.clone()).unwrap();
    let sink = EventSink {
        owner: "regression".into(),
        request_id: None,
        cancel: service.stop.clone(),
        bus: service.events.clone(),
    };
    let marker = data.join("docling/bin/started");
    let release = data.join("docling/bin/release");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !marker.is_file() && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let started = marker.is_file();
    fs::write(first.join("b.txt"), "sameforegroundtoken").unwrap();
    fs::write(second.join("b.txt"), "otherforegroundtoken").unwrap();
    let refresh = service
        .handle(Operation::RefreshRoot { path: root.clone() }, &sink)
        .await;
    let add = service
        .handle(
            Operation::AddRoot {
                path: second.to_string_lossy().into_owned(),
            },
            &sink,
        )
        .await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut searchable = false;
    while std::time::Instant::now() < deadline {
        searchable = !service
            .store
            .search_files("sameforegroundtoken", &[], 10)
            .unwrap()
            .is_empty()
            && !service
                .store
                .search_files("otherforegroundtoken", &[], 10)
                .unwrap()
                .is_empty();
        if searchable {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let still_blocked = !release.is_file();
    fs::write(&release, "").unwrap();
    service.finish().await.unwrap();
    assert!(started, "controlled OCR process must start");
    assert!(refresh.is_ok() && add.is_ok());
    assert!(still_blocked);
    assert!(searchable,"native files in both the same root and another root must be searchable before releasing blocked OCR");
}

#[test]
fn basic_resource_total_uses_preparation_budget_without_spending_the_prose_budget() {
    let (_dir, store, root, key) = setup();
    let source = root.join("resources.pdf");
    pdf(&source, true, false);
    let mut doc = lopdf::Document::load(&source).unwrap();
    // Multiple individually small support streams must not consume the
    // document's prose allowance. No parser needs to inflate raster data.
    for _ in 0..2 {
        doc.add_object(lopdf::Stream::new(
            lopdf::dictionary! {},
            vec![b'x'; 600 * 1024],
        ));
    }
    doc.compress();
    doc.save(&source).unwrap();
    let mut settings = store.settings().unwrap();
    settings.runtime.conversion_output_mib = 1;
    settings.runtime.max_text_input_mib = 1;
    settings.runtime.max_decoded_text_mib = 1;
    store.save_settings(&settings).unwrap();
    store.scan_basic(&key, |_| {}).unwrap();
    assert!(!store
        .search_files("progressivetoken", &[], 10)
        .unwrap()
        .is_empty());
}

#[cfg(unix)]
fn background_service_fixture() -> (
    tempfile::TempDir,
    std::sync::Arc<sag_core::service::Service>,
    sag_core::service::EventSink,
    String,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    background_service_fixture_with_workers(1)
}
#[cfg(unix)]
fn background_service_fixture_with_workers(
    workers: usize,
) -> (
    tempfile::TempDir,
    std::sync::Arc<sag_core::service::Service>,
    sag_core::service::EventSink,
    String,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    fs::create_dir(&data).unwrap();
    let root = dir.path().join("files");
    fs::create_dir(&root).unwrap();
    pdf(&root.join("a.pdf"), true, true);
    if workers > 1 {
        pdf(&root.join("b.pdf"), true, true);
    }
    fake_docling(
        &data,
        r#"#!/bin/sh
set -eu
source="$1"; output=''; shift
while [ "$#" -gt 0 ]; do
  if [ "$1" = '--output-file' ]; then shift; output="$1"; fi
  shift
done
touch "$(dirname "$0")/$(basename "$source").started"
touch "$(dirname "$0")/started"
while [ ! -f "$(dirname "$0")/release" ]; do sleep 0.02; done
printf 'enhancedtoken\n' > "$output"
"#,
    );
    let seed = Store::open(&data.join("sag.sqlite")).unwrap();
    let mut settings = seed.settings().unwrap();
    settings.conversion_workers = workers;
    settings.convert_other_formats = true;
    settings.conversion_formats = Some(vec!["pdf".into()]);
    settings.runtime.watch_debounce_ms = 50;
    settings.runtime.watch_max_delay_ms = 100;
    settings.runtime.conversion_job_mib = 64;
    seed.save_settings(&settings).unwrap();
    let key = seed.add_root(&root).unwrap();
    drop(seed);
    let service = sag_core::service::Service::open(data.clone()).unwrap();
    let sink = sag_core::service::EventSink {
        owner: "regression".into(),
        request_id: None,
        cancel: service.stop.clone(),
        bus: service.events.clone(),
    };
    (
        dir,
        service,
        sink,
        key,
        data.join("docling/bin/started"),
        data.join("docling/bin/release"),
    )
}

#[cfg(unix)]
async fn await_marker(marker: &std::path::Path) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !marker.is_file() && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    marker.is_file()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn changed_active_pdf_retracts_old_basic_and_recovers_without_blocking_native_files() {
    let (_dir, service, sink, key, marker, release) = background_service_fixture();
    let started = await_marker(&marker).await;
    let source = std::path::Path::new(&key).join("a.pdf");
    let mut replacement = lopdf::Document::load(&source).unwrap();
    let page = replacement.get_pages()[&1];
    let contents = replacement
        .get_object(page)
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_reference()
        .unwrap();
    replacement
        .get_object_mut(contents)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_plain_content(b"BT /F1 12 Tf 20 30 Td (newgenerationtoken) Tj ET".to_vec());
    replacement.save(&source).unwrap();
    fs::write(
        std::path::Path::new(&key).join("b.txt"),
        "unblockednativetoken",
    )
    .unwrap();
    let refresh = service
        .handle(
            sag_protocol::Operation::RefreshRoot { path: key.clone() },
            &sink,
        )
        .await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut recovered = false;
    while std::time::Instant::now() < deadline {
        recovered = service
            .store
            .search_files("progressivetoken", &[], 10)
            .unwrap()
            .is_empty()
            && !service
                .store
                .search_files("newgenerationtoken", &[], 10)
                .unwrap()
                .is_empty()
            && !service
                .store
                .search_files("unblockednativetoken", &[], 10)
                .unwrap()
                .is_empty();
        if recovered {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let pre_release = !release.is_file();
    fs::write(&release, "").unwrap();
    service.finish().await.unwrap();
    assert!(started && refresh.is_ok() && pre_release);
    assert!(recovered,"changed A needs a fresh basic generation, old A must disappear, and native B must be searchable before OCR release");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn shutdown_cancels_both_lanes_while_ocr_is_blocked_and_paused() {
    let (_dir, service, _sink, _key, marker, release) = background_service_fixture();
    let started = await_marker(&marker).await;
    service.store.indexing.set_paused(true);
    let finished = tokio::time::timeout(std::time::Duration::from_secs(5), service.finish()).await;
    let closed = matches!(finished, Ok(Ok(())));
    fs::write(&release, "").unwrap();
    assert!(
        started && closed,
        "shutdown must stop parsing and wake both paused lanes without waiting for OCR release"
    );
    assert!(service.store.index_status().unwrap().tasks.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn removing_root_cancels_its_background_job_without_resurrecting_sources() {
    let (dir, service, sink, key, marker, release) = background_service_fixture();
    let started = await_marker(&marker).await;
    let removed = service
        .handle(
            sag_protocol::Operation::RemoveRoot { path: key.clone() },
            &sink,
        )
        .await;
    let second = dir.path().join("other");
    fs::create_dir(&second).unwrap();
    fs::write(second.join("b.txt"), "otherrootstillworks").unwrap();
    let added = service
        .handle(
            sag_protocol::Operation::AddRoot {
                path: second.to_string_lossy().into_owned(),
            },
            &sink,
        )
        .await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut done = false;
    while std::time::Instant::now() < deadline {
        done = service
            .store
            .search_files("progressivetoken", &[], 10)
            .unwrap()
            .is_empty()
            && !service
                .store
                .index_status()
                .unwrap()
                .tasks
                .iter()
                .any(|task| task.path == key)
            && !service
                .store
                .search_files("otherrootstillworks", &[], 10)
                .unwrap()
                .is_empty();
        if done {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let diagnostic=format!("started={started}; removed_ok={}; removed_error={:?}; added_ok={}; added_error={:?}; done={done}; roots={:?}; status={:?}",removed.is_ok(),removed.as_ref().err(),added.is_ok(),added.as_ref().err(),service.store.roots().unwrap(),service.store.index_status().unwrap());
    fs::write(&release, "").unwrap();
    service.finish().await.unwrap();
    assert!(
        started && removed.is_ok() && added.is_ok() && done,
        "{diagnostic}"
    );
    assert!(service
        .store
        .roots()
        .unwrap()
        .iter()
        .all(|root| root.path != key));
    assert!(service
        .store
        .index_status()
        .unwrap()
        .conversion_states
        .iter()
        .all(|state| state.root != key));
}

#[test]
fn sequential_page_work_does_not_consume_the_retained_resource_budget() {
    use lopdf::{dictionary, Object, Stream};
    let (_dir, store, root, key) = setup();
    let source = root.join("many-pages.pdf");
    pdf(&source, true, false);
    let mut doc = lopdf::Document::load(&source).unwrap();
    let first = *doc.get_pages().values().next().unwrap();
    let template = doc.get_object(first).unwrap().as_dict().unwrap().clone();
    let parent = template.get(b"Parent").unwrap().as_reference().unwrap();
    let mut kids = Vec::new();
    // >64 MiB of sequential instruction work, each page <1 MiB. The parser
    // discards instruction buffers; this is not 64 MiB of retained text.
    for _ in 0..129 {
        let mut bytes = vec![b'x'; 512 * 1024];
        bytes[0] = b'%';
        bytes.extend_from_slice(b"\nBT /F1 12 Tf 20 30 Td (progressivetoken) Tj ET");
        let content = doc.add_object(Stream::new(dictionary! {}, bytes));
        let mut page = template.clone();
        page.set("Contents", content);
        kids.push(Object::Reference(doc.add_object(page)));
    }
    doc.get_object_mut(parent)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Kids", kids);
    doc.get_object_mut(parent)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Count", 129);
    doc.compress();
    doc.save(&source).unwrap();
    drop(doc);
    let mut settings = store.settings().unwrap();
    settings.runtime.text_preparation_mib = 64;
    settings.runtime.conversion_output_mib = 1;
    settings.runtime.max_text_input_mib = 1;
    settings.runtime.max_decoded_text_mib = 1;
    store.save_settings(&settings).unwrap();
    store.scan_basic(&key, |_| {}).unwrap();
    assert!(!store
        .search_files("progressivetoken", &[], 10)
        .unwrap()
        .is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn new_pdf_basic_text_is_searchable_in_same_root_while_another_pdf_is_enhanced() {
    let (_dir, service, sink, key, marker, release) = background_service_fixture();
    let started = await_marker(&marker).await;
    let source = std::path::Path::new(&key).join("b.pdf");
    pdf(&source, true, true);
    let mut doc = lopdf::Document::load(&source).unwrap();
    let page = doc.get_pages()[&1];
    let stream = doc
        .get_object(page)
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.get_object_mut(stream)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_plain_content(b"BT /F1 12 Tf 20 30 Td (newpdfbetoken) Tj ET".to_vec());
    doc.save(&source).unwrap();
    let refreshed = service
        .handle(
            sag_protocol::Operation::RefreshRoot { path: key.clone() },
            &sink,
        )
        .await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut searchable = false;
    while std::time::Instant::now() < deadline {
        searchable = !service
            .store
            .search_files("newpdfbetoken", &[], 10)
            .unwrap()
            .is_empty();
        if searchable {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let before_release = !release.is_file();
    fs::write(&release, "").unwrap();
    service.finish().await.unwrap();
    assert!(
        started && refreshed.is_ok() && before_release && searchable,
        "new PDF B basic extraction must not queue behind active PDF A OCR in the same root"
    );
}

#[test]
fn failed_primary_scan_keeps_error_status_and_existing_index_until_successful_retry() {
    for unavailable in [false, true] {
        let (dir, store, root, key) = setup();
        fs::write(root.join("kept.txt"), "preservedevidence").unwrap();
        store.scan_basic(&key, |_| {}).unwrap();
        if unavailable {
            fs::rename(&root, dir.path().join("unavailable")).unwrap();
        } else {
            fs::write(root.join(".sagignore"), "[z-a]\n").unwrap();
        }
        assert!(store.scan_basic(&key, |_| {}).is_err());
        let status = store.roots().unwrap().remove(0);
        assert_eq!(status.status, "error");
        assert!(status.error.is_some());
        assert_eq!(store.files().unwrap().len(), 1);
        if unavailable {
            fs::rename(dir.path().join("unavailable"), &root).unwrap();
        } else {
            fs::write(root.join(".sagignore"), "").unwrap();
        }
        store.scan_basic(&key, |_| {}).unwrap();
        assert_eq!(store.roots().unwrap()[0].status, "ready");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn excluding_an_active_source_cancels_ocr_and_never_republishes_it() {
    let (_dir, service, sink, key, marker, release) = background_service_fixture();
    let started = await_marker(&marker).await;
    let source = std::path::Path::new(&key).join("a.pdf");
    let removed = service
        .handle(
            sag_protocol::Operation::RemoveRoot {
                path: source.to_string_lossy().into_owned(),
            },
            &sink,
        )
        .await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut cancelled = false;
    while std::time::Instant::now() < deadline {
        cancelled = !service
            .store
            .index_status()
            .unwrap()
            .tasks
            .iter()
            .any(|task| task.path == key && task.kind == "enhancement");
        if cancelled {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let no_evidence = service
        .store
        .search_files("progressivetoken", &[], 10)
        .unwrap()
        .is_empty();
    let no_state = service
        .store
        .index_status()
        .unwrap()
        .conversion_states
        .iter()
        .all(|state| std::path::Path::new(&state.source) != source);
    let untouched = source.is_file();
    let before_release = !release.is_file();
    fs::write(&release, "").unwrap();
    service.finish().await.unwrap();
    assert!(
        started
            && removed.is_ok()
            && cancelled
            && no_evidence
            && no_state
            && untouched
            && before_release
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn bulk_clear_and_reindex_remain_blocked_when_primary_fails_during_background_work() {
    let (_dir, service, sink, key, marker, release) = background_service_fixture();
    let started = await_marker(&marker).await;
    let rules = std::path::Path::new(&key).join(".sagignore");
    fs::write(&rules, "[z-a]\n").unwrap();
    service
        .handle(
            sag_protocol::Operation::RefreshRoot { path: key.clone() },
            &sink,
        )
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while service.store.roots().unwrap()[0].status != "error"
        && std::time::Instant::now() < deadline
    {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let failed = service.store.roots().unwrap()[0].status == "error";
    let background = service
        .store
        .index_status()
        .unwrap()
        .tasks
        .iter()
        .any(|task| task.path == key && task.kind == "enhancement");
    let protected = service.store.remove_all_roots().is_err()
        && service.store.clear_all_data().is_err()
        && service.store.prepare_reindex().is_err();
    fs::write(&rules, "").unwrap();
    fs::write(&release, "").unwrap();
    service.finish().await.unwrap();
    assert!(
        started && failed && background && protected,
        "active lanes must protect data even while the root reports a foreground error"
    );
}

#[test]
fn pending_source_disappearance_finishes_cleanly_but_unmounted_root_keeps_its_index() {
    for unmounted in [false, true] {
        let (dir, store, root, key) = setup();
        let source = root.join("a.pdf");
        pdf(&source, true, true);
        store.scan_basic(&key, |_| {}).unwrap();
        if unmounted {
            fs::rename(&root, dir.path().join("unavailable")).unwrap();
            assert!(store.enhance(&key, |_| {}).is_err());
            assert_eq!(store.files().unwrap().len(), 1);
            assert_eq!(store.roots().unwrap()[0].status, "error");
        } else {
            fs::remove_file(source).unwrap();
            store.enhance(&key, |_| {}).unwrap();
            assert!(store.files().unwrap().is_empty());
            assert!(store.index_status().unwrap().conversion_states.is_empty());
            assert_eq!(store.roots().unwrap()[0].status, "ready");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn configured_parallel_conversion_workers_start_two_ocr_jobs_without_blocking_primary() {
    let (_dir, service, sink, key, marker, release) = background_service_fixture_with_workers(2);
    let a = marker.with_file_name("a.pdf.started");
    let b = marker.with_file_name("b.pdf.started");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !(a.is_file() && b.is_file()) && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let parallel = a.is_file() && b.is_file();
    fs::write(
        std::path::Path::new(&key).join("native.txt"),
        "batchforegroundtoken",
    )
    .unwrap();
    let refreshed = service
        .handle(
            sag_protocol::Operation::RefreshRoot { path: key.clone() },
            &sink,
        )
        .await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut native = false;
    while std::time::Instant::now() < deadline {
        native = !service
            .store
            .search_files("batchforegroundtoken", &[], 10)
            .unwrap()
            .is_empty();
        if native {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let diagnostic = format!(
        "parallel={parallel}; native={native}; status={:?}",
        service.store.index_status().unwrap()
    );
    let before_release = !release.is_file();
    fs::write(&release, "").unwrap();
    service.finish().await.unwrap();
    assert!(
        parallel && native && refreshed.is_ok() && before_release,
        "{diagnostic}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn retry_failed_ocr_keeps_foreground_ingestion_available() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    fs::create_dir(&data).unwrap();
    let root = dir.path().join("files");
    fs::create_dir(&root).unwrap();
    pdf(&root.join("a.pdf"), true, true);
    fake_docling(
        &data,
        r#"#!/bin/sh
set -eu
base=$(dirname "$0")
if [ ! -f "$base/failed-once" ]; then touch "$base/failed-once"; exit 1; fi
output=''; shift
while [ "$#" -gt 0 ]; do
 if [ "$1" = '--output-file' ]; then shift; output="$1"; fi
 shift
done
touch "$base/retry-started"
while [ ! -f "$base/release" ]; do sleep 0.02; done
printf 'enhancedtoken\n' > "$output"
"#,
    );
    let seed = Store::open(&data.join("sag.sqlite")).unwrap();
    let mut settings = seed.settings().unwrap();
    settings.convert_other_formats = true;
    settings.conversion_formats = Some(vec!["pdf".into()]);
    settings.runtime.watch_debounce_ms = 50;
    settings.runtime.watch_max_delay_ms = 100;
    settings.runtime.conversion_job_mib = 64;
    seed.save_settings(&settings).unwrap();
    let key = seed.add_root(&root).unwrap();
    drop(seed);
    let service = sag_core::service::Service::open(data.clone()).unwrap();
    let sink = sag_core::service::EventSink {
        owner: "retry-regression".into(),
        request_id: None,
        cancel: service.stop.clone(),
        bus: service.events.clone(),
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut failed = false;
    while std::time::Instant::now() < deadline {
        failed = service.store.index_status().unwrap().failed_files == 1
            && service.store.indexing.status().tasks.is_empty();
        if failed {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let retry = service
        .handle(sag_protocol::Operation::RetryFailedIndexes, &sink)
        .await;
    let started = await_marker(&data.join("docling/bin/retry-started")).await;
    fs::write(root.join("b.txt"), "retryshouldnotblocknative").unwrap();
    service
        .handle(sag_protocol::Operation::RefreshRoot { path: key }, &sink)
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut searchable = false;
    while std::time::Instant::now() < deadline {
        searchable = !service
            .store
            .search_files("retryshouldnotblocknative", &[], 10)
            .unwrap()
            .is_empty();
        if searchable {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    fs::write(data.join("docling/bin/release"), "").unwrap();
    service.finish().await.unwrap();
    assert!(failed && retry.is_ok() && started);
    assert!(
        searchable,
        "manual OCR retries must run in the background lane so newly added text can be indexed"
    );
}

// Keep this deterministic: publish through the real enhancement path exactly
// after the primary scan takes its converted-file snapshot.
#[test]
#[cfg(unix)]
fn converted_scan_keeps_results_published_after_its_snapshot() {
    let (dir, store, root, key) = setup();
    pdf(&root.join("a-scan.pdf"), false, true);
    pdf(&root.join("b-text.pdf"), true, true);
    store.scan_basic(&key, |_| {}).unwrap();
    fake_docling(
        dir.path(),
        r#"#!/bin/sh
set -eu
shift
output=''
while [ "$#" -gt 0 ]; do
 if [ "$1" = '--output-file' ]; then shift; output="$1"; fi
 shift
done
printf 'concurrentenhancedtoken\n' > "$output"
"#,
    );
    // Separate controls allow this synchronous probe to model an already-running
    // service enhancement completing while the primary lane is scanning.
    let background = Store::open(&dir.path().join("index.sqlite")).unwrap();
    let published = std::cell::Cell::new(false);
    store
        .scan_basic(&key, |progress| {
            if store.indexing.task(&key).stage == "indexing_converted"
                && progress.count == 0
                && !published.replace(true)
            {
                background.enhance(&key, |_| {}).unwrap();
                assert_eq!(
                    store
                        .search_files("concurrentenhancedtoken", &[], 10)
                        .unwrap()
                        .len(),
                    1
                );
            }
        })
        .unwrap();
    assert!(published.get());
    assert_eq!(
        store
            .search_files("concurrentenhancedtoken", &[], 10)
            .unwrap()
            .len(),
        1,
        "an old scan must not remove a newly published enhancement"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn new_rtf_is_searchable_while_pdf_enhancement_is_blocked() {
    let (_dir, service, sink, key, marker, release) = background_service_fixture();
    let started = await_marker(&marker).await;
    let mut settings = service.store.settings().unwrap();
    settings.conversion_formats = Some(vec!["pdf".into(), "rtf".into()]);
    service.store.save_settings(&settings).unwrap();
    fs::write(
        std::path::Path::new(&key).join("new.rtf"),
        r"{\rtf1\ansi newrtfsearchtoken}",
    )
    .unwrap();
    let refreshed = service
        .handle(
            sag_protocol::Operation::RefreshRoot { path: key.clone() },
            &sink,
        )
        .await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut searchable = false;
    while std::time::Instant::now() < deadline {
        searchable = !service
            .store
            .search_files("newrtfsearchtoken", &[], 10)
            .unwrap()
            .is_empty();
        if searchable {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let before_release = !release.is_file();
    fs::write(&release, "").unwrap();
    service.finish().await.unwrap();
    assert!(
        started && refreshed.is_ok() && before_release && searchable,
        "built-in RTF parsing must not wait behind PDF OCR"
    );
}
