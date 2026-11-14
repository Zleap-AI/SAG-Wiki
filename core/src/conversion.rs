//! Ordered parsing and per-engine history; only complete output is published.
use crate::{
    anydoc,
    docling::Docling,
    process_supervisor::{Failure, FailureKind},
};
use anyhow::{bail, Context, Result};
use sag_protocol::types::Settings;
use std::{fs, path::Path, time::Instant};

mod pdf_segments;

pub struct Attempt {
    pub segment: Option<sag_protocol::ConversionSegment>,
    pub phase: Option<String>,
    pub engine: String,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug)]
struct EngineFailures {
    messages: Vec<String>,
    resource: bool,
    /// Every engine stopped because a required component is not installed, so
    /// the same source fails the same way until the component is added.
    component_missing: bool,
}
impl std::fmt::Display for EngineFailures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "所有适用解析器均失败：\n{}", self.messages.join("\n"))
    }
}
impl std::error::Error for EngineFailures {}
pub(crate) fn resource_failure(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<EngineFailures>()
        .is_some_and(|error| error.resource)
        || error
            .downcast_ref::<Failure>()
            .is_some_and(|error| error.kind == FailureKind::Memory)
}
/// True when nothing but installing a component can change the outcome.
pub(crate) fn component_missing(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<EngineFailures>()
        .is_some_and(|error| error.component_missing)
}
fn run_engines(
    engines: &[&str],
    cancel: &impl Fn() -> bool,
    mut convert: impl FnMut(&str) -> Result<()>,
    mut report: impl FnMut(Attempt),
) -> Result<String> {
    let mut failures = vec![];
    let mut resource = false;
    let mut component_missing = true;
    for engine in engines {
        if cancel() {
            bail!("已停止转换，未发布输出")
        }
        report(Attempt {
            segment: None,
            phase: Some("starting".into()),
            engine: (*engine).into(),
            elapsed_ms: 0,
            error: None,
        });
        let started = Instant::now();
        let result = convert(engine);
        let message = result.as_ref().err().map(|error| format!("{error:#}"));
        report(Attempt {
            segment: None,
            phase: None,
            engine: (*engine).into(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            error: message.clone(),
        });
        if cancel() {
            bail!("已停止转换，未发布输出")
        }
        match result {
            Ok(()) => return Ok((*engine).into()),
            Err(error) => {
                resource |= resource_failure(&error);
                component_missing &= crate::docling::component_missing(&error);
                failures.push(format!("{engine}：{}", message.unwrap()))
            }
        }
    }
    let component_missing = component_missing && !failures.is_empty();
    Err(EngineFailures {
        messages: failures,
        resource,
        component_missing,
    }
    .into())
}

pub(crate) fn cloud_supports(source:&Path, config:&sag_protocol::mineru::MineruConfig)->bool {
    config.enabled && crate::mineru::supports(source) && (config.provider != sag_protocol::mineru::MineruProvider::Ai302 || source.extension().is_some_and(|ext|ext.eq_ignore_ascii_case("pdf")))
}
pub(crate) fn local_formats(formats:&Option<Vec<String>>,config:&sag_protocol::mineru::MineruConfig)->Option<Vec<String>> {
    formats.as_ref().map(|formats|formats.iter().filter(|extension|!cloud_supports(Path::new(&format!("file.{extension}")),config)).cloned().collect())
}

/// Remembered conversion failures survive routine scans. This key changes only
/// when something able to change the outcome changes: the parsed-content
/// identity, the installed tools, or the conversion budgets the runner
/// receives. A retry therefore happens after an upgrade, an install or a
/// settings change, but a known bad file is not parsed again on every scan.
pub fn failure_identity(source: &Path, settings: &Settings, tools: &str) -> String {
    let runtime = &settings.runtime;
    format!(
        "{}:{tools}:timeout={}:input={}:output={}:threads={}:ocr_batch={}:layout_batch={}:table_batch={}:queue={}",
        identity(source, settings),
        runtime.docling_timeout_secs,
        runtime.conversion_input_mib,
        runtime.conversion_output_mib,
        runtime.docling_threads,
        runtime.ocr_batch_size,
        runtime.layout_batch_size,
        runtime.table_batch_size,
        runtime.queue_max_size,
    )
}

pub fn identity(source: &Path, settings: &Settings) -> String {
    let base = engine_identity(source, settings);
    // Scope invalidation to DOCX/PPTX; changing this switch must not rebuild PDF caches.
    if anydoc::office_text_only(source, &settings.runtime) {
        return format!("office-text-only-v2:{base}");
    }
    if source
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    {
        format!(
            "pdf-content-v1:segments-{}:{base}",
            crate::mineru::PDF_SEGMENT_PAGES
        )
    } else {
        base
    }
}

/// Successful PDF output follows a content contract, not the optional runtime's
/// package revision. Bump pdf-content-v1 for incompatible parser/output changes,
/// including built-in, Docling and fallback behavior. Failures still include the
/// installed tools in failure_identity so installing a runtime enables retry.
/// Only the known equivalent legacy contract is admitted; older parser output
/// (including pre-segmentation versions) must pass through conversion again.
pub(crate) fn compatible_success_hash(
    source: &Path,
    settings: &Settings,
    stored: &str,
) -> Option<String> {
    let (prefix, stored) = if let Some(hash) = stored.strip_prefix("basic-v1:") {
        ("basic-v1:", hash)
    } else if let Some(hash) = stored.strip_prefix("basic-text-v1:") {
        ("basic-text-v1:", hash)
    } else {
        ("", stored)
    };
    let content = stored.rsplit(':').next()?;
    let identity = identity(source, settings);
    let canonical = if identity.is_empty() {
        content.to_owned()
    } else {
        format!("{identity}:{content}")
    };
    // Keep the destination contract explicit as well: a future v2 bump must not
    // silently declare this v1 legacy result compatible with different output.
    let legacy_destination = format!(
        "pdf-content-v1:segments-{}:{}:{content}",
        crate::mineru::PDF_SEGMENT_PAGES,
        engine_identity(source, settings),
    );
    let compatible = stored == canonical
        || source
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
            && canonical == legacy_destination
            && stored == format!(
                "pdf-segments-v1:{}:2.140.0-documents.1:{}:{content}",
                crate::mineru::PDF_SEGMENT_PAGES,
                engine_identity(source, settings),
            );
    compatible.then(|| format!("{prefix}{canonical}"))
}

fn engine_identity(source: &Path, settings: &Settings) -> String {
    let local = if anydoc::handles(source) {
        anydoc::CACHE_VERSION
    } else {
        "docling-v1"
    };
    let cloud = if cloud_supports(source, &settings.mineru) {
        format!("{}:local-images-v2", settings.mineru.content_identity())
    } else {
        "local".into()
    };
    if cloud == "local"
        && settings.runtime.content_identity()
            == sag_protocol::runtime::RuntimeSettings::default().content_identity()
    {
        return if anydoc::handles(source) {
            anydoc::CACHE_VERSION.into()
        } else {
            String::new()
        };
    }
    format!("{local}:{}:{cloud}", settings.runtime.content_identity())
}

pub fn convert(
    source: &Path,
    output: &Path,
    docling: &Docling,
    settings: &Settings,
    cancel: impl Fn() -> bool + Sync,
    report: impl FnMut(Attempt),
) -> Result<()> {
    convert_mode(source, output, docling, settings, cancel, report, false)
}

pub fn convert_enhanced(
    source: &Path,
    output: &Path,
    docling: &Docling,
    settings: &Settings,
    cancel: impl Fn() -> bool + Sync,
    report: impl FnMut(Attempt),
) -> Result<()> {
    convert_mode(source, output, docling, settings, cancel, report, true)
}

fn convert_mode(
    source: &Path,
    output: &Path,
    docling: &Docling,
    settings: &Settings,
    cancel: impl Fn() -> bool + Sync,
    mut report: impl FnMut(Attempt),
    enhanced: bool,
) -> Result<()> {
    if cancel() {
        bail!("已停止转换，未发布输出")
    }
    if fs::metadata(source)?.len() > settings.runtime.conversion_input_mib as u64 * 1024 * 1024 {
        return Err(Failure::new(FailureKind::Memory, "文档超过配置的转换输入大小限制").into());
    }
    if source
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    {
        let limit = if cloud_supports(source, &settings.mineru) {
            settings.mineru.provider.upload_limit_bytes()
        } else {
            settings.runtime.conversion_input_mib as u64 * 1024 * 1024
        };
        let parts =
            match crate::mineru::prepare_pdf_segments(source, limit, &settings.runtime, &cancel) {
                Ok(parts) => parts,
                // The preflight only decides how to split a PDF. A file it cannot
                // measure (lopdf recovers from an unreadable cross-reference by
                // scanning, then sees no catalog in object-stream exports), a file
                // it rejects, or a missing QPDF splitter must not veto conversion:
                // Docling can still repair it. Only a decision the user made
                // (page/input limits) or a cancellation stays fatal.
                Err(error) if preflight_falls_back(&error) => {
                    return convert_single(source, output, docling, settings, cancel, report, None, enhanced, None);
                }
                Err(error) => return Err(error),
            };
        if parts.len() > 1 {
            let report = std::cell::RefCell::new(&mut report);
            let session = (!cloud_supports(source, &settings.mineru)).then(|| docling.pdf_session(&settings.runtime));
            let cancelled = &cancel;
            let reporter = &report;
            return pdf_segments::convert_parts(
                &parts,
                output,
                &format!("{}:enhanced={enhanced}", identity(source, settings)),
                settings.runtime.conversion_output_mib as u64 * 1024 * 1024,
                &cancel,
                |part, current, total| report.borrow_mut()(Attempt {
                    segment: Some(sag_protocol::ConversionSegment { current, total, start_page: part.start, end_page: part.end }),
                    phase: Some("segment".into()), engine: String::new(), elapsed_ms: 0, error: None,
                }),
                move |part, input, result| {
                    convert_single(
                        input,
                        result,
                        docling,
                        settings,
                        cancelled,
                        |attempt| reporter.borrow_mut()(attempt),
                        Some(part.start),
                        enhanced,
                        session.as_ref(),
                    )
                },
            );
        }
    }
    convert_single(source, output, docling, settings, cancel, report, None, enhanced, None)
}

/// Whether a failed PDF segment preflight leaves the file convertible as a whole.
/// Configured limits and cancellation keep their meaning; every other preflight
/// failure only means "cannot segment this file", not "cannot read this file".
fn preflight_falls_back(error: &anyhow::Error) -> bool {
    !error.chain().any(|cause| {
        cause
            .downcast_ref::<Failure>()
            .is_some_and(|failure| matches!(failure.kind, FailureKind::Cancelled | FailureKind::Memory))
    })
}

fn convert_single(
    source: &Path,
    output: &Path,
    docling: &Docling,
    settings: &Settings,
    cancel: impl Fn() -> bool + Sync,
    report: impl FnMut(Attempt),
    original_start: Option<u32>,
    enhanced: bool,
    session: Option<&crate::docling::PdfSession<'_>>,
) -> Result<()> {
    if cancel() {
        bail!("已停止转换，未发布输出")
    }
    if fs::metadata(source)?.len() > settings.runtime.conversion_input_mib as u64 * 1024 * 1024 {
        return Err(Failure::new(FailureKind::Memory, "文档超过配置的转换输入大小限制").into());
    }
    let mut engines = vec![];
    if !anydoc::office_text_only(source, &settings.runtime) && cloud_supports(source, &settings.mineru) {
        engines.push("MinerU")
    }
    if !enhanced && anydoc::handles(source) {
        engines.push("anydoc")
    }
    // Ordinary fallback parsers may load assets, so they cannot serve text-only requests.
    if !anydoc::office_text_only(source, &settings.runtime) {
        engines.push("Docling");
    }
    let parent = output.parent().context("转换输出缺少父目录")?;
    fs::create_dir_all(parent)?;
    // Parsers atomically replace this path. Close our handle first: Windows
    // cannot replace a NamedTempFile that is still held open by its caller.
    let temporary = tempfile::NamedTempFile::new_in(parent)?.into_temp_path();
    let report = std::cell::RefCell::new(report);
    run_engines(
        &engines,
        &cancel,
        |engine| {
            // Every fallback gets a fresh, unpublished result path.
            fs::write(&temporary, b"")?;
            match engine {
                "MinerU" => {
                    let key = crate::mineru::key(docling.data_directory(), &settings.mineru)?;
                    let progress = |phase: &str| {
                        report.borrow_mut()(Attempt {
                            segment: None,
                            phase: Some(phase.into()),
                            engine: "MinerU".into(),
                            elapsed_ms: 0,
                            error: None,
                        });
                    };
                    if let Some(start) = original_start {
                        crate::mineru::convert_pdf_segment_with_runtime(
                            source,
                            &temporary,
                            &settings.mineru,
                            &key,
                            &settings.runtime,
                            &cancel,
                            &progress,
                            start,
                        )?;
                    } else {
                        crate::mineru::convert_with_runtime(
                            source,
                            &temporary,
                            &settings.mineru,
                            &key,
                            &settings.runtime,
                            &cancel,
                            &progress,
                        )?;
                    }
                }
                "anydoc" => match anydoc::convert_with_runtime(
                    source,
                    &temporary,
                    &settings.runtime,
                    &cancel,
                )? {
                    anydoc::Outcome::Converted => {}
                    anydoc::Outcome::NeedsOcr(detail)
                        if !docling.has_profile(sag_protocol::DoclingProfile::Documents) =>
                    {
                        return Err(crate::docling::ComponentMissing(format!(
                            "内置解析需要 OCR：{detail}"
                        ))
                        .into())
                    }
                    anydoc::Outcome::NeedsOcr(detail) => bail!("内置解析需要 OCR：{detail}"),
                },
                _ => match session {
                    Some(session) => session.convert(source, &temporary, &cancel)?,
                    None => docling.convert_docling_with_runtime(source, &temporary, &settings.runtime, &cancel)?,
                },
            }
            let size = fs::metadata(&temporary)?.len();
            if size > settings.runtime.conversion_output_mib as u64 * 1024 * 1024 {
                return Err(Failure::new(FailureKind::Memory, "转换输出超过配置的大小限制").into());
            }
            anyhow::ensure!(
                !fs::read_to_string(&temporary)?.trim().is_empty(),
                "解析器未生成有效正文"
            );
            Ok(())
        },
        |attempt| report.borrow_mut()(attempt),
    )?;
    if cancel() {
        bail!("已停止转换，未发布输出")
    }
    temporary.persist(output).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembered_failures_expire_only_after_relevant_changes() {
        let settings = Settings::default();
        let source = Path::new("report.pdf");
        let tools = "component=missing:splitter=missing";
        let base = failure_identity(source, &settings, tools);
        assert_eq!(base, failure_identity(source, &settings, tools));
        // Installing a component or the page splitter, changing a budget or the
        // parsed-content options can turn the same source into a success.
        assert_ne!(base, failure_identity(source, &settings, "component=2.140.0-documents.1:splitter=missing"));
        assert_ne!(base, failure_identity(source, &settings, "component=missing:splitter=qpdf"));
        for change in [
            |runtime: &mut sag_protocol::runtime::RuntimeSettings| runtime.docling_timeout_secs += 300,
            |runtime: &mut sag_protocol::runtime::RuntimeSettings| runtime.conversion_input_mib += 1,
            |runtime: &mut sag_protocol::runtime::RuntimeSettings| runtime.conversion_output_mib += 1,
            |runtime: &mut sag_protocol::runtime::RuntimeSettings| runtime.ocr_batch_size += 1,
            |runtime: &mut sag_protocol::runtime::RuntimeSettings| runtime.queue_max_size += 1,
            |runtime: &mut sag_protocol::runtime::RuntimeSettings| runtime.do_ocr = false,
        ] {
            let mut changed = settings.clone();
            change(&mut changed.runtime);
            assert_ne!(base, failure_identity(source, &changed, tools));
        }
    }

    fn text_pdf(pages: u32) -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};
        let mut pdf = Document::with_version("1.5");
        let pages_id = pdf.new_object_id();
        let font =
            pdf.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
        let mut kids = Vec::<Object>::new();
        for page in 1..=pages {
            let content = pdf.add_object(Stream::new(
                dictionary! {},
                format!("BT /F1 12 Tf 10 50 Td (Evidence page {page}) Tj ET").into_bytes(),
            ));
            let id = pdf.add_object(dictionary!{"Type"=>"Page","Parent"=>pages_id,"MediaBox"=>vec![0.into(),0.into(),200.into(),100.into()],"Contents"=>content,"Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}}});
            kids.push(id.into());
        }
        pdf.objects.insert(
            pages_id,
            dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>pages}.into(),
        );
        let catalog = pdf.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
        pdf.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        pdf.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn long_pdf_uses_50_page_parts_and_merges_original_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("report.pdf");
        fs::write(&source, text_pdf(51)).unwrap();
        let output = dir.path().join("converted/report.md");
        let settings = Settings::default();
        convert(
            &source,
            &output,
            &Docling::new(dir.path()),
            &settings,
            || false,
            |_| {},
        )
        .unwrap();
        let result = fs::read_to_string(&output).unwrap();
        assert!(
            result.contains("source-pages:1-50"),
            "missing first segment range"
        );
        assert!(
            result.contains("source-pages:51-51"),
            "missing original last page"
        );
        assert!(result.find("Evidence page 1").unwrap() < result.find("Evidence page 51").unwrap());
        assert_eq!(
            fs::read_dir(output.parent().unwrap().join(".sag-pdf-parts"))
                .unwrap()
                .filter_map(Result::ok)
                .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
                .count(),
            2
        );
    }

    #[test]
    fn segmented_pdf_falls_back_per_part_and_retry_only_reads_cache() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("report.pdf");
        fs::write(&source, text_pdf(51)).unwrap();
        let output = dir.path().join(".sag-converted/report.pdf.md");
        let mut settings = Settings::default();
        settings.mineru.enabled = true;
        settings.mineru.provider = sag_protocol::mineru::MineruProvider::Official;
        let mut attempts = vec![];
        convert(
            &source,
            &output,
            &Docling::new(dir.path()),
            &settings,
            || false,
            |a| attempts.push(a),
        )
        .unwrap();
        let engines = attempts
            .iter()
            .filter(|a| a.phase.is_none())
            .map(|a| a.engine.as_str())
            .collect::<Vec<_>>();
        assert_eq!(engines, ["MinerU", "anydoc", "MinerU", "anydoc"]);
        assert!(attempts
            .iter()
            .filter(|a| a.engine == "MinerU" && a.phase.is_none())
            .all(|a| a.error.is_some()));
        let first = fs::read(&output).unwrap();
        convert(
            &source,
            &output,
            &Docling::new(dir.path()),
            &settings,
            || false,
            |attempt| assert_eq!(attempt.phase.as_deref(), Some("segment"), "cached parts must only report progress"),
        )
        .unwrap();
        assert_eq!(fs::read(output).unwrap(), first);
    }

    // A valid PDF whose trailer points at a missing catalog object: lopdf loads it
    // without an error and reports no pages, which is what object-stream exports
    // look like after lopdf recovers an unreadable cross-reference.
    fn pdf_without_a_resolvable_catalog() -> Vec<u8> {
        let mut bytes = text_pdf(1);
        let marker = b"/Root ";
        let start = bytes.windows(marker.len()).position(|window| window == marker).unwrap() + marker.len();
        let end = start + bytes[start..].iter().position(|byte| !byte.is_ascii_digit()).unwrap();
        bytes.splice(start..end, b"9".iter().copied());
        bytes
    }

    #[test]
    fn pdf_preflight_that_cannot_count_pages_still_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("no-catalog.pdf");
        fs::write(&source, pdf_without_a_resolvable_catalog()).unwrap();
        let runtime = sag_protocol::runtime::RuntimeSettings::default();
        let error = crate::mineru::prepare_pdf_segments(&source, 512 * 1024 * 1024, &runtime, &|| false)
            .unwrap_err();
        // lopdf accepts the file, so this error is not a lopdf::Error and the old
        // fallback condition failed the file instead of reaching Docling.
        assert!(!error.chain().any(|cause| cause.is::<lopdf::Error>()));
        assert!(error.to_string().contains("PDF 中没有可解析页面"), "{error:#}");
        assert!(preflight_falls_back(&error));
    }

    #[test]
    fn configured_limits_and_cancellation_stay_fatal() {
        let cancelled = anyhow::Error::new(Failure::new(FailureKind::Cancelled, "已停止转换，未发布输出"));
        assert!(!preflight_falls_back(&cancelled));
        let pages = anyhow::Error::new(Failure::new(FailureKind::Memory, "文档页数超过配置上限"));
        assert!(!preflight_falls_back(&pages));
        assert!(preflight_falls_back(&anyhow::anyhow!("缺少 QPDF 拆页程序，请重新安装完整安装包")));
    }

    #[cfg(unix)]
    fn docling_recovery_fixture(dir: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let home = dir.join("docling");
        fs::create_dir_all(home.join("bin")).unwrap();
        fs::write(home.join("ready"), "2.140.0").unwrap();
        let binary = home.join("bin/docling");
        fs::write(
            &binary,
            r#"#!/bin/sh
set -eu
while [ "$#" -gt 0 ]; do
 if [ "$1" = '--output-file' ]; then shift; output="$1"; fi
 shift
done
printf '# recovered by alternate parser\n' > "$output"
"#,
        )
        .unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn pdf_split_preflight_failure_still_reaches_docling() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("repairable.pdf");
        fs::write(&source, "PDF rejected by the splitter").unwrap();
        docling_recovery_fixture(dir.path());
        let output = dir.path().join(".sag-converted/repaired.md");
        let mut attempts = vec![];
        convert(
            &source,
            &output,
            &Docling::new(dir.path()),
            &Settings::default(),
            || false,
            |a| attempts.push(a),
        )
        .unwrap();
        assert!(fs::read_to_string(output)
            .unwrap()
            .contains("recovered by alternate parser"));
        assert_eq!(
            attempts
                .iter()
                .filter(|a| a.phase.is_none())
                .map(|a| a.engine.as_str())
                .collect::<Vec<_>>(),
            ["anydoc", "Docling"]
        );
    }

    // The icd_test.pdf failure: a valid PDF that lopdf can load but whose pages it
    // cannot enumerate. The preflight must not decide the file is unreadable.
    #[cfg(unix)]
    #[test]
    fn pdf_without_an_enumerated_catalog_still_reaches_docling() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("no-catalog.pdf");
        fs::write(&source, pdf_without_a_resolvable_catalog()).unwrap();
        docling_recovery_fixture(dir.path());
        let output = dir.path().join(".sag-converted/no-catalog.md");
        let mut attempts = vec![];
        convert(
            &source,
            &output,
            &Docling::new(dir.path()),
            &Settings::default(),
            || false,
            |attempt| attempts.push(attempt),
        )
        .unwrap();
        assert!(fs::read_to_string(output)
            .unwrap()
            .contains("recovered by alternate parser"));
        assert_eq!(
            attempts
                .iter()
                .filter(|attempt| attempt.phase.is_none())
                .map(|attempt| attempt.engine.as_str())
                .collect::<Vec<_>>(),
            ["anydoc", "Docling"]
        );
    }

    #[test]
    fn folder_cloud_formats_save_without_docling_but_media_still_requires_component() {
        let dir=tempfile::tempdir().unwrap();let root=dir.path().join("library");fs::create_dir(&root).unwrap();
        let store=crate::store::Store::open(&dir.path().join("data/index.sqlite")).unwrap();let registered=store.add_root(&root).unwrap();store.scan(&registered,|_|{}).unwrap();
        let mut settings=store.settings().unwrap();settings.mineru.provider=sag_protocol::mineru::MineruProvider::Ai302;
        crate::mineru::save_key(&dir.path().join("data"),&settings.mineru,"fixture-key").unwrap();
        assert!(store.settings().unwrap().mineru.enabled);
        store.save_settings(&settings).unwrap();
        let mut options=store.folder_index_options(&root).unwrap();options.convert_other_formats=true;options.conversion_formats=Some(vec!["pdf".into()]);
        store.save_folder_index_options(&root,&options,false).unwrap();
        assert_eq!(store.folder_index_options(&root).unwrap().conversion_formats,Some(vec!["pdf".into()]));
        let open=sag_protocol::mineru::MineruConfig{enabled:true,..Default::default()};
        assert_eq!(local_formats(&Some(vec!["pdf".into(),"png".into()]),&open),Some(vec!["png".into()]));
        options.conversion_formats=Some(vec!["wav".into()]);assert!(store.save_folder_index_options(&root,&options,false).is_err());
        crate::mineru::clear_key(&dir.path().join("data"), &settings.mineru).unwrap();
        assert!(!store.settings().unwrap().mineru.enabled);
    }
    #[test]
    fn mineru_first_then_anydoc_then_docling_and_stops_at_success() {
        let mut seen = vec![];
        let mut reports = vec![];
        let chosen = run_engines(
            &["MinerU", "anydoc", "Docling"],
            &|| false,
            |engine| {
                seen.push(engine.to_string());
                if engine == "Docling" {
                    Ok(())
                } else {
                    anyhow::bail!("test rejection")
                }
            },
            |attempt| reports.push(attempt),
        )
        .unwrap();
        assert_eq!(seen, ["MinerU", "anydoc", "Docling"]);
        assert_eq!(chosen, "Docling");
        let reports: Vec<_> = reports.iter().filter(|attempt| attempt.phase.is_none()).collect();
        assert_eq!(reports.len(), 3);
        assert!(reports[0].error.is_some());
        assert!(reports[2].error.is_none());
        let mut seen = vec![];
        run_engines(
            &["MinerU", "anydoc", "Docling"],
            &|| false,
            |engine| {
                seen.push(engine.to_string());
                Ok(())
            },
            |_| {},
        )
        .unwrap();
        assert_eq!(seen, ["MinerU"]);
    }
    #[test]
    fn enabled_cloud_with_missing_key_falls_back_to_real_builtin_parser() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("report.docx");
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#).unwrap();
        zip.start_file("word/document.xml", options).unwrap();
        zip.write_all(br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>builtin fallback evidence</w:t></w:r></w:p></w:body></w:document>"#).unwrap();
        let original = zip.finish().unwrap().into_inner();
        fs::write(&source, &original).unwrap();
        let output = dir.path().join(".sag-converted/report.md");
        let mut settings = Settings::default();
        settings.mineru.enabled = true;
        assert!(!cloud_supports(&source,&settings.mineru));
        settings.mineru.provider=sag_protocol::mineru::MineruProvider::Ai302;
        let mut attempts = vec![];
        convert(
            &source,
            &output,
            &Docling::new(dir.path()),
            &settings,
            || false,
            |attempt| attempts.push(attempt),
        )
        .unwrap();
        assert!(fs::read_to_string(&output)
            .unwrap()
            .contains("builtin fallback evidence"));
        assert_eq!(
            attempts
                .iter()
                .filter(|attempt| attempt.phase.is_none())
                .map(|attempt| attempt.engine.as_str())
                .collect::<Vec<_>>(),
            ["anydoc"]
        );
        assert!(attempts[0].error.is_none());
        assert_eq!(fs::read(&source).unwrap(), original);
    }
    #[test]
    fn cancellation_prevents_fallback_and_all_failures_are_preserved() {
        let stopped = std::cell::Cell::new(false);
        let mut seen = vec![];
        let result = run_engines(
            &["MinerU", "anydoc"],
            &|| stopped.get(),
            |engine| {
                seen.push(engine.to_string());
                stopped.set(true);
                anyhow::bail!("network stopped")
            },
            |_| {},
        );
        assert!(result.is_err());
        assert_eq!(seen, ["MinerU"]);
        let error = run_engines(
            &["MinerU", "anydoc"],
            &|| false,
            |engine| anyhow::bail!("{engine} failed"),
            |_| {},
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("MinerU failed") && error.contains("anydoc failed"));
    }
}

#[cfg(test)]
mod parser_start_tests {
    use super::*;
    #[test]
    fn reports_each_parser_before_work_including_fallback() {
        let active = std::cell::RefCell::new(String::new());
        let seen = std::cell::RefCell::new(Vec::new());
        run_engines(&["MinerU", "anydoc", "Docling"], &|| false, |engine| {
            assert_eq!(*active.borrow(), engine, "parser must be visible before work starts");
            seen.borrow_mut().push(engine.to_string());
            if engine == "Docling" { Ok(()) } else { bail!("fallback") }
        }, |attempt| {
            if attempt.phase.as_deref() == Some("starting") { *active.borrow_mut() = attempt.engine; }
            else if attempt.phase.is_none() { active.borrow_mut().clear(); }
        }).unwrap();
        assert_eq!(*seen.borrow(), ["MinerU", "anydoc", "Docling"]);
        assert!(active.borrow().is_empty());
    }
}
