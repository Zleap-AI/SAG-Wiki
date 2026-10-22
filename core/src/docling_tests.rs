use super::*;
use crate::store::Store;

#[test]
fn other_formats_are_opt_in_and_never_reconvert_native_or_generated_text() {
    assert!(!crate::types::Settings::default().convert_other_formats);
    for name in ["report.PDF", "notes.docx", "slides.pptx", "book.epub", "voice.mp3", "clip.mp4", "data.xlsx", "letter.eml", "archive.tar.gz"] {
        assert!(is_source(Path::new(name)), "{name}");
    }
    for name in ["notes.md", "code.rs", "page.html", ".sag-converted/report.pdf", "image.exe"] {
        assert!(!is_source(Path::new(name)), "{name}");
    }
    assert_eq!(output_path(Path::new("/library"), Path::new("/library/a/report.pdf")).unwrap(), Path::new("/library/.sag-converted/a/report.pdf.md"));
}

#[cfg(unix)]
fn fixture() -> (tempfile::TempDir, Store, String) {
    fixture_with_policy(None)
}

#[cfg(unix)]
fn fixture_with_policy(policy: Option<crate::store::FileAccess>) -> (tempfile::TempDir, Store, String) {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    fs::create_dir(&root).unwrap();
    let store = Store::open_with_file_access(&temp.path().join("data/index.sqlite"), policy).unwrap();
    let bin = store.docling.home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let command = bin.join("docling");
    fs::write(&command, r#"#!/bin/sh
set -eu
source="$1"
shift
output=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = '--output-file' ]; then shift; output="$1"; fi
  shift
done
if [ "$(cat "$source")" = 'FAIL' ]; then echo 'invalid PDF' >&2; exit 1; fi
cat "$source" > "$output"
echo converted >> "$(dirname "$0")/calls"
"#).unwrap();
    fs::set_permissions(command, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(store.docling.home.join("ready"), VERSION).unwrap();
    let root = store.add_root(&root).unwrap();
    (temp, store, root)
}

#[test]
fn builtin_formats_convert_without_any_component_and_component_formats_stay_opt_in() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    fs::create_dir(&library).unwrap();
    let store = Store::open(&temp.path().join("data/index.sqlite")).unwrap();
    let root = store.add_root(&library).unwrap();
    fs::write(library.join("table.csv"), "型号,价格\nBCD-215,1999\n").unwrap();
    fs::write(library.join("note.docx"), crate::anydoc::tests::docx("内置转换 builtindocxmarker")).unwrap();
    fs::write(library.join("photo.png"), "not really an image").unwrap();
    assert!(!store.docling.installed());
    assert!(store.docling.require_formats(&Some(vec!["csv".into(), "xlsx".into()])).is_ok());
    assert!(store.docling.require_formats(&Some(vec!["png".into()])).is_err());
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.search_files("BCD-215", &[], 5).unwrap().len(), 1);
    assert_eq!(store.search_files("builtindocxmarker", &[], 5).unwrap().len(), 1);
    let state = &store.roots().unwrap()[0];
    assert_eq!(state.status, "ready", "{:?}", state.error);
    assert!(!library.join(OUTPUT_DIRECTORY).join("photo.png.md").exists());
    assert!(fs::read_to_string(library.join(OUTPUT_DIRECTORY).join("note.docx.md")).unwrap().contains("note.docx"));
    let status = store.docling.status();
    assert!(status.builtin_formats.iter().any(|(_, ext)| ext.iter().any(|e| e == "pdf")));
    assert!(status.formats.iter().all(|(_, ext)| ext.iter().all(|e| !crate::anydoc::handles_extension(e))));
    assert!(status.formats.iter().any(|(_, ext)| ext.iter().any(|e| e == "png")));
    // Removing the source retires our derivative even though no folder opted into other formats.
    fs::remove_file(library.join("note.docx")).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert!(store.search_files("builtindocxmarker", &[], 5).unwrap().is_empty());
    assert!(!library.join(OUTPUT_DIRECTORY).join("note.docx.md").exists());
}

