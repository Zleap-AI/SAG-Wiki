use super::*;
use crate::store::Store;

#[test]
fn audio_video_catalog_is_restored_without_implicitly_installing_it() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Docling::new(temp.path());
    for extension in MEDIA_EXTENSIONS {
        assert!(
            is_source(Path::new(&format!("recording.{extension}"))),
            "{extension}"
        );
    }
    assert!(!converter.has_profile(DoclingProfile::Media));
    assert!(converter
        .require_formats(&Some(vec!["mp3".into()]))
        .is_err());
    assert!(!converter.home.exists());
}

#[cfg(unix)]
fn fake_media_uv(converter: &Docling) {
    fake_online_uv(converter);
    let uv = converter.home.join("uv");
    let script = fs::read_to_string(&uv).unwrap().replace("case \"$1\" in\n -m)", "case \"$*\" in\n *--download-media-models*)\n  [ \"$mode\" != models-fail ] || exit 2\n  echo started > \"$base/model-started\"\n  [ \"$mode\" != cancel ] || sleep 20\n  for last do :; done\n  mkdir -p \"$last/models/whisper-tiny-mlx\" \"$last/bin\"\n  echo model > \"$last/models/whisper-tiny-mlx/weights.npz\"\n  printf '#!/bin/sh\\nexit 0\\n' > \"$last/bin/ffmpeg\"\n  chmod +x \"$last/bin/ffmpeg\"\n  ;;\n *-m*)");
    fs::write(uv, script).unwrap();
}

#[cfg(unix)]
#[test]
fn media_install_is_explicit_isolated_and_does_not_download_document_models() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Docling::new(temp.path());
    fake_online_uv(&converter);
    converter.install().unwrap();
    let documents = fs::read(converter.home.join("active.json")).unwrap();
    let media = converter.profile_view(DoclingProfile::Media);
    fake_media_uv(&media);
    media.install().unwrap();
    assert!(converter.has_profile(DoclingProfile::Media));
    assert_eq!(
        fs::read(converter.home.join("active.json")).unwrap(),
        documents
    );
    let runtime = crate::docling_component::active_for(&media.home, DoclingProfile::Media).unwrap();
    assert!(runtime.root.join("bin/ffmpeg").is_file());
    let calls = fs::read_to_string(media.home.join("online-calls")).unwrap();
    assert!(calls.contains("--download-media-models"), "{calls}");
    assert!(
        !calls.contains("layout") && !calls.contains("tableformer") && !calls.contains("rapidocr"),
        "{calls}"
    );
    assert!(
        calls.contains(if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            "--no-build"
        } else {
            "--only-binary :all: --no-binary openai-whisper"
        }),
        "{calls}"
    );
    assert!(calls.contains("--probe"), "{calls}");
    let old = fs::read(media.home.join("active.json")).unwrap();
    fs::write(media.home.join("mode"), "probe-fail").unwrap();
    assert!(media.install().is_err());
    assert_eq!(fs::read(media.home.join("active.json")).unwrap(), old);
    assert_eq!(
        fs::read(converter.home.join("active.json")).unwrap(),
        documents
    );
}

#[cfg(unix)]
#[test]
fn media_conversion_routes_to_its_own_runtime_and_cancel_keeps_both_profiles() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Arc::new(Docling::new(temp.path()));
    fake_online_uv(&converter);
    converter.install().unwrap();
    let media = Arc::new(converter.profile_view(DoclingProfile::Media));
    fake_media_uv(&media);
    media.install().unwrap();
    let doc_calls = fs::read(converter.home.join("online-calls")).unwrap();
    let source = temp.path().join("recording.mp3");
    fs::write(&source, "fixture").unwrap();
    let output = temp.path().join(".sag-converted/recording.mp3.md");
    converter.convert(&source, &output).unwrap();
    assert!(fs::read_to_string(output).unwrap().contains("SAG probe"));
    assert_eq!(
        fs::read(converter.home.join("online-calls")).unwrap(),
        doc_calls
    );
    let media_calls = fs::read_to_string(media.home.join("online-calls")).unwrap();
    assert!(media_calls.contains("recording.mp3"), "{media_calls}");
    let docs_active = fs::read(converter.home.join("active.json")).unwrap();
    let media_active = fs::read(media.home.join("active.json")).unwrap();
    fs::write(media.home.join("mode"), "cancel").unwrap();
    fs::remove_file(media.home.join("model-started")).unwrap();
    let install = media.clone();
    let thread = std::thread::spawn(move || install.install());
    let started = std::time::Instant::now();
    while !media.home.join("model-started").exists() {
        assert!(started.elapsed() < Duration::from_secs(60));
        std::thread::sleep(Duration::from_millis(10));
    }
    converter.cancel();
    assert!(thread.join().unwrap().is_err());
    assert_eq!(
        fs::read(converter.home.join("active.json")).unwrap(),
        docs_active
    );
    assert_eq!(
        fs::read(media.home.join("active.json")).unwrap(),
        media_active
    );
    assert!(converter.has_profile(DoclingProfile::Documents));
    assert!(converter.has_profile(DoclingProfile::Media));
}

#[cfg(unix)]
#[test]
fn explicit_documents_install_does_not_authorize_a_media_download() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Arc::new(Docling::new(temp.path()));
    fake_online_uv(&converter);
    converter.start().unwrap();
    let start = std::time::Instant::now();
    while converter.status().running {
        assert!(start.elapsed() < Duration::from_secs(60));
        std::thread::sleep(Duration::from_millis(10));
    }
    let status = converter.status();
    assert!(status.installed, "{:?}", status.error);
    assert_eq!(status.requested_profiles, vec![DoclingProfile::Documents]);
    assert!(status.profiles.iter().any(|p| p.id == DoclingProfile::Media
        && !p.installed
        && p.extensions.contains(&"mp4".into())));
    assert!(!converter.home.join("media").exists());
}

