use super::{
    bytes, id_part, markdown, request, value, Item, Page, Remote, Resource, Url, FILE_LIMIT,
    JSON_LIMIT,
};
use anyhow::{ensure, Context, Result};
use reqwest::{Method, StatusCode};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::sync::Arc;

const REPOSITORY: &str = "https://github.com/";
const ROOT: &str = "github:root";
const FOLDER: &str = "github:folder:";
const DOCUMENT: &str = "github:file:";
const BRANCH_FOLDER: &str = "github:tree:";
const BRANCH_DOCUMENT: &str = "github:blob:";

pub(in crate::online_import) fn github_address(address: &str) -> Result<String> {
    let error = "请输入 GitHub 仓库链接，例如 https://github.com/owner/repo";
    let url = Url::parse(address).context(error)?;
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("github.com")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none(),
        "{error}"
    );
    let (owner, repository) = url
        .path()
        .trim_matches('/')
        .split_once('/')
        .context(error)?;
    let repository = repository.strip_suffix(".git").unwrap_or(repository);
    ensure!(
        !owner.is_empty()
            && owner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !repository.is_empty()
            && !matches!(repository, "." | "..")
            && repository
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
        "{error}"
    );
    Ok(format!("{REPOSITORY}{owner}/{repository}").to_ascii_lowercase())
}

#[derive(Deserialize)]
pub(super) struct File {
    path: String,
    sha: String,
    mode: String,
}

#[derive(Deserialize)]
struct Tree {
    tree: Vec<File>,
    truncated: bool,
}

fn scope(resource: &str) -> Result<(Option<&str>, &str, bool)> {
    if resource == ROOT {
        return Ok((None, "", true));
    }
    if let Some(path) = resource.strip_prefix(FOLDER) {
        return Ok((None, path, true));
    }
    if let Some(path) = resource.strip_prefix(DOCUMENT) {
        return Ok((None, path, false));
    }
    let (value, directory) = if let Some(value) = resource.strip_prefix(BRANCH_FOLDER) {
        (value, true)
    } else {
        (
            resource
                .strip_prefix(BRANCH_DOCUMENT)
                .context("GitHub 导入范围无效")?,
            false,
        )
    };
    let (branch, path) = value.split_once(':').context("GitHub 导入范围无效")?;
    ensure!(
        !branch.is_empty() && (directory || !path.is_empty()),
        "GitHub 导入范围无效"
    );
    Ok((Some(branch), path, directory))
}

pub(in crate::online_import) fn github_resources(resources: &[String]) -> Vec<String> {
    let scopes: Vec<_> = resources
        .iter()
        .map(|resource| scope(resource).ok())
        .collect();
    resources
        .iter()
        .zip(&scopes)
        .filter(|(_, scope)| {
            let Some((branch, path, true)) = scope else {
                return true;
            };
            !scopes
                .iter()
                .flatten()
                .any(|(candidate_branch, candidate_path, _)| {
                    branch == candidate_branch
                        && path != candidate_path
                        && (path.is_empty()
                            || candidate_path
                                .strip_prefix(*path)
                                .is_some_and(|suffix| suffix.starts_with('/')))
                })
        })
        .map(|(resource, _)| resource.clone())
        .collect()
}

impl Remote {
    fn github_repository(&self) -> &str {
        self.connection.address.trim_start_matches(REPOSITORY)
    }

    async fn github_files(&self, branch: Option<&str>) -> Result<Arc<[File]>> {
        let key = branch.map(str::to_owned);
        let mut cache = self.github_files.lock().await;
        if let Some(files) = cache.get(&key) {
            return Ok(files.clone());
        }
        let repository = self.github_repository();
        let tree = if let Some(branch) = branch {
            let mut url = Url::parse(&format!("{}/repos/{repository}/branches/", self.origin()))?;
            url.path_segments_mut().unwrap().pop_if_empty().push(branch);
            let info = self.get(url.path(), &[]).await?;
            id_part(value(&info["commit"]["commit"]["tree"], "sha")?)?.to_owned()
        } else {
            "HEAD".into()
        };
        let result = self
            .get(
                &format!("/repos/{repository}/git/trees/{tree}"),
                &[("recursive", "1")],
            )
            .await?;
        let tree: Tree = serde_json::from_value(result)?;
        ensure!(!tree.truncated, "GitHub 仓库文件过多，无法完整读取目录");
        let files: Arc<[File]> = tree
            .tree
            .into_iter()
            .filter(|file| {
                let name = file.path.rsplit('/').next().unwrap_or_default();
                matches!(file.mode.as_str(), "100644" | "100755")
                    && (matches!(super::ext(name).as_str(), "md" | "markdown")
                        || name.eq_ignore_ascii_case("README"))
            })
            .collect();
        cache.insert(key, files.clone());
        Ok(files)
    }

