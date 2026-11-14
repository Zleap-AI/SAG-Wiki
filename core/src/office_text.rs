//! Restricted OOXML package: binary members never enter the decompressor.
use super::{Failure, FailureKind, Outcome};
use anyhow::{ensure, Context, Result};
use quick_xml::{events::Event, Reader, Writer};
use sag_protocol::runtime::RuntimeSettings;
use std::{
    fs,
    io::{BufRead, BufReader, Cursor, Read, Write},
    path::Path,
    time::{Duration, Instant},
};

fn allowed(name: &str, prefix: &str) -> bool {
    if name == "[Content_Types].xml" || name == "_rels/.rels" {
        return true;
    }
    let Some(part) = name.strip_prefix(prefix) else {
        return false;
    };
    if part.starts_with("media/") || part.starts_with("embeddings/") || part.starts_with("activeX/")
    {
        return false;
    }
    let is_xml = part.ends_with(".xml") || part.ends_with(".xml.rels");
    is_xml
        && (matches!(
            part,
            "document.xml"
                | "presentation.xml"
                | "styles.xml"
                | "numbering.xml"
                | "settings.xml"
                | "fontTable.xml"
                | "footnotes.xml"
                | "endnotes.xml"
                | "comments.xml"
        ) || ["header", "footer"].iter().any(|p| {
            part.strip_prefix(p).is_some_and(|rest| {
                rest.strip_suffix(".xml")
                    .is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()))
            })
        }) || [
            "_rels/",
            "slides/",
            "slideLayouts/",
            "slideMasters/",
            "notesSlides/",
            "notesMasters/",
            "theme/",
            "charts/",
            "diagrams/",
        ]
        .iter()
        .any(|p| part.starts_with(p)))
}
fn sanitize(input: impl BufRead, check: &dyn Fn() -> Result<()>) -> Result<Vec<u8>> {
    let mut reader = Reader::from_reader(input);
    let mut buffer = Vec::new();
    let mut writer = Writer::new(Vec::new());
    let mut depth = 0usize;
    let mut nodes = 0usize;
    let mut skip = 0usize;
    loop {
        buffer.clear();
        let event = reader.read_event_into(&mut buffer)?;
        nodes += 1;
        if nodes % 1024 == 0 {
            check()?;
        }
        ensure!(nodes <= 2_000_000, "Office XML 节点超过上限");
        let binary = match &event {
            Event::Start(e) | Event::Empty(e) => {
                matches!(e.local_name().as_ref(), b"binData" | b"binaryData")
            }
            _ => false,
        };
        ensure!(
            !binary,
            "仅文字解析不支持 XML 内联二进制内容，不会回退至普通解析"
        );
        match &event {
            Event::Start(e) => {
                depth += 1;
                ensure!(depth <= 256, "Office XML 嵌套超过上限");
                if skip > 0 {
                    skip += 1;
                } else if matches!(
                    e.local_name().as_ref(),
                    b"blip" | b"imagedata" | b"OLEObject" | b"oleObj"
                ) {
                    skip = 1;
                    continue;
                }
            }
            Event::End(_) => {
                ensure!(depth > 0, "Office XML 结构无效");
                depth -= 1;
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
            }
            Event::Empty(e)
                if matches!(
                    e.local_name().as_ref(),
                    b"blip" | b"imagedata" | b"OLEObject" | b"oleObj"
                ) =>
            {
                continue
            }
            Event::DocType(_) => anyhow::bail!("仅文字解析不支持 XML DTD"),
            Event::Eof => {
                ensure!(depth == 0, "Office XML 未完整结束");
                break;
            }
            _ => {}
        }
        if skip == 0 {
            writer.write_event(event)?;
        }
    }
    Ok(writer.into_inner())
}
fn content_types(
    bytes: &[u8],
    check: &dyn Fn() -> Result<()>,
) -> Result<(
    std::collections::HashMap<String, String>,
    std::collections::HashMap<String, String>,
)> {
    let clean = sanitize(BufReader::new(bytes), check)?;
    let mut reader = Reader::from_reader(clean.as_slice());
    let (mut overrides, mut defaults) = (
        std::collections::HashMap::new(),
        std::collections::HashMap::new(),
    );
    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e)
                if matches!(e.local_name().as_ref(), b"Override" | b"Default") =>
            {
                let mut name = None;
                let mut mime = None;
                for a in e.attributes() {
                    let a = a?;
                    let v = a
                        .decoded_and_normalized_value(
                            quick_xml::XmlVersion::Implicit1_0,
                            reader.decoder(),
                        )?
                        .into_owned();
                    match a.key.local_name().as_ref() {
                        b"PartName" | b"Extension" => name = Some(v),
                        b"ContentType" => mime = Some(v),
                        _ => {}
                    }
                }
                if let (Some(name), Some(mime)) = (name, mime) {
                    if e.local_name().as_ref() == b"Override" {
                        ensure!(
                            overrides
                                .insert(name.trim_start_matches('/').to_owned(), mime)
                                .is_none(),
                            "Office 内容类型重名"
                        );
                    } else {
                        ensure!(defaults.insert(name, mime).is_none(), "Office 内容类型重名");
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok((overrides, defaults))
}
// OPC gives text parts their identity through content types and relationships,
// not conventional filenames. Keep this list exact so media/OLE types stay excluded.
fn word_text_part(mime: &str) -> bool {
    mime.strip_prefix("application/vnd.openxmlformats-officedocument.wordprocessingml.")
        .is_some_and(|kind| {
            matches!(
                kind,
                "styles+xml"
                    | "numbering+xml"
                    | "settings+xml"
                    | "fontTable+xml"
                    | "footnotes+xml"
                    | "endnotes+xml"
                    | "comments+xml"
                    | "header+xml"
                    | "footer+xml"
            )
        })
}

fn text_mime(mime: &str) -> bool {
    mime == "application/vnd.openxmlformats-package.relationships+xml"
        || (mime.ends_with("+xml")
            && [
                "application/vnd.openxmlformats-officedocument.wordprocessingml.",
                "application/vnd.openxmlformats-officedocument.presentationml.",
                "application/vnd.openxmlformats-officedocument.drawingml.",
            ]
            .iter()
            .any(|p| mime.starts_with(p)))
}
pub(super) fn convert(
    source: &Path,
    output: &Path,
    runtime: &RuntimeSettings,
    cancel: impl Fn() -> bool,
) -> Result<Outcome> {
    let start = Instant::now();
    let checkpoint = || -> Result<()> {
        ensure!(
            !cancel(),
            Failure::new(FailureKind::Cancelled, "已停止 Office 文字提取")
        );
        ensure!(
            start.elapsed() < Duration::from_secs(runtime.anydoc_timeout_secs),
            Failure::new(FailureKind::Timeout, "Office 文字提取超时")
        );
        Ok(())
    };
    checkpoint()?;
    let mut zip = zip::ZipArchive::new(fs::File::open(source)?)?;
    ensure!(zip.len() <= 100_000, "Office ZIP 条目数超过上限");
    let prefix = if source
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("docx"))
    {
        "word/"
    } else {
        "ppt/"
    };
    let mut names = std::collections::HashSet::new();
    for name in zip.file_names() {
        ensure!(
            name.len() <= 4096
                && !name.contains('\\')
                && !name.starts_with('/')
                && !name.split('/').any(|part| part == ".." || part == "."),
            "Office ZIP 路径无效"
        );
        // zip exposes one effective entry per name; copy that same view into the restricted package.
        names.insert(name.to_owned());
    }
    let main = if prefix == "word/" {
        "word/document.xml"
    } else {
        "ppt/presentation.xml"
    };
    ensure!(
        names.contains(main),
        "不支持此 Office 主文档位置，未回退至普通资源解析"
    );
    let cap = (runtime.max_decoded_text_mib.min(128) * 1024 * 1024) as u64;
    let mut types = Vec::new();
    {
        let entry = zip.by_name("[Content_Types].xml")?;
        ensure!(entry.size() <= cap, "Office 内容类型部件超过上限");
        entry.take(cap + 1).read_to_end(&mut types)?;
        ensure!(types.len() as u64 <= cap, "Office 内容类型部件超过上限");
    }
    let (overrides, defaults) = content_types(&types, &checkpoint)?;
    let content_type = |name: &str| {
        overrides
            .get(name)
            .or_else(|| name.rsplit_once('.').and_then(|(_, ext)| defaults.get(ext)))
    };
    let mut text_parts = std::collections::HashSet::new();
    if prefix == "word/" {
        for name in &names {
            if content_type(name).is_some_and(|mime| word_text_part(mime)) {
                text_parts.insert(name.clone());
                // Retain the part's relationship metadata too (e.g. links inside notes).
                let (parent, file) = name
                    .rsplit_once('/')
                    .map_or(("", name.as_str()), |(p, f)| (p, f));
                let parent = if parent.is_empty() {
                    String::new()
                } else {
                    format!("{parent}/")
                };
                text_parts.insert(format!("{parent}_rels/{file}.rels"));
            }
        }
    }
    let mut names: Vec<_> = names
        .into_iter()
        .filter(|n| {
            (allowed(n, prefix) || text_parts.contains(n))
                && (n == "[Content_Types].xml"
                    || content_type(n).is_some_and(|mime| text_mime(mime)))
        })
        .collect();
    names.sort();
    ensure!(names.iter().any(|n| n == main), "Office 主文档内容类型无效");
    let mut package = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let total_cap = runtime
        .text_preparation_mib
        .min(512)
        .min(runtime.max_decoded_text_mib)
        * 1024
        * 1024;
    let mut total = 0usize;
    for name in names {
        checkpoint()?;
        // Filtering happens BEFORE by_name/read: excluded assets are never opened.
        let entry = zip.by_name(&name)?;
        ensure!(entry.size() <= cap, "Office 文字部件超过解压上限");
        let budget = cap.min(total_cap.saturating_sub(total) as u64);
        let mut input = BufReader::new(entry.take(budget + 1));
        let bytes = sanitize(&mut input, &checkpoint)
            .with_context(|| format!("无法读取 Office 文字部件 {name}"))?;
        let consumed = budget + 1 - input.get_ref().limit();
        ensure!(consumed <= budget, "Office 文字解压总量超过上限");
        total = total
            .checked_add(consumed as usize)
            .context("Office 文字大小溢出")?;
        ensure!(bytes.len() <= total_cap, "Office 文字输出超过准备上限");
        package.start_file(name, options)?;
        package.write_all(&bytes)?;
    }
    checkpoint()?;
    let bytes = package.finish()?.into_inner();
    let format = if prefix == "word/" {
        anydoc::Format::Docx
    } else {
        anydoc::Format::Pptx
    };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("office-text".into())
        .spawn(move || {
            let _ = tx.send(anydoc::to_markdown_bytes(&bytes, format));
        })?;
    let body = loop {
        checkpoint()?;
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(result) => break result.map_err(super::map_error)?,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => anyhow::bail!("Office 文字解析线程异常退出"),
        }
    };
    checkpoint()?;
    ensure!(
        body.len() <= runtime.conversion_output_mib * 1024 * 1024,
        "Office 文字输出超过上限"
    );
    ensure!(
        !body.trim().is_empty(),
        "Office 未提取到文字；不会自动读取图片或对象"
    );
    fs::write(output, body)?;
    Ok(Outcome::Converted)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_and_embeddings_are_not_allowed_even_with_xml_suffix() {
        for path in [
            "word/media/diagram.xml",
            "word/embeddings/object.xml",
            "word/activeX/control.xml",
            "word/customXml/foo.xml",
        ] {
            assert!(!allowed(path, "word/"));
        }
        assert!(!text_mime("image/svg+xml"));
        assert!(text_mime(
            "application/vnd.openxmlformats-officedocument.drawingml.chart+xml"
        ));
    }
    #[test]
    fn removes_binary_references_but_preserves_text_boxes_and_tables() {
        let xml=br#"<root><drawing><txbxContent><p>keep</p></txbxContent><blip id="image"/><OLEObject><payload/></OLEObject></drawing><tbl>table</tbl></root>"#;
        let out = String::from_utf8(sanitize(BufReader::new(xml.as_slice()), &|| Ok(())).unwrap())
            .unwrap();
        assert!(out.contains("keep") && out.contains("table"));
        assert!(!out.contains("blip") && !out.contains("OLEObject"));
        assert!(sanitize(
            BufReader::new(b"<!DOCTYPE x><root/>".as_slice()),
            &|| Ok(())
        )
        .is_err());
        assert!(sanitize(
            BufReader::new(b"<root><binData>payload</binData></root>".as_slice()),
            &|| Ok(())
        )
        .is_err());
    }
}