#[test]
fn pem_keys_are_skipped_without_excluding_keynote_documents() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    fs::create_dir(&library).unwrap();
    fs::write(library.join("server.key"), "-----BEGIN PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----\n").unwrap();
    fs::write(library.join("deck.key"), "PK\x03\x04 not a certificate").unwrap();
    assert!(is_pem_key(&library.join("server.key")).unwrap());
    assert!(!is_pem_key(&library.join("deck.key")).unwrap());
    assert!(is_source(Path::new("deck.key")));
    let store = Store::open(&temp.path().join("data/index.sqlite")).unwrap();
    let root = store.add_root(&library).unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    settings.conversion_formats = None;
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    // The private key is never handed to a converter; the Keynote file is reported as needing Docling.
    let state = &store.roots().unwrap()[0];
    assert_eq!(state.status, "partial");
    let error = state.error.clone().unwrap();
    assert!(error.contains("deck.key") && !error.contains("server.key"), "{error}");
    assert!(store.search_files("secret", &[], 5).unwrap().is_empty());
}

#[test]
fn scanned_pdf_without_a_component_is_reported_as_an_index_issue() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    fs::create_dir(&library).unwrap();
    let store = Store::open(&temp.path().join("data/index.sqlite")).unwrap();
    let root = store.add_root(&library).unwrap();
    fs::write(library.join("note.md"), "nativemarker").unwrap();
    fs::write(library.join("scan.pdf"), crate::anydoc::tests::image_only_pdf()).unwrap();
    store.scan(&root, |_| {}).unwrap();
    let state = &store.roots().unwrap()[0];
    assert_eq!(state.status, "partial");
    let error = state.error.clone().unwrap();
    assert!(error.contains("OCR") && error.contains("Docling"), "{error}");
    assert_eq!(store.search_files("nativemarker", &[], 5).unwrap().len(), 1);
    assert!(!library.join(OUTPUT_DIRECTORY).join("scan.pdf.md").exists());
}

#[cfg(unix)]
#[test]
fn conversion_indexes_markdown_preserves_sources_and_reuses_unchanged_outputs() {
    let (_temp, store, root) = fixture();
    let root_path = Path::new(&root);
    fs::create_dir(root_path.join("nested")).unwrap();
    let source = root_path.join("nested/report.pages");
    fs::write(&source, "evidenceone").unwrap();
    fs::write(root_path.join("nested/report.numbers"), "evidencetwo").unwrap();
    fs::write(root_path.join("ignored.pages"), "invisible").unwrap();
    fs::write(root_path.join(".sagignore"), "ignored.pages\n").unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert!(!root_path.join(OUTPUT_DIRECTORY).exists());
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    settings.file_types.markdown = false; // This switch concerns original Markdown only.
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    let output = output_path(root_path, &source).unwrap();
    assert!(fs::read_to_string(&output).unwrap().contains("../../nested/report.pages"));
    assert_eq!(fs::read_to_string(&source).unwrap(), "evidenceone");
    let history_id = store.index_history().unwrap()[0].id;
    let raw = store.index_history_log(history_id).unwrap().raw.unwrap();
    let recorded = raw.lines().map(|line| serde_json::from_str::<sag_protocol::IndexFileLog>(line).unwrap()).collect::<Vec<_>>();
    assert!(recorded.iter().any(|file| file.path == source.to_str().unwrap()));
    assert!(recorded.iter().all(|file| !file.path.contains(OUTPUT_DIRECTORY)));
    assert_eq!(store.authorize_index_history_file(history_id, source.to_str().unwrap()).unwrap(), source);
    assert!(!store.entries(root_path, false).unwrap().iter().any(|entry| entry.name == OUTPUT_DIRECTORY));
    assert!(store.entries(root_path, true).unwrap().iter().any(|entry| entry.name == OUTPUT_DIRECTORY));
    assert_eq!(store.search_files("evidenceone", &[], 5).unwrap()[0].path, output.to_str().unwrap());
    assert!(store.search_files("invisible", &[], 5).unwrap().is_empty());
    let scope = vec![root_path.join("nested").to_string_lossy().into_owned()];
    assert_eq!(store.search_files("evidenceone", &scope, 5).unwrap().len(), 1);
    assert_eq!(store.candidate_query("evidenceone", None, &scope, &[], &[], 5).unwrap().len(), 1);
    assert!(root_path.join(".sag-converted/nested/report.numbers.md").is_file());
    let calls = store.docling.home.join("bin/calls");
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 2);
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 2);
    store.remove_root(&root).unwrap();
    store.add_root(root_path).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 2);
    assert_eq!(store.search_files("evidenceone", &[], 5).unwrap().len(), 1);
    fs::write(&source, "updatedevidence").unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.search_files("updatedevidence", &[], 5).unwrap().len(), 1);
    assert!(store.search_files("evidenceone", &[], 5).unwrap().is_empty());
    store.remove_path(&root_path.join("nested")).unwrap();
    assert!(store.search_files("updatedevidence", &[], 5).unwrap().is_empty());
    assert!(source.exists() && output.exists());
    store.add_root(&root_path.join("nested")).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.search_files("updatedevidence", &[], 5).unwrap().len(), 1);
    settings.convert_other_formats = false;
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert!(store.search_files("updatedevidence", &[], 5).unwrap().is_empty());
    assert!(source.exists() && output.exists());
}