#[test]
#[ignore = "explicit real media installation, writes only SAG_TEST_DOCLING_DATA"]
fn real_media_installation_passes_the_offline_audio_video_probe() {
    let data = PathBuf::from(
        std::env::var_os("SAG_TEST_DOCLING_DATA")
            .expect("set an isolated temporary QA data directory"),
    );
    assert!(data.starts_with("/tmp") || data.starts_with("/private/tmp"));
    let converter = Arc::new(Docling::new(&data));
    assert!(
        !converter.has_profile(DoclingProfile::Media),
        "use a fresh QA home"
    );
    converter
        .start_profiles(vec![DoclingProfile::Media])
        .unwrap();
    let start = std::time::Instant::now();
    while converter.status().running {
        assert!(start.elapsed() < Duration::from_secs(2700));
        std::thread::sleep(Duration::from_millis(100));
    }
    let status = converter.status();
    assert!(
        converter.has_profile(DoclingProfile::Media),
        "{:?}",
        status.error
    );
    assert!(!converter.has_profile(DoclingProfile::Documents));
    let runtime =
        crate::docling_component::active_for(&converter.home.join("media"), DoclingProfile::Media)
            .unwrap();
    assert!(runtime
        .root
        .join(if cfg!(windows) {
            "bin/ffmpeg.exe"
        } else {
            "bin/ffmpeg"
        })
        .is_file());
}

#[test]
fn legacy_ffmpeg_is_not_proof_that_media_models_are_offline_ready() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Docling::new(temp.path());
    fs::create_dir_all(converter.home.join("bin")).unwrap();
    fs::write(converter.home.join("ready"), VERSION).unwrap();
    fs::write(converter.binary("docling"), "legacy").unwrap();
    fs::write(converter.binary("ffmpeg"), "legacy").unwrap();
    assert!(converter.installed());
    assert!(!converter.has_profile(DoclingProfile::Media));
    assert!(converter
        .require_formats(&Some(vec!["mp4".into()]))
        .is_err());
    assert!(converter.require_formats(&None).is_ok());
}

#[cfg(unix)]
fn fake_online_uv(converter: &Docling) {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(&converter.home).unwrap();
    let script = r#"#!/bin/sh
set -eu
base=$(dirname "$0")
echo "$*" >> "$base/online-calls"
case "$1" in
 --version) echo 'uv 0.12.1';;
 python)
  [ "$2" = install ]
  python="$UV_PYTHON_INSTALL_DIR/cpython-3.12.13-fixture/bin/python3.12"
  mkdir -p "$(dirname "$python")"
  cat > "$python" <<'PY'
#!/bin/sh
set -eu
base=$(dirname "$0")
while [ ! -f "$base/uv" ]; do base=$(dirname "$base"); done
echo "python $*" >> "$base/online-calls"
mode=$(cat "$base/mode" 2>/dev/null || true)
case "$1" in
 -m)
  attempt=$(cat "$base/model-attempts" 2>/dev/null || echo 0)
  attempt=$((attempt+1)); echo "$attempt" > "$base/model-attempts"
  echo started > "$base/model-started"
  [ "$mode" != models-fail ] || { echo model-download-failed >&2; exit 2; }
  [ "$mode" != models-transient ] || [ "$attempt" -ge 3 ] || { echo transient-network-error >&2; exit 2; }
  [ "$mode" != models-memory ] || { echo model-memory-limit >&2; exit 75; }
  [ "$mode" != cancel ] || sleep 20
  for last do :; done
  mkdir -p "$last/RapidOcr"
  echo model > "$last/RapidOcr/model.onnx"
  ;;
 *)
  [ "$HF_HUB_OFFLINE" = 1 ]
  [ "$mode" != probe-fail ] || { echo offline-probe-failed >&2; exit 3; }
  while [ "$#" -gt 0 ]; do
   if [ "$1" = --output-file ]; then shift; echo 'SAG probe' > "$1"; fi
   shift
  done
  ;;
esac
PY
  chmod +x "$python"
  ;;
 pip)
  mode=$(cat "$base/mode" 2>/dev/null || true)
  [ "$mode" != dependencies-fail ] || { echo wheel-unavailable >&2; exit 2; }
  while [ "$#" -gt 0 ]; do
   if [ "$1" = --target ]; then shift; mkdir -p "$1"; echo package > "$1/package.py"; fi
   shift
  done
  ;;
 *) exit 77;;
esac
"#;
    fs::write(converter.home.join("uv"), script).unwrap();
    fs::set_permissions(converter.home.join("uv"), fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn a_missing_bundle_installs_online_only_after_the_user_requests_it() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Docling::new(temp.path());
    assert!(!converter.status().installed);
    assert!(
        !converter.home.exists(),
        "status must not bootstrap or download"
    );
    fake_online_uv(&converter);
    converter.install().unwrap();
    assert!(converter.installed());
    let runtime = crate::docling_component::active(&converter.home).unwrap();
    assert!(runtime
        .python
        .starts_with(converter.home.join("components")));
    assert!(runtime.models.join("RapidOcr/model.onnx").is_file());
    let calls = fs::read_to_string(converter.home.join("online-calls")).unwrap();
    assert!(calls.contains("python install 3.12.13"), "{calls}");
    assert!(
        calls.contains("--no-bin") && calls.contains("--no-registry"),
        "{calls}"
    );
    assert!(calls.contains("--no-build"), "{calls}");
    assert!(
        calls.contains("models download layout tableformer rapidocr"),
        "{calls}"
    );
    assert!(
        calls.contains("--rapidocr-backend-lang onnxruntime:ch"),
        "{calls}"
    );
    assert!(!calls.contains("torch:ch"), "{calls}");
    assert!(calls.contains("--probe"), "{calls}");
    assert!(
        !calls.contains("audio") && !calls.contains("video") && !calls.contains("ffmpeg"),
        "{calls}"
    );
}

#[cfg(unix)]
#[test]
fn online_install_failure_and_cancel_preserve_the_previous_runtime() {
    use std::time::Instant;
    let temp = tempfile::tempdir().unwrap();
    let converter = Arc::new(Docling::new(temp.path()));
    fake_online_uv(&converter);
    converter.install().unwrap();
    let previous = fs::read(converter.home.join("active.json")).unwrap();
    let python = crate::docling_component::active(&converter.home)
        .unwrap()
        .python;
    fs::write(converter.home.join("ready"), VERSION).unwrap();
    for mode in ["dependencies-fail", "models-fail", "probe-fail"] {
        fs::write(converter.home.join("mode"), mode).unwrap();
        assert!(converter.install().is_err(), "{mode}");
        assert_eq!(
            fs::read(converter.home.join("active.json")).unwrap(),
            previous
        );
        assert_eq!(
            fs::read_to_string(converter.home.join("ready")).unwrap(),
            VERSION
        );
        assert!(python.is_file());
        assert_eq!(
            fs::read_dir(converter.home.join("components"))
                .unwrap()
                .count(),
            1
        );
    }
    fs::write(converter.home.join("mode"), "cancel").unwrap();
    fs::write(converter.home.join("model-attempts"), "0").unwrap();
    fs::remove_file(converter.home.join("model-started")).unwrap();
    let copy = converter.clone();
    let task = std::thread::spawn(move || copy.install());
    let deadline = Instant::now() + Duration::from_secs(3);
    while !converter.home.join("model-started").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    converter.cancel();
    assert!(task.join().unwrap().is_err());
    assert_eq!(
        fs::read(converter.home.join("active.json")).unwrap(),
        previous
    );
    assert_eq!(
        fs::read_dir(converter.home.join("components"))
            .unwrap()
            .count(),
        1
    );
    assert!(python.is_file());
    assert!(converter.installed());
    assert_eq!(
        fs::read_to_string(converter.home.join("model-attempts"))
            .unwrap()
            .trim(),
        "1"
    );
}

