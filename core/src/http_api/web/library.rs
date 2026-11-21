use super::{response, ApiError, HttpResponse};
use crate::{
    indexer::{self, IndexRequest},
    service::Service,
    store::path_text,
};
use anyhow::{ensure, Context, Result};
use http_body_util::BodyExt;
use hyper::{body::Incoming, header, Request, StatusCode};
use sag_protocol::{EventEnvelope, ServiceEvent};
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
    time::Duration,
};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

const MAX_UPLOAD_BYTES: usize = 512 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(tag = "operation", content = "args", rename_all = "snake_case")]
pub(super) enum Operation {
    WebLibrary,
    CreateWebFolder {
        parent: Option<String>,
        name: String,
    },
}

fn library(service: &Service) -> Result<PathBuf> {
    Ok(service.data_dir.canonicalize()?.join("web-library"))
}

fn component(name: &str) -> Result<&str> {
    ensure!(
        !name.is_empty()
            && !name.starts_with('.')
            && !name.ends_with([' ', '.'])
            && name.len() <= 255
            && !name
                .chars()
                .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c)),
        "文件或文件夹名称无效"
    );
    Ok(name)
}

fn create_directory(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => anyhow::bail!("文件夹已存在"),
        Err(error) => Err(error.into()),
    }
}

fn destination(service: &Service, library: &Path, path: &Path) -> Result<(PathBuf, String)> {
    let relative = path
        .strip_prefix(library)
        .context("请选择网页创建的文件夹")?;
    ensure!(!relative.as_os_str().is_empty(), "请选择网页创建的文件夹");
    for part in relative.components() {
        let Component::Normal(name) = part else {
            anyhow::bail!("请选择网页创建的文件夹");
        };
        component(name.to_str().context("文件或文件夹名称无效")?)?;
    }
    let actual = path.canonicalize()?;
    ensure!(actual == path && actual.is_dir(), "请选择网页创建的文件夹");
    service.store.authorize(&actual)?;
    let root = service
        .store
        .roots()?
        .into_iter()
        .find(|root| actual.starts_with(&root.path))
        .context("文件夹未加入知识库")?;
    Ok((actual, root.path))
}

fn changed(service: &Service) {
    let _ = service.events.send((
        None,
        EventEnvelope {
            request_id: None,
            event: ServiceEvent::DataChanged,
        },
    ));
}

pub(super) fn operate(
    operation: Operation,
    service: &Service,
    cancel: &CancellationToken,
) -> Result<String> {
    let library = library(service)?;
    match operation {
        Operation::WebLibrary => path_text(&library),
        Operation::CreateWebFolder { parent, name } => {
            let name = component(name.trim())?;
            ensure!(!cancel.is_cancelled(), "已停止");
            let path = if let Some(parent) = parent {
                let (parent, _) = destination(service, &library, Path::new(&parent))?;
                let path = parent.join(name);
                create_directory(&path)?;
                path
            } else {
                fs::create_dir_all(&library)?;
                ensure!(library.canonicalize()? == library, "网页文件存放位置无效");
                let path = library.join(name);
                create_directory(&path)?;
                let root = match service.store.add_root(&path) {
                    Ok(root) => root,
                    Err(error) => {
                        fs::remove_dir(&path)
                            .with_context(|| format!("添加文件夹失败：{error}"))?;
                        return Err(error);
                    }
                };
                indexer::queue_scan(&service.store, &service.scan, root)?;
                path
            };
            changed(service);
            path_text(&path.canonicalize()?)
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UploadTarget {
    folder: String,
    path: String,
}

pub(super) async fn upload(
    mut request: Request<Incoming>,
    service: &Service,
) -> Result<HttpResponse, ApiError> {
    if request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        != Some("application/octet-stream")
    {
        return Err(ApiError::bad("需要 application/octet-stream"));
    }
    let url = reqwest::Url::parse(&format!("http://localhost{}", request.uri()))
        .map_err(ApiError::bad)?;
    let metadata = Value::Object(
        url.query_pairs()
            .map(|(name, value)| (name.into_owned(), Value::String(value.into_owned())))
            .collect(),
    );
    let target: UploadTarget = serde_json::from_value(metadata).map_err(ApiError::bad)?;
    let parts = target
        .path
        .split('/')
        .map(component)
        .collect::<Result<Vec<_>>>()
        .map_err(ApiError::bad)?;
    let (name, folders) = parts
        .split_last()
        .expect("split always returns at least one part");
    let library = library(service).map_err(ApiError::internal)?;
    destination(service, &library, Path::new(&target.folder)).map_err(ApiError::bad)?;
    let too_large = || {
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "upload_too_large",
            "单个文件不能超过 512 MB",
        )
    };
    if request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok()?.parse::<usize>().ok())
        .is_some_and(|length| length > MAX_UPLOAD_BYTES)
    {
        return Err(too_large());
    }
    let staged = tempfile::NamedTempFile::new_in(&library).map_err(ApiError::internal)?;
    let mut output = tokio::fs::File::from_std(staged.reopen().map_err(ApiError::internal)?);
    let mut size = 0;
    while let Some(frame) =
        tokio::time::timeout(Duration::from_secs(60), request.body_mut().frame())
            .await
            .map_err(ApiError::bad)?
    {
        if let Ok(bytes) = frame.map_err(ApiError::bad)?.into_data() {
            size += bytes.len();
            if size > MAX_UPLOAD_BYTES {
                return Err(too_large());
            }
            output.write_all(&bytes).await.map_err(ApiError::internal)?;
        }
    }
    output.flush().await.map_err(ApiError::internal)?;
    drop(output);
    // Check the destination again after the transfer, before creating or publishing any files.
    let (mut parent, root) =
        destination(service, &library, Path::new(&target.folder)).map_err(ApiError::bad)?;
    let path = folders
        .iter()
        .fold(parent.clone(), |path, folder| path.join(folder))
        .join(name);
    if service
        .store
        .is_excluded(&path)
        .map_err(ApiError::internal)?
    {
        return Err(ApiError::bad("此路径已从知识库移除，请先重新添加"));
    }
    for folder in folders {
        parent.push(folder);
        match fs::create_dir(&parent) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                if !fs::symlink_metadata(&parent)
                    .map_err(ApiError::bad)?
                    .is_dir()
                {
                    return Err(ApiError::bad("上传路径与已有文件冲突"));
                }
            }
            Err(error) => return Err(ApiError::internal(error)),
        }
    }
    staged.persist_noclobber(&path).map_err(|error| {
        if error.error.kind() == ErrorKind::AlreadyExists {
            ApiError::new(StatusCode::CONFLICT, "file_exists", "同名文件已存在")
        } else {
            ApiError::internal(error)
        }
    })?;
    service
        .scan
        .send(IndexRequest::Paths {
            root,
            paths: vec![path],
        })
        .map_err(ApiError::internal)?;
    changed(service);
    Ok(response(StatusCode::CREATED, Value::Null))
}
