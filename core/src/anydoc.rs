//! Built-in document conversion with the anydoc crate (MIT).
//!
//! Handles PDF, Word, PowerPoint, Excel/CSV, OpenDocument and EPUB in
//! process without any downloaded component. It does not do OCR: a scanned
//! PDF is reported as [`Outcome::NeedsOcr`] so the caller can hand it to the
//! Docling component when that is installed.
use crate::process_supervisor::{Failure, FailureKind};
use anyhow::Result;
use std::{fs, path::Path, time::Duration};

/// Extensions the built-in converter owns. Everything else that Docling can
/// read stays with Docling.
pub const EXTENSIONS: &[&str] = &[
    "pdf", "doc", "docx", "docm", "ppt", "pps", "pot", "pptx", "pptm", "ppsx", "ppsm",
    "xls", "xlsx", "xlsm", "xlsb", "csv", "rtf", "odt", "ods", "odp", "epub",
];
/// Bump when the parser or our normalization changes so cached conversions are rebuilt.
pub const CACHE_VERSION: &str = "anydoc-0.2.4:adapter-1";


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
/// the indexer past the configured timeout; the thread is detached on timeout and the
/// result discarded.
pub fn convert(source: &Path, output: &Path, cancel: impl Fn() -> bool) -> Result<Outcome> {
    convert_with_runtime(source, output, &Default::default(), cancel)
}
pub fn convert_with_runtime(source: &Path, output: &Path, runtime: &sag_protocol::runtime::RuntimeSettings, cancel: impl Fn() -> bool) -> Result<Outcome> {
    if cancel() {
        return Err(Failure::new(FailureKind::Cancelled, "已停止转换，未发布输出").into());
    }
    let size = fs::metadata(source)?.len();
    if size > (runtime.conversion_input_mib as u64) * 1024 * 1024 {
        return Err(Failure::new(FailureKind::Memory, format!("文档超过 {} MiB 转换输入限制", runtime.conversion_input_mib)).into());
    }
    let bytes = fs::read(source)?;
    let bytes = normalise_csv(source, bytes);
    let format = anydoc::Format::from_bytes(&bytes).or_else(|| anydoc::Format::from_path(source));
    if matches!(format, Some(anydoc::Format::Pdf)) {
        let pdf = lopdf::Document::load_mem(&bytes).map_err(|error|
            Failure::new(FailureKind::Compatibility, format!("无法检查 PDF 页数：{error}")))?;
        let pages = pdf.get_pages().len();
        if pages > runtime.max_document_pages {
            return Err(Failure::new(FailureKind::Memory,
                format!("PDF 页数超过配置上限（{pages} > {}）", runtime.max_document_pages)).into());
        }
        // Skip image decoding only when neither fonts nor possible text/marked
        // content are present. ActualText can supply accessible text without fonts.
        if !pdf_has_text_resources(&pdf) {
            return Ok(Outcome::NeedsOcr("PDF 没有文字字体资源，需要 OCR".into()));
        }
    }
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
                if started.elapsed() >= Duration::from_secs(runtime.anydoc_timeout_secs) {
                    return Err(Failure::new(FailureKind::Timeout, format!("内置转换超过 {} 秒时间限制", runtime.anydoc_timeout_secs)).into());
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
    if markdown.len() > runtime.conversion_output_mib * 1024 * 1024 {
        return Err(Failure::new(FailureKind::Memory, format!("转换输出超过 {} MiB 限制", runtime.conversion_output_mib)).into());
    }
    if markdown.trim().is_empty() {
        return Err(Failure::new(FailureKind::Compatibility, "内置转换未提取到正文").into());
    }
    fs::write(output, markdown)?;
    Ok(Outcome::Converted)
}

fn pdf_has_text_resources(pdf: &lopdf::Document) -> bool {
    use lopdf::Object;
    let mut pending: Vec<&Object> = pdf.objects.values().collect();
    let mut streams = Vec::new();
    while let Some(object) = pending.pop() {
        let dictionary = match object {
            Object::Dictionary(dictionary) => dictionary,
            Object::Stream(stream) => {
                streams.push(stream);
                &stream.dict
            }
            Object::Array(values) => {
                pending.extend(values);
                continue;
            }
            _ => continue,
        };
        if dictionary.get(b"Font").is_ok()
            || dictionary.get(b"ActualText").is_ok()
            || matches!(dictionary.get(b"Type"), Ok(Object::Name(name)) if name == b"Font")
        {
            return true;
        }
        pending.extend(dictionary.iter().map(|(_, value)| value));
    }
    streams.into_iter().any(stream_may_contain_text)
}

fn stream_may_contain_text(stream: &lopdf::Stream) -> bool {
    use std::io::Read;
    if matches!(stream.dict.get(b"Subtype"), Ok(lopdf::Object::Name(name)) if name == b"Image") {
        return false;
    }
    // Never expand an image or an unbounded content stream for this shortcut.
    // Unknown encodings/parameters retain the existing native parser path.
    const LIMIT: u64 = 1024 * 1024;
    let mut decoded = Vec::new();
    let bytes = match stream.dict.get(b"Filter") {
        Err(_) => stream.content.as_slice(),
        Ok(lopdf::Object::Name(name))
            if name == b"FlateDecode" && stream.dict.get(b"DecodeParms").is_err() =>
        {
            if flate2::read::ZlibDecoder::new(stream.content.as_slice())
                .take(LIMIT + 1)
                .read_to_end(&mut decoded)
                .is_err()
            {
                return true;
            }
            decoded.as_slice()
        }
        _ => return true,
    };
    bytes.len() as u64 > LIMIT
        || [b"BT".as_slice(), b"Tj", b"TJ", b"BDC", b"DP"]
            .iter()
            .any(|marker| bytes.windows(marker.len()).any(|part| part == *marker))
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
        // Docling cannot open it either; do not hand off.
        Encrypted => Failure::new(FailureKind::Exit, "文档已加密，无法转换"),
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
    #[test]
    fn configured_input_limit_is_applied_before_parsing() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("large.pdf");
        fs::File::create(&source).unwrap().set_len(2 * 1024 * 1024).unwrap();
        let mut runtime = sag_protocol::runtime::RuntimeSettings::default();
        runtime.conversion_input_mib = 1;
        let error = convert_with_runtime(&source, &dir.path().join("out.md"), &runtime, || false).unwrap_err();
        assert!(error.to_string().contains("1 MiB"));
        assert!(!dir.path().join("out.md").exists());
    }

    #[test]
    fn review_pdf_page_limit_rejects_full_document_without_overwriting_output() {
        use lopdf::{dictionary, Document, Object, Stream};
        let mut pdf=Document::with_version("1.5");let pages_id=pdf.new_object_id();
        let font=pdf.add_object(dictionary!{"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
        let mut kids=Vec::<Object>::new();
        for text in ["first page evidence","second page evidence"] {
            let content=pdf.add_object(Stream::new(dictionary!{},format!("BT /F1 12 Tf 10 50 Td ({text}) Tj ET").into_bytes()));
            let page=pdf.add_object(dictionary!{"Type"=>"Page","Parent"=>pages_id,"MediaBox"=>vec![0.into(),0.into(),200.into(),100.into()],"Contents"=>content,"Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}}});kids.push(page.into());
        }
        pdf.objects.insert(pages_id,dictionary!{"Type"=>"Pages","Kids"=>kids,"Count"=>2}.into());
        let catalog=pdf.add_object(dictionary!{"Type"=>"Catalog","Pages"=>pages_id});pdf.trailer.set("Root",catalog);
        let mut bytes=vec![];pdf.save_to(&mut bytes).unwrap();let dir=tempfile::tempdir().unwrap();let source=dir.path().join("two.pdf");fs::write(&source,bytes).unwrap();
        let mut runtime=sag_protocol::runtime::RuntimeSettings::default();runtime.max_document_pages=2;
        let allowed=dir.path().join("allowed.md");assert!(matches!(convert_with_runtime(&source,&allowed,&runtime,||false).unwrap(),Outcome::Converted));
        assert!(fs::read_to_string(&allowed).unwrap().contains("second page evidence"));
        runtime.max_document_pages=1;let blocked=dir.path().join("blocked.md");fs::write(&blocked,"previous result").unwrap();
        let error=convert_with_runtime(&source,&blocked,&runtime,||false).unwrap_err();
        assert!(error.to_string().contains("页数"));assert_eq!(fs::read_to_string(blocked).unwrap(),"previous result");
    }

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

#[cfg(test)]
mod pdf_text_resource_tests {
    use super::*;
    #[test]
    fn no_fonts_means_ocr_but_inline_and_form_fonts_keep_native_parsing() {
        use lopdf::{dictionary, Document, Object, Stream};
        let mut doc = Document::with_version("1.5");
        doc.add_object(Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Image"},
            vec![0; 100],
        ));
        assert!(!pdf_has_text_resources(&doc));
        let mut inline = doc.clone();
        inline.add_object(dictionary!{"Type"=>"Page","Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>dictionary!{"Subtype"=>"Type1","BaseFont"=>"Helvetica"}}}});
        assert!(pdf_has_text_resources(&inline));
        doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","Resources"=>dictionary!{"Font"=>Object::Reference((99,0))}},vec![]));
        assert!(
            pdf_has_text_resources(&doc),
            "form resources must not be misclassified as image-only"
        );
    }
}

#[cfg(test)]
#[test]
fn fontless_actual_text_keeps_the_native_parser() {
    use lopdf::{dictionary, Document, Object, Stream};
    let mut indirect = Document::with_version("1.5");
    indirect
        .add_object(dictionary! {"ActualText"=>Object::string_literal("Accessible outline text")});
    assert!(pdf_has_text_resources(&indirect));
    let mut inline = Document::with_version("1.5");
    let mut stream = Stream::new(
        dictionary! {},
        b"/Span << /ActualText (Accessible outline text) >> BDC /Image1 Do EMC ".repeat(20),
    );
    stream.compress().unwrap();
    assert!(stream.dict.get(b"Filter").is_ok());
    inline.add_object(stream);
    assert!(pdf_has_text_resources(&inline));
}

#[cfg(test)]
#[test]
#[ignore = "requires SAG_PDF_OCR_PROBE_SOURCE pointing to an image-only PDF"]
fn real_image_only_pdf_hands_off_without_native_decoding() {
    let source=std::path::PathBuf::from(std::env::var("SAG_PDF_OCR_PROBE_SOURCE").unwrap());
    let dir=tempfile::tempdir().unwrap();
    let started=std::time::Instant::now();
    let result=convert(&source,&dir.path().join("output.md"),||false).unwrap();
    assert!(matches!(result,Outcome::NeedsOcr(_)));
    assert!(started.elapsed()<Duration::from_secs(10),"image-only handoff must not spend the native conversion timeout");
    println!("image-only PDF OCR handoff_ms={}",started.elapsed().as_millis());
}
