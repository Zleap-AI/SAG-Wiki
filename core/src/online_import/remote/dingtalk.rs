use super::{id_part, list, markdown, request, value, Content, Item, Page, Remote, Resource, Url};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::HashSet;

const FOLDER: &str = "dingtalk:folder:";
const DOCUMENT: &str = "dingtalk:doc:";

fn folder(row: &Value) -> bool {
    row["type"]
        .as_str()
        .is_some_and(|value| value.eq_ignore_ascii_case("folder"))
}

fn document(row: &Value) -> bool {
    [
        ("type", "FILE"),
        ("category", "ALIDOC"),
        ("extension", "adoc"),
    ]
    .iter()
    .all(|(key, expected)| {
        row[*key]
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case(expected))
    })
}

fn title<'a>(row: &'a Value, id: &'a str) -> &'a str {
    row["name"]
        .as_str()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(id)
}

impl Remote {
    pub(super) async fn dingtalk_token(&self) -> Result<String> {
        let response = request(
            self.http
                .post(format!("{}/v1.0/oauth2/accessToken", self.origin()))
                .json(&json!({"appKey": self.connection.app_id, "appSecret": self.token})),
            None,
        )
        .await?;
        let body = self.json_body(response).await?;
        Ok(value(&body, "accessToken")?.into())
    }