#[cfg(unix)]
#[test]
fn online_model_download_retries_only_ordinary_exits_at_most_three_times() {
    for (mode, succeeds, attempts) in [
        ("models-transient", true, "3"),
        ("models-fail", false, "3"),
        ("models-memory", false, "1"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let converter = Docling::new(temp.path());
        fake_online_uv(&converter);
        fs::write(converter.home.join("mode"), mode).unwrap();
        let result = converter.install();
        assert_eq!(result.is_ok(), succeeds, "{mode}: {result:?}");
        assert_eq!(
            fs::read_to_string(converter.home.join("model-attempts"))
                .unwrap()
                .trim(),
            attempts,
            "{mode}"
        );
        assert_eq!(converter.installed(), succeeds, "{mode}");
        if !succeeds {
            assert_eq!(
                fs::read_dir(converter.home.join("components"))
                    .unwrap()
                    .count(),
                0
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn online_model_retries_share_one_deadline_and_stop_on_timeout() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Docling::new(temp.path());
    fs::create_dir_all(&converter.home).unwrap();
    let mut command = Command::new("sh");
    command
        .current_dir(&converter.home)
        .args(["-c", "echo attempt >> attempts; sleep .1; exit 2"]);
    let error = converter
        .run_model_download(&mut command, Duration::from_millis(250))
        .unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<crate::process_supervisor::Failure>()
            .unwrap()
            .kind,
        crate::process_supervisor::FailureKind::Timeout,
    );
    let attempts = fs::read_to_string(converter.home.join("attempts"))
        .unwrap()
        .lines()
        .count();
    assert!((1..=3).contains(&attempts));
}

#[cfg(unix)]
#[test]
fn failed_online_bootstrap_download_preserves_an_existing_runtime() {
    use std::net::TcpListener;
    let temp = tempfile::tempdir().unwrap();
    let converter = Docling::new(temp.path());
    fake_online_uv(&converter);
    converter.install().unwrap();
    let previous = fs::read(converter.home.join("active.json")).unwrap();
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/uv.tar.gz", server.local_addr().unwrap());
    let task = std::thread::spawn(move || {
        let (mut stream, _) = server.accept().unwrap();
        let mut request = [0; 2048];
        let _ = stream.read(&mut request);
        stream
            .write_all(
                b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
    });
    assert!(converter.download_archive(&url).is_err());
    task.join().unwrap();
    assert_eq!(
        fs::read(converter.home.join("active.json")).unwrap(),
        previous
    );
    assert!(converter.installed());
}

#[test]
#[ignore = "downloads pinned UV/Python/document wheels/models into explicit isolated test data"]
fn real_online_installation_passes_the_offline_ocr_probe() {
    let data = PathBuf::from(
        std::env::var_os("SAG_TEST_ONLINE_DATA_ROOT")
            .expect("set explicit isolated data directory"),
    );
    assert!(data.starts_with("/tmp") || data.starts_with("/private/tmp"));
    assert!(
        !data.join("docling").exists(),
        "use a clean test data directory"
    );
    let converter = Docling::new(&data);
    let result = converter.install();
    eprintln!(
        "online installation diagnostics: {}",
        converter.home.join("install-diagnostics.log").display()
    );
    assert!(result.is_ok(), "{result:#?}");
    assert!(converter.installed());
    assert!(crate::docling_component::active(&converter.home).is_ok());
}

#[test]
#[ignore = "validates an explicitly prepared real UV-managed Python tree without model downloads"]
fn real_managed_python_links_can_be_manifested_without_weakening_boundaries() {
    let root = PathBuf::from(
        std::env::var_os("SAG_TEST_MANAGED_PYTHON_ROOT")
            .expect("set explicit isolated UV Python root"),
    );
    assert!(root.starts_with("/tmp") || root.starts_with("/private/tmp"));
    let python = walkdir::WalkDir::new(root.join("python"))
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .find(|entry| entry.file_name() == "python3.12" && entry.file_type().is_file())
        .unwrap()
        .into_path();
    fs::create_dir_all(root.join("site-packages")).unwrap();
    fs::create_dir_all(root.join("models")).unwrap();
    crate::docling_component::write_manifest(&root, &python, &|| false).unwrap();
    crate::docling_component::verify(&root, &|| false).unwrap();
}

#[test]
fn other_formats_are_opt_in_and_never_reconvert_native_or_generated_text() {
    assert!(!crate::types::Settings::default().convert_other_formats);
    for name in [
        "report.PDF",
        "notes.docx",
        "slides.pptx",
        "book.epub",
        "data.xlsx",
        "letter.eml",
        "archive.tar.gz",
    ] {
        assert!(is_source(Path::new(name)), "{name}");
    }
    for name in [
        "notes.md",
        "code.rs",
        "page.html",
        ".sag-converted/report.pdf",
        "image.exe",
    ] {
        assert!(!is_source(Path::new(name)), "{name}");
    }
    assert_eq!(
        output_path(Path::new("/library"), Path::new("/library/a/report.pdf")).unwrap(),
        Path::new("/library/.sag-converted/a/report.pdf.md")
    );
}

#[test]
fn pem_keys_are_skipped_without_excluding_keynote_documents() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    fs::create_dir(&library).unwrap();
    let key = library.join("_.example.com.key");
    fs::write(&key, "-----BEGIN PRIVATE KEY-----\nfixture, not a key\n").unwrap();
    assert!(is_pem_key(&key).unwrap());
    let keynote = library.join("presentation.KEY");
    fs::write(&keynote, b"PK\x03\x04fixture").unwrap();
    assert!(!is_pem_key(&keynote).unwrap());
    assert!(is_source(&keynote));
    fs::remove_file(keynote).unwrap();
    let store = Store::open(&temp.path().join("data/index.sqlite")).unwrap();
    let root = store.add_root(&library).unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.roots().unwrap()[0].status, "ready");
    assert!(store.files().unwrap().is_empty());
    assert!(store.index_history().unwrap()[0].error.is_none());
}

#[cfg(unix)]
#[test]
fn pdf_without_markdown_retries_with_pdfium_and_preserves_both_errors_on_failure() {
    use std::os::unix::fs::PermissionsExt;
    let (_temp, store, root) = fixture();
    let binary = store.docling.binary("docling");
    fs::write(
        &binary,
        r#"#!/bin/sh
set -eu
source="$1"
shift
backend=''
output=''
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output-file) shift; output="$1";;
    --pdf-backend) shift; backend="$1";;
  esac
  shift
done
echo "${backend:-default}" >> "$(dirname "$0")/calls"
if [ "$backend" = pypdfium2 ]; then
  if [ "$(cat "$source")" = FAIL ]; then echo 'invalid PDF' >&2; exit 1; fi
  echo recoveredpdf > "$output"
fi
"#,
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let source = Path::new(&root).join("report.pdf");
    let output = output_path(Path::new(&root), &source).unwrap();
    fs::write(&source, "PDF").unwrap();
    store.docling.convert(&source, &output).unwrap();
    assert!(fs::read_to_string(&output)
        .unwrap()
        .contains("recoveredpdf"));
    assert_eq!(
        fs::read_to_string(store.docling.home.join("bin/calls")).unwrap(),
        "default\npypdfium2\n"
    );
    fs::remove_file(&output).unwrap();
    fs::write(&source, "FAIL").unwrap();
    let error = store
        .docling
        .convert(&source, &output)
        .unwrap_err()
        .to_string();
    assert!(error.contains("Docling 未生成 Markdown"));
    assert!(error.contains("invalid PDF"));
    assert!(!output.exists());
}

#[test]
fn completed_conversions_are_searchable_before_the_conversion_phase_finishes() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    fs::create_dir(&library).unwrap();
    let store = Store::open(&temp.path().join("data/index.sqlite")).unwrap();
    let root = store.add_root(&library).unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    store.save_settings(&settings).unwrap();
    let mut records = std::collections::BTreeMap::new();
    for name in ["first.pdf", "second.pdf"] {
        let source = Path::new(&root).join(name);
        fs::write(&source, "source").unwrap();
        let output = output_path(Path::new(&root), &source).unwrap();
        fs::create_dir_all(output.parent().unwrap()).unwrap();
        fs::write(&output, "streamingevidence").unwrap();
        records.insert(
            name,
            (fingerprint(&source).unwrap(), fingerprint(&output).unwrap()),
        );
    }
    fs::write(
        Path::new(&root).join(".sag-converted/.sag-converted.json"),
        serde_json::to_vec(&records).unwrap(),
    )
    .unwrap();
    let observed = Mutex::new(Vec::new());
    store
        .scan(&root, |progress| {
            if store.indexing.task(&root).stage == "converting" && progress.count > 0 {
                assert_eq!(store.files().unwrap().len(), progress.count);
                assert_eq!(
                    store
                        .search_files("streamingevidence", &[], 5)
                        .unwrap()
                        .len(),
                    progress.count
                );
                observed.lock().unwrap().push(progress.count);
            }
        })
        .unwrap();
    assert_eq!(*observed.lock().unwrap(), vec![1, 2]);
    assert_eq!(store.index_history().unwrap()[0].completed, 2);
}