#[cfg(unix)]
#[test]
fn failed_updates_never_serve_stale_evidence_and_do_not_overwrite_manual_edits() {
    let (_temp, store, root) = fixture();
    let root_path = Path::new(&root);
    let source = root_path.join("report.pages");
    fs::write(&source, "originalevidence").unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    let output = output_path(root_path, &source).unwrap();
    fs::write(&source, "FAIL").unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert!(store.search_files("originalevidence", &[], 5).unwrap().is_empty());
    assert!(store.roots().unwrap()[0].error.as_ref().unwrap().contains("invalid PDF"));
    fs::write(&source, "newevidence").unwrap();
    fs::write(&output, "handedited").unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(fs::read_to_string(&output).unwrap(), "handedited");
    assert!(store.search_files("handedited", &[], 5).unwrap().is_empty());
    fs::remove_file(&source).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert!(output.exists());
    assert!(store.search_files("handedited", &[], 5).unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn removed_and_ignored_sources_remove_only_owned_derivatives() {
    let (_temp, store, root) = fixture();
    let root_path = Path::new(&root);
    let source = root_path.join("report.pages");
    fs::write(&source, "evidence").unwrap();
    let mut settings = store.settings().unwrap();settings.convert_other_formats = true;
    store.save_settings(&settings).unwrap();store.scan(&root, |_| {}).unwrap();
    let output = output_path(root_path, &source).unwrap();
    fs::write(root_path.join(".gitignore"), "*.pages\n").unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert!(!output.exists());
    assert!(source.exists());
    assert!(store.search_files("evidence", &[], 5).unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn output_links_cannot_escape_the_library() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    fs::create_dir(&root).unwrap();
    std::os::unix::fs::symlink(temp.path(), root.join(OUTPUT_DIRECTORY)).unwrap();
    assert!(output_path(&root, &root.join("report.pdf")).is_err());
}

#[cfg(unix)]
#[test]
fn component_runs_when_the_application_account_home_is_missing() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Docling::new(temp.path());
    fs::create_dir_all(&converter.home).unwrap();
    converter.run(converter.command("/bin/sh")
        .env("HOME", temp.path().join("missing-account-home"))
        .args(["-c", "printf 'component started\\n'"]), true).unwrap();
    assert!(converter.status().message.contains("component started"));
}

#[cfg(unix)]
#[test]
fn installation_startup_error_preserves_the_underlying_reason() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Arc::new(Docling::new(temp.path()));
    fs::create_dir_all(&converter.home).unwrap();
    fs::write(converter.home.join("uv"), "not executable").unwrap();
    converter.start().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while converter.status().running {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let error = converter.status().error.unwrap();
    assert!(error.contains("not executable"), "{error}");
    assert!(error.contains("uv"), "{error}");
}

#[cfg(unix)]
#[test]
fn download_cancellation_stops_the_process_and_never_marks_installation_ready() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Arc::new(Docling::new(temp.path()));
    let child_converter = converter.clone();
    let task = std::thread::spawn(move || child_converter.run(
        child_converter.command("sh").args(["-c", "printf 'download started\\n'; sleep 30"]), true));
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while converter.status().message.is_empty() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    converter.cancel();
    assert!(task.join().unwrap().unwrap_err().to_string().contains("已取消下载"));
    assert!(!converter.installed());
}

#[cfg(unix)]
#[test]
fn live_download_progress_is_visible_before_a_carriage_return_line_finishes() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Arc::new(Docling::new(temp.path()));
    let child_converter = converter.clone();
    let task = std::thread::spawn(move || child_converter.run(
        child_converter.command("sh").args(["-c", "printf 'torch 1.2 MiB/10.0 MiB\\r'; sleep 30"]), true));
    let deadline = std::time::Instant::now() + Duration::from_millis(600);
    let mut visible=false;
    while std::time::Instant::now()<deadline {
        if converter.status().message.contains("torch") {visible=true;break;}
        std::thread::sleep(Duration::from_millis(10));
    }
    converter.cancel();
    assert!(task.join().unwrap().is_err());
    assert!(visible,"下载进度被缓冲到进程结束，用户看不到正在下载的组件");
    assert_eq!(converter.status().downloaded, (1.2*1048576.0) as u64);
    assert_eq!(converter.status().total, Some(10*1048576));
}

