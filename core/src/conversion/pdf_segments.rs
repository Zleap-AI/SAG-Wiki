//! Sequential PDF parts with durable successes and one atomic final publication.
use crate::mineru::Part;
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

fn read_cached(path: &Path, limit: u64) -> Result<Option<String>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                meta.is_file() && !meta.file_type().is_symlink(),
                "PDF 分段缓存不是普通文件"
            );
            if meta.len() > limit {
                return Ok(None);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    Ok(String::from_utf8(bytes)
        .ok()
        .filter(|text| !text.trim().is_empty() && text.len() as u64 <= limit))
}

pub(super) fn convert_parts(
    parts: &[Part],
    output: &Path,
    identity: &str,
    limit: u64,
    cancel: &impl Fn() -> bool,
    mut convert: impl FnMut(&Part, &Path, &Path) -> Result<()>,
) -> Result<()> {
    let parent = output.parent().context("转换输出缺少父目录")?;
    fs::create_dir_all(parent)?;
    let cache = parent.join(".sag-pdf-parts");
    if let Ok(meta) = fs::symlink_metadata(&cache) {
        ensure!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "PDF 分段缓存目录无效"
        );
    }
    fs::create_dir_all(&cache)?;
    let work = tempfile::Builder::new()
        .prefix(".sag-pdf-work-")
        .tempdir_in(parent)?;
    let mut merged = tempfile::NamedTempFile::new_in(parent)?;
    let mut size = 0u64;
    for part in parts {
        ensure!(!cancel(), "已停止转换，未发布输出");
        let mut hash = Sha256::new();
        hash.update(identity);
        hash.update(part.start.to_le_bytes());
        hash.update(part.end.to_le_bytes());
        hash.update(&part.bytes);
        let path = cache.join(format!("{:x}.md", hash.finalize()));
        let text = if let Some(text) = read_cached(&path, limit)? {
            text
        } else {
            let source = work.path().join(&part.name);
            fs::write(&source, &part.bytes)?;
            // Parsers receive an unpublished path beside the durable cloud journals.
            let parsed = tempfile::NamedTempFile::new_in(&cache)?.into_temp_path();
            convert(part, &source, &parsed)
                .with_context(|| format!("PDF 第 {}–{} 页转换失败", part.start, part.end))?;
            ensure!(!cancel(), "已停止转换，未发布输出");
            let text = read_cached(&parsed, limit)?
                .context("PDF 分段未生成有效正文或超过大小限制")?;
            // Docling prepends a source link. The split PDF is temporary; keep
            // the original document identity instead of publishing a dead link.
            let text = if text.starts_with(&format!("[{}](<", part.name)) {
                text.split_once("\n\n")
                    .filter(|(header, _)| header.contains(".sag-pdf-work-"))
                    .map_or(text.clone(), |(_, body)| body.to_owned())
            } else {
                text
            };
            ensure!(!text.trim().is_empty(), "PDF 分段未生成有效正文");
            let mut saved = tempfile::NamedTempFile::new_in(&cache)?;
            saved.write_all(text.as_bytes())?;
            saved.persist(&path).map_err(|e| e.error)?;
            text
        };
        // MinerU stores assets beside each cached part; merged Markdown lives one directory up.
        static ASSET_DESTINATIONS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
            regex::Regex::new(r#"(?P<prefix>\]\(\s*<?|(?i:\bsrc\s*=\s*["'])|(?m:^\s*\[[^\]\n]+\]:\s*<?))\.sag-mineru-assets/"#).unwrap()
        });
        let text = ASSET_DESTINATIONS.replace_all(&text, "${prefix}.sag-pdf-parts/.sag-mineru-assets/");
        let marker = format!("\n\n<!-- source-pages:{}-{} -->\n\n", part.start, part.end);
        size = size
            .checked_add(marker.len() as u64)
            .and_then(|n| n.checked_add(text.len() as u64))
            .context("PDF 合并正文超过大小限制")?;
        if size > limit {
            return Err(crate::process_supervisor::Failure::new(
                crate::process_supervisor::FailureKind::Memory,
                "PDF 合并正文超过配置的大小限制",
            )
            .into());
        }
        merged.write_all(marker.as_bytes())?;
        merged.write_all(text.as_bytes())?;
    }
    ensure!(!cancel(), "已停止转换，未发布输出");
    merged.persist(output).map_err(|e| e.error)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn parts() -> Vec<crate::mineru::Part> {
        vec![(1, 50), (51, 100), (101, 101)]
            .into_iter()
            .map(|(start, end)| crate::mineru::Part {
                bytes: vec![start as u8],
                start,
                end,
                name: format!("pages-{start}-{end}.pdf"),
            })
            .collect()
    }
    #[test]
    fn cached_parts_keep_assets_reachable_and_allow_atomic_parser_output() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.md");
        let convert = |_: &Part, _: &Path, out: &Path| -> Result<()> {
            let assets = out.parent().unwrap().join(".sag-mineru-assets/hash/images");
            fs::create_dir_all(&assets)?;
            fs::write(assets.join("x.png"), b"image")?;
            let mut replacement = tempfile::NamedTempFile::new_in(out.parent().unwrap())?;
            replacement.write_all(b"![curve [red]](.sag-mineru-assets/hash/images/x.png)\n<img src=\".sag-mineru-assets/hash/images/x.png\">\n[ref]: .sag-mineru-assets/hash/images/x.png\n![remote](https://host/.sag-mineru-assets/x.png)\nplain .sag-mineru-assets/ text")?;
            replacement.persist(out).map_err(|e| e.error)?;
            Ok(())
        };
        convert_parts(&parts()[..1], &output, "v1", 1024, &|| false, convert).unwrap();
        let first = fs::read_to_string(&output).unwrap();
        let destination = ".sag-pdf-parts/.sag-mineru-assets/hash/images/x.png";
        assert!(first.contains(destination));
        assert_eq!(first.matches(destination).count(), 3);
        assert!(first.contains("https://host/.sag-mineru-assets/x.png"));
        assert!(first.contains("plain .sag-mineru-assets/ text"));
        assert_eq!(fs::read(dir.path().join(destination)).unwrap(), b"image");
        convert_parts(&parts()[..1], &output, "v1", 1024, &|| false,
            |_, _, _| panic!("cached parts must not invoke parser")).unwrap();
        assert_eq!(fs::read_to_string(output).unwrap(), first);
    }

    #[test]
    fn retry_reuses_successful_parts_and_never_publishes_partial_output() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.md");
        fs::write(&output, "previous complete document").unwrap();
        let mut visited = vec![];
        let result = convert_parts(
            &parts(),
            &output,
            "version-1",
            1024,
            &|| false,
            |part, _, out| {
                visited.push(part.start);
                if part.start == 51 {
                    anyhow::bail!("middle part failed");
                }
                fs::write(out, format!("body {}", part.start))?;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(visited, [1, 51]);
        assert_eq!(
            fs::read_to_string(&output).unwrap(),
            "previous complete document"
        );
        visited.clear();
        convert_parts(
            &parts(),
            &output,
            "version-1",
            1024,
            &|| false,
            |part, _, out| {
                visited.push(part.start);
                fs::write(out, format!("body {}", part.start))?;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(visited, [51, 101]);
        let merged = fs::read_to_string(&output).unwrap();
        assert!(merged.find("body 1").unwrap() < merged.find("body 51").unwrap());
        assert!(merged.contains("source-pages:101-101"));
        convert_parts(
            &parts(),
            &output,
            "version-1",
            1024,
            &|| false,
            |_, _, _| panic!("all parts cached"),
        )
        .unwrap();
        let mut count = 0;
        convert_parts(
            &parts(),
            &output,
            "version-2",
            1024,
            &|| false,
            |_, _, out| {
                count += 1;
                fs::write(out, "new version")?;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(count, 3);
    }
    #[test]
    fn cancellation_and_merged_limit_preserve_existing_output() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.md");
        fs::write(&output, "previous").unwrap();
        let stopped = std::cell::Cell::new(false);
        let result = convert_parts(
            &parts(),
            &output,
            "v1",
            1024,
            &|| stopped.get(),
            |_, _, out| {
                fs::write(out, "body")?;
                stopped.set(true);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&output).unwrap(), "previous");
        let result = convert_parts(&parts(), &output, "v1", 50, &|| false, |_, _, out| {
            fs::write(out, "body")?;
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&output).unwrap(), "previous");
    }
    #[test]
    fn invalid_cache_is_rebuilt_and_temporary_source_links_are_not_merged() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.md");
        let parts = parts();
        convert_parts(
            &parts[..1],
            &output,
            "v1",
            1024,
            &|| false,
            |part, input, out| {
                fs::write(
                    out,
                    format!("[{}](<{}>)\n\nbody", part.name, input.display()),
                )?;
                Ok(())
            },
        )
        .unwrap();
        assert!(!fs::read_to_string(&output)
            .unwrap()
            .contains(".sag-pdf-work-"));
        let entry = fs::read_dir(dir.path().join(".sag-pdf-parts"))
            .unwrap()
            .filter_map(Result::ok)
            .find(|e| e.path().extension().is_some_and(|x| x == "md"))
            .unwrap();
        fs::write(entry.path(), [0xff]).unwrap();
        let mut count = 0;
        convert_parts(&parts[..1], &output, "v1", 1024, &|| false, |_, _, out| {
            count += 1;
            fs::write(out, "rebuilt")?;
            Ok(())
        })
        .unwrap();
        assert_eq!(count, 1);
        assert!(fs::read_to_string(&output).unwrap().contains("rebuilt"));
    }
}

/// Opt-in acceptance against a caller-supplied PDF and installed document
/// component. Uses an isolated knowledge root/database; never alters the source.
#[cfg(all(test, unix))]
#[test]
#[ignore = "requires SAG_PDF_ACCEPTANCE_SOURCE, SAG_PDF_ACCEPTANCE_DATA and SAG_PDF_ACCEPTANCE_DIR"]
fn real_pdf_conversion_indexing_and_cached_retry() {
    use crate::{conversion, docling::Docling, store::Store};
    use sag_protocol::types::Settings;
    let source = std::path::PathBuf::from(std::env::var("SAG_PDF_ACCEPTANCE_SOURCE").unwrap());
    let original_data = std::path::PathBuf::from(std::env::var("SAG_PDF_ACCEPTANCE_DATA").unwrap());
    let artifacts = std::path::PathBuf::from(std::env::var("SAG_PDF_ACCEPTANCE_DIR").unwrap());
    fs::create_dir_all(&artifacts).unwrap();
    let sandbox = std::env::var("SAG_PDF_ACCEPTANCE_REUSE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            tempfile::Builder::new()
                .prefix("real-pdf-")
                .tempdir_in(&artifacts)
                .unwrap()
                .keep()
        });
    let root = sandbox.join("library");
    let data = sandbox.join("data");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&data).unwrap();
    let input = root.join("acceptance.pdf");
    if !input.exists() {
        fs::copy(&source, &input).unwrap();
    }
    let original_hash = crate::docling::fingerprint(&source).unwrap();
    assert_eq!(crate::docling::fingerprint(&input).unwrap(), original_hash);
    if !data.join("docling").exists() {
        std::os::unix::fs::symlink(original_data.join("docling"), data.join("docling")).unwrap();
    }
    let docling = Docling::new(&data);
    assert!(
        docling.has_profile(sag_protocol::DoclingProfile::Documents),
        "installed document component required"
    );
    let db = rusqlite::Connection::open_with_flags(
        original_data.join("sag.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let raw: String = db
        .query_row("SELECT value FROM preferences", [], |r| r.get(0))
        .unwrap();
    let mut settings: Settings = serde_json::from_str(&raw).unwrap();
    let key = crate::mineru::key(&original_data, &settings.mineru).unwrap();
    crate::mineru::save_key(&data, &settings.mineru, &key).unwrap();
    // Keep service waiting bounded for this acceptance; local parser limits,
    // OCR, models and the actual configured cloud provider are unchanged.
    settings.mineru.request_timeout_secs = 10;
    settings.mineru.task_timeout_secs = 15;
    let store = Store::open(&data.join("sag.sqlite")).unwrap();
    store.save_settings(&settings).unwrap();
    let registered = store.add_root(&root).unwrap();
    let started = std::time::Instant::now();
    println!(
        "acceptance sandbox={} pages={} cloud_task_timeout_secs=15",
        sandbox.display(),
        lopdf::Document::load(&source).unwrap().get_pages().len()
    );
    store.scan(&registered, |_| {}).unwrap();
    let state = store
        .roots()
        .unwrap()
        .into_iter()
        .find(|r| r.path == registered)
        .unwrap();
    println!(
        "root_status={} elapsed_ms={} error={:?}",
        state.status,
        started.elapsed().as_millis(),
        state.error
    );
    for h in store.index_history().unwrap() {
        if let Some(log) = store.index_history_log(h.id).unwrap().raw {
            for line in log.lines() {
                let row: serde_json::Value = serde_json::from_str(line).unwrap();
                if row.get("engine").is_some() && row.get("phase").is_none() {
                    println!(
                        "engine={} elapsed_ms={} status={} error={}",
                        row["engine"], row["elapsed_ms"], row["status"], row["error"]
                    );
                }
            }
        }
    }
    assert_eq!(state.status, "ready", "{:?}", state.error);
    let output = root.join(".sag-converted/acceptance.pdf.md");
    let text = fs::read_to_string(&output).unwrap();
    let page_count = lopdf::Document::load(&source).unwrap().get_pages().len();
    let ranges = regex::Regex::new(r"<!-- source-pages:(\d+)-(\d+) -->").unwrap();
    let actual = ranges
        .captures_iter(&text)
        .map(|c| {
            (
                c[1].parse::<usize>().unwrap(),
                c[2].parse::<usize>().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let expected = (1..=page_count)
        .step_by(50)
        .map(|start| (start, (start + 49).min(page_count)))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    assert!(!text.contains(".sag-pdf-work-"));
    let terms = regex::Regex::new(r"[\p{Han}]{2,}|[A-Za-z]{4,}").unwrap();
    let body = text
        .lines()
        .filter(|l| !l.contains("<!--") && !l.starts_with("<a "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        terms.find_iter(&body).take(100).any(|term| !store
            .search_files(term.as_str(), &[], 5)
            .unwrap()
            .is_empty()),
        "merged content must be searchable"
    );
    let retry = std::time::Instant::now();
    conversion::convert(
        &input,
        &output,
        &docling,
        &settings,
        || false,
        |_| panic!("cached retry must not invoke any parser"),
    )
    .unwrap();
    assert_eq!(fs::read_to_string(&output).unwrap(), text);
    assert_eq!(crate::docling::fingerprint(&source).unwrap(), original_hash);
    println!(
        "acceptance passed: segments={} output_bytes={} cached_retry_ms={}",
        actual.len(),
        text.len(),
        retry.elapsed().as_millis()
    );
}
