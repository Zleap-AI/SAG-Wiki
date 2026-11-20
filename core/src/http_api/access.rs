//! Key-specific grants, separate from the store's global knowledge-base authorization.
use super::{directory, scopes, ApiError, Store};
use anyhow::{bail, Result};
use hyper::StatusCode;
use sag_protocol::ApiScope;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum KeyScope {
    #[default]
    All,
    Folders {
        directories: Vec<DirectoryGrant>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct DirectoryGrant {
    path: String,
    canonical: String,
}

fn unavailable() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "scope_unavailable",
        "授权文件夹已失效或发生变化，请在本机重新设置此 Key 的访问范围",
    )
}
fn forbidden() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "path_not_allowed",
        "目录不在此 Key 允许的知识库范围内",
    )
}

impl KeyScope {
    pub(super) fn relocate(&mut self, relocate: &impl Fn(&Path) -> Option<std::path::PathBuf>) -> Result<()> {
        if let Self::Folders { directories } = self {
            for grant in directories {
                if let Some(path) = relocate(Path::new(&grant.path)) {
                    grant.path = crate::store::path_text(&path)?;
                }
                if let Some(path) = relocate(Path::new(&grant.canonical)) {
                    grant.canonical = crate::store::path_text(&path)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn create(store: &Store, scope: ApiScope) -> Result<Self> {
        match scope {
            ApiScope::All => Ok(Self::All),
            ApiScope::Folders { paths } => {
                if paths.is_empty() {
                    bail!("请选择至少一个文件夹")
                }
                let paths =
                    scopes(store, &paths).map_err(|error| anyhow::anyhow!(error.message))?;
                let directories = paths
                    .into_iter()
                    .map(|path| {
                        let actual = Path::new(&path).canonicalize()?;
                        if crate::docling::in_output(&actual) {
                            bail!("请选择原始资料文件夹，不能授权转换结果目录")
                        }
                        Ok(DirectoryGrant {
                            path,
                            canonical: crate::store::path_text(&actual)?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(Self::Folders { directories })
            }
        }
    }

    pub(super) fn public(&self) -> ApiScope {
        match self {
            Self::All => ApiScope::All,
            Self::Folders { directories } => ApiScope::Folders {
                paths: directories.iter().map(|grant| grant.path.clone()).collect(),
            },
        }
    }

    /// None is explicitly unrestricted; Some(empty) must never represent all.
    pub(super) fn validate(
        &self,
        store: &Store,
    ) -> std::result::Result<Option<Vec<String>>, ApiError> {
        let Self::Folders { directories } = self else {
            return Ok(None);
        };
        if directories.is_empty() {
            return Err(unavailable());
        }
        let paths = directories
            .iter()
            .map(|grant| {
                let path = directory(store, &grant.path).map_err(|_| unavailable())?;
                let actual = path.canonicalize().map_err(|_| unavailable())?;
                if path != Path::new(&grant.path) || actual != Path::new(&grant.canonical) {
                    return Err(unavailable());
                }
                Ok(grant.path.clone())
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Some(paths))
    }

    pub(super) fn resolve(
        &self,
        store: &Store,
        requested: &[String],
    ) -> std::result::Result<Vec<String>, ApiError> {
        let Some(granted) = self.validate(store)? else {
            return scopes(store, requested);
        };
        if requested.is_empty() {
            return Ok(granted);
        }
        // Authorize the nearest existing ancestor before reporting existence/type
        // errors. Requests outside the grant always get the same denial, including
        // missing paths and in-scope symlinks leading into another project.
        for value in requested {
            let mut ancestor = Path::new(value);
            if !ancestor.is_absolute() {
                return Err(forbidden());
            }
            loop {
                match std::fs::symlink_metadata(ancestor) {
                    Ok(_) => break,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                        ) =>
                    {
                        ancestor = ancestor.parent().ok_or_else(forbidden)?;
                    }
                    Err(_) => return Err(forbidden()),
                }
            }
            self.check_document(store, ancestor)?;
        }
        let selected = scopes(store, requested)?;
        if selected.iter().any(|path| {
            !granted
                .iter()
                .any(|grant| Path::new(path).starts_with(grant))
        }) {
            return Err(forbidden());
        }
        // Check actual targets as well as the index namespace (including historical roots).
        for path in &selected {
            self.check_document(store, Path::new(path))?;
        }
        Ok(selected)
    }

    /// Restricted requests use exact indexed file identities before retrieval.
    /// Legacy directory SQL normalizes backslashes for Windows and therefore
    /// cannot be a permission boundary for Unix filenames containing '\\'.
    pub(super) fn search_scope(
        &self,
        store: &Store,
        requested: &[String],
        exclude: &[String],
    ) -> std::result::Result<(Vec<String>, bool), ApiError> {
        let selected = self.resolve(store, requested)?;
        if matches!(self, Self::All) {
            return Ok((
                store
                    .search_scopes_excluding_each(&selected, exclude)
                    .map_err(ApiError::bad)?,
                !exclude.is_empty(),
            ));
        }
        let canonical_selected = selected
            .iter()
            .map(|path| Path::new(path).canonicalize())
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|_| unavailable())?;
        let mut files = Vec::new();
        for file in store.files().map_err(ApiError::internal)? {
            let source = Path::new(file.source_path.as_deref().unwrap_or(&file.path));
            // Prune unrelated metadata before filesystem/SQLite authorization.
            if !selected.iter().any(|path| source.starts_with(path))
                && !canonical_selected
                    .iter()
                    .any(|path| source.starts_with(path))
            {
                continue;
            }
            let Ok(indexed_source) = store.indexed_path(source) else {
                continue;
            };
            if !selected
                .iter()
                .any(|scope| indexed_source.starts_with(scope))
            {
                continue;
            }
            if self.check_document(store, source).is_err() {
                continue;
            }
            let output = Path::new(&file.path);
            // Do not let a stale indexed file become a directory or a different
            // symlink target while constructing the exact candidate set.
            if !output.is_file() || store.indexed_path(output).ok().as_deref() != Some(output) {
                continue;
            }
            files.push(file.path);
        }
        if !exclude.is_empty() {
            let allowed = store
                .search_scopes_excluding_each(&selected, exclude)
                .map_err(ApiError::bad)?
                .into_iter()
                .collect::<std::collections::HashSet<_>>();
            files.retain(|path| allowed.contains(path));
        }
        self.validate(store)?;
        files.sort();
        files.dedup();
        Ok((files, true))
    }

    pub(super) fn check_document(
        &self,
        store: &Store,
        path: &Path,
    ) -> std::result::Result<(), ApiError> {
        let Self::Folders { directories } = self else {
            return Ok(());
        };
        let actual = store.authorize(path).map_err(|_| forbidden())?;
        if !directories
            .iter()
            .any(|grant| actual.starts_with(&grant.canonical))
        {
            return Err(forbidden());
        }
        Ok(())
    }

    pub(super) fn status_roots(&self, store: &Store) -> std::result::Result<Vec<Value>, ApiError> {
        let roots = store.roots().map_err(ApiError::internal)?;
        let Some(granted) = self.validate(store)? else {
            return Ok(roots
                .into_iter()
                .map(|r| json!({"path":r.path,"status":r.status,"indexed_files":r.count}))
                .collect());
        };
        granted
            .into_iter()
            .map(|path| {
                let root = roots
                    .iter()
                    .find(|root| Path::new(&path).starts_with(&root.path))
                    .ok_or_else(unavailable)?;
                let count = store
                    .indexed_file_count(Path::new(&path))
                    .map_err(ApiError::internal)?;
                Ok(json!({"path":path,"status":root.status,"indexed_files":count}))
            })
            .collect()
    }
}
