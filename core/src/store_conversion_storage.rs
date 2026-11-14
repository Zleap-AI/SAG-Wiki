use super::*;
use std::io::ErrorKind;

pub(super) struct Migration {
    old: PathBuf,
    new: PathBuf,
    staged: tempfile::TempDir,
}

impl Store {
    pub(super) fn validate_conversion_storage(
        &self,
        storage: &ConversionStorage,
    ) -> Result<ConversionStorage> {
        let ConversionStorage::Custom { directory } = storage else {
            return Ok(storage.clone());
        };
        anyhow::ensure!(
            Path::new(directory).is_absolute(),
            "请选择资料文件夹之外的目录"
        );
        let path = Path::new(directory)
            .canonicalize()
            .context("无法打开所选保存目录")?;
        anyhow::ensure!(path.is_dir(), "请选择文件夹");
        let cache = path.join(crate::docling::OUTPUT_DIRECTORY);
        anyhow::ensure!(
            !crate::docling::in_output(&path)
                && !self
                    .roots()?
                    .iter()
                    .any(|root| cache.starts_with(&root.path)),
            "请选择资料文件夹之外的目录"
        );
        Ok(ConversionStorage::Custom {
            directory: path_text(&path)?,
        })
    }

    pub(super) fn conversion_directory(&self, root: &Path, storage: &ConversionStorage) -> PathBuf {
        let base = match storage {
            ConversionStorage::Library => return root.join(crate::docling::OUTPUT_DIRECTORY),
            ConversionStorage::AppData => self.docling.data_directory(),
            ConversionStorage::Custom { directory } => Path::new(directory),
        };
        base.join(crate::docling::OUTPUT_DIRECTORY)
            .join(hash(root.as_os_str().as_encoded_bytes()))
    }

    pub(super) fn conversion_path(
        &self,
        root: &Path,
        source: &Path,
        settings: &Settings,
    ) -> Result<PathBuf> {
        let directory = self.conversion_directory(root, &settings.conversion_storage);
        crate::docling::output_path_in(root, source, &directory)
    }

    pub(super) fn prepare_conversion_migration(
        &self,
        old: &ConversionStorage,
        new: &ConversionStorage,
    ) -> Result<Vec<Migration>> {
        let mut migrations = Vec::new();
        for root in self.roots()? {
            let root = Path::new(&root.path);
            let old = self.conversion_directory(root, old);
            let new = self.conversion_directory(root, new);
            if old == new {
                continue;
            }
            // Never traverse a linked managed directory during migration.
            let probe = root.join(".ownership");
            crate::docling::output_path_in(root, &probe, &old)?;
            crate::docling::output_path_in(root, &probe, &new)?;
            anyhow::ensure!(
                !new.starts_with(&old) && !old.starts_with(&new),
                "转换结果目录不能互相包含"
            );
            let parent = new.parent().unwrap();
            let metadata = match fs::symlink_metadata(&old) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    let has_results: bool = self.connection().query_row(
                        "SELECT EXISTS(SELECT 1 FROM conversions WHERE root=?)",
                        [path_text(root)?],
                        |row| row.get(0),
                    )?;
                    anyhow::ensure!(!has_results, "无法读取原转换结果目录：{}", old.display());
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            anyhow::ensure!(metadata.is_dir(), "原转换位置不是文件夹：{}", old.display());
            anyhow::ensure!(
                !new.try_exists()?,
                "目标位置已有转换结果：{}",
                new.display()
            );
            fs::create_dir_all(parent).context("无法创建转换结果目录")?;
            let staged = tempfile::tempdir_in(parent)?;
            for entry in walkdir::WalkDir::new(&old).min_depth(1) {
                let entry = entry?;
                let target = staged.path().join(entry.path().strip_prefix(&old)?);
                if entry.file_type().is_dir() {
                    fs::create_dir(&target)?;
                } else {
                    anyhow::ensure!(
                        entry.file_type().is_file(),
                        "转换目录含有无法迁移的文件：{}",
                        entry.path().display()
                    );
                    fs::copy(entry.path(), &target)?;
                }
            }
            migrations.push(Migration { old, new, staged });
        }
        Ok(migrations)
    }

