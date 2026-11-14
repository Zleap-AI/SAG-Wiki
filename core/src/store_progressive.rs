//! Durable basic PDF text publication. No images are decoded in this pass.
use super::*;

impl Store {
    pub(super) fn save_conversion_ownership(&self, root: &str) -> Result<()> {
        Self::save_conversion_ownership_from(&self.connection(), root)
    }
    pub(super) fn save_conversion_ownership_from(db: &Connection, root: &str) -> Result<()> {
        let metadata_path =
            crate::docling::output_path(Path::new(root), &Path::new(root).join(".ownership"))?
                .parent()
                .unwrap()
                .join(".sag-converted.json");
        if !metadata_path.parent().unwrap().is_dir() {
            return Ok(());
        }
        anyhow::ensure!(
            !metadata_path
                .symlink_metadata()
                .is_ok_and(|metadata| metadata.file_type().is_symlink()),
            ".sag-converted 转换记录不能是符号链接"
        );
        let records = {
            let mut query =
                db.prepare("SELECT source,source_hash,output_hash FROM conversions WHERE root=?")?;
            let rows = query
                .query_map([root], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows.into_iter()
                .map(|(source, source_hash, output_hash)| {
                    Ok((
                        Path::new(&source)
                            .strip_prefix(root)?
                            .to_string_lossy()
                            .into_owned(),
                        (source_hash, output_hash),
                    ))
                })
                .collect::<Result<std::collections::BTreeMap<_, _>>>()?
        };
        let mut file = tempfile::NamedTempFile::new_in(metadata_path.parent().unwrap())?;
        serde_json::to_writer(file.as_file_mut(), &records)?;
        file.persist(&metadata_path).map_err(|error| error.error)?;
        Ok(())
    }
    pub fn index_status(&self) -> Result<sag_protocol::IndexStatus> {
        let mut status = self.indexing.status();
        status.failed_files = self.failed_index_count()?;
        let db = self.connection();
        let mut query = db.prepare("SELECT s.source,s.root,s.status,EXISTS(SELECT 1 FROM conversions c JOIN documents d ON d.path=c.output WHERE c.source=s.source AND c.valid=1),s.error FROM conversion_states s WHERE s.status!='complete' ORDER BY s.root,s.source")?;
        status.conversion_states = query
            .query_map([], |row| {
                Ok(sag_protocol::ConversionState {
                    source: row.get(0)?,
                    root: row.get(1)?,
                    status: row.get(2)?,
                    searchable: row.get(3)?,
                    error: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(status)
    }
    pub(crate) fn has_pending_enhancement(&self, root: &str) -> Result<bool> {
        Ok(self.connection().query_row("SELECT EXISTS(SELECT 1 FROM conversion_states WHERE root=? AND status IN ('basic','enhancing','awaiting_ocr','awaiting_conversion'))", [root], |row| row.get(0))?)
    }
    pub(super) fn conversion_state(
        &self,
        source: &str,
        root: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        self.connection().execute("INSERT INTO conversion_states(source,root,status,error) SELECT ?1,path,?3,?4 FROM roots WHERE path=?2 AND NOT EXISTS(SELECT 1 FROM root_exclusions e WHERE e.path=?1 OR instr(?1,e.path || ?5)=1) ON CONFLICT(source) DO UPDATE SET root=excluded.root,status=excluded.status,error=excluded.error", params![source, root, status, error, std::path::MAIN_SEPARATOR.to_string()])?;
        Ok(())
    }
    /// Finish local built-in formats without queueing behind OCR/table work.
    /// Cloud-selected formats retain their configured parser order unless text-only is required.
    fn publish_builtin_source(
        &self,
        root: &str,
        source: &Path,
        settings: &Settings,
    ) -> Result<bool> {
        if !crate::anydoc::handles(source)
            || (!crate::anydoc::office_text_only(source, &settings.runtime)
                && crate::conversion::cloud_supports(source, &settings.mineru))
        {
            return Ok(false);
        }
        let metadata = fs::metadata(source)?;
        let stat = (
            metadata.modified()?.duration_since(UNIX_EPOCH)?.as_nanos() as i64,
            metadata.len() as i64,
        );
        let fingerprint = crate::docling::fingerprint(source)?;
        let output = crate::docling::output_path(Path::new(root), source)?;
        let previous: Option<String> = self
            .connection()
            .query_row(
                "SELECT output_hash FROM conversions WHERE source=?",
                [path_text(source)?],
                |row| row.get(0),
            )
            .optional()?;
        fs::create_dir_all(output.parent().unwrap())?;
        let staged = tempfile::Builder::new()
            .prefix(".sag-basic-")
            .suffix(".md")
            .tempfile_in(output.parent().unwrap())?
            .into_temp_path();
        match crate::anydoc::convert_with_runtime(source, &staged, &settings.runtime, || {
            self.indexing.cancelled(root)
        })? {
            crate::anydoc::Outcome::NeedsOcr(_) => return Ok(false),
            crate::anydoc::Outcome::Converted => {}
        }
        let identity = crate::conversion::identity(source, settings);
        let hash = if identity.is_empty() {
            fingerprint.clone()
        } else {
            format!("{identity}:{fingerprint}")
        };
        self.publish_converted_file(
            root,
            source,
            &hash,
            &fingerprint,
            stat,
            &output,
            staged,
            previous.as_deref(),
            settings.retrieval.chunk_chars,
            &settings.runtime,
        )?;
        self.conversion_state(&path_text(source)?, root, "complete", None)?;
        Ok(true)
    }
    pub(super) fn publish_basic_sources(
        &self,
        root: &str,
        sources: &[PathBuf],
        settings: &Settings,
        progress: &impl Fn(IndexProgress),
    ) -> Result<()> {
        if sources.is_empty() {
            return self.save_conversion_ownership(root);
        }
        self.indexing.phase(root, "extracting_basic", sources.len());
        progress(IndexProgress {
            count: 0,
            total: sources.len(),
        });
        for (index, source) in sources.iter().enumerate() {
            self.indexing.checkpoint_root(root)?;
            if self.defer_active_enhancement(root, source, settings)? {
                continue;
            }
            let metadata = fs::metadata(source)?;
            let mtime = metadata.modified()?.duration_since(UNIX_EPOCH)?.as_nanos() as i64;
            let remembered: Option<(String, String)> = self.connection().query_row(
                "SELECT source_hash,error FROM conversion_failures WHERE source=?1 AND mtime=?2 AND size=?3",
                params![path_text(source)?,mtime,metadata.len() as i64],
                |row| Ok((row.get(0)?,row.get(1)?)),
            ).optional()?;
            if let Some((hash, error)) = remembered {
                let tools = self.conversion_tools(crate::docling::required_profile(source));
                let identity = crate::conversion::failure_identity(source, settings, &tools);
                if hash == format!("{identity}:{}", hash.rsplit(':').next().unwrap_or(&hash)) {
                    let source = path_text(source)?;
                    self.conversion_state(&source, root, "enhancement_failed", Some(&error))?;
                    self.connection().execute("INSERT INTO index_issues(path,root,error) VALUES(?,?,?) ON CONFLICT(path) DO UPDATE SET root=excluded.root,error=excluded.error", params![source,root,error])?;
                    continue;
                }
            }
            if !source
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
            {
                let stat = fs::metadata(source)?;
                let mtime = stat.modified()?.duration_since(UNIX_EPOCH)?.as_nanos() as i64;
                let identity = crate::conversion::identity(source, settings);
                let previous: Option<(String,String)> = self.connection().query_row("SELECT source_hash,output FROM conversions WHERE source=?1 AND valid=1 AND mtime=?2 AND size=?3", params![path_text(source)?,mtime,stat.len() as i64], |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
                let current = previous.is_some_and(|(hash, output)| {
                    let content = hash.rsplit(':').next().unwrap_or(&hash);
                    let expected = if identity.is_empty() {
                        content.to_owned()
                    } else {
                        format!("{identity}:{content}")
                    };
                    hash == expected && Path::new(&output).is_file()
                });
                if !current {
                    let result = self.indexing.file(root, source, || {
                        self.publish_builtin_source(root, source, settings)
                    });
                    match result {
                        Ok(true) => {}
                        Ok(false) => self.conversion_state(
                            &path_text(source)?,
                            root,
                            "awaiting_conversion",
                            None,
                        )?,
                        Err(error) if self.indexing.cancelled(root) => return Err(error),
                        Err(error) => self.conversion_state(
                            &path_text(source)?,
                            root,
                            "awaiting_conversion",
                            Some(&format!("{error:#}")),
                        )?,
                    }
                }
                continue;
            }
            let source_text = path_text(source)?;
            let output = crate::docling::output_path(Path::new(root), source)?;
            let metadata = fs::metadata(source)?;
            let mtime = metadata.modified()?.duration_since(UNIX_EPOCH)?.as_nanos() as i64;
            let size = metadata.len() as i64;
            let identity = crate::conversion::identity(source, settings);
            let previous: Option<(String,String,bool,Option<i64>,Option<i64>)> = self.connection().query_row("SELECT source_hash,output_hash,valid,mtime,size FROM conversions WHERE source=?", [&source_text], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional()?;
            if let Some((hash, _, true, Some(old_mtime), Some(old_size))) = &previous {
                let expected = format!("{identity}:{}", hash.rsplit(':').next().unwrap_or(hash));
                if *old_mtime == mtime
                    && *old_size == size
                    && output.is_file()
                    && (hash == &expected
                        || hash == &format!("basic-v1:{expected}")
                        || hash == &format!("basic-text-v1:{expected}"))
                {
                    if hash.starts_with("basic-") {
                        self.conversion_state(&source_text, root, "basic", None)?;
                    }
                    continue;
                }
            }
            let result = self.indexing.file(root, source, || -> Result<()> {
                anyhow::ensure!(
                    size as u64 <= settings.runtime.conversion_input_mib as u64 * 1024 * 1024,
                    "文档超过配置的转换输入大小限制"
                );
                let fingerprint = crate::docling::fingerprint(source)?;
                if previous.as_ref().is_some_and(|(hash, _, _, _, _)| {
                    hash.rsplit(':').next() != Some(fingerprint.as_str())
                }) {
                    self.connection().execute(
                        "UPDATE conversions SET valid=0 WHERE source=?",
                        [&source_text],
                    )?;
                    self.remove_indexed(&path_text(&output)?)?;
                }
                // Imported ownership journals lack stat fields. Reuse an unchanged
                // complete structured result rather than downgrading it to basic text.
                if let Some((previous_source, previous_output, true, _, _)) = &previous {
                    let expected = if identity.is_empty() {
                        fingerprint.clone()
                    } else {
                        format!("{identity}:{fingerprint}")
                    };
                    if *previous_source == expected
                        && output.is_file()
                        && crate::docling::fingerprint(&output)? == *previous_output
                    {
                        if let Some(file) = self.prepare_indexed_file(
                            root,
                            &output,
                            settings.retrieval.chunk_chars,
                            false,
                            &settings.runtime,
                        )? {
                            let mut db = self.connection();
                            let tx = db.transaction()?;
                            Self::write_prepared(&tx, root, file)?;
                            tx.commit()?;
                        }
                        self.connection().execute(
                            "UPDATE conversions SET mtime=?2,size=?3 WHERE source=?1",
                            params![&source_text, mtime, size],
                        )?;
                        self.conversion_state(&source_text, root, "complete", None)?;
                        return Ok(());
                    }
                }
                let (body, needs_enhancement) =
                    basic_pdf_text(source, &settings.runtime, || self.indexing.cancelled(root))?;
                if body.is_empty() {
                    self.conversion_state(&source_text, root, "awaiting_ocr", None)?;
                    return Ok(());
                }
                fs::create_dir_all(output.parent().unwrap())?;
                let staged = tempfile::Builder::new()
                    .prefix(".sag-basic-")
                    .suffix(".md")
                    .tempfile_in(output.parent().unwrap())?
                    .into_temp_path();
                fs::write(&staged, body)?;
                let source_hash = if needs_enhancement {
                    format!("basic-v1:{identity}:{fingerprint}")
                } else {
                    format!("basic-text-v1:{identity}:{fingerprint}")
                };
                self.publish_converted_file(
                    root,
                    source,
                    &source_hash,
                    &fingerprint,
                    (mtime, size),
                    &output,
                    staged,
                    previous.as_ref().map(|(_, hash, _, _, _)| hash.as_str()),
                    settings.retrieval.chunk_chars,
                    &settings.runtime,
                )?;
                self.conversion_state(&source_text, root, "basic", None)?;
                Ok(())
            });
            if let Err(error) = result {
                if self.indexing.cancelled(root) {
                    return Err(error);
                }
                // Extraction incompatibility must still reach the normal parser chain.
                self.conversion_state(
                    &source_text,
                    root,
                    "awaiting_conversion",
                    Some(&format!("{error:#}")),
                )?;
            }
            progress(IndexProgress {
                count: index + 1,
                total: sources.len(),
            });
        }
        self.save_conversion_ownership(root)?;
        Ok(())
    }
}

fn basic_pdf_text(
    source: &Path,
    runtime: &sag_protocol::runtime::RuntimeSettings,
    cancel: impl Fn() -> bool,
) -> Result<(String, bool)> {
    let started = std::time::Instant::now();
    anyhow::ensure!(!cancel(), "已停止转换，未发布输出");
    let bytes = fs::read(source)?;
    let decoded_limit = runtime.max_decoded_text_mib * 1024 * 1024;
    let pdf = lopdf::Document::load_mem_with_options(
        &bytes,
        lopdf::LoadOptions::with_max_decompressed_size(decoded_limit),
    )?;
    let pages = pdf.get_pages();
    let page_count = pages.len();
    anyhow::ensure!(
        page_count <= runtime.max_document_pages,
        "PDF 页数超过配置上限"
    );
    // Conservatively validate every non-raster stream once before the
    // positional engine follows Forms, form-local fonts, or embedded font
    // programs. Reference targets are already in objects: do not follow them
    // again. Nested inline dictionaries/streams are visited without cycles.
    // Font programs and colour profiles are resources, not decoded prose.
    // Many legitimate PDFs embed more resource data than their text budget.
    // Keep each stream bounded, share the preparation budget across resources,
    // PDF drawing instructions are not prose: a vector-heavy document can have
    // large content streams with very little extracted text. Preserve the
    // bounded per-stream/per-page decoder and use the preparation budget for
    // retained resources. Page instruction buffers are transient, so summing
    // all pages would confuse total work with peak working memory.
    let page_content_ids: HashSet<_> = pages
        .values()
        .flat_map(|page| pdf.get_page_contents(*page))
        .collect();
    let mut remaining_resources = runtime.text_preparation_mib * 1024 * 1024;
    let mut pending: Vec<_> = pdf
        .objects
        .iter()
        .map(|(id, object)| (Some(*id), object))
        .collect();
    while let Some((id, object)) = pending.pop() {
        anyhow::ensure!(!cancel(), "已停止转换，未发布输出");
        anyhow::ensure!(
            started.elapsed().as_secs() < runtime.anydoc_timeout_secs,
            "基础文本提取超时"
        );
        match object {
            lopdf::Object::Stream(stream) => {
                let raster = matches!(stream.dict.get(b"Subtype"),Ok(lopdf::Object::Name(name)) if name==b"Image")
                    || stream.filters().is_ok_and(|filters| {
                        filters.iter().any(|filter| {
                            [
                                b"DCTDecode".as_slice(),
                                b"DCT",
                                b"JPXDecode",
                                b"JBIG2Decode",
                                b"CCITTFaxDecode",
                                b"CCF",
                            ]
                            .contains(filter)
                        })
                    });
                if raster {
                    continue;
                }
                let page_content = id.is_some_and(|id| page_content_ids.contains(&id));
                let limit = if page_content {
                    decoded_limit
                } else {
                    decoded_limit.min(remaining_resources)
                };
                let content = stream
                    .get_plain_content_with_limit(limit)
                    .with_context(|| {
                        format!(
                            "基础文本资源解码超过 {} MiB 限制或无法解码",
                            runtime.max_decoded_text_mib
                        )
                    })?;
                if !page_content {
                    remaining_resources = remaining_resources
                        .checked_sub(content.len())
                        .context("基础文本资源超过准备预算")?;
                }
                pending.extend(stream.dict.iter().map(|(_, value)| (None, value)));
            }
            lopdf::Object::Dictionary(dictionary) => {
                pending.extend(dictionary.iter().map(|(_, value)| (None, value)))
            }
            lopdf::Object::Array(values) => {
                pending.extend(values.iter().map(|value| (None, value)))
            }
            _ => {}
        }
    }
    let mut fonts = HashSet::new();
    for page in pages.values() {
        anyhow::ensure!(!cancel(), "已停止转换，未发布输出");
        anyhow::ensure!(
            started.elapsed().as_secs() < runtime.anydoc_timeout_secs,
            "基础文本提取超时"
        );
        // A page can concatenate several streams, so retain its combined bound
        // too. These bytes were already charged above; do not count them twice.
        pdf.get_page_content_with_limit(*page, decoded_limit)
            .with_context(|| {
                format!(
                    "基础文本页面解码超过 {} MiB 限制或无法解码",
                    runtime.max_decoded_text_mib
                )
            })?;
        for font in pdf.get_page_fonts(*page)?.values() {
            if fonts.insert((*font as *const lopdf::Dictionary) as usize) {
                if let Err(lopdf::Error::Decompress(error)) =
                    font.get_font_encoding_with_limit(&pdf, decoded_limit)
                {
                    return Err(anyhow::Error::new(error).context(format!(
                        "基础文本字体解码超过 {} MiB 限制或无法解码",
                        runtime.max_decoded_text_mib
                    )));
                }
            }
        }
    }
    drop(pdf);
    let output_limit = runtime
        .conversion_output_mib
        .min(runtime.max_decoded_text_mib)
        * 1024
        * 1024;
    let (tx, rx) = std::sync::mpsc::channel();
    // Reuse AnyDoc's positional parser, whose per-page API retains partial
    // extraction instead of rejecting a mixed PDF. Plain lopdf text joins
    // positioned glyph runs without spaces and is unsuitable for full-text search.
    std::thread::Builder::new()
        .name("pdf-basic".into())
        .spawn(move || {
            let result = (|| -> Result<(String, bool)> {
                let extraction = pdf_inspector::extract_pages_markdown_mem(&bytes, None)?;
                let mut body = String::new();
                let mut needs_enhancement = !extraction.pages_needing_ocr.is_empty();
                for page in extraction.pages {
                    needs_enhancement |= page.needs_ocr;
                    if !page.markdown.chars().any(char::is_alphanumeric) {
                        continue;
                    }
                    let number = page.page + 1;
                    let marker = format!("\n\n<!-- source-pages:{number}-{number} -->\n\n");
                    anyhow::ensure!(
                        body.len() + marker.len() + page.markdown.len() <= output_limit,
                        "基础文本超过转换输出限制"
                    );
                    body.push_str(&marker);
                    body.push_str(&page.markdown);
                }
                Ok((body, needs_enhancement))
            })();
            let _ = tx.send(result);
        })?;
    loop {
        anyhow::ensure!(!cancel(), "已停止转换，未发布输出");
        anyhow::ensure!(
            started.elapsed().as_secs() < runtime.anydoc_timeout_secs,
            "基础文本提取超时"
        );
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(result) => return result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => bail!("基础文本提取线程异常退出"),
        }
    }
}
