use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use pdfium_render::prelude::*;
use sag_protocol::{
    types::{DocumentParser, Settings},
    ConversionSegment,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

const PDF_PROMPT: &str = include_str!("../../../protocol/src/pdf-recognition-prompt.txt");
const IMAGE_PROMPT: &str = include_str!("../../../protocol/src/image-recognition-prompt.txt");
const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

fn format(source: &Path) -> Option<&'static str> {
    match source.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "pdf" => Some("application/pdf"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        _ => None,
    }
}

pub(crate) fn selected(source: &Path, settings: &Settings) -> bool {
    settings.document_parser == Some(DocumentParser::Vision) && format(source).is_some()
}

fn recognition_prompt(pdf: bool, settings: &Settings) -> (&str, &'static str) {
    let (custom, default) = if pdf {
        (&settings.document_parser_pdf_prompt, PDF_PROMPT)
    } else {
        (&settings.document_parser_image_prompt, IMAGE_PROMPT)
    };
    let default = default.trim_end();
    match custom.trim() {
        "" => (default, default),
        prompt => (prompt, default),
    }
}

pub(super) fn identity(source: &Path, settings: &Settings) -> String {
    let mut hash = Sha256::new();
    hash.update(settings.answer.url.trim());
    hash.update([0]);
    hash.update(&settings.document_parser_model);
    let (prompt, default) = recognition_prompt(format(source) == Some("application/pdf"), settings);
    if prompt != default {
        hash.update(b"\0prompt\0");
        hash.update(prompt);
    }
    format!("vision-v1:{:x}", hash.finalize())
}

pub(crate) fn library_path() -> Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let directory = executable.parent().context("无法确定 PDFium 组件位置")?;
    Ok(directory
        .join("pdfium")
        .join(Pdfium::pdfium_platform_library_name()))
}

fn pdfium(path: &Path) -> Result<&'static Pdfium> {
    static LIBRARY: OnceLock<Pdfium> = OnceLock::new();
    static LOAD: Mutex<()> = Mutex::new(());
    let _load = LOAD.lock().unwrap();
    if let Some(library) = LIBRARY.get() {
        return Ok(library);
    }
    let bindings =
        Pdfium::bind_to_library(path).context("无法加载 PDFium 渲染组件，请检查安装包")?;
    Ok(LIBRARY.get_or_init(|| Pdfium::new(bindings)))
}

fn page_image(page: &PdfPage<'_>) -> Result<Vec<u8>> {
    let width = page.width().value;
    let height = page.height().value;
    ensure!(
        width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0,
        "PDF 页面尺寸无效"
    );
    let scale = (200.0 / 72.0_f32).min(2400.0 / width.max(height));
    let bitmap = page
        .render_with_config(
            &PdfRenderConfig::new()
                .set_target_size(
                    (width * scale).round().max(1.0) as i32,
                    (height * scale).round().max(1.0) as i32,
                )
                .set_format(PdfBitmapFormat::BGRA)
                .set_clear_color(PdfColor::WHITE),
        )
        .context("PDF 页面渲染失败")?;
    let mut image = Vec::new();
    let mut encoder = png::Encoder::new(&mut image, bitmap.width() as u32, bitmap.height() as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder
        .write_header()?
        .write_image_data(&bitmap.as_rgba_bytes())?;
    Ok(image)
}

pub(super) fn convert(
    source: &Path,
    output: &Path,
    data: &Path,
    settings: &Settings,
    cancelled: &impl Fn() -> bool,
    mut progress: impl FnMut(ConversionSegment),
) -> Result<()> {
    let mut config = settings.answer.clone();
    config.model = settings.document_parser_model.clone();
    config.reasoning_effort = None;
    ensure!(
        settings.answer.models.contains(&config.model),
        "解析模型必须从可选问答模型中选择"
    );
    let key = crate::models::secret(data, "answer")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let client = crate::model_http::ModelHttp::default().client(&settings.model_proxy)?;
    let cancel = CancellationToken::new();
    let recognize = |bytes: &[u8], mime: &str, prompt: &str| -> Result<String> {
        ensure!(!cancelled(), "已停止转换，未发布输出");
        ensure!(
            bytes.len() <= MAX_IMAGE_BYTES,
            "模型解析图片不得超过 20 MiB"
        );
        let url = format!("data:{mime};base64,{}", STANDARD.encode(bytes));
        let text = runtime.block_on(async {
            let check = async {
                let mut interval = tokio::time::interval(Duration::from_millis(100));
                loop {
                    interval.tick().await;
                    if cancelled() {
                        cancel.cancel();
                        return;
                    }
                }
            };
            tokio::select! {
                result = crate::models::recognize_image(&client, &config, &key, &url, prompt,
                    settings.timeout_secs, &settings.runtime, &cancel) => result,
                _ = check => anyhow::bail!("已停止转换，未发布输出"),
            }
        })?;
        let text = text.trim();
        Ok(if text == "[NO_READABLE_TEXT]" {
            String::new()
        } else {
            text.to_owned()
        })
    };
    let mime = format(source).context("不支持此多模态解析格式")?;
    let (prompt, _) = recognition_prompt(mime == "application/pdf", settings);
    let limit = settings.runtime.conversion_output_mib as usize * 1024 * 1024;
    let mut markdown = String::new();
    if mime == "application/pdf" {
        let library = library_path()?;
        let document = pdfium(&library)?
            .load_pdf_from_file(source, None)
            .context("PDFium 无法打开 PDF")?;
        let total = document.pages().len();
        ensure!(
            total > 0 && total as usize <= settings.runtime.max_document_pages,
            "PDF 页数为空或超过配置上限"
        );
        for index in 0..total {
            ensure!(!cancelled(), "已停止转换，未发布输出");
            let number = index as u32 + 1;
            progress(ConversionSegment {
                current: number as usize,
                total: total as usize,
                start_page: number,
                end_page: number,
            });
            let page = document
                .pages()
                .get(index)
                .with_context(|| format!("无法读取 PDF 第 {number} 页"))?;
            let image =
                page_image(&page).with_context(|| format!("无法渲染 PDF 第 {number} 页"))?;
            let text = recognize(&image, "image/png", prompt)
                .with_context(|| format!("PDF 第 {number} 页识别失败"))?;
            if !text.is_empty() {
                use std::fmt::Write;
                write!(
                    &mut markdown,
                    "\n\n<!-- source-pages:{number}-{number} -->\n\n{text}\n"
                )?;
            }
            ensure!(markdown.len() <= limit, "转换结果超过配置的大小限制");
        }
    } else {
        use std::io::Read;
        let mut image = Vec::new();
        fs::File::open(source)?
            .take(MAX_IMAGE_BYTES as u64 + 1)
            .read_to_end(&mut image)?;
        ensure!(
            image.len() <= MAX_IMAGE_BYTES,
            "模型解析图片不得超过 20 MiB"
        );
        ensure!(
            match mime {
                "image/png" => image.starts_with(b"\x89PNG\r\n\x1a\n"),
                _ => image.starts_with(b"\xff\xd8\xff"),
            },
            "图片内容与扩展名不符"
        );
        markdown = recognize(&image, mime, prompt)?;
    }
    ensure!(!markdown.trim().is_empty(), "模型未识别出可索引的内容");
    ensure!(markdown.len() <= limit, "转换结果超过配置的大小限制");
    fs::write(output, markdown.trim_start())?;
    Ok(())
}
