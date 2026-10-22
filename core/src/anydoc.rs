//! Built-in document conversion with the anydoc crate (MIT).
//!
//! Handles PDF, Word, PowerPoint, Excel/CSV, OpenDocument, RTF and EPUB in
//! process without any downloaded component. It does not do OCR: a scanned
//! PDF is reported as [`Outcome::NeedsOcr`] so the caller can hand it to the
//! Docling component when that is installed.
use crate::process_supervisor::{Failure, FailureKind};
use anyhow::Result;
use std::{fs, path::Path, time::Duration};

/// Extensions the built-in converter owns. Everything else that Docling can
/// read stays with Docling.
/// RTF is deliberately absent: the indexer already reads it natively.
pub const EXTENSIONS: &[&str] = &[
    "pdf", "doc", "docx", "docm", "ppt", "pps", "pot", "pptx", "pptm", "ppsx", "ppsm",
    "xls", "xlsx", "xlsm", "xlsb", "csv", "odt", "ods", "odp", "epub",
];
const INPUT_LIMIT: u64 = 512 * 1024 * 1024;
const OUTPUT_LIMIT: usize = 64 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(120);

pub fn handles(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(handles_extension)
}
pub fn handles_extension(extension: &str) -> bool {
    EXTENSIONS.iter().any(|known| known.eq_ignore_ascii_case(extension))
}
/// Whether the built-in converter gave up for a reason Docling may handle
/// (unparseable or image-only content), as opposed to a limit or cancel.
pub fn is_handoff_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<Failure>()
        .is_some_and(|failure| failure.kind == FailureKind::Compatibility)
}

#[derive(Debug)]
pub enum Outcome {
    /// Markdown was written to the output path.
    Converted,
    /// The document has pages without a text layer; message names them.
    NeedsOcr(String),
}

/// Converts `source` to Markdown at `output`, atomically through the caller's
/// staging path. Runs on a worker thread so a pathological file cannot hold
/// the indexer past [`TIMEOUT`]; the thread is detached on timeout and the
/// result discarded.
pub fn convert(source: &Path, output: &Path, cancel: impl Fn() -> bool) -> Result<Outcome> {
    if cancel() {
        return Err(Failure::new(FailureKind::Cancelled, "已停止转换，未发布输出").into());
    }
    let size = fs::metadata(source)?.len();
    if size > INPUT_LIMIT {
        return Err(Failure::new(FailureKind::Memory, "文档超过 512 MiB 转换输入限制").into());
    }
    let bytes = fs::read(source)?;
    let bytes = normalise_csv(source, bytes);
    let format = anydoc::Format::from_bytes(&bytes).or_else(|| anydoc::Format::from_path(source));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("anydoc".into())
        .spawn(move || {
            let _ = tx.send(anydoc::to_markdown_bytes(&bytes, format));
        })?;
    let started = std::time::Instant::now();
    let result = loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(result) => break result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if cancel() {
                    return Err(Failure::new(FailureKind::Cancelled, "已停止转换，未发布输出").into());
                }
                if started.elapsed() >= TIMEOUT {
                    return Err(Failure::new(FailureKind::Timeout, "内置转换超过 120 秒时间限制").into());
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Failure::new(FailureKind::Signal, "内置转换线程异常退出").into())
            }
        }
    };
    let markdown = match result {
        Ok(markdown) => markdown,
        Err(anydoc::ConvertError::NeedsOcr { .. }) => {
            let detail = result_message(&result);
            return Ok(Outcome::NeedsOcr(detail));
        }
        Err(error) => return Err(map_error(error).into()),
    };
    if markdown.len() > OUTPUT_LIMIT {
        return Err(Failure::new(FailureKind::Memory, "转换输出超过 64 MiB 限制").into());
    }
    if markdown.trim().is_empty() {
        return Err(Failure::new(FailureKind::Compatibility, "内置转换未提取到正文").into());
    }
    fs::write(output, markdown)?;
    Ok(Outcome::Converted)
}

fn result_message(result: &std::result::Result<String, anydoc::ConvertError>) -> String {
    match result {
        Err(error) => error.to_string(),
        Ok(_) => String::new(),
    }
}