#[test]
fn pipe_mode_uv_output_reports_progress_per_package() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Docling::new(temp.path());
    converter.progress("dependencies", "installing");
    converter.update_download_line(b"Resolved 111 packages in 1.51s");
    assert_eq!(converter.status().total, None);
    converter.update_download_line(b"Downloading torch (121.4MiB)");
    converter.update_download_line(b"Downloading numpy (5.2MiB)");
    let status = converter.status();
    assert_eq!(status.packages_total, 2);
    assert_eq!(status.packages_done, 0);
    assert_eq!(status.downloaded, 0);
    assert_eq!(status.total, Some((121.4 * 1048576.0) as u64 + (5.2 * 1048576.0) as u64));
    converter.update_download_line(b" Downloaded numpy");
    let status = converter.status();
    assert_eq!(status.packages_done, 1);
    assert_eq!(status.downloaded, (5.2 * 1048576.0) as u64);
    assert!(status.message.contains("Downloaded numpy"));
    converter.update_download_line(b"Prepared 111 packages in 16.05s");
    let status = converter.status();
    assert_eq!(status.packages_done, 2);
    assert_eq!(status.downloaded, status.total.unwrap());
    // A new step starts from zero.
    converter.progress("models", "models");
    let status = converter.status();
    assert_eq!(status.packages_total, 0);
    assert_eq!(status.total, None);
}

#[test]
fn terminal_download_frames_include_all_active_packages_and_model_percentages() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Docling::new(temp.path());
    converter.update_download_line(b"\x1b[2Kscipy    ------------------------------ 512 KiB/19.5 MiB       torch    ------------------------------ 2.0 MiB/121.4 MiB\x1b[2A");
    let status = converter.status();
    assert_eq!(status.downloaded, 512 * 1024 + 2 * 1048576);
    assert_eq!(status.total, Some((19.5 * 1048576.0) as u64 + (121.4 * 1048576.0) as u64));
    assert_eq!(status.message, "scipy: 512 KiB/19.5 MiB\ntorch: 2.0 MiB/121.4 MiB");
    converter.update_download_line(b"model.safetensors:  42%|####      | 42M/100M [00:03<00:04, 12MB/s]");
    assert_eq!(converter.status().percent, Some(42.0));
    assert_eq!(converter.status().total, None);
}

