//! Explicit recovery of current file failures, without scanning healthy files.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn failed_files(db: &Connection) -> Result<Vec<(String, Vec<PathBuf>)>> {
    let mut query = db.prepare(
        "SELECT i.root,COALESCE(c.source,i.path) FROM index_issues i
         JOIN roots r ON r.path=i.root
         LEFT JOIN conversions c ON c.root=i.root AND c.output=i.path
         WHERE r.status IN ('partial','error') AND i.error NOT LIKE ?1 ORDER BY i.root,i.path",
    )?;
    let rows = query.query_map([format!("{DIRECTORY_READ_ERROR_PREFIX}%")], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut groups = BTreeMap::<String, BTreeSet<PathBuf>>::new();
    for row in rows {
        let (root, file) = row?;
        let path = PathBuf::from(file);
        // walk_entry's persisted prefix excludes genuine directory failures.
        // Keep file-origin failures even if the path is now a directory, so
        // exact-file cleanup can retire the old issue without touching children.
        if path != Path::new(&root) && path.starts_with(&root) && !ignore_rules::is_rule_file(&path)
        {
            groups.entry(root).or_default().insert(path);
        }
    }
    Ok(groups
        .into_iter()
        .map(|(root, paths)| (root, paths.into_iter().collect()))
        .collect())
}

impl Store {
    pub fn failed_index_count(&self) -> Result<usize> {
        Ok(failed_files(&self.connection())?
            .iter()
            .map(|(_, paths)| paths.len())
            .sum())
    }