#[cfg(unix)]
fn fixture() -> (tempfile::TempDir, Store, String) {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    fs::create_dir(&root).unwrap();
    let store = Store::open(&temp.path().join("data/index.sqlite")).unwrap();
    let bin = store.docling.home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let command = bin.join("docling");
    fs::write(
        &command,
        r#"#!/bin/sh
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
"#,
    )
    .unwrap();
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
    fs::write(library.join("photo.png"), "not really an image").unwrap();
    assert!(!store.docling.installed());
    assert!(store.docling.require_formats(&Some(vec!["csv".into(), "xlsx".into()])).is_ok());
    assert!(store.docling.require_formats(&Some(vec!["png".into()])).is_err());
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.search_files("BCD-215", &[], 5).unwrap().len(), 1);
    let state = &store.roots().unwrap()[0];
    assert_eq!(state.status, "ready", "{:?}", state.error);
    assert!(!library.join(OUTPUT_DIRECTORY).join("photo.png.md").exists());
    let status = store.docling.status();
    assert!(status.builtin_formats.iter().any(|(_, ext)| ext.iter().any(|e| e == "pdf")));
    assert!(status.formats.iter().all(|(_, ext)| ext.iter().all(|e| !crate::anydoc::handles_extension(e))));
    assert!(status.formats.iter().any(|(_, ext)| ext.iter().any(|e| e == "png")));
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
    assert!(fs::read_to_string(&output)
        .unwrap()
        .contains("../../nested/report.pages"));
    assert_eq!(fs::read_to_string(&source).unwrap(), "evidenceone");
    let history_id = store.index_history().unwrap()[0].id;
    let raw = store.index_history_log(history_id).unwrap().raw.unwrap();
    let recorded = raw
        .lines()
        .map(|line| serde_json::from_str::<sag_protocol::IndexFileLog>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(recorded
        .iter()
        .any(|file| file.path == source.to_str().unwrap()));
    assert!(recorded
        .iter()
        .all(|file| !file.path.contains(OUTPUT_DIRECTORY)));
    assert_eq!(
        store
            .authorize_index_history_file(history_id, source.to_str().unwrap())
            .unwrap(),
        source
    );
    assert!(!store
        .entries(root_path, false)
        .unwrap()
        .iter()
        .any(|entry| entry.name == OUTPUT_DIRECTORY));
    assert!(store
        .entries(root_path, true)
        .unwrap()
        .iter()
        .any(|entry| entry.name == OUTPUT_DIRECTORY));
    assert_eq!(
        store.search_files("evidenceone", &[], 5).unwrap()[0].path,
        output.to_str().unwrap()
    );
    assert!(store.search_files("invisible", &[], 5).unwrap().is_empty());
    let scope = vec![root_path.join("nested").to_string_lossy().into_owned()];
    assert_eq!(
        store.search_files("evidenceone", &scope, 5).unwrap().len(),
        1
    );
    assert_eq!(
        store
            .candidate_query("evidenceone", None, &scope, &[], &[], 5)
            .unwrap()
            .len(),
        1
    );
    assert!(root_path
        .join(".sag-converted/nested/report.numbers.md")
        .is_file());
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
    assert_eq!(
        store.search_files("updatedevidence", &[], 5).unwrap().len(),
        1
    );
    assert!(store
        .search_files("evidenceone", &[], 5)
        .unwrap()
        .is_empty());
    store.remove_path(&root_path.join("nested")).unwrap();
    assert!(store
        .search_files("updatedevidence", &[], 5)
        .unwrap()
        .is_empty());
    assert!(source.exists() && output.exists());
    store.add_root(&root_path.join("nested")).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(
        store.search_files("updatedevidence", &[], 5).unwrap().len(),
        1
    );
    settings.convert_other_formats = false;
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert!(store
        .search_files("updatedevidence", &[], 5)
        .unwrap()
        .is_empty());
    assert!(source.exists() && output.exists());
}