#[test]
fn stalled_http_download_cancels_without_waiting_for_network_timeout() {
    use std::net::TcpListener;
    let temp = tempfile::tempdir().unwrap();
    let converter = Arc::new(Docling::new(temp.path()));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut buffer = [0;4096];socket.read(&mut buffer).unwrap();
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\nx").unwrap();
        stopped.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let copy = converter.clone();
    let task = std::thread::spawn(move || copy.download_archive(&format!("http://{address}/archive")));
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while converter.status().downloaded == 0 {
        assert!(std::time::Instant::now()<deadline);std::thread::sleep(Duration::from_millis(10));
    }
    let start=std::time::Instant::now();converter.cancel();
    let result=task.join().unwrap();stop.send(()).unwrap();server.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("已取消下载"));
    assert!(start.elapsed()<Duration::from_millis(500));
}

#[cfg(unix)]
#[test]
fn nested_conversion_options_use_source_folder_rules_and_keep_siblings_unchanged() {
    use crate::types::{FileTypes, FolderIndexOptions};
    let (_temp, store, root) = fixture();
    let root_path = Path::new(&root);
    let a = root_path.join("a");
    let b = root_path.join("b");
    fs::create_dir_all(a.join("excluded")).unwrap();
    fs::create_dir(&b).unwrap();
    let enabled = FolderIndexOptions { extensions: None, file_types: FileTypes { markdown: false, text: false, code: false, config: false }, convert_other_formats: true, conversion_formats: None };
    let disabled = FolderIndexOptions { convert_other_formats: false, ..enabled.clone() };
    crate::folder_index::write(root_path, &disabled).unwrap();
    crate::folder_index::write(&a, &enabled).unwrap();
    crate::folder_index::write(&b, &enabled).unwrap();
    crate::folder_index::write(&a.join("excluded"), &disabled).unwrap();
    fs::write(a.join("report.pages"), "alpha evidence").unwrap();
    fs::write(b.join("report.pages"), "beta evidence").unwrap();
    fs::write(a.join("excluded/report.pages"), "excluded evidence").unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.files().unwrap().len(), 2);
    assert!(store.search_files("excluded", &[], 5).unwrap().is_empty());
    let a_output = output_path(root_path, &a.join("report.pages")).unwrap();
    let b_output = output_path(root_path, &b.join("report.pages")).unwrap();
    let b_before = fs::read(&b_output).unwrap();
    fs::write(b.join("report.pages"), "FAIL").unwrap();
    store.save_folder_index_options(&a, &disabled, true).unwrap();
    store.update_paths(&root, &[a.join(".sagignore")], |_| {}).unwrap();
    assert_eq!(store.roots().unwrap()[0].status, "ready");
    assert_eq!(store.files().unwrap().len(), 1);
    assert!(a_output.exists() && b_output.exists());
    assert_eq!(fs::read(&b_output).unwrap(), b_before);
    store.save_folder_index_options(&a, &enabled, true).unwrap();
    store.update_paths(&root, &[a.join(".sagignore")], |_| {}).unwrap();
    assert_eq!(store.roots().unwrap()[0].status, "ready");
    assert_eq!(store.files().unwrap().len(), 2);
    assert_eq!(fs::read_to_string(store.docling.home.join("bin/calls")).unwrap().lines().count(), 2);
    assert_eq!(fs::read_to_string(a.join("report.pages")).unwrap(), "alpha evidence");
}

