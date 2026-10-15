use crate::cli_protocol::*;
use crate::{
    models,
    sag_mcp::SagTools,
    store::{self, Store},
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, RwLock, Weak},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Access {
    pub enabled: bool,
    pub roots: Vec<String>,
}

pub struct CliService {
    store: Arc<Store>,
    client: reqwest::Client,
    data: PathBuf,
    access: RwLock<Access>,
    token: String,
    stop: CancellationToken,
    query_lock: Arc<tokio::sync::Mutex<()>>,
    _instance: File,
    add_root: Arc<dyn Fn(&Path) -> Result<String> + Send + Sync>,
}
// Held for the full application lifetime, including when access is disabled.
pub fn lock_instance(data: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join("cli-instance.lock"))?;
    file.try_lock()
        .context("SAG-Desktop 已在运行，请使用已有窗口")?;
    Ok(file)
}
impl CliService {
    pub fn start(
        store: Arc<Store>,
        client: reqwest::Client,
        data: PathBuf,
        query_lock: Arc<tokio::sync::Mutex<()>>,
        instance: File,
        add_root: Arc<dyn Fn(&Path) -> Result<String> + Send + Sync>,
    ) -> Result<Arc<Self>> {
        let access_file = data.join("cli-access.json");
        let access = match fs::read(access_file) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Access::default(),
            Err(e) => return Err(e.into()),
        };
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let endpoint = Endpoint {
            protocol_version: VERSION,
            port: listener.local_addr()?.port(),
            pid: std::process::id(),
            token: token.clone(),
        };
        let service = Arc::new(Self {
            store,
            client,
            data,
            access: RwLock::new(access),
            token,
            stop: CancellationToken::new(),
            query_lock,
            _instance: instance,
            add_root,
        });
        let path = service.data.join("cli-endpoint.json");
        // The private file is replaced atomically; stale records never claim a live port.
        let mut temp = tempfile::NamedTempFile::new_in(&service.data)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            temp.as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        #[cfg(windows)]
        {
            let username = std::env::var("USERNAME").context("无法获取当前用户")?;
            let result = std::process::Command::new("icacls")
                .arg(temp.path())
                .args(["/inheritance:r", "/grant:r", &format!("{username}:F")])
                .output()?;
            if !result.status.success() {
                bail!("无法保护 CLI 连接记录")
            }
        }
        temp.write_all(&serde_json::to_vec(&endpoint)?)?;
        temp.persist(path)?;
        let weak = Arc::downgrade(&service);
        let stop = service.stop.clone();
        tauri::async_runtime::spawn(async move {
            let result = Self::listen(listener, weak, stop).await;
            if let Err(e) = result {
                eprintln!("CLI 服务失败：{e:#}");
            }
        });
        Ok(service)
    }
    async fn listen(
        listener: std::net::TcpListener,
        weak: Weak<Self>,
        stop: CancellationToken,
    ) -> Result<()> {
        let listener = TcpListener::from_std(listener)?;
        let connections = Arc::new(tokio::sync::Semaphore::new(20));
        loop {
            let (stream, _) = tokio::select! { _ = stop.cancelled() => return Ok(()), result = listener.accept() => result? };
            let permit = connections.clone().try_acquire_owned();
            let Ok(permit) = permit else {
                let mut stream = stream;
                let refusal = tokio::time::timeout(
                    Duration::from_secs(1),
                    stream.write_all(
                        b"{\"type\":\"error\",\"data\":\"CLI connections busy; retry later\"}\n",
                    ),
                )
                .await;
                if !matches!(refusal, Ok(Ok(()))) {
                    eprintln!("CLI 忙碌连接无法写入错误响应");
                }
                continue;
            };
            let Some(service) = weak.upgrade() else {
                return Ok(());
            };
            tokio::spawn(async move {
                let _permit = permit;
                if let Err(e) = service.serve(stream).await {
                    eprintln!("CLI 连接失败：{e:#}");
                }
            });
        }
    }
    pub fn access(&self) -> Access {
        self.access.read().unwrap().clone()
    }
    fn save_access(&self, access: &Access) -> Result<()> {
        let mut temp = tempfile::NamedTempFile::new_in(&self.data)?;
        temp.write_all(&serde_json::to_vec(access)?)?;
        temp.persist(self.data.join("cli-access.json"))?;
        Ok(())
    }
    pub fn configure(&self, mut access: Access) -> Result<()> {
        let mut current = self.access.write().unwrap();
        let roots = self.store.roots()?;
        for root in &mut access.roots {
            if current.roots.contains(root) || roots.iter().any(|r| r.path == *root) {
                continue;
            }
            let canonical = Path::new(root).canonicalize()?;
            if !canonical.is_dir() || !roots.iter().any(|r| canonical.starts_with(&r.path)) {
                bail!("请选择已添加的文件夹或其子目录")
            }
            *root = store::path_text(&canonical)?;
        }
        access.roots.sort();
        access.roots.dedup();
        self.save_access(&access)?;
        *current = access;
        Ok(())
    }
    pub fn revoke_root(&self, path: &str) -> Result<()> {
        let mut current = self.access.write().unwrap();
        let mut next = current.clone();
        next.roots.retain(|root| !Path::new(root).starts_with(path));
        self.save_access(&next)?;
        *current = next;
        Ok(())
    }
    fn authorized(&self, requested: &Path) -> Result<PathBuf> {
        let path = self.store.authorize(requested)?;
        if !path.is_dir() {
            bail!("CLI 搜索范围必须是目录")
        }
        let access = self.access();
        if !access.enabled {
            bail!("CLI 接入未启用，请在 SAG-Desktop 设置中启用")
        }
        if !access.roots.iter().any(|root| path.starts_with(root)) {
            bail!("目录未授权，请在 SAG-Desktop 的 CLI 设置中授权该目录")
        }
        Ok(path)
    }
    pub fn status(&self, path: &Path) -> Result<Status> {
        let access = self.access();
        let canonical = path.canonicalize()?;
        let roots = self.store.roots()?;
        let selected: Vec<_> = roots
            .iter()
            .filter(|r| canonical.starts_with(&r.path))
            .collect();
        let allowed = canonical.is_dir()
            && !selected.is_empty()
            && access.roots.iter().any(|r| canonical.starts_with(r));
        let settings = self.store.settings()?;
        Ok(Status {
            protocol_version: VERSION,
            connected: true,
            enabled: access.enabled,
            allowed,
            index_ready: allowed && selected.iter().all(|r| r.status == "ready"),
            jev_configured: models::has_secret(&self.data, "jev")?
                && !settings.jev.model.trim().is_empty(),
            path: store::path_text(&canonical)?,
        })
    }
    async fn execute(&self, request: Request, cancel: CancellationToken) -> Result<Response> {
        if request.protocol_version != VERSION {
            bail!("CLI 协议版本不兼容，请重新安装 CLI")
        }
        if request.token != self.token {
            bail!("CLI 访问令牌无效，请重试连接")
        }
        if !request.path.is_absolute() {
            bail!("请求路径必须是完整路径")
        }
        match request.command {
            Command::Status => Ok(Response::Status(self.status(&request.path)?)),
            Command::Add => {
                let mut current = self.access.write().unwrap();
                if !current.enabled {
                    bail!("CLI 接入未启用，请在 SAG-Desktop 设置中启用")
                }
                let path = request.path.canonicalize().context("无法打开这个文件夹")?;
                if !path.is_dir() { bail!("请选择文件夹") }
                (self.add_root)(&path)?;
                let mut next = current.clone();
                next.roots.push(store::path_text(&path)?);
                next.roots.sort();
                next.roots.dedup();
                self.save_access(&next)?;
                *current = next;
                let index_status = self.store.roots()?.into_iter()
                    .find(|item| path.starts_with(&item.path))
                    .context("目录已从知识库移除，请重新添加")?.status;
                Ok(Response::Added(AddOutput { path: store::path_text(&path)?, index_status, cli_authorized: true }))
            }
            Command::Search {
                query,
                max_results,
                content,
                timeout,
            } => {
                if !(1..=100).contains(&max_results) || !(1..=86400).contains(&timeout) {
                    bail!("搜索参数超出允许范围")
                }
                let path = self.authorized(&request.path)?;
                let status = self.status(&path)?;
                if !status.index_ready {
                    bail!("目录索引未就绪，请等待 SAG-Desktop 完成索引")
                }
                if !status.jev_configured {
                    bail!("JEV 未配置，请在 SAG-Desktop 模型设置中配置")
                }
                let operation = async {
                    let _guard = self.query_lock.lock().await;
                    let path = self.authorized(&request.path)?;
                    let scope = store::path_text(&path)?;
                    let tools = SagTools {
                        store: self.store.clone(),
                        client: self.client.clone(),
                        data_dir: self.data.clone(),
                        settings: self.store.settings()?,
                        scope: vec![scope.clone()],
                        references: vec![],
                        enabled: vec!["search".into()],
                        cancel,
                        progress: Arc::new(|_, _| {}),
                    };
                    let output = tools
                        .execute(
                            "search",
                            serde_json::json!({"query":query,"top_k":max_results}),
                        )
                        .await?;
                    // Recheck authorization after awaiting remote requests, including revocation.
                    self.authorized(&request.path)?;
                    let mut result = SearchOutput {
                        protocol_version: VERSION,
                        scope,
                        results: vec![],
                        truncated: false,
                    };
                    'candidates: for candidate in output.sources {
                        let document = self.store.document(Path::new(&candidate.path))?;
                        if document.hash != candidate.hash {
                            bail!("文件已修改，请重新搜索：{}", candidate.path)
                        }
                        let canonical = Path::new(&candidate.path).canonicalize()?;
                        if !canonical.starts_with(&path) {
                            bail!("搜索结果超出授权目录")
                        }
                        // Each source span is contiguous. Do not label separated spans as one passage.
                        for span in candidate.source_spans {
                            if result.results.len() == max_results {
                                break 'candidates;
                            }
                            let relative = canonical.strip_prefix(&path)?;
                            result.results.push(SearchResult {
                                path: format!("./{}", relative.display()),
                                absolute_path: store::path_text(&canonical)?,
                                start_line: span.start_line,
                                end_line: span.end_line,
                                content: content.then(|| {
                                    document.body[span.start_byte..span.end_byte].to_owned()
                                }),
                            });
                            if serde_json::to_vec(&result)?.len() as u64 > MAX_RESPONSE - 1024 {
                                result.results.pop();
                                result.truncated = true;
                                break;
                            }
                        }
                        if result.truncated {
                            break;
                        }
                    }
                    Ok(Response::Search(result))
                };
                tokio::time::timeout(Duration::from_secs(timeout), operation)
                    .await
                    .context("搜索超时，请增加 --timeout 或缩小范围")?
            }
        }
    }
    async fn serve(&self, stream: TcpStream) -> Result<()> {
        let (read, mut write) = stream.into_split();
        let mut read = BufReader::new(read);
        let mut line = Vec::new();
        let response = async {
            tokio::time::timeout(
                Duration::from_secs(10),
                (&mut read)
                    .take(MAX_REQUEST + 1)
                    .read_until(b'\n', &mut line),
            )
            .await
            .context("请求读取超时")??;
            if line.len() as u64 > MAX_REQUEST || !line.ends_with(b"\n") {
                bail!("请求不完整或超过长度上限")
            }
            let request: Request = serde_json::from_slice(&line).context("请求格式无效")?;
            let cancel = CancellationToken::new();
            let drop_guard = cancel.clone().drop_guard();
            let mut next = [0u8; 1];
            let response = tokio::select! {
                result = self.execute(request, cancel) => result,
                result = read.read(&mut next) => { result?; bail!("客户端已断开或发送了额外数据") },
                _ = self.stop.cancelled() => { bail!("SAG-Desktop 正在退出") }
            };
            drop(drop_guard);
            response
        }
        .await;
        let response = response.unwrap_or_else(|e| Response::Error(format!("{e:#}")));
        let mut bytes = serde_json::to_vec(&response)?;
        if bytes.len() as u64 >= MAX_RESPONSE {
            bytes =
                serde_json::to_vec(&Response::Error("响应超过长度上限，请缩小搜索范围".into()))?;
        }
        bytes.push(b'\n');
        tokio::time::timeout(Duration::from_secs(10), write.write_all(&bytes)).await??;
        Ok(())
    }
    pub fn shutdown(&self) {
        self.stop.cancel();
        match fs::remove_file(self.data.join("cli-endpoint.json")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => eprintln!("清理 CLI 连接记录失败：{e}"),
        }
    }
}
impl Drop for CliService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
#[path = "cli_service_tests.rs"]
mod tests;
