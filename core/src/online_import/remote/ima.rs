use super::{
    bytes, download, feed_markdown, list, markdown, public_url, request, value, Content, Item,
    Page, Remote, Resource, FILE_LIMIT, JSON_LIMIT,
};
use anyhow::{anyhow, ensure, Context, Result};
use reqwest::Method;
use serde_json::{json, Value};
use std::collections::HashSet;

const WIKI_API: &str = "/openapi/wiki/v1";
const NOTE_API: &str = "/openapi/note/v1";
const BASE: &str = "ima:base:";

fn file_extension(kind: i64) -> Option<&'static str> {
    match kind {
        1 => Some("pdf"),
        3 => Some("docx"),
        4 => Some("pptx"),
        5 => Some("xlsx"),
        7 => Some("md"),
        13 => Some("txt"),
        20 => Some("html"),
        21 => Some("epub"),
        _ => None,
    }
}

fn cursor(data: &Value) -> Option<String> {
    if data["is_end"] == true {
        return None;
    }
    data["next_cursor"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

impl Remote {
    async fn ima_api(&self, prefix: &str, action: &str, body: Value) -> Result<Value> {
        let result = self
            .json(Method::POST, &format!("{prefix}/{action}"), &[], Some(body))
            .await?;
        let code = result["code"]
            .as_i64()
            .filter(|code| *code != 0)
            .or_else(|| result["retcode"].as_i64())
            .unwrap_or(0);
        ensure!(
            code == 0,
            "腾讯 ima 请求失败（{code}），请检查凭据和知识库权限"
        );
        result.get("data").cloned().context("腾讯 ima 未返回数据")
    }

    pub(super) async fn ima_browse(&self, current: Option<&str>) -> Result<Page> {
        let current = current.unwrap_or_default();
        let mut search_cursor = current.strip_prefix("search:");
        let mut data = Value::Null;
        if search_cursor.is_none() {
            data = self
                .ima_api(
                    WIKI_API,
                    "get_addable_knowledge_base_list",
                    json!({"limit": 50, "cursor": current}),
                )
                .await?;
            if current.is_empty()
                && list(&data, "addable_knowledge_base_list")?.is_empty()
                && cursor(&data).is_none()
            {
                search_cursor = Some("");
            }
        }
        // The find endpoint can also expose shared knowledge bases.
        let (rows, next) = if let Some(search_cursor) = search_cursor {
            data = self
                .ima_api(
                    WIKI_API,
                    "search_knowledge_base",
                    json!({"limit": 20, "query":"", "cursor": search_cursor}),
                )
                .await?;
            (
                list(&data, "info_list")?,
                cursor(&data).map(|cursor| format!("search:{cursor}")),
            )
        } else {
            (list(&data, "addable_knowledge_base_list")?, cursor(&data))
        };
        let items = rows
            .iter()
            .map(|row| {
                let id = value(row, "id")?;
                Ok(Resource {
                    id: format!("{BASE}{id}"),
                    title: value(row, "name")?.into(),
                    expandable: false,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Page {
            items,
            cursor: next,
        })
    }

    pub(super) async fn ima_items(&self, resource: &str) -> Result<Vec<Item>> {
        let base = resource
            .strip_prefix(BASE)
            .filter(|id| !id.is_empty())
            .context("腾讯 ima 知识库标识无效")?;
        let mut work = vec![(String::new(), Vec::<String>::new())];
        let mut visited = HashSet::new();
        let mut items = Vec::new();
        while let Some((folder, folders)) = work.pop() {
            ensure!(
                folders.len() < 100 && visited.insert(folder.clone()),
                "腾讯 ima 目录层级或结构无效"
            );
            let mut current = String::new();
            let mut seen = HashSet::new();
            loop {
                let mut query = json!({"limit": 50, "cursor": current, "knowledge_base_id": base});
                if !folder.is_empty() {
                    query["folder_id"] = json!(folder);
                }
                let data = self.ima_api(WIKI_API, "get_knowledge_list", query).await?;
                for row in list(&data, "knowledge_list")? {
                    let media = value(row, "media_id")?;
                    let title = value(row, "title")?;
                    let media_type = row["media_type"].as_i64().unwrap_or(0);
                    if media_type == 99 {
                        let mut path = folders.clone();
                        path.push(title.to_owned());
                        work.push((media.to_owned(), path));
                    } else if matches!(media_type, 0 | 2 | 6 | 11)
                        || file_extension(media_type).is_some()
                    {
                        let identity = json!([base, folder, title]).to_string();
                        let version = file_extension(media_type)
                            .map(|_| media.to_owned())
                            .unwrap_or_default();
                        items.push(Item {
                            id: crate::online_import::digest(identity.as_bytes()),
                            title: title.to_owned(),
                            version,
                            link: String::new(),
                            folders: folders.clone(),
                            kind: "ima".into(),
                            inline: Some(media.to_owned()),
                        });
                    }
                }
                if data["is_end"] == true {
                    break;
                }
                let next = cursor(&data).context("腾讯 ima 未返回下一页位置")?;
                ensure!(
                    seen.len() < 1000 && seen.insert(next.clone()),
                    "腾讯 ima 分页未正常结束"
                );
                current = next;
            }
        }
        Ok(items)
    }

    pub(super) async fn ima_fetch(&self, item: &Item) -> Result<Content> {
        let media = item.inline.as_deref().context("腾讯 ima 缺少文档标识")?;
        let data = self
            .ima_api(WIKI_API, "get_media_info", json!({"media_id": media}))
            .await?;
        let kind = data["media_type"]
            .as_i64()
            .context("腾讯 ima 缺少文档类型")?;
        if kind == 11 {
            let id = value(&data["notebook_ext_info"], "notebook_id")?;
            let data = self
                .ima_api(
                    NOTE_API,
                    "get_doc_content",
                    json!({"note_id": id, "target_content_format": 0}),
                )
                .await?;
            let content = data["content"]
                .as_str()
                .context("腾讯 ima 未返回笔记正文")?;
            return Ok(markdown(content.to_owned()));
        }

        let web = matches!(kind, 2 | 6);
        let extension = if web {
            "html"
        } else {
            file_extension(kind).ok_or_else(|| anyhow!("腾讯 ima 暂不支持该文档类型（{kind}）"))?
        };
        let address = value(&data["url_info"], "url")?;
        let url = public_url(address)?;
        ensure!(url.scheme() == "https", "腾讯 ima 文件下载必须使用 HTTPS");
        let limit = if web { JSON_LIMIT } else { FILE_LIMIT };
        let headers = data["url_info"]["headers"].as_object();
        let body = if let Some(headers) = headers.filter(|headers| !headers.is_empty()) {
            let mut builder = self.http.get(url.clone());
            for (name, value) in headers {
                builder = builder.header(
                    name.as_str(),
                    value.as_str().context("腾讯 ima 文件下载响应格式无效")?,
                );
            }
            bytes(request(builder, None).await?, limit).await?
        } else {
            download(&self.http, url.clone(), limit).await?
        };
        if web {
            let body = feed_markdown(&String::from_utf8(body)?, address);
            ensure!(!body.trim().is_empty(), "腾讯 ima 网页没有可读正文");
            Ok(markdown(body))
        } else {
            Ok(Content {
                bytes: body,
                extension: extension.into(),
                assets: vec![],
            })
        }
    }
}