#[cfg(unix)]
#[test]
fn failed_updates_never_serve_stale_evidence_and_do_not_overwrite_manual_edits() {
    let (_temp, store, root) = fixture();
    let root_path = Path::new(&root);
    let source = root_path.join("report.docx");
    fs::write(&source, "originalevidence").unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    let output = output_path(root_path, &source).unwrap();
    fs::write(&source, "FAIL").unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert!(store
        .search_files("originalevidence", &[], 5)
        .unwrap()
        .is_empty());
    assert!(store.roots().unwrap()[0]
        .error
        .as_ref()
        .unwrap()
        .contains("invalid PDF"));
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
    let source = root_path.join("report.docx");
    fs::write(&source, "evidence").unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    store.save_settings(&settings).unwrap();
    store.scan(&root, |_| {}).unwrap();
    let output = output_path(root_path, &source).unwrap();
    fs::write(root_path.join(".gitignore"), "*.docx\n").unwrap();
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
fn download_cancellation_stops_the_process_and_never_marks_installation_ready() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Arc::new(Docling::new(temp.path()));
    let child_converter = converter.clone();
    let task = std::thread::spawn(move || {
        child_converter.run(
            child_converter
                .command("sh")
                .args(["-c", "printf 'download started\\n'; sleep 30"]),
            true,
        )
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while converter.status().message.is_empty() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    converter.cancel();
    assert!(task
        .join()
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("已取消下载"));
    assert!(!converter.installed());
}

#[cfg(unix)]
#[test]
fn live_download_progress_is_visible_before_a_carriage_return_line_finishes() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Arc::new(Docling::new(temp.path()));
    let child_converter = converter.clone();
    let task = std::thread::spawn(move || {
        child_converter.run(
            child_converter
                .command("sh")
                .args(["-c", "printf 'torch 1.2 MiB/10.0 MiB\\r'; sleep 30"]),
            true,
        )
    });
    let deadline = std::time::Instant::now() + Duration::from_millis(600);
    let mut visible = false;
    while std::time::Instant::now() < deadline {
        if converter.status().message.contains("torch") {
            visible = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    converter.cancel();
    assert!(task.join().unwrap().is_err());
    assert!(
        visible,
        "下载进度被缓冲到进程结束，用户看不到正在下载的组件"
    );
    assert_eq!(converter.status().downloaded, (1.2 * 1048576.0) as u64);
    assert_eq!(converter.status().total, Some(10 * 1048576));
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
    assert_eq!(
        status.total,
        Some((19.5 * 1048576.0) as u64 + (121.4 * 1048576.0) as u64)
    );
    assert_eq!(
        status.message,
        "scipy: 512 KiB/19.5 MiB\ntorch: 2.0 MiB/121.4 MiB"
    );
    converter.update_download_line(
        b"model.safetensors:  42%|####      | 42M/100M [00:03<00:04, 12MB/s]",
    );
    assert_eq!(converter.status().percent, Some(42.0));
    assert_eq!(converter.status().total, None);
}

#[cfg(windows)]
#[test]
fn windows_component_command_starts_and_exits_without_waiting_for_cursor_report() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Arc::new(Docling::new(temp.path()));
    let copy = converter.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let task = std::thread::spawn(move || {
        tx.send(
            copy.run(
                copy.command("cmd.exe")
                    .args(["/d", "/c", "echo component-ready"]),
                true,
            ),
        )
        .unwrap();
    });
    let result = rx.recv_timeout(Duration::from_secs(5));
    if result.is_err() {
        converter.cancel();
    }
    let result = result.expect("Windows 组件命令卡在 ConPTY 光标位置握手");
    task.join().unwrap();
    result.unwrap();
    assert!(converter.status().message.contains("component-ready"));
}

#[cfg(windows)]
#[test]
fn windows_component_progress_is_live_and_cancellation_stops_the_process_tree() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Arc::new(Docling::new(temp.path()));
    let copy = converter.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let task = std::thread::spawn(move || {
        tx.send(copy.run(
            copy.command("cmd.exe").args([
                "/d",
                "/c",
                "echo torch 1.2 MiB/10.0 MiB & ping -n 30 127.0.0.1 >NUL",
            ]),
            true,
        ))
        .unwrap();
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut visible = false;
    while std::time::Instant::now() < deadline {
        if converter.status().message.contains("torch") {
            visible = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    converter.cancel();
    let result = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("取消后 Windows 安装进程没有退出");
    task.join().unwrap();
    assert!(visible, "Windows 安装进度未在进程运行时显示");
    assert!(result.unwrap_err().to_string().contains("已取消下载"));
    assert!(!converter.installed());
    assert_eq!(converter.status().downloaded, (1.2 * 1048576.0) as u64);
    assert_eq!(converter.status().total, Some(10 * 1048576));
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
        let mut buffer = [0; 4096];
        socket.read(&mut buffer).unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\nx")
            .unwrap();
        stopped.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let copy = converter.clone();
    let task =
        std::thread::spawn(move || copy.download_archive(&format!("http://{address}/archive")));
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while converter.status().downloaded == 0 {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let start = std::time::Instant::now();
    converter.cancel();
    let result = task.join().unwrap();
    stop.send(()).unwrap();
    server.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("已取消下载"));
    assert!(start.elapsed() < Duration::from_millis(500));
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
    let enabled = FolderIndexOptions {
        extensions: None,
        file_types: FileTypes {
            markdown: false,
            text: false,
            code: false,
            config: false,
        },
        convert_other_formats: true,
        conversion_formats: None,
    };
    let disabled = FolderIndexOptions {
        convert_other_formats: false,
        ..enabled.clone()
    };
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
    store
        .save_folder_index_options(&a, &disabled, true)
        .unwrap();
    store
        .update_paths(&root, &[a.join(".sagignore")], |_| {})
        .unwrap();
    assert_eq!(store.roots().unwrap()[0].status, "ready");
    assert_eq!(store.files().unwrap().len(), 1);
    assert!(a_output.exists() && b_output.exists());
    assert_eq!(fs::read(&b_output).unwrap(), b_before);
    store.save_folder_index_options(&a, &enabled, true).unwrap();
    store
        .update_paths(&root, &[a.join(".sagignore")], |_| {})
        .unwrap();
    assert_eq!(store.roots().unwrap()[0].status, "ready");
    assert_eq!(store.files().unwrap().len(), 2);
    assert_eq!(
        fs::read_to_string(store.docling.home.join("bin/calls"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert_eq!(
        fs::read_to_string(a.join("report.pages")).unwrap(),
        "alpha evidence"
    );
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
    fs::write(nested.join("book.eml"), "selectedaudio evidence").unwrap();
    let mut options = FolderIndexOptions {
        extensions: None,
        file_types: FileTypes::default(),
        convert_other_formats: true,
        conversion_formats: None,
    };
    crate::folder_index::write(root_path, &options).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.files().unwrap().len(), 3);
    let word_output = output_path(root_path, &root_path.join("notes.numbers")).unwrap();
    let word_before = fs::read(&word_output).unwrap();
    // A deselected source must not be converted again, even after it changes.
    fs::write(root_path.join("notes.numbers"), "FAIL").unwrap();
    options.conversion_formats = Some(vec!["pages".into()]);
    store
        .save_folder_index_options(root_path, &options, false)
        .unwrap();
    store
        .update_paths(&root, &[root_path.join(".sagignore")], |_| {})
        .unwrap();
    assert_eq!(store.roots().unwrap()[0].status, "ready");
    assert_eq!(
        store
            .folder_index_options(&nested)
            .unwrap()
            .conversion_formats,
        options.conversion_formats
    );
    assert_eq!(store.files().unwrap().len(), 1);
    assert!(store.files().unwrap()[0].path.ends_with("report.PAGES.md"));
    assert!(store
        .search_files("selectedword", &[], 5)
        .unwrap()
        .is_empty());
    assert_eq!(fs::read(&word_output).unwrap(), word_before);
    let calls = store.docling.home.join("bin/calls");
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 3);
    fs::write(nested.join("added.numbers"), "FAIL").unwrap();
    fs::write(nested.join("added.pages"), "newselectedpdf evidence").unwrap();
    store
        .update_paths(
            &root,
            &[nested.join("added.numbers"), nested.join("added.pages")],
            |_| {},
        )
        .unwrap();
    assert_eq!(store.roots().unwrap()[0].status, "ready");
    assert_eq!(store.files().unwrap().len(), 2);
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 4);
    // A child can select EPUB while keeping the parent's PDF selection unchanged.
    let child_options = FolderIndexOptions {
        conversion_formats: Some(vec!["eml".into()]),
        ..options.clone()
    };
    store
        .save_folder_index_options(&nested, &child_options, true)
        .unwrap();
    store
        .update_paths(&root, &[nested.join(".sagignore")], |_| {})
        .unwrap();
    let files = store.files().unwrap();
    assert_eq!(files.len(), 2);
    assert!(files
        .iter()
        .any(|file| file.path.ends_with("report.PAGES.md")));
    assert!(files.iter().any(|file| file.path.ends_with("book.eml.md")));
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 4);
    assert_eq!(
        fs::read_to_string(root_path.join("report.PAGES")).unwrap(),
        "selectedpdf evidence"
    );
    assert_eq!(
        crate::folder_index::read(root_path).unwrap().unwrap(),
        options
    );
    assert_eq!(
        crate::folder_index::read(&nested).unwrap().unwrap(),
        child_options
    );
}

#[test]
fn old_rules_keep_all_formats_and_format_choices_exclude_native_types() {
    let options: crate::types::FolderIndexOptions = serde_json::from_str(r#"{"file_types":{"markdown":true,"text":true,"code":false,"config":false},"convert_other_formats":true}"#).unwrap();
    assert_eq!(options.conversion_formats, None);
    assert!(is_selected(
        Path::new("report.PDF"),
        &options.conversion_formats
    ));
    assert!(is_selected(
        Path::new("archive.tar.gz"),
        &Some(vec!["tar.gz".into()])
    ));
    assert!(!is_selected(
        Path::new("report.docx"),
        &Some(vec!["pdf".into()])
    ));
    assert!(!is_selected(Path::new("report.pdf"), &Some(vec![])));
    for (_, extensions) in formats() {
        for extension in extensions {
            assert!(is_source(Path::new(&format!("file.{extension}"))));
        }
    }
}

#[cfg(unix)]
#[test]
fn uninstall_removes_only_components_and_keeps_sources_converted_files_and_indexes() {
    let (_temp, store, root) = fixture();
    let source = Path::new(&root).join("report.pdf");
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
        fs::write(
            store.docling.home.join(directory).join("fixture"),
            "component",
        )
        .unwrap();
    }
    store.docling.uninstall().unwrap();
    assert!(!store.docling.home.exists());
    assert!(!store.docling.status().installed);
    assert!(!store.docling.status().running);
    assert_eq!(store.docling.status().stage, "uninstalled");
    assert_eq!(
        fs::read_to_string(&source).unwrap(),
        "keptconversion evidence"
    );
    assert_eq!(fs::read(&output).unwrap(), converted);
    assert_eq!(store.files().unwrap().len(), indexed.len());
    assert!(!store
        .search_files("keptconversion", &[], 5)
        .unwrap()
        .is_empty());
}

#[cfg(unix)]
#[test]
fn uninstall_rejects_installing_and_active_conversion_without_removing_components() {
    let (_temp, store, _root) = fixture();
    store.docling.status.lock().unwrap().running = true;
    assert!(store
        .docling
        .uninstall()
        .unwrap_err()
        .to_string()
        .contains("正在安装"));
    assert!(store.docling.installed());
    store.docling.status.lock().unwrap().running = false;
    let active = store.docling.usage.read().unwrap();
    assert!(store
        .docling
        .uninstall()
        .unwrap_err()
        .to_string()
        .contains("正在使用"));
    assert!(store.docling.installed());
    drop(active);
    store.docling.uninstall().unwrap();
    assert!(!store.docling.installed());
}

#[cfg(unix)]
#[test]
fn text_is_searchable_before_parallel_conversion_and_converted_indexing_preserves_it() {
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    for workers in [4, 2] {
        let (_temp, store, root) = fixture();
        fs::write(Path::new(&root).join("quick.md"), "nativeevidence").unwrap();
        for i in 0..6 {
            fs::write(
                Path::new(&root).join(format!("report-{i}.pdf")),
                "convertedevidence",
            )
            .unwrap();
        }
        let command = store.docling.home.join("bin/docling");
        let script = fs::read_to_string(&command).unwrap().replace(
            "cat \"$source\" > \"$output\"",
            r#"touch "$(dirname "$0")/started-$(basename "$source")"
    while [ ! -f "$(dirname "$0")/release" ]; do sleep 0.01; done
    cat "$source" > "$output""#,
        );
        fs::write(&command, script).unwrap();
        let mut settings = store.settings().unwrap();
        assert_eq!(settings.conversion_workers, 1);
        settings.convert_other_formats = true;
        settings.conversion_workers = workers;
        store.save_settings(&settings).unwrap();
        let store = Arc::new(store);
        let worker = store.clone();
        let worker_root = root.clone();
        let thread = std::thread::spawn(move || worker.scan(&worker_root, |_| {}));
        let bin = store.docling.home.join("bin");
        let started = || {
            fs::read_dir(&bin)
                .unwrap()
                .filter(|entry| {
                    entry
                        .as_ref()
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with("started-")
                })
                .count()
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        let admitted = workers.min(crate::docling_budget::parallelism());
        while started() < admitted && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(100));
        let concurrent = started();
        let searchable = store.search_files("nativeevidence", &[], 10).unwrap().len();
        let status = store.indexing.status();
        store.indexing.set_paused(true);
        fs::write(bin.join("release"), "").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !store.indexing.task(&root).files.is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let paused = store.indexing.status();
        store.indexing.set_paused(false);
        let result = thread.join().unwrap().unwrap();
        assert_eq!(concurrent, admitted);
        assert_eq!(searchable, 1);
        assert_eq!(status.tasks[0].stage, "converting");
        assert_eq!(status.tasks[0].files.len(), workers);
        assert!(paused.paused);
        assert!(paused.tasks[0].completed <= workers);
        assert!(paused.tasks[0].files.is_empty());
        assert_eq!(result, 7);
        assert_eq!(
            store.search_files("nativeevidence", &[], 10).unwrap().len(),
            1
        );
        assert_eq!(
            store
                .search_files("convertedevidence", &[], 10)
                .unwrap()
                .len(),
            6
        );
        let history = &store.index_history().unwrap()[0];
        assert_eq!((history.completed, history.total), (7, 7));
        let log = store.index_history_log(history.id).unwrap().raw.unwrap();
        let stages = log
            .lines()
            .map(|line| {
                serde_json::from_str::<sag_protocol::IndexFileLog>(line)
                    .unwrap()
                    .stage
            })
            .collect::<Vec<_>>();
        assert_eq!(stages[0], "indexing_native");
        assert_eq!(&stages[1..7], vec!["converting"; 6]);
        assert_eq!(&stages[7..], vec!["indexing_converted"; 6]);
        fs::write(
            Path::new(&root).join("report-0.pdf"),
            "newconvertedevidence",
        )
        .unwrap();
        store.scan(&root, |_| {}).unwrap();
        assert_eq!(
            store.search_files("nativeevidence", &[], 10).unwrap().len(),
            1
        );
        assert_eq!(store.files().unwrap().len(), 7);
    }
}

#[test]
fn conversion_parallelism_defaults_persists_and_rejects_invalid_values() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("index.sqlite");
    let store = Store::open(&database).unwrap();
    let mut settings = store.settings().unwrap();
    assert_eq!(settings.conversion_workers, 1);
    settings.conversion_workers = 2;
    store.save_settings(&settings).unwrap();
    for value in [0, 17] {
        settings.conversion_workers = value;
        assert!(store.save_settings(&settings).is_err());
    }
    drop(store);
    assert_eq!(
        Store::open(&database)
            .unwrap()
            .settings()
            .unwrap()
            .conversion_workers,
        2
    );
}

#[test]
fn readiness_rejects_a_marker_from_a_different_component_version() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Docling::new(temp.path());
    fs::create_dir_all(converter.home.join("bin")).unwrap();
    fs::write(converter.binary("docling"), "fixture").unwrap();
    fs::write(converter.home.join("ready"), "different-version").unwrap();
    assert!(
        !converter.installed(),
        "a file named ready cannot validate a different runtime"
    );
}

#[cfg(unix)]
#[test]
fn legacy_ready_fifo_returns_uninstalled_without_blocking() {
    use std::{os::unix::ffi::OsStrExt, process::Stdio, time::Instant};
    if let Some(data) = std::env::var_os("SAG_TEST_LEGACY_READY_FIFO_DATA") {
        let converter = Docling::new(Path::new(&data));
        assert!(!converter.installed());
        assert!(!converter.status().installed);
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let converter = Docling::new(temp.path());
    fs::create_dir_all(converter.home.join("bin")).unwrap();
    fs::write(converter.binary("docling"), "fixture").unwrap();
    let ready = converter.home.join("ready");
    let name = std::ffi::CString::new(ready.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "docling::tests::legacy_ready_fifo_returns_uninstalled_without_blocking",
            "--nocapture",
        ])
        .env("SAG_TEST_LEGACY_READY_FIFO_DATA", temp.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let completed = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if completed.is_none() {
        child.kill().unwrap();
        child.wait().unwrap();
    }
    assert!(
        completed.is_some_and(|status| status.success()),
        "legacy ready FIFO blocked installed/status or was accepted"
    );
}

#[cfg(unix)]
#[test]
fn conversion_cancellation_is_observed_by_the_running_child() {
    use std::os::unix::fs::PermissionsExt;
    let (_temp, store, root) = fixture();
    let binary = store.docling.binary("docling");
    fs::write(
        &binary,
        r#"#!/bin/sh
echo started > "$(dirname "$0")/started"
sleep 20
"#,
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let source = Path::new(&root).join("cancel.pdf");
    fs::write(&source, "source").unwrap();
    let output = output_path(Path::new(&root), &source).unwrap();
    let result = store
        .docling
        .convert_cancellable(&source, &output, || {
            store.docling.home.join("bin/started").exists()
        })
        .unwrap_err();
    assert_eq!(
        result
            .downcast_ref::<crate::process_supervisor::Failure>()
            .unwrap()
            .kind,
        crate::process_supervisor::FailureKind::Cancelled
    );
    assert!(!output.exists());
}

#[cfg(unix)]
#[test]
fn resource_failure_never_retries_pdf_or_publishes_partial_output() {
    use std::os::unix::fs::PermissionsExt;
    let (_temp, store, root) = fixture();
    let binary = store.docling.binary("docling");
    fs::write(
        &binary,
        r#"#!/bin/sh
set -eu
echo call >> "$(dirname "$0")/resource-calls"
shift
while [ "$#" -gt 0 ]; do
 if [ "$1" = '--output-file' ]; then shift; echo partial > "$1"; fi
 shift
done
exit 75
"#,
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let source = Path::new(&root).join("resource.pdf");
    fs::write(&source, "source").unwrap();
    let output = output_path(Path::new(&root), &source).unwrap();
    fs::create_dir_all(output.parent().unwrap()).unwrap();
    fs::write(&output, "previous usable output").unwrap();
    assert!(store.docling.convert(&source, &output).is_err());
    assert_eq!(
        fs::read_to_string(store.docling.home.join("bin/resource-calls"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(
        fs::read_to_string(output).unwrap(),
        "previous usable output"
    );
}

#[test]
#[ignore = "requires the explicit, locally built document component"]
fn real_prebuilt_component_passes_offline_installation_probe() {
    let temp = tempfile::tempdir().unwrap();
    let converter = Docling::new(temp.path());
    converter.install().unwrap();
    assert!(converter.installed());
    let runtime = crate::docling_component::active(&converter.home).unwrap();
    let previous = fs::read(converter.home.join("active.json")).unwrap();
    let ocr = runtime.models.join("RapidOcr");
    let missing = runtime.models.join("RapidOcr.saved");
    fs::rename(&ocr, &missing).unwrap();
    let result = converter.probe_component(&runtime, &|| false);
    fs::rename(&missing, &ocr).unwrap();
    assert!(
        result.is_err(),
        "offline readiness accepted a runtime without OCR models"
    );
    assert_eq!(
        fs::read(converter.home.join("active.json")).unwrap(),
        previous
    );
}

#[cfg(unix)]
#[test]
fn unclassified_and_filesystem_failures_do_not_retry_pdf() {
    use std::os::unix::fs::PermissionsExt;
    for command in ["exit 1", "mkdir \"$output\""] {
        let (_temp, store, root) = fixture();
        let binary = store.docling.binary("docling");
        let script=format!("#!/bin/sh\nset -eu\necho call >> \"$(dirname \"$0\")/calls-failure\"\nshift\noutput=''\nwhile [ \"$#\" -gt 0 ]; do if [ \"$1\" = '--output-file' ]; then shift; output=\"$1\"; fi; shift; done\n{command}\n");
        fs::write(&binary, script).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let source = Path::new(&root).join("bad.pdf");
        fs::write(&source, "source").unwrap();
        let output = output_path(Path::new(&root), &source).unwrap();
        assert!(store.docling.convert(&source, &output).is_err());
        assert_eq!(
            fs::read_to_string(store.docling.home.join("bin/calls-failure"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        assert!(!output.exists());
    }
}

#[cfg(unix)]
#[test]
fn cancelling_component_install_does_not_cancel_a_preserved_converter() {
    let (_temp, store, root) = fixture();
    let source = Path::new(&root).join("preserved.pdf");
    fs::write(&source, "preserved conversion").unwrap();
    store.docling.cancel();
    let output = output_path(Path::new(&root), &source).unwrap();
    store.docling.convert(&source, &output).unwrap();
    assert!(fs::read_to_string(output)
        .unwrap()
        .contains("preserved conversion"));
}

#[test]
#[ignore = "requires the explicit, locally built document component"]
fn real_component_direct_probe_diagnostics() {
    let source = PathBuf::from(std::env::var_os("SAG_DOCLING_COMPONENT_DIR").unwrap());
    let runtime = crate::docling_component::runtime(&source).unwrap();
    let temp = tempfile::tempdir().unwrap().keep();
    let converter = Docling::new(&temp);
    let result = converter.probe_component(&runtime, &|| false);
    eprintln!(
        "probe diagnostics {}",
        converter.home.join("install-diagnostics.log").display()
    );
    assert!(result.is_ok(), "{result:#?}");
    fs::remove_dir_all(temp).unwrap();
}

#[test]
fn a_running_service_uses_its_registered_component_source() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("bundle");
    fs::create_dir(&source).unwrap();
    fs::write(
        source.join("manifest.json"),
        serde_json::to_vec(&crate::docling_component::Manifest {
            schema: 1,
            component_version: crate::docling_component::VERSION.into(),
            profile: crate::docling_component::PROFILE.into(),
            platform: crate::docling_component::platform(),
            python: "python".into(),
            site_packages: "site-packages".into(),
            models: "models".into(),
            files: vec![],
            supported_extensions: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    let converter = Docling::new(&temp.path().join("data"));
    converter.set_component_source(source).unwrap();
    assert!(converter
        .install()
        .unwrap_err()
        .to_string()
        .contains("没有可校验文件"));
}

#[cfg(unix)]
#[test]
fn installation_status_preserves_nested_probe_error_diagnostics() {
    use std::{os::unix::fs::PermissionsExt, time::Instant};
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("python/bin")).unwrap();
    fs::create_dir(source.join("site-packages")).unwrap();
    fs::create_dir(source.join("models")).unwrap();
    let script = b"#!/bin/sh\necho 'missing OCR model diagnostic' >&2\nexit 1\n";
    let python = "python/bin/python3.12";
    fs::write(source.join(python), script).unwrap();
    fs::set_permissions(source.join(python), fs::Permissions::from_mode(0o755)).unwrap();
    let manifest = crate::docling_component::Manifest {
        schema: 1,
        component_version: crate::docling_component::VERSION.into(),
        profile: crate::docling_component::PROFILE.into(),
        platform: crate::docling_component::platform(),
        python: python.into(),
        site_packages: "site-packages".into(),
        models: "models".into(),
        files: vec![crate::docling_component::File {
            path: python.into(),
            size: script.len() as u64,
            sha256: format!("{:x}", Sha256::digest(script)),
        }],
        supported_extensions: vec!["pdf".into()],
    };
    fs::write(
        source.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let converter = Arc::new(Docling::new(&temp.path().join("data")));
    converter.set_component_source(source).unwrap();
    converter.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        let status = converter.status();
        if !status.running {
            break status;
        }
        assert!(Instant::now() < deadline, "installation did not finish");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.stage, "error");
    let error = status.error.unwrap();
    assert!(error.contains("组件离线转换验收失败"), "{error}");
    assert!(error.contains("missing OCR model diagnostic"), "{error}");
    assert!(!status.installed);
}