    pub fn prepare_failed_reindex(&self) -> Result<Vec<(String, Vec<PathBuf>)>> {
        if self.indexing.status().paused {
            bail!("索引已暂停，请先继续索引")
        }
        if !self.indexing.status().tasks.is_empty() {
            bail!("正在索引，请等待当前任务完成")
        }
        let mut db = self.connection();
        let tx = db.transaction()?;
        let busy: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM roots WHERE status IN ('pending','indexing'))",
            [],
            |row| row.get(0),
        )?;
        if busy {
            bail!("正在索引，请等待当前任务完成")
        }
        let work = failed_files(&tx)?;
        for (root, _) in &work {
            tx.execute("UPDATE roots SET status='pending' WHERE path=?", [root])?;
        }
        tx.commit()?;
        Ok(work)
    }

    pub fn retry_failed_indexes(
        &self,
        root: &str,
        paths: &[PathBuf],
        progress: impl Fn(IndexProgress),
    ) -> Result<usize> {
        self.retry_failed_indexes_mode(root, paths, true, progress)
    }

    pub(crate) fn retry_failed_indexes_basic(
        &self,
        root: &str,
        paths: &[PathBuf],
        progress: impl Fn(IndexProgress),
    ) -> Result<usize> {
        self.retry_failed_indexes_mode(root, paths, false, progress)
    }

    fn retry_failed_indexes_mode(
        &self,
        root: &str,
        paths: &[PathBuf],
        enhance: bool,
        progress: impl Fn(IndexProgress),
    ) -> Result<usize> {
        if !self.begin_indexing(root)? {
            return Ok(0);
        }
        let started_at = now();
        self.indexing.begin(root);
        let result = self.indexing.checkpoint_root(root).and_then(|_| {
            self.retry_failed_inner(root, paths, enhance, |p| {
                self.indexing.progress(root, p.count, p.total);
                progress(p);
            })
        });
        match &result {
            Ok(_) => self.finish_indexing(root)?,
            Err(error) if !self.indexing.cancelled(root) => {
                self.root_status(root, "error", Some(&error.to_string()))?
            }
            Err(_) => {}
        }
        let saved = self.record_index_history(root, started_at);
        self.indexing.finish(root);
        saved?;
        result
    }

    fn retry_failed_inner(
        &self,
        root: &str,
        paths: &[PathBuf],
        enhance: bool,
        progress: impl Fn(IndexProgress),
    ) -> Result<usize> {
        let settings = self.settings()?;
        if !Path::new(root).is_dir() {
            bail!("文件夹不可用，请检查磁盘或重新添加")
        }
        let mut rules = crate::folder_index::Rules::new(Path::new(root), &settings)?;
        let excluded = self.excluded_directories()?;
        let mut ignore = ignore_rules::matcher(Path::new(root));
        let mut sources = vec![];
        let mut native = vec![];
        for path in paths {
            self.indexing.checkpoint_root(root)?;
            if !path.starts_with(root) || path == Path::new(root) {
                continue;
            }
            let still_failed: bool = self.connection().query_row(
                "SELECT EXISTS(SELECT 1 FROM index_issues WHERE root=?1 AND (path=?2 OR path IN (SELECT output FROM conversions WHERE root=?1 AND source=?2)))",
                params![root,path_text(path)?], |row| row.get(0),
            )?;
            if !still_failed {
                continue;
            }
            let metadata = match fs::symlink_metadata(path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.retire_failed_file(root, path)?;
                    continue;
                }
                Err(error) => {
                    self.connection().execute(
                        "UPDATE index_issues SET error=? WHERE root=? AND path=?",
                        params![error.to_string(), root, path_text(path)?],
                    )?;
                    continue;
                }
            };
            let options = rules.file(path)?;
            let eligible = metadata.is_file()
                && !metadata.file_type().is_symlink()
                && path.canonicalize()?.starts_with(root)
                && !excluded.iter().any(|directory| path.starts_with(directory))
                && !path.strip_prefix(root)?.components().any(|part| {
                    settings
                        .excluded
                        .iter()
                        .any(|name| part.as_os_str() == name.as_str())
                })
                && !text::in_hidden_directory(Path::new(root), path, false)
                && !ignore_rules::is_ignored(&mut ignore, path, false)?;
            if !eligible {
                self.retire_failed_file(root, path)?;
            } else if crate::docling::should_convert(path, options)
                && !crate::docling::is_pem_key(path)?
            {
                self.connection().execute(
                    "DELETE FROM conversion_failures WHERE root=? AND source=?",
                    params![root, path_text(path)?],
                )?;
                sources.push(path.clone());
            } else if Self::indexable_path(path, options) {
                native.push(path.clone());
            } else {
                self.retire_failed_file(root, path)?;
            }
        }
        self.indexing.phase(root, "indexing", native.len());
        progress(IndexProgress {
            count: 0,
            total: native.len(),
        });
        for (index, path) in native.iter().enumerate() {
            self.indexing.checkpoint_root(root)?;
            let result = self.indexing.file(root, path, || {
                self.prepare_file_with_runtime(
                    root,
                    path,
                    settings.retrieval.chunk_chars,
                    true,
                    &settings.runtime,
                )
            });
            if let Some(file) = self.prepared_or_issue(root, path, result)? {
                let mut db = self.connection();
                let tx = db.transaction()?;
                Self::write_prepared(&tx, root, file)?;
                tx.commit()?;
            }
            progress(IndexProgress {
                count: index + 1,
                total: native.len(),
            });
        }
        if !sources.is_empty() {
            let conversion_progress = |p: IndexProgress| {
                // Selective conversion accepts output in place, without a
                // subsequent full scan. Record it while preserving the live
                // converting stage and any partially completed retry history.
                self.indexing.record_indexed_progress(
                    root,
                    native.len() + p.count,
                    native.len() + p.total,
                );
                progress(p);
            };
            if !enhance {
                // Retrying OCR must not occupy the foreground lane. Re-publish
                // basic text and let the independent worker consume pending states.
                self.publish_basic_sources(root, &sources, &settings, &conversion_progress)?;
                for source in &sources {
                    let output = crate::docling::output_path(Path::new(root), source)?;
                    let owned: bool = self.connection().query_row(
                        "SELECT EXISTS(SELECT 1 FROM conversions WHERE source=? AND valid=1)",
                        [path_text(source)?],
                        |row| row.get(0),
                    )?;
                    if !owned || !output.is_file() {
                        continue;
                    }
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
                    self.connection().execute("DELETE FROM index_issues WHERE root=?1 AND path=?2 AND EXISTS(SELECT 1 FROM documents WHERE root=?1 AND path=?3) AND NOT EXISTS(SELECT 1 FROM index_issues WHERE root=?1 AND path=?3)", params![root,path_text(source)?,path_text(&output)?])?;
                }
                self.indexing.record_indexed_progress(
                    root,
                    native.len() + sources.len(),
                    native.len() + sources.len(),
                );
                return Ok(native.len() + sources.len());
            }
            let issues = self.convert_sources(
                root,
                Path::new(root),
                &sources,
                &settings,
                false,
                &conversion_progress,
            )?;
            for (path, error) in issues {
                self.connection().execute("INSERT INTO index_issues(path,root,error) VALUES(?,?,?) ON CONFLICT(path) DO UPDATE SET root=excluded.root,error=excluded.error", params![path,root,error])?;
            }
        }
        Ok(native.len() + sources.len())
    }

    fn retire_failed_file(&self, root: &str, path: &Path) -> Result<()> {
        let source = path_text(path)?;
        let output: Option<String> = self
            .connection()
            .query_row(
                "SELECT output FROM conversions WHERE root=? AND source=?",
                params![root, source],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(output) = output {
            self.remove_indexed(&output)?;
        }
        // Exact-file cleanup is intentional: a failed file may now be a
        // directory containing successfully indexed children.
        self.remove_indexed(&source)?;
        let mut db = self.connection();
        let tx = db.transaction()?;
        tx.execute(
            "UPDATE conversions SET valid=0 WHERE root=? AND source=?",
            params![root, source],
        )?;
        tx.execute(
            "DELETE FROM conversion_states WHERE root=? AND source=?",
            params![root, source],
        )?;
        tx.execute(
            "DELETE FROM conversion_failures WHERE root=? AND source=?",
            params![root, source],
        )?;
        tx.commit()?;
        Ok(())
    }
}