    async fn dingtalk_page(
        &self,
        parent: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<(Vec<Value>, Option<String>)> {
        let mut query = vec![("operatorId", self.connection.address.as_str())];
        let (route, field) = if let Some(parent) = parent {
            query.extend([("parentNodeId", id_part(parent)?), ("maxResults", "50")]);
            ("/v2.0/wiki/nodes", "nodes")
        } else {
            query.push(("maxResults", "30"));
            ("/v2.0/wiki/workspaces", "workspaces")
        };
        if let Some(cursor) = cursor {
            query.push(("nextToken", cursor));
        }
        let result = self.get(route, &query).await?;
        let next = result["nextToken"]
            .as_str()
            .filter(|token| !token.is_empty())
            .map(str::to_owned);
        Ok((list(&result, field)?.to_vec(), next))
    }

    pub(super) async fn dingtalk_browse(
        &self,
        parent: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<Page> {
        let parent = parent
            .map(|id| id.strip_prefix(FOLDER).context("钉钉文件夹标识无效"))
            .transpose()?;
        let (rows, cursor) = self.dingtalk_page(parent, cursor).await?;
        let mut items = Vec::new();
        for row in &rows {
            let (resource, name, expandable) = match parent {
                None => {
                    let id = value(row, "workspaceId")?;
                    let Some(root) = row["rootNodeId"].as_str().filter(|root| !root.is_empty())
                    else {
                        continue;
                    };
                    (format!("{FOLDER}{root}"), title(row, id), true)
                }
                Some(_) if folder(row) => {
                    let id = value(row, "nodeId")?;
                    (format!("{FOLDER}{id}"), title(row, id), true)
                }
                Some(parent) if document(row) => {
                    let id = value(row, "nodeId")?;
                    (format!("{DOCUMENT}{parent}:{id}"), title(row, id), false)
                }
                Some(_) => continue,
            };
            items.push(Resource {
                id: resource,
                title: name.into(),
                expandable,
            });
        }
        Ok(Page { items, cursor })
    }

    pub(super) async fn dingtalk_items(&self, resource: &str) -> Result<Vec<Item>> {
        let (parent, selected) = if let Some(parent) = resource.strip_prefix(FOLDER) {
            (parent, None)
        } else {
            let (parent, node) = resource
                .strip_prefix(DOCUMENT)
                .and_then(|id| id.split_once(':'))
                .context("钉钉文档标识无效")?;
            (parent, Some(node))
        };
        let mut pending = vec![(parent.to_owned(), Vec::<String>::new())];
        let mut visited = HashSet::new();
        let mut items = Vec::new();
        while let Some((parent, folders)) = pending.pop() {
            ensure!(
                folders.len() < 100 && visited.insert(parent.clone()),
                "钉钉目录层级或结构无效"
            );
            let mut cursor = None;
            let mut seen = HashSet::new();
            loop {
                let (rows, next) = self.dingtalk_page(Some(&parent), cursor.as_deref()).await?;
                for row in &rows {
                    if !folder(row) && !document(row) {
                        continue;
                    }
                    let id = value(row, "nodeId")?;
                    let name = title(row, id);
                    if folder(row) && selected.is_none() {
                        let mut path = folders.clone();
                        path.push(name.into());
                        pending.push((id.into(), path));
                    } else if document(row) && selected.is_none_or(|node| node == id) {
                        items.push(Item {
                            id: id.into(),
                            title: name.into(),
                            version: row["modifiedTimestamp"]
                                .as_i64()
                                .filter(|value| *value > 0)
                                .map(|value| value.to_string())
                                .or_else(|| row["modifiedTime"].as_str().map(str::to_owned))
                                .unwrap_or_default(),
                            link: row["url"].as_str().unwrap_or_default().into(),
                            folders: folders.clone(),
                            kind: "dingtalk".into(),
                            inline: None,
                        });
                    }
                }
                let Some(next) = next else {
                    break;
                };
                ensure!(
                    seen.len() < 1000 && seen.insert(next.clone()),
                    "钉钉分页未正常结束"
                );
                cursor = Some(next);
            }
        }
        if selected.is_some() {
            ensure!(!items.is_empty(), "钉钉文档已移动、删除或不可读");
        }
        Ok(items)
    }

    pub(super) async fn dingtalk_fetch(&self, id: &str) -> Result<Content> {
        const SIZE: usize = 100;
        let path = format!("/v1.0/doc/suites/documents/{}/blocks", id_part(id)?);
        let mut out = String::new();
        let mut seen = HashSet::new();
        for page in 0..1000 {
            let start = (page * SIZE).to_string();
            let end = ((page + 1) * SIZE - 1).to_string();
            let data = self
                .get(
                    &path,
                    &[
                        ("operatorId", &self.connection.address),
                        ("startIndex", &start),
                        ("endIndex", &end),
                    ],
                )
                .await?;
            ensure!(data["success"] == true, "钉钉未返回文档正文");
            let blocks = list(&data["result"], "data")?;
            for block in blocks {
                render_block(block, &mut out);
            }
            if blocks.len() < SIZE {
                return Ok(markdown(out));
            }
            ensure!(
                seen.insert(crate::online_import::digest(
                    serde_json::to_string(blocks)?.as_bytes()
                )),
                "钉钉文档分页返回了重复内容"
            );
        }
        anyhow::bail!("钉钉文档分页超出上限")
    }
}

fn escape(text: &str) -> String {
    let mut out = String::new();
    for character in text.chars() {
        if "\\`*_[]<>|".contains(character) {
            out.push('\\');
        }
        out.push(character);
    }
    out
}

fn inline(value: &Value) -> String {
    let children = value["children"]
        .as_array()
        .map(|children| children.iter().map(inline).collect::<String>())
        .unwrap_or_default();
    let properties = &value["properties"];
    let mut text = value["text"].as_str().map(escape).unwrap_or(children);
    if value["bold"] == true && !text.is_empty() {
        text = format!("**{text}**");
    }
    if value["italic"] == true && !text.is_empty() {
        text = format!("*{text}*");
    }
    if value["strike"] == true || value["stike"] == true {
        text = format!("~~{text}~~");
    }
    let kind = value["elementType"].as_str().unwrap_or("");
    let url = match kind {
        "link" => properties["href"].as_str(),
        "image" => properties["src"].as_str(),
        _ => None,
    }
    .and_then(|url| Url::parse(url).ok())
    .filter(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.username().is_empty()
            && url.password().is_none()
    });
    match (kind, url) {
        ("image", Some(url)) => format!("![图片](<{url}>)"),
        ("link", Some(url)) if text.is_empty() => format!("<{url}>"),
        ("link", Some(url)) => format!("[{text}](<{url}>)"),
        _ => text,
    }
}

fn render_block(block: &Value, out: &mut String) {
    let kind = block["blockType"].as_str().unwrap_or("");
    let details = &block[kind];
    let children = block["children"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let raw = details["text"].as_str().unwrap_or("");
    let inlines: String = children
        .iter()
        .filter(|child| child["blockType"].is_null())
        .map(inline)
        .collect();
    let text = if inlines.is_empty() {
        escape(raw)
    } else {
        inlines
    };
    let mut paragraph = true;
    match kind {
        "heading" if !text.is_empty() => {
            let level = details["level"]
                .as_i64()
                .or_else(|| {
                    details["level"].as_str().and_then(|s| {
                        s.trim_start_matches("heading-")
                            .trim_start_matches('h')
                            .parse()
                            .ok()
                    })
                })
                .unwrap_or(1)
                .clamp(1, 6);
            out.push_str(&format!("{} {text}\n", "#".repeat(level as usize)));
        }
        "orderedList" | "unorderedList" if !text.is_empty() => {
            let level = details["list"]["level"].as_u64().unwrap_or(0).min(20);
            let prefix = if kind == "orderedList" { "1. " } else { "- " };
            let indent = "  ".repeat(level as usize);
            let body = text.replace('\n', &format!("\n{indent}{}", " ".repeat(prefix.len())));
            out.push_str(&format!("{indent}{prefix}{body}\n"));
            paragraph = false;
        }
        "blockquote" => {
            for line in text.lines() {
                out.push_str(&format!("> {line}\n"));
            }
        }
        "code" if !raw.is_empty() => {
            let fence = "`".repeat(
                raw.split(|c| c != '`')
                    .map(str::len)
                    .max()
                    .unwrap_or(0)
                    .max(2)
                    + 1,
            );
            let syntax = details["syntax"]
                .as_str()
                .filter(|name| {
                    name.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_+".contains(&b))
                })
                .unwrap_or("");
            out.push_str(&format!("{fence}{syntax}\n{raw}\n{fence}\n"));
        }
        "table" => {
            if let Some(rows) = details["cells"].as_array() {
                let columns = rows
                    .iter()
                    .filter_map(Value::as_array)
                    .map(Vec::len)
                    .max()
                    .unwrap_or(0);
                for (index, row) in rows.iter().enumerate().filter(|(_, _)| columns > 0) {
                    let cells = (0..columns)
                        .map(|column| {
                            let cell = &row[column];
                            cell.as_str()
                                .map(escape)
                                .or_else(|| cell.as_bool().map(|b| b.to_string()))
                                .or_else(|| cell.as_number().map(ToString::to_string))
                                .unwrap_or_else(|| inline(cell))
                                .replace(['\r', '\n'], " ")
                        })
                        .collect::<Vec<_>>();
                    out.push_str(&format!("| {} |\n", cells.join(" | ")));
                    if index == 0 {
                        out.push_str(&format!("| {} |\n", vec!["---"; columns].join(" | ")));
                    }
                }
            }
        }
        _ if !text.is_empty() => out.push_str(&format!("{text}\n")),
        _ => paragraph = false,
    }
    if paragraph {
        out.push('\n');
    }
    for child in children
        .iter()
        .filter(|child| !child["blockType"].is_null())
    {
        render_block(child, out);
    }
}