#[cfg(unix)]
#[test]
fn selected_formats_are_persisted_inherited_and_filter_existing_and_incremental_indexes() {
    use crate::types::{FileTypes, FolderIndexOptions};
    let (_temp, store, root) = fixture();
    let root_path = Path::new(&root);
    let nested = root_path.join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(root_path.join("report.PAGES"), "selectedpdf evidence").unwrap();
    fs::write(root_path.join("notes.numbers"), "selectedword evidence").unwrap();
    fs::write(nested.join("voice.mp3"), "selectedaudio evidence").unwrap();
    let mut options = FolderIndexOptions {
        extensions: None,
        file_types: FileTypes::default(), convert_other_formats: true, conversion_formats: None,
    };
    crate::folder_index::write(root_path, &options).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.files().unwrap().len(), 3);
    let word_output = output_path(root_path, &root_path.join("notes.numbers")).unwrap();
    let word_before = fs::read(&word_output).unwrap();
    // A deselected source must not be converted again, even after it changes.
    fs::write(root_path.join("notes.numbers"), "FAIL").unwrap();
    options.conversion_formats = Some(vec!["pages".into()]);
    store.save_folder_index_options(root_path, &options, false).unwrap();
    store.update_paths(&root, &[root_path.join(".sagignore")], |_| {}).unwrap();
    assert_eq!(store.roots().unwrap()[0].status, "ready");
    assert_eq!(store.folder_index_options(&nested).unwrap().conversion_formats, options.conversion_formats);
    assert_eq!(store.files().unwrap().len(), 1);
    assert!(store.files().unwrap()[0].path.ends_with("report.PAGES.md"));
    assert!(store.search_files("selectedword", &[], 5).unwrap().is_empty());
    assert_eq!(fs::read(&word_output).unwrap(), word_before);
    let calls = store.docling.home.join("bin/calls");
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 3);
    fs::write(nested.join("added.numbers"), "FAIL").unwrap();
    fs::write(nested.join("added.pages"), "newselectedpdf evidence").unwrap();
    store.update_paths(&root, &[nested.join("added.numbers"), nested.join("added.pages")], |_| {}).unwrap();
    assert_eq!(store.roots().unwrap()[0].status, "ready");
    assert_eq!(store.files().unwrap().len(), 2);
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 4);
    // A child can select audio while keeping the parent's PDF selection unchanged.
    let child_options = FolderIndexOptions { conversion_formats: Some(vec!["mp3".into()]), ..options.clone() };
    store.save_folder_index_options(&nested, &child_options, true).unwrap();
    store.update_paths(&root, &[nested.join(".sagignore")], |_| {}).unwrap();
    let files = store.files().unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().any(|file| file.path.ends_with("report.PAGES.md")));
    assert!(files.iter().any(|file| file.path.ends_with("voice.mp3.md")));
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 4);
    assert_eq!(fs::read_to_string(root_path.join("report.PAGES")).unwrap(), "selectedpdf evidence");
    assert_eq!(crate::folder_index::read(root_path).unwrap().unwrap(), options);
    assert_eq!(crate::folder_index::read(&nested).unwrap().unwrap(), child_options);
}