fn map_error(error: anydoc::ConvertError) -> Failure {
    use anydoc::ConvertError::*;
    match error {
        Unsupported(detail) => Failure::new(FailureKind::Compatibility, format!("内置转换不支持该文件：{detail}")),
        Malformed { .. } | MissingPart { .. } => Failure::new(FailureKind::Compatibility, format!("文档结构无法读取：{error}")),
        Encrypted => Failure::new(FailureKind::Compatibility, "文档已加密，无法转换"),
        ResourceLimit { .. } => Failure::new(FailureKind::Memory, format!("文档超过安全限制：{error}")),
        Io(inner) => Failure::new(FailureKind::Io, inner.to_string()),
        _ => Failure::new(FailureKind::Compatibility, error.to_string()),
    }
}

/// anydoc decodes non-UTF-8 CSV as Windows-1252, which garbles GBK files.
/// Re-encode with the same detection the indexer uses for text files.
fn normalise_csv(source: &Path, bytes: Vec<u8>) -> Vec<u8> {
    let csv = source.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("csv"));
    if !csv || std::str::from_utf8(&bytes).is_ok() {
        return bytes;
    }
    match crate::document_text::read(source, &bytes) {
        Ok(text) => text.into_bytes(),
        Err(_) => bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn docx(paragraph: &str) -> Vec<u8> {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#).unwrap();
        zip.start_file("_rels/.rels", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#).unwrap();
        zip.start_file("word/document.xml", options).unwrap();
        write!(zip, r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>{paragraph}</w:t></w:r></w:p></w:body></w:document>"#).unwrap();
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn converts_word_documents_in_process() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("note.docx");
        fs::write(&source, docx("型号 BCD-215 内置转换")).unwrap();
        let output = dir.path().join("note.md");
        assert!(matches!(convert(&source, &output, || false).unwrap(), Outcome::Converted));
        assert!(fs::read_to_string(&output).unwrap().contains("BCD-215"));
    }
    #[test]
    fn converts_gbk_csv_without_garbling() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("list.csv");
        let (gbk, _, _) = encoding_rs::GBK.encode("型号,价格\nBCD-215,1999\n");
        fs::write(&source, gbk.as_ref()).unwrap();
        let output = dir.path().join("list.md");
        convert(&source, &output, || false).unwrap();
        let text = fs::read_to_string(&output).unwrap();
        assert!(text.contains("型号") && text.contains("BCD-215"), "{text}");
    }
    #[test]
    fn scanned_pdf_is_reported_as_needing_ocr_not_converted() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("scan.pdf");
        fs::write(&source, image_only_pdf()).unwrap();
        let output = dir.path().join("scan.md");
        match convert(&source, &output, || false).unwrap() {
            Outcome::NeedsOcr(message) => assert!(message.contains("OCR"), "{message}"),
            Outcome::Converted => panic!("image-only PDF must not convert"),
        }
        assert!(!output.exists());
    }
    #[test]
    fn unsupported_or_cancelled_inputs_fail_with_typed_errors() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("broken.docx");
        fs::write(&source, b"not a zip").unwrap();
        let output = dir.path().join("broken.md");
        let error = convert(&source, &output, || false).unwrap_err();
        assert_eq!(error.downcast_ref::<Failure>().unwrap().kind, FailureKind::Compatibility);
        let error = convert(&source, &output, || true).unwrap_err();
        assert_eq!(error.downcast_ref::<Failure>().unwrap().kind, FailureKind::Cancelled);
        assert!(handles(Path::new("a.PDF")) && handles(Path::new("b.xls")) && !handles(Path::new("c.png")));
    }
    fn image_only_pdf() -> Vec<u8> {
        use std::io::Write;
        let (w, h) = (20u32, 10u32);
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&vec![200u8; (w * h) as usize]).unwrap();
        let image = encoder.finish().unwrap();
        let content = b"q 20 0 0 10 0 0 cm /Im0 Do Q";
        let objects: Vec<Vec<u8>> = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 20 10] /Contents 4 0 R /Resources << /XObject << /Im0 5 0 R >> >> >>".to_vec(),
            [format!("<< /Length {} >>stream\n", content.len()).into_bytes(), content.to_vec(), b"\nendstream".to_vec()].concat(),
            [format!("<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>stream\n", image.len()).into_bytes(), image, b"\nendstream".to_vec()].concat(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = vec![];
        for (index, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n", index + 1).into_bytes());
            out.extend(object);
            out.extend(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).into_bytes());
        for offset in offsets {
            out.extend(format!("{offset:010} 00000 n \n").into_bytes());
        }
        out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).into_bytes());
        out
    }
}
