use super::{
    bytes, ext, id_part, markdown, public_url, request, value, Content, Item, Page, Remote,
    Resource, Url, FILE_LIMIT, JSON_LIMIT,
};
use anyhow::{ensure, Context, Result};
use reqwest::Method;
use serde_json::Value;

const HTTPS: &str = "https://";
const DIRECTORY: &str = "gitlab:tree:";
const FILE: &str = "gitlab:file:";
const PAGE_SIZE: usize = 100;

pub(in crate::online_import) fn gitlab_address(address: &str) -> Result<String> {
    let error = "请输入 GitLab 项目的 HTTPS 地址";
    let mut url = public_url(address).context(error)?;
    ensure!(
        url.scheme() == "https" && url.query().is_none() && url.fragment().is_none(),
        "{error}"
    );
    let path = url.path().trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let parts = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
    ensure!(
        parts.len() >= 2
            && parts.iter().all(|part| {
                !part.is_empty()
                    && !matches!(*part, "." | ".." | "-")
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            }),
        "{error}"
    );
    let path = path.to_owned();
    url.set_path(&path);
    Ok(url.into())
}

fn project_segment(path: &str) -> String {
    let mut url = Url::parse("https://gitlab.com/").unwrap();
    url.path_segments_mut().unwrap().push(path);
    url.path()[1..].into()
}

fn scope(id: &str) -> Result<(&str, &str, bool)> {
    let (rest, directory) = if let Some(rest) = id.strip_prefix(DIRECTORY) {
        (rest, true)
    } else {
        (id.strip_prefix(FILE).context("GitLab 导入范围无效")?, false)
    };
    let (branch, path) = rest.split_once(':').context("GitLab 导入范围无效")?;
    ensure!(
        !branch.is_empty() && (directory || !path.is_empty()),
        "GitLab 导入范围无效"
    );
    Ok((branch, path, directory))
}

fn is_markdown(path: &str) -> bool {
    matches!(ext(path).as_str(), "md" | "markdown")
        || path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("README"))
}

impl Remote {
    pub(super) fn gitlab_origin(&self) -> &str {
        let address = &self.connection.address;
        let path_start = HTTPS.len() + address[HTTPS.len()..].find('/').unwrap();
        &address[..path_start]
    }

    fn gitlab_project(&self) -> String {
        let project = project_segment(&self.connection.address[self.gitlab_origin().len() + 1..]);
        format!("/api/v4/projects/{project}/repository")
    }

    async fn gitlab_page(
        &self,
        route: &str,
        query: &[(&str, &str)],
        cursor: Option<&str>,
    ) -> Result<(Vec<Value>, Option<String>)> {
        let page = cursor
            .unwrap_or("1")
            .parse::<usize>()
            .context("GitLab 分页位置无效")?;
        ensure!((1..=1000).contains(&page), "GitLab 分页超出上限");
        let page_number = page.to_string();
        let page_size = PAGE_SIZE.to_string();
        let mut params = query.to_vec();
        params.extend([
            ("page", page_number.as_str()),
            ("per_page", page_size.as_str()),
        ]);
        let response = request(self.call(Method::GET, route, &params), None).await?;
        let next_header = response
            .headers()
            .get("x-next-page")
            .map(|header| header.to_str().map(str::to_owned))
            .transpose()?;
        let rows: Vec<Value> = serde_json::from_slice(&bytes(response, JSON_LIMIT).await?)?;
        let next = match next_header {
            Some(number) if number.is_empty() => None,
            Some(number) => {
                ensure!(
                    number.parse::<usize>()? == page + 1,
                    "GitLab 分页返回了重复内容"
                );
                Some(number)
            }
            None if rows.len() == PAGE_SIZE => Some((page + 1).to_string()),
            None => None,
        };
        Ok((rows, next))
    }

    pub(super) async fn gitlab_browse(
        &self,
        parent: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<Page> {
        let base = self.gitlab_project();
        let Some(parent) = parent else {
            let (rows, cursor) = self
                .gitlab_page(&format!("{base}/branches"), &[], cursor)
                .await?;
            let items = rows
                .iter()
                .map(|row| {
                    let name = value(row, "name")?;
                    Ok(Resource {
                        id: format!("{DIRECTORY}{name}:"),
                        title: name.into(),
                        expandable: true,
                    })
                })
                .collect::<Result<_>>()?;
            return Ok(Page { items, cursor });
        };
        let (branch, path, directory) = scope(parent)?;
        ensure!(directory, "GitLab 导入范围无效");
        let (rows, cursor) = self
            .gitlab_page(
                &format!("{base}/tree"),
                &[("ref", branch), ("path", path)],
                cursor,
            )
            .await?;
        let mut items = Vec::new();
        for row in &rows {
            let path = value(row, "path")?;
            let (prefix, expandable) = match row["type"].as_str() {
                Some("tree") => (DIRECTORY, true),
                Some("blob") if row["mode"] != "120000" && is_markdown(path) => (FILE, false),
                _ => continue,
            };
            items.push(Resource {
                id: format!("{prefix}{branch}:{path}"),
                title: value(row, "name")?.into(),
                expandable,
            });
        }
        Ok(Page { items, cursor })
    }

    pub(super) async fn gitlab_items(&self, resource: &str) -> Result<Vec<Item>> {
        let (branch, path, directory) = scope(resource)?;
        let parent = if directory {
            path
        } else {
            path.rsplit_once('/').map_or("", |(parent, _)| parent)
        };
        let query = [
            ("ref", branch),
            ("path", parent),
            ("recursive", if directory { "true" } else { "false" }),
        ];
        let tree = format!("{}/tree", self.gitlab_project());
        let mut cursor = None;
        let mut items = Vec::new();
        loop {
            let (rows, next) = self.gitlab_page(&tree, &query, cursor.as_deref()).await?;
            for row in &rows {
                let row_path = value(row, "path")?;
                if row["type"] != "blob"
                    || row["mode"] == "120000"
                    || !is_markdown(row_path)
                    || (!directory && row_path != path)
                {
                    continue;
                }
                let (folder, name) = row_path.rsplit_once('/').unwrap_or(("", row_path));
                let mut folders = vec![branch.to_owned()];
                folders.extend(
                    folder
                        .split('/')
                        .filter(|part| !part.is_empty())
                        .map(str::to_owned),
                );
                items.push(Item {
                    id: format!("{branch}:{row_path}"),
                    title: std::path::Path::new(name)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(name)
                        .into(),
                    version: value(row, "id")?.into(),
                    link: String::new(),
                    folders,
                    kind: "gitlab".into(),
                    inline: None,
                });
            }
            let Some(next) = next else {
                break;
            };
            cursor = Some(next);
        }
        ensure!(!items.is_empty(), "GitLab 所选范围没有可导入的 Markdown");
        Ok(items)
    }

    pub(super) async fn gitlab_fetch(&self, sha: &str) -> Result<Content> {
        let sha = id_part(sha)?;
        let endpoint = format!("{}/blobs/{sha}/raw", self.gitlab_project());
        let response = request(self.call(Method::GET, &endpoint, &[]), None).await?;
        Ok(markdown(String::from_utf8(
            bytes(response, FILE_LIMIT).await?,
        )?))
    }
}
