use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, UTF_8};
use regex::Regex;
use std::{borrow::Cow, fmt, sync::LazyLock};

pub fn read(path: &std::path::Path, bytes: &[u8]) -> Result<String, DecodeError> {
    Ok(read_borrowed(path, bytes)?.into_owned())
}

pub(crate) fn read_borrowed<'a>(path: &std::path::Path, bytes: &'a [u8]) -> Result<Cow<'a, str>, DecodeError> {
    if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("rtf")) {
        rtf_text(bytes).map(Cow::Owned)
    } else {
        Ok(decode(bytes)?.body)
    }
}

#[cfg(target_os = "macos")]
fn rtf_text(bytes: &[u8]) -> Result<String, DecodeError> {
    use objc2::{rc::autoreleasepool, AnyThread};
    use objc2_app_kit::NSAttributedStringAppKitDocumentFormats;
    use objc2_foundation::{NSAttributedString, NSData};

    if !bytes.starts_with(br"{\rtf") {
        return Err(DecodeError("RTF 格式无效，无法读取正文".into()));
    }
    autoreleasepool(|_| {
        let data = NSData::with_bytes(bytes);
        // The system RTF reader handles document code pages and Unicode escapes.
        // No document attributes or file URL are supplied; the source is never written.
        let document = unsafe {
            NSAttributedString::initWithRTF_documentAttributes(NSAttributedString::alloc(), &data, None)
        }.ok_or_else(|| DecodeError("RTF 格式无效，无法读取正文".into()))?;
        // RTF soft line breaks use Unicode separators; normalize them so the
        // displayed lines and the index's line/column coordinates agree.
        Ok(document.string().to_string().replace(['\u{2028}', '\u{2029}'], "\n"))
    })
}

#[cfg(not(target_os = "macos"))]
fn rtf_text(_: &[u8]) -> Result<String, DecodeError> {
    Err(DecodeError("RTF 正文读取目前仅支持 macOS".into()))
}

static DECLARATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r#"(?i)(?:\A(?:[^\r\n]*\r?\n)?[ \t]*\#[^\r\n]*?coding\s*[:=]\s*|<meta\b[^>]*?charset\s*=\s*["']?|\A<\?xml\b[^>]*?encoding\s*=\s*["'])([a-z0-9._-]+)"#
).unwrap());

#[derive(Debug)]
pub struct DecodeError(pub String);
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for DecodeError {}

pub struct DecodedText<'a> {
    pub body: Cow<'a, str>,
    encoding: &'static Encoding,
    bom: Vec<u8>,
}

pub fn decode(bytes: &[u8]) -> Result<DecodedText<'_>, DecodeError> {
    let (encoding, bom_len) = if let Some(bom) = Encoding::for_bom(bytes) {
        bom
    } else {
        let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
        let lower = prefix.to_ascii_lowercase();
        let declared = if lower.contains("coding") || lower.contains("charset") || lower.contains("encoding") {
            DECLARATION.captures(&prefix)
        } else { None };
        let encoding = if let Some(captures) = declared {
            let label = &captures[1];
            Encoding::for_label_no_replacement(label.as_bytes())
                .ok_or_else(|| DecodeError(format!("不支持文件声明的编码：{label}")))?
        } else if let Ok(body) = std::str::from_utf8(bytes) {
            return Ok(DecodedText { body: Cow::Borrowed(body), encoding: UTF_8, bom: vec![] });
        } else {
            let mut detector = EncodingDetector::new(Iso2022JpDetection::Allow);
            detector.feed(bytes, true);
            detector.guess(None, Utf8Detection::Deny)
        };
        (encoding, 0)
    };
    let body = encoding.decode_without_bom_handling_and_without_replacement(&bytes[bom_len..])
        .ok_or_else(|| DecodeError(format!("文件无法按 {0} 编码读取", encoding.name())))?;
    Ok(DecodedText { body, encoding, bom: bytes[..bom_len].to_vec() })
}

impl DecodedText<'_> {
    pub fn encode(&self, text: &str) -> Result<Vec<u8>, DecodeError> {
        let mut bytes = self.bom.clone();
        if self.encoding == UTF_16LE {
            bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        } else if self.encoding == UTF_16BE {
            bytes.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
        } else {
            let (encoded, _, errors) = self.encoding.encode(text);
            if errors {
                return Err(DecodeError(format!("新增字符无法用 {0} 编码保存，原文件未修改", self.encoding.name())));
            }
            bytes.extend_from_slice(&encoded);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn rtf_handles_legacy_chinese_code_pages_and_unicode_fallbacks() {
        for (bytes, expected) in [
            (br"{\rtf1\ansi\ansicpg936{\fonttbl{\f0\fcharset134 SimSun;}}\f0 \'d6\'d0\'ce\'c4}".as_slice(), "中文"),
            (br"{\rtf1\ansi\ansicpg950{\fonttbl{\f0\fcharset136 MingLiU;}}\f0 \'a4\'a4\'a4\'e5}".as_slice(), "中文"),
            (br"{\rtf1\ansi\uc2 \u26143??\u27827?? \{ok\} \\ \tab done\line end}".as_slice(), "星河 {ok} \\ \tdone\nend"),
        ] {
            assert_eq!(read(std::path::Path::new("test.rtf"), bytes).unwrap(), expected);
        }
        assert!(read(std::path::Path::new("test.rtf"), b"not an RTF document").is_err());
    }

    #[test]
    fn malformed_and_unknown_declared_encodings_are_rejected() {
        for bytes in [b"# coding: utf-8\n\xff".as_slice(), &[0xff, 0xfe, 0x00], b"# coding: unknown\ntext"] {
            assert!(decode(bytes).is_err());
        }
    }

    #[test]
    fn mixed_case_encoding_declarations_keep_their_original_meaning() {
        let original = "<META CHARSET='big5'>繁體中文";
        let encoded = encoding_rs::BIG5.encode(original).0;
        assert_eq!(decode(&encoded).unwrap().body, original);
        assert!(decode(b"# CoDiNg: unknown\nvalid utf8").is_err());
        assert!(decode(b"<?xml EnCoDiNg='utf-8'?>\xff").is_err());
    }

    #[test]
    fn source_code_can_contain_literal_control_characters() {
        let source = "const pattern = /[\u{0000}-\u{001f}]/;";
        assert_eq!(decode(source.as_bytes()).unwrap().body, source);
    }

    #[test]
    fn valid_unicode_text_and_bom_round_trip_without_changes() {
        for bytes in ["汉字 😀\r\ntext".as_bytes(), "\u{feff}汉字 😀\r\ntext".as_bytes()] {
            let decoded = decode(bytes).unwrap();
            assert_eq!(decoded.body, "汉字 😀\r\ntext");
            assert_eq!(decoded.encode(&decoded.body).unwrap(), bytes);
        }
    }

    #[test]
    fn legacy_encoding_is_preserved_and_unmappable_edits_fail() {
        let original = "# coding: big5\n繁體中文\r\n";
        let bytes = encoding_rs::BIG5.encode(original).0;
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.body, original);
        assert_eq!(decoded.encode(original).unwrap(), bytes.as_ref());
        assert!(decoded.encode("😀").is_err());
    }
}