#[test]
fn old_rules_keep_all_formats_and_format_choices_exclude_native_types() {
    let options: crate::types::FolderIndexOptions = serde_json::from_str(r#"{"file_types":{"markdown":true,"text":true,"code":false,"config":false},"convert_other_formats":true}"#).unwrap();
    assert_eq!(options.conversion_formats, None);
    assert!(is_selected(Path::new("report.PDF"), &options.conversion_formats));
    assert!(is_selected(Path::new("archive.tar.gz"), &Some(vec!["tar.gz".into()])));
    assert!(!is_selected(Path::new("report.docx"), &Some(vec!["pdf".into()])));
    assert!(!is_selected(Path::new("report.pdf"), &Some(vec![])));
    for (_, extensions) in formats() {
        for extension in extensions { assert!(is_source(Path::new(&format!("file.{extension}")))); }
    }
}

#[cfg(unix)]
#[test]
fn uninstall_removes_only_components_and_keeps_sources_converted_files_and_indexes() {
    let (_temp, store, root) = fixture();
    let source = Path::new(&root).join("report.pages");
    fs::write(&source, "keptconversion evidence").unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    let output = output_path(Path::new(&root), &source).unwrap();
    let converted = fs::read(&output).unwrap();
    let indexed = store.files().unwrap();
    for directory in ["models", "cache", "python", "huggingface"] {
        fs::create_dir_all(store.docling.home.join(directory)).unwrap();
        fs::write(store.docling.home.join(directory).join("fixture"), "component").unwrap();
    }
    store.docling.uninstall().unwrap();
    assert!(!store.docling.home.exists());
    assert!(!store.docling.status().installed);
    assert!(!store.docling.status().running);
    assert_eq!(store.docling.status().stage, "uninstalled");
    assert_eq!(fs::read_to_string(&source).unwrap(), "keptconversion evidence");
    assert_eq!(fs::read(&output).unwrap(), converted);
    assert_eq!(store.files().unwrap().len(), indexed.len());
    assert!(!store.search_files("keptconversion", &[], 5).unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn uninstall_rejects_installing_and_active_conversion_without_removing_components() {
    let (_temp, store, _root) = fixture();
    store.docling.status.lock().unwrap().running = true;
    assert!(store.docling.uninstall().unwrap_err().to_string().contains("正在安装"));
    assert!(store.docling.installed());
    store.docling.status.lock().unwrap().running = false;
    let active = store.docling.usage.read().unwrap();
    assert!(store.docling.uninstall().unwrap_err().to_string().contains("正在使用"));
    assert!(store.docling.installed());
    drop(active);
    store.docling.uninstall().unwrap();
    assert!(!store.docling.installed());
}

#[cfg(unix)]
#[test]
fn text_is_searchable_before_parallel_conversion_and_converted_indexing_preserves_it() {
    use std::{sync::Arc,time::{Duration,Instant}};
    for workers in [4,2] {
        let (_temp, store, root) = fixture();
        fs::write(Path::new(&root).join("quick.md"), "nativeevidence").unwrap();
        for i in 0..6 { fs::write(Path::new(&root).join(format!("report-{i}.pages")), "convertedevidence").unwrap(); }
        let command = store.docling.home.join("bin/docling");
        let script = fs::read_to_string(&command).unwrap().replace("cat \"$source\" > \"$output\"", r#"touch "$(dirname "$0")/started-$(basename "$source")"
    while [ ! -f "$(dirname "$0")/release" ]; do sleep 0.01; done
    cat "$source" > "$output""#);
        fs::write(&command, script).unwrap();
        let mut settings = store.settings().unwrap();
        assert_eq!(settings.conversion_workers, 4);
        settings.convert_other_formats = true;
        settings.conversion_workers = workers;
        store.save_settings(&settings).unwrap();
        let store = Arc::new(store);
        let worker = store.clone();
        let worker_root = root.clone();
        let thread = std::thread::spawn(move || worker.scan(&worker_root, |_| {}));
        let bin = store.docling.home.join("bin");
        let started = || fs::read_dir(&bin).unwrap().filter(|entry| entry.as_ref().unwrap().file_name().to_string_lossy().starts_with("started-")).count();
        let deadline = Instant::now()+Duration::from_secs(5);
        while started()<workers && Instant::now()<deadline { std::thread::sleep(Duration::from_millis(10)); }
        std::thread::sleep(Duration::from_millis(100));
        let concurrent = started();
        let searchable = store.search_files("nativeevidence", &[], 10).unwrap().len();
        let status = store.indexing.status();
        store.indexing.set_paused(true);
        fs::write(bin.join("release"), "").unwrap();
        let deadline = Instant::now()+Duration::from_secs(5);
        while store.indexing.task(&root).completed<workers && Instant::now()<deadline { std::thread::sleep(Duration::from_millis(10)); }
        let paused = store.indexing.status();
        store.indexing.set_paused(false);
        let result = thread.join().unwrap().unwrap();
        assert_eq!(concurrent, workers);
        assert_eq!(searchable, 1);
        assert_eq!(status.tasks[0].stage, "converting");
        assert_eq!(status.tasks[0].files.len(), workers);
        assert_eq!(paused.tasks[0].completed, workers);
        assert!(paused.tasks[0].files.is_empty());
        assert_eq!(result, 7);
        assert_eq!(store.search_files("nativeevidence", &[], 10).unwrap().len(), 1);
        assert_eq!(store.search_files("convertedevidence", &[], 10).unwrap().len(), 6);
        let history = &store.index_history().unwrap()[0];
        assert_eq!((history.completed,history.total), (7,7));
        let log = store.index_history_log(history.id).unwrap().raw.unwrap();
        let stages = log.lines().map(|line| serde_json::from_str::<sag_protocol::IndexFileLog>(line).unwrap().stage).collect::<Vec<_>>();
        assert_eq!(stages[0], "indexing_native");
        assert_eq!(&stages[1..7], vec!["converting";6]);
        assert_eq!(&stages[7..], vec!["indexing_converted";6]);
        fs::write(Path::new(&root).join("report-0.pages"), "newconvertedevidence").unwrap();
        store.scan(&root, |_| {}).unwrap();
        assert_eq!(store.search_files("nativeevidence", &[], 10).unwrap().len(), 1);
        assert_eq!(store.files().unwrap().len(), 7);
    }
}

#[test]
fn conversion_parallelism_defaults_persists_and_rejects_invalid_values() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("index.sqlite");
    let store = Store::open(&database).unwrap();
    let mut settings = store.settings().unwrap();
    assert_eq!(settings.conversion_workers, 4);
    settings.conversion_workers = 2;
    store.save_settings(&settings).unwrap();
    for value in [0,17] { settings.conversion_workers=value;assert!(store.save_settings(&settings).is_err()); }
    drop(store);
    assert_eq!(Store::open(&database).unwrap().settings().unwrap().conversion_workers, 2);
}

#[cfg(unix)]
#[test]
fn native_conversion_checks_writes_reuses_readonly_output_and_requires_delete_permission() {
    use crate::store::FileAccessMode;
    use std::sync::atomic::{AtomicBool, Ordering};
    let writable = Arc::new(AtomicBool::new(false));
    let deletable = Arc::new(AtomicBool::new(false));
    let allow_write = writable.clone();
    let allow_delete = deletable.clone();
    let (_temp, store, root) = fixture_with_policy(Some(Arc::new(move |_, action| Ok(match action {
        FileAccessMode::Read | FileAccessMode::Publish => true,
        FileAccessMode::Write => allow_write.load(Ordering::SeqCst),
        FileAccessMode::Delete => allow_delete.load(Ordering::SeqCst),
    }))));
    let source = Path::new(&root).join("report.pages");
    fs::write(&source, "nativeconversionmarker").unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    let output = output_path(Path::new(&root), &source).unwrap();
    assert!(!output.exists());
    writable.store(true, Ordering::SeqCst);
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.search_files("nativeconversionmarker", &[], 10).unwrap().len(), 1);
    writable.store(false, Ordering::SeqCst);
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(fs::read_to_string(store.docling.home.join("bin/calls")).unwrap().lines().count(), 1);
    fs::remove_file(&source).unwrap();
    assert!(store.scan(&root, |_| {}).is_err());
    assert!(output.exists());
    deletable.store(true, Ordering::SeqCst);
    writable.store(true, Ordering::SeqCst);
    store.scan(&root, |_| {}).unwrap();
    assert!(!output.exists());
}

#[test]
fn docling_subprocess_does_not_inherit_the_native_api_token() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Docling::new(temp.path());
    let command = converter.command("docling");
    assert!(command.get_envs().any(|(name, value)| name == "TRIM_API_TOKEN" && value.is_none()));
}

#[cfg(unix)]
#[test]
fn heading_only_converted_documents_are_searchable() {
    let (_temp, store, root) = fixture();
    let source = Path::new(&root).join("heading.pages");
    fs::write(&source, "## convertedheadingmarker").unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    let result = store.search_files("convertedheadingmarker", &[], 10).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].content.contains("convertedheadingmarker"));
    assert!(result[0].path.ends_with("heading.pages.md"));
}
