//! Ordered parsing and per-engine history; only complete output is published.
use crate::{
    anydoc,
    docling::Docling,
    process_supervisor::{Failure, FailureKind},
};
use anyhow::{bail, Context, Result};
use sag_protocol::types::Settings;
use std::{fs, path::Path, time::Instant};

pub struct Attempt {
    pub phase: Option<String>,
    pub engine: String,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug)]
struct EngineFailures {
    messages: Vec<String>,
    resource: bool,
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
fn run_engines(
    engines: &[&str],
    cancel: &impl Fn() -> bool,
    mut convert: impl FnMut(&str) -> Result<()>,
    mut report: impl FnMut(Attempt),
) -> Result<String> {
    let mut failures = vec![];
    let mut resource = false;
    for engine in engines {
        if cancel() {
            bail!("已停止转换，未发布输出")
        }
        let started = Instant::now();
        let result = convert(engine);
        let message = result.as_ref().err().map(|error| format!("{error:#}"));
        report(Attempt {
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
                failures.push(format!("{engine}：{}", message.unwrap()))
            }
        }
    }
    Err(EngineFailures {
        messages: failures,
        resource,
    }
    .into())
}

pub(crate) fn local_formats(formats:&Option<Vec<String>>,remote:bool)->Option<Vec<String>> {
    formats.as_ref().map(|formats|formats.iter().filter(|extension|!remote||!crate::mineru::supports(Path::new(&format!("file.{extension}")))).cloned().collect())
}

pub fn identity(source: &Path, settings: &Settings) -> String {
    let local = if anydoc::handles(source) {
        anydoc::CACHE_VERSION
    } else {
        "docling-v1"
    };
    let cloud = if settings.mineru.enabled && crate::mineru::supports(source) {
        settings.mineru.content_identity()
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
    if cancel() {
        bail!("已停止转换，未发布输出")
    }
    if fs::metadata(source)?.len() > settings.runtime.conversion_input_mib as u64 * 1024 * 1024 {
        return Err(Failure::new(FailureKind::Memory, "文档超过配置的转换输入大小限制").into());
    }
    let mut engines = vec![];
    if settings.mineru.enabled && crate::mineru::supports(source) {
        engines.push("MinerU")
    }
    if anydoc::handles(source) {
        engines.push("anydoc")
    }
    engines.push("Docling");
    let parent = output.parent().context("转换输出缺少父目录")?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    let report = std::cell::RefCell::new(report);
    run_engines(
        &engines,
        &cancel,
        |engine| {
            // Every fallback gets a fresh, unpublished result path.
            fs::write(temporary.path(), b"")?;
            match engine {
                "MinerU" => {
                    let key = crate::mineru::key(docling.data_directory(), &settings.mineru)?;
                    crate::mineru::convert_with_runtime(
                        source,
                        temporary.path(),
                        &settings.mineru,
                        &key,
                        &settings.runtime,
                        &cancel,
                        &|phase| {
                            report.borrow_mut()(Attempt {
                                phase: Some(phase.into()),
                                engine: "MinerU".into(),
                                elapsed_ms: 0,
                                error: None,
                            });
                        },
                    )?;
                }
                "anydoc" => match anydoc::convert_with_runtime(
                    source,
                    temporary.path(),
                    &settings.runtime,
                    &cancel,
                )? {
                    anydoc::Outcome::Converted => {}
                    anydoc::Outcome::NeedsOcr(detail) => bail!("内置解析需要 OCR：{detail}"),
                },
                _ => docling.convert_docling_with_runtime(
                    source,
                    temporary.path(),
                    &settings.runtime,
                    &cancel,
                )?,
            }
            let size = fs::metadata(temporary.path())?.len();
            if size > settings.runtime.conversion_output_mib as u64 * 1024 * 1024 {
                return Err(Failure::new(FailureKind::Memory, "转换输出超过配置的大小限制").into());
            }
            anyhow::ensure!(
                !fs::read_to_string(temporary.path())?.trim().is_empty(),
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
    fn folder_cloud_formats_save_without_docling_but_media_still_requires_component() {
        let dir=tempfile::tempdir().unwrap();let root=dir.path().join("library");fs::create_dir(&root).unwrap();
        let store=crate::store::Store::open(&dir.path().join("data/index.sqlite")).unwrap();let registered=store.add_root(&root).unwrap();store.scan(&registered,|_|{}).unwrap();
        let mut settings=store.settings().unwrap();settings.mineru.enabled=true;store.save_settings(&settings).unwrap();
        let mut options=store.folder_index_options(&root).unwrap();options.convert_other_formats=true;options.conversion_formats=Some(vec!["png".into()]);
        store.save_folder_index_options(&root,&options,false).unwrap();
        assert_eq!(store.folder_index_options(&root).unwrap().conversion_formats,Some(vec!["png".into()]));
        options.conversion_formats=Some(vec!["wav".into()]);assert!(store.save_folder_index_options(&root,&options,false).is_err());
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
                .map(|attempt| attempt.engine.as_str())
                .collect::<Vec<_>>(),
            ["MinerU", "anydoc"]
        );
        assert!(attempts[0].error.is_some());
        assert!(attempts[1].error.is_none());
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