    pub(super) fn commit_conversion_migration(
        &self,
        tx: rusqlite::Transaction<'_>,
        migrations: Vec<Migration>,
    ) -> Result<()> {
        for migration in &migrations {
            let old = path_text(&migration.old)?;
            let new = path_text(&migration.new)?;
            let prefix = format!("{old}{}", std::path::MAIN_SEPARATOR);
            for (table, column) in [
                ("conversions", "output"),
                ("documents", "path"),
                ("index_issues", "path"),
                ("note_sources", "path"),
            ] {
                tx.execute(&format!("UPDATE {table} SET {column}=?1||substr({column},length(?2)+1) WHERE instr({column},?3)=1"), params![new,old,prefix])?;
            }
            tx.execute(
                "UPDATE drafts SET value=json_set(value,'$.path',?1||substr(path,length(?2)+1)),
                    path=?1||substr(path,length(?2)+1) WHERE instr(path,?3)=1",
                params![new, old, prefix],
            )?;
        }
        if !migrations.is_empty() {
            tx.execute("UPDATE metadata SET version=version+1", [])?;
        }
        let mut published = Vec::new();
        let outcome = (|| -> Result<()> {
            for migration in &migrations {
                anyhow::ensure!(
                    !migration.new.try_exists()?,
                    "目标位置已有转换结果：{}",
                    migration.new.display()
                );
                fs::rename(migration.staged.path(), &migration.new)?;
                published.push(&migration.new);
            }
            tx.commit()?;
            Ok(())
        })();
        if let Err(error) = outcome {
            for new in published {
                fs::remove_dir_all(new).with_context(|| {
                    format!("{error}；请清理未完成的迁移目录 {}", new.display())
                })?;
            }
            return Err(error);
        }
        for Migration { old, new, .. } in migrations {
            // Do not discard anything added or edited outside SAG Wiki after staging.
            for entry in walkdir::WalkDir::new(&old).contents_first(true) {
                let entry = entry?;
                if entry.file_type().is_dir() {
                    if fs::read_dir(entry.path())?.next().is_none() {
                        fs::remove_dir(entry.path())?;
                    }
                } else if entry.file_type().is_file() {
                    let copied = new.join(entry.path().strip_prefix(&old)?);
                    if copied.is_file()
                        && crate::docling::fingerprint(entry.path())?
                            == crate::docling::fingerprint(&copied)?
                    {
                        fs::remove_file(entry.path())?;
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn conversion_source(&self, output: &Path) -> Result<Option<PathBuf>> {
        if !crate::docling::in_output(output) {
            return Ok(None);
        }
        Ok(self
            .connection()
            .query_row(
                "SELECT source FROM conversions WHERE output=? AND valid=1",
                [path_text(output)?],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(PathBuf::from))
    }

    pub(crate) fn authorize_image(&self, path: &Path) -> Result<PathBuf> {
        if path
            .components()
            .any(|part| part.as_os_str() == ".sag-mineru-assets")
            && crate::docling::in_output(path)
        {
            let canonical = path.canonicalize()?;
            let path = canonical.as_path();
            let assets = path
                .ancestors()
                .find(|part| {
                    part.file_name()
                        .is_some_and(|name| name == ".sag-mineru-assets")
                })
                .context("转换配图路径无效")?;
            let parent = assets.parent().context("转换配图路径无效")?;
            let parent = if parent
                .file_name()
                .is_some_and(|name| name == ".sag-pdf-parts")
            {
                parent.parent().context("转换配图路径无效")?
            } else {
                parent
            };
            let prefix = format!("{}{}", path_text(parent)?, std::path::MAIN_SEPARATOR);
            let sources = {
                let db = self.connection();
                let mut query = db.prepare(
                    "SELECT source,output FROM conversions WHERE valid=1 AND instr(output,?)=1",
                )?;
                let rows = query
                    .query_map([prefix], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                rows
            };
            for (source, output) in sources {
                if Path::new(&output).parent() == Some(parent)
                    && self.authorize(Path::new(&source)).is_ok()
                {
                    return Ok(canonical);
                }
            }
        }
        self.authorize(path)
    }
}