    async fn github_branches(&self, cursor: Option<&str>) -> Result<Page> {
        let page = cursor
            .unwrap_or("1")
            .parse::<u32>()
            .context("GitHub 分页位置无效")?;
        ensure!(page > 0, "GitHub 分页位置无效");
        let path = format!("/repos/{}/branches", self.github_repository());
        let response = request(
            self.call(
                Method::GET,
                &path,
                &[("page", &page.to_string()), ("per_page", "100")],
            ),
            None,
        )
        .await?;
        let has_next = response.headers().get_all("link").iter().any(|link| {
            link.to_str()
                .is_ok_and(|value| value.split(',').any(|part| part.contains("rel=\"next\"")))
        });
        let mut rows: Vec<serde_json::Value> =
            serde_json::from_slice(&bytes(response, JSON_LIMIT).await?)?;
        if page == 1 {
            if let Some(index) = rows.iter().position(|row| row["name"] == "main") {
                rows[..=index].rotate_right(1);
            } else if has_next {
                let lookup = self.call(Method::GET, &format!("{path}/main"), &[]);
                match request(lookup, None).await {
                    Ok(response) if response.status() == StatusCode::MOVED_PERMANENTLY => {}
                    Ok(response) => rows.insert(
                        0,
                        serde_json::from_slice(&bytes(response, JSON_LIMIT).await?)?,
                    ),
                    Err(error)
                        if error
                            .downcast_ref::<reqwest::Error>()
                            .and_then(reqwest::Error::status)
                            == Some(StatusCode::NOT_FOUND) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        let items = rows
            .iter()
            .map(|row| {
                let name = value(row, "name")?;
                Ok(Resource {
                    id: format!("{BRANCH_FOLDER}{name}:"),
                    title: name.into(),
                    expandable: true,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Page {
            items,
            cursor: page
                .checked_add(1)
                .filter(|_| has_next)
                .map(|page| page.to_string()),
        })
    }

    pub(super) async fn github_browse(
        &self,
        parent: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<Page> {
        let Some(parent) = parent else {
            return self.github_branches(cursor).await;
        };
        let (branch, folder, directory) = scope(parent)?;
        ensure!(directory, "GitHub 文件夹标识无效");
        let prefix = if folder.is_empty() {
            String::new()
        } else {
            format!("{folder}/")
        };
        let (folder_id, file_id) = if let Some(branch) = branch {
            (
                format!("{BRANCH_FOLDER}{branch}:"),
                format!("{BRANCH_DOCUMENT}{branch}:"),
            )
        } else {
            (FOLDER.into(), DOCUMENT.into())
        };
        let files = self.github_files(branch).await?;
        let mut folders = BTreeSet::new();
        let mut documents = Vec::new();
        for file in files.iter() {
            let Some(relative) = file.path.strip_prefix(&prefix) else {
                continue;
            };
            if let Some((folder, _)) = relative.split_once('/') {
                folders.insert(folder);
            } else {
                documents.push(Resource {
                    id: format!("{file_id}{}", file.path),
                    title: relative.into(),
                    expandable: false,
                });
            }
        }
        let mut items: Vec<_> = folders
            .into_iter()
            .map(|folder| Resource {
                id: format!("{folder_id}{prefix}{folder}"),
                title: folder.into(),
                expandable: true,
            })
            .collect();
        items.extend(documents);
        Ok(Page {
            items,
            cursor: None,
        })
    }

    pub(super) async fn github_items(&self, resource: &str) -> Result<Vec<Item>> {
        let (branch, path, directory) = scope(resource)?;
        let files = self.github_files(branch).await?;
        let selected: Vec<_> = if directory && path.is_empty() {
            files.iter().collect()
        } else if directory {
            let prefix = format!("{path}/");
            files
                .iter()
                .filter(|file| file.path.starts_with(&prefix))
                .collect()
        } else {
            files.iter().filter(|file| file.path == path).collect()
        };
        ensure!(!selected.is_empty(), "GitHub 所选范围没有可导入的 Markdown");
        Ok(selected
            .into_iter()
            .map(|file| {
                let (folder, name) = file
                    .path
                    .rsplit_once('/')
                    .unwrap_or(("", file.path.as_str()));
                Item {
                    // Legacy imports used the path alone. NUL separates branch-scoped
                    // identities from those existing files and is invalid in a Git path.
                    id: branch.map_or_else(
                        || file.path.clone(),
                        |branch| format!("{branch}\0{}", file.path),
                    ),
                    title: std::path::Path::new(name)
                        .file_stem()
                        .and_then(|name| name.to_str())
                        .unwrap_or(name)
                        .into(),
                    version: file.sha.clone(),
                    link: String::new(),
                    folders: branch
                        .into_iter()
                        .chain(folder.split('/').filter(|part| !part.is_empty()))
                        .map(str::to_owned)
                        .collect(),
                    kind: "github".into(),
                    inline: None,
                }
            })
            .collect())
    }

    pub(super) async fn github_fetch(&self, sha: &str) -> Result<super::Content> {
        let sha = super::id_part(sha)?;
        let path = format!("/repos/{}/git/blobs/{sha}", self.github_repository());
        let builder = self
            .call(Method::GET, &path, &[])
            .header("Accept", "application/vnd.github.raw+json");
        let response = request(builder, None).await?;
        Ok(markdown(String::from_utf8(
            bytes(response, FILE_LIMIT).await?,
        )?))
    }
}
