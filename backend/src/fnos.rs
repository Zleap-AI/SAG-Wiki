use sag_core::store::FileAccessMode;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::{Path, PathBuf}, time::Duration};

pub const APP_NAME: &str = "sag-desktop-fnos";
pub const PREFIX: &str = "/app/sag-desktop-fnos";

#[derive(Clone)]
pub struct Fnos {
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct Response {
    code: i64,
    msg: String,
    data: Value,
}

#[derive(Deserialize)]
pub struct Acl {
    pub path: String,
    pub readable: bool,
    pub writable: bool,
    pub deletable: bool,
}

#[derive(Deserialize, serde::Serialize)]
pub struct SemanticPath {
    path: String,
    #[serde(rename = "semanticPath")]
    semantic_path: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ConvertedPaths {
    Paths(Vec<SemanticPath>),
    Wrapped { status: i64, result: Vec<SemanticPath> },
}

impl Fnos {
    pub fn new(socket: PathBuf) -> Result<Self> {
        Ok(Self { client: reqwest::Client::builder()
            .unix_socket(socket).timeout(Duration::from_secs(15)).build()? })
    }

    pub async fn call(&self, req: &str, data: Value) -> Result<Value> {
        let token = std::env::var("TRIM_API_TOKEN").context("飞牛未提供 TRIM_API_TOKEN，请从应用中心重新启动")?;
        let response = self.client.post("http://localhost/api/v1/trimapp")
            .bearer_auth(token)
            .json(&json!({"reqId":uuid::Uuid::new_v4().to_string(),"req":req,"appName":APP_NAME,"data":data}))
            .send().await.context("无法连接飞牛原生 API")?
            .error_for_status().context("飞牛原生 API 请求失败")?
            .json::<Response>().await.context("飞牛原生 API 响应格式无效")?;
        if response.code != 0 { bail!("飞牛 API {}：{}", response.code, response.msg); }
        Ok(response.data)
    }

    pub async fn folders(&self, uid: u32) -> Result<Vec<String>> {
        #[derive(Deserialize)] struct Folders { paths: Vec<String> }
        Ok(serde_json::from_value::<Folders>(self.call("trim.file.getUserAccessibleFolders", json!({"uid":uid})).await?)?.paths)
    }

    pub async fn acl(&self, uid: u32, paths: &[String]) -> Result<Vec<Acl>> {
        if paths.is_empty() { return Ok(vec![]); }
        Ok(serde_json::from_value(self.call("trim.file.checkUserACL", json!({"uid":uid,"path":paths})).await?)?)
    }

    pub async fn convert_paths(&self, paths: &[String], language: &str) -> Result<Vec<SemanticPath>> {
        let value = self.call("trim.file.convertPath", json!({"path":paths,"language":language})).await?;
        // fnOS 1.2.0701 returns the array directly; the developer guide also
        // specifies a status/result envelope. Normalize both at the API boundary.
        match serde_json::from_value(value).context("飞牛路径转换响应格式无效")? {
            ConvertedPaths::Paths(paths) => Ok(paths),
            ConvertedPaths::Wrapped { status: 0, result } => Ok(result),
            ConvertedPaths::Wrapped { status, .. } => bail!("飞牛路径转换失败（状态 {status}）"),
        }
    }

    pub async fn allowed(&self, uid: u32, path: &Path, write: bool) -> Result<bool> {
        self.allowed_action(uid, path, if write { FileAccessMode::Write } else { FileAccessMode::Read }).await
    }

    pub async fn allowed_action(&self, uid: u32, path: &Path, action: FileAccessMode) -> Result<bool> {
        let Some((canonical, acl_path)) = resolve_path(path, action == FileAccessMode::Write)? else { return Ok(false); };
        if action != FileAccessMode::Delete {
            let mut root = PathBuf::new();
            for component in canonical.components() {
                if component.as_os_str() == ".sag-converted" {
                    let relative = canonical.strip_prefix(root.join(".sag-converted"))?;
                    if let Some(name) = relative.file_name().and_then(|name| name.to_str()).and_then(|name| name.strip_suffix(".md")) {
                        let source = root.join(relative).with_file_name(name);
                        if !self.allowed_basic(uid, &source, FileAccessMode::Read).await? { return Ok(false); }
                        if action == FileAccessMode::Write && !self.allowed_basic(uid, source.parent().context("原文件缺少父目录")?, FileAccessMode::Write).await? { return Ok(false); }
                    }
                    break;
                }
                root.push(component);
            }
        }
        self.allowed_resolved(uid, &canonical, &acl_path, action).await
    }

    async fn allowed_basic(&self, uid: u32, path: &Path, action: FileAccessMode) -> Result<bool> {
        let Some((canonical, acl_path)) = resolve_path(path, action == FileAccessMode::Write)? else { return Ok(false); };
        self.allowed_resolved(uid, &canonical, &acl_path, action).await
    }

    async fn allowed_resolved(&self, uid: u32, canonical: &Path, acl_path: &Path, action: FileAccessMode) -> Result<bool> {
        let folders = self.folders(uid).await?;
        let mut within_grant = false;
        for folder in folders {
            let root = match Path::new(&folder).canonicalize() {
                Ok(root) => root,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if canonical.starts_with(root) { within_grant = true; break; }
        }
        if !within_grant { return Ok(false); }
        let paths = vec![acl_path.to_str().context("路径不是有效的 UTF-8")?.to_owned()];
        let acls = self.acl(uid, &paths).await?;
        let acl = acls.iter().find(|acl| acl.path == paths[0]).context("飞牛未返回目标文件的权限")?;
        Ok(acl.readable && match action { FileAccessMode::Read => true, FileAccessMode::Write => acl.writable, FileAccessMode::Delete => acl.deletable, FileAccessMode::Publish => unreachable!("publication is handled by the app service") })
    }

    pub async fn require(&self, uid: u32, path: &str, write: bool) -> Result<()> {
        if !self.allowed(uid, Path::new(path), write).await? {
            bail!("当前用户未授权此路径或没有{}权限：{path}", if write { "写入" } else { "读取" });
        }
        Ok(())
    }
}

// Resolve links before applying source permissions, including aliases of converted files.
// For a new nested output, its nearest existing parent supplies the creation ACL.
fn resolve_path(path: &Path, allow_missing: bool) -> Result<Option<(PathBuf, PathBuf)>> {
        if !path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            bail!("文件路径必须是绝对路径，且不能包含上级目录跳转");
        }
        let mut existing = path.to_path_buf();
        let mut missing = Vec::new();
        let base = loop {
            match existing.canonicalize() {
                Ok(base) => break base,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if !allow_missing { return Ok(None); }
                    if existing.symlink_metadata().is_ok_and(|metadata| metadata.file_type().is_symlink()) {
                        bail!("目标路径包含不可用的符号链接");
                    }
                    missing.push(existing.file_name().context("文件名无效")?.to_owned());
                    existing = existing.parent().context("文件缺少父目录")?.into();
                }
                Err(error) => return Err(error.into()),
            }
        };
        let mut canonical = base.clone();
        for component in missing.iter().rev() { canonical.push(component); }
    let acl_path = if missing.is_empty() { canonical.clone() } else { base };
    Ok(Some((canonical, acl_path)))
}
