use crate::protocol::*;
use crate::{
    models,
    sag_mcp::SagTools,
    store::{self, Store},
};
use anyhow::{bail, Context, Result};
#[cfg(test)]
use std::sync::Weak;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::Duration,
};
#[cfg(test)]
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;

pub use sag_protocol::Access;

pub struct CliService {
    store: Arc<Store>,
    client: reqwest::Client,
    data: PathBuf,
    access: RwLock<Access>,
    token: String,
    stop: CancellationToken,
    _instance: Option<File>,
    preparation: Option<File>,
    add_root: Arc<dyn Fn(&Path) -> Result<String> + Send + Sync>,
    remove_root: Arc<dyn Fn(&Path) -> Result<()> + Send + Sync>,
    standalone: bool,
}
// Held for the full application lifetime, including when access is disabled.
pub fn lock_instance(data: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join("cli-instance.lock"))?;
    file.try_lock()
        .context("SAG 核心正在使用，请稍后重试")?;
    Ok(file)
}
impl CliService {
    pub(crate) fn shared(
        store: Arc<Store>, client: reqwest::Client, data: PathBuf,
        add_root: Arc<dyn Fn(&Path) -> Result<String> + Send + Sync>,
        remove_root: Arc<dyn Fn(&Path) -> Result<()> + Send + Sync>,
    ) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            store, client, access: RwLock::new(Self::load_access(&data)?), data,
            token: String::new(), stop: CancellationToken::new(), _instance: None,
            preparation: None, add_root, remove_root, standalone: false,
        }))
    }
    pub(crate) async fn handle(&self, path: PathBuf, command: Command, cancel: CancellationToken) -> Result<Response> {
        self.execute(Request { protocol_version: VERSION, token: self.token.clone(), path, command }, cancel).await
    }
    #[cfg(test)]
    fn local(data: PathBuf) -> Result<Arc<Self>> {
        fs::create_dir_all(&data)?;
        // Serialize standalone initialization and index writes, not model requests.
        let preparation = OpenOptions::new().create(true).truncate(false).write(true)
            .open(data.join("cli-prepare.lock"))?;
        preparation.lock().context("无法准备 SAG 索引")?;
        let instance = lock_instance(&data)?;
        let store = Arc::new(Store::open(&data.join("sag.sqlite"))?);
        let add_store = store.clone();
        let remove_store = store.clone();
        Ok(Arc::new(Self {
            store,
            client: reqwest::Client::new(),
            access: RwLock::new(Self::load_access(&data)?),
            data,
            token: String::new(),
            stop: CancellationToken::new(),
            _instance: Some(instance),
            preparation: Some(preparation),
            add_root: Arc::new(move |path| {
                let root = add_store.add_root(path)?;
                add_store.scan(&root, |_| {})?;
                Ok(root)
            }),
            remove_root: Arc::new(move |path| remove_store.remove_path(path)),
            standalone: true,
        }))
    }
    fn load_access(data: &Path) -> Result<Access> {
        match fs::read(data.join("cli-access.json")) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Access::default()),
            Err(e) => Err(e.into()),
        }
    }
    #[cfg(test)]
    async fn run_local(&self, path: PathBuf, command: Command, cancel: CancellationToken) -> Result<Response> {
        if matches!(command, Command::Search { .. } | Command::Match(_)) {
            let (scope, _) = self.authorized(&path)?;
            for root in self.store.roots()? {
                if scope.starts_with(&root.path) || Path::new(&root.path).starts_with(&scope) {
                    self.store.scan(&root.path, |_| {})?;
                }
            }
        }
        self.execute(Request { protocol_version: VERSION, token: self.token.clone(), path, command }, cancel).await
    }
    fn release_preparation(&self) -> Result<()> {
        if let Some(preparation) = &self.preparation {
            self._instance.as_ref().unwrap().unlock()?;
            preparation.unlock()?;
        }
        Ok(())
    }
    #[cfg(test)]
    fn start(
        store: Arc<Store>,
        client: reqwest::Client,
        data: PathBuf,
        instance: File,
        add_root: Arc<dyn Fn(&Path) -> Result<String> + Send + Sync>,
        remove_root: Arc<dyn Fn(&Path) -> Result<()> + Send + Sync>,
    ) -> Result<Arc<Self>> {
        let access = Self::load_access(&data)?;
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
            _instance: Some(instance),
            preparation: None,
            add_root,
            remove_root,
            standalone: false,
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
        tokio::spawn(async move {
            let result = Self::listen(listener, weak, stop).await;
            if let Err(e) = result {
                eprintln!("CLI 服务失败：{e:#}");
            }
        });
        Ok(service)
    }
    #[cfg(test)]
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
    pub fn configure(&self, access: Access) -> Result<()> {
        let mut current = self.access.write().unwrap();
        self.save_access(&access)?;
        *current = access;
        Ok(())
    }
    fn indexed_scope(&self, requested: &Path) -> Result<(PathBuf, Vec<crate::types::Root>)> {
        let path = requested.canonicalize()?;
        if !path.is_dir() { bail!("CLI 搜索范围必须是目录") }
        if self.store.is_excluded(&path)? { return Ok((path, vec![])); }
        let roots = self.store.roots()?.into_iter()
            .filter(|root| path.starts_with(&root.path) || Path::new(&root.path).starts_with(&path))
            .collect();
        Ok((path, roots))
    }
    fn authorized(&self, requested: &Path) -> Result<(PathBuf, Vec<String>)> {
        let access = if self.standalone { Self::load_access(&self.data)? } else { self.access() };
        if !access.enabled {
            bail!("CLI 接入未启用，请运行 sag config --enable，或在桌面设置中启用")
        }
        let (path, roots) = self.indexed_scope(requested)?;
        if roots.is_empty() { bail!("该文件夹下没有已索引目录，请运行 sag add") }
        let scopes = roots.into_iter().map(|root| {
            if path.starts_with(&root.path) { store::path_text(&path) } else { Ok(root.path) }
        }).collect::<Result<Vec<_>>>()?;
        Ok((path, scopes))
    }
    pub fn status(&self) -> Result<Status> {
        let access = self.access();
        let settings = self.store.settings()?;
        Ok(Status {
            protocol_version: VERSION,
            connected: !self.standalone,
            enabled: access.enabled,
            jev_configured: models::has_secret(&self.data, "jev")?
                && !settings.jev.model.trim().is_empty(),
        })
    }
    fn index_ready(&self, requested: &Path) -> Result<bool> {
        let (_, roots) = self.indexed_scope(requested)?;
        Ok(!roots.is_empty() && roots.iter().all(|root| root.status == "ready"))
    }
    fn check(&self, requested: &Path) -> Result<CheckOutput> {
        let (path, roots) = self.indexed_scope(requested)?;
        let covering = roots.iter().find(|root| path.starts_with(&root.path));
        let mut excluded = self.store.is_excluded(&path)?;
        if let Some(root) = covering {
            let settings = self.store.settings()?;
            let mut matcher = crate::ignore_rules::matcher(Path::new(&root.path));
            let mut ancestor = PathBuf::from(&root.path);
            for component in path.strip_prefix(&root.path)?.components() {
                ancestor.push(component);
                let name = component.as_os_str();
                if name.as_encoded_bytes().starts_with(b".")
                    || settings.excluded.iter().any(|item| name == item.as_str())
                    || crate::ignore_rules::is_ignored(&mut matcher, &ancestor, true)? {
                    excluded = true;
                    break;
                }
            }
        }
        let coverage = if excluded { IndexCoverage::Excluded }
            else if covering.is_some() { IndexCoverage::Included }
            else if roots.is_empty() { IndexCoverage::None }
            else { IndexCoverage::Partial };
        Ok(CheckOutput {
            path: store::path_text(&path)?,
            coverage,
            index_ready: !excluded && !roots.is_empty() && roots.iter().all(|root| root.status == "ready"),
            enabled: self.access().enabled,
            indexed_files: if excluded { 0 } else { self.store.indexed_file_count(&path)? },
            roots,
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
            Command::Status => Ok(Response::Status(self.status()?)),
            Command::List => Ok(Response::Roots(self.store.roots()?)),
            Command::Check => Ok(Response::Checked(self.check(&request.path)?)),
            Command::Configure { enable, url, model, key } => {
                if enable { self.configure(Access { enabled: true })?; }
                let mut settings = self.store.settings()?;
                let previous = settings.jev.clone();
                let changed = url.is_some() || model.is_some() || key.is_some();
                if changed && !self.access().enabled {
                    bail!("CLI 接入未启用，请运行 sag config --enable，或在桌面设置中启用")
                }
                if let Some(url) = url { settings.jev.url = url; }
                if let Some(model) = model { settings.jev.model = model; }
                if changed {
                    models::check_key_provider(&previous, &settings.jev, key.as_deref().unwrap_or(""), models::has_secret(&self.data, "jev")?)?;
                    // Clear the previous provider's credential before changing its
                    // address. A failed write must never leave a new URL with an old key.
                    if reqwest::Url::parse(&previous.url)?.origin() != reqwest::Url::parse(&settings.jev.url)?.origin() {
                        models::clear_secret(&self.data, "jev")?;
                    }
                    self.store.save_settings(&settings)?;
                    if let Some(key) = key { models::save_secret(&self.data, "jev", &key)?; }
                }
                settings.jev.has_key = models::has_secret(&self.data, "jev")?;
                Ok(Response::Configured(settings.jev))
            }
            Command::Remove => {
                if !self.access().enabled {
                    bail!("CLI 接入未启用，请运行 sag config --enable，或在桌面设置中启用")
                }
                let path = match self.store.roots()?.into_iter().find(|root| Path::new(&root.path) == request.path) {
                    Some(root) => PathBuf::from(root.path),
                    None => request.path.canonicalize().context("无法打开这个文件夹")?,
                };
                if path.is_file() { bail!("CLI 移除命令仅支持文件夹") }
                (self.remove_root)(&path)?;
                Ok(Response::Removed { path: store::path_text(&path)? })
            }
            Command::Add => {
                if !self.access().enabled {
                    bail!("CLI 接入未启用，请运行 sag config --enable，或在桌面设置中启用")
                }
                let path = request.path.canonicalize().context("无法打开这个文件夹")?;
                if !path.is_dir() { bail!("请选择文件夹") }
                (self.add_root)(&path)?;
                let root = loop {
                    let root=self.store.roots()?.into_iter().find(|item|path.starts_with(&item.path)).context("目录已从知识库移除，请重新添加")?;
                    if self._instance.is_some() || root.status=="ready" || root.error.is_some(){break root;}
                    tokio::select!{_=cancel.cancelled()=>bail!("添加操作已取消，后台索引继续执行"),_=tokio::time::sleep(Duration::from_millis(20))=>{}}
                };
                if self.standalone || self._instance.is_none() {
                    if let Some(error) = root.error { bail!("目录已添加，但索引未完成：{error}") }
                }
                Ok(Response::Added(AddOutput { path: store::path_text(&path)?, index_status: root.status, cli_authorized: true }))
            }
            Command::Match(requested) => {
                if requested.max_results.is_some_and(|n| !(1..=100).contains(&n)) || !(1..=86400).contains(&requested.timeout)
                    || requested.options.before_context > 1000 || requested.options.after_context > 1000 {
                    bail!("搜索参数超出允许范围")
                }
                let (path, scopes) = self.authorized(&request.path)?;
                if !self.index_ready(&path)? {
                    bail!("目录索引未就绪，请运行 sag add 重试或检查桌面端索引错误")
                }
                self.release_preparation()?;
                let tools = SagTools {
                    store: self.store.clone(), client: self.client.clone(), data_dir: self.data.clone(),
                    settings: self.store.settings()?, scope: scopes, references: vec![],
                    enabled: vec![], cancel: cancel.clone(), progress: Arc::new(|_, _| {}),
                };
                let timeout = requested.timeout;
                let task = tokio::task::spawn_blocking(move || crate::cli_match::run(tools, path, requested));
                let output = match tokio::time::timeout(Duration::from_secs(timeout), task).await {
                    Ok(result) => result.context("本地匹配任务失败")??,
                    Err(error) => { cancel.cancel(); return Err(error).context("搜索超时，请增加 --timeout 或缩小范围"); }
                };
                self.authorized(&request.path)?;
                Ok(Response::Matches(output))
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
                let (path, _) = self.authorized(&request.path)?;
                if !self.index_ready(&path)? {
                    bail!("目录索引未就绪，请运行 sag add 重试或检查桌面端索引错误")
                }
                if !self.status()?.jev_configured {
                    bail!("JEV 未配置，请运行 sag config，或在 SAG Wiki 模型设置中配置")
                }
                self.release_preparation()?;
                let operation = async {
                    let (path, scopes) = self.authorized(&request.path)?;
                    let scope = store::path_text(&path)?;
                    let tools = SagTools {
                        store: self.store.clone(),
                        client: self.client.clone(),
                        data_dir: self.data.clone(),
                        settings: self.store.settings()?,
                        scope: scopes.clone(),
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
                    let mut groups = indexmap::IndexMap::<(String, String), (String, Vec<(usize, usize)>)>::new();
                    for candidate in output.sources {
                        let (_, ranges) = groups.entry((candidate.path, candidate.heading)).or_insert_with(|| (candidate.hash, vec![]));
                        ranges.extend(candidate.source_spans.iter().map(|span| (span.start_byte, span.end_byte)));
                    }
                    for ((candidate_path, heading), (hash, mut ranges)) in groups {
                        let document = self.store.document(Path::new(&candidate_path))?;
                        if document.hash != hash {
                            bail!("文件已修改，请重新搜索：{}", candidate_path)
                        }
                        let canonical = Path::new(&candidate_path).canonicalize()?;
                        if !canonical.starts_with(&path) || !scopes.iter().any(|scope| canonical.starts_with(scope)) {
                            bail!("搜索结果超出授权目录")
                        }
                        // Merge selected source ranges before rendering overlap or repeated table headers.
                        ranges.sort_unstable();
                        let mut merged: Vec<(usize, usize)> = vec![];
                        for (start, end) in ranges {
                            if let Some(previous) = merged.last_mut().filter(|previous| start <= previous.1) {
                                previous.1 = previous.1.max(end);
                            } else { merged.push((start, end)); }
                        }
                        let lines = crate::text::Lines::new(&document.body);
                        for (start, end) in merged {
                            let span = lines.span(&document.body, start, end);
                            let relative = canonical.strip_prefix(&path)?;
                            result.results.push(SearchResult {
                                heading: heading.clone(),
                                path: format!("./{}", relative.display()),
                                absolute_path: store::path_text(&canonical)?,
                                start_line: span.start_line,
                                end_line: span.end_line,
                                content: content.then(|| {
                                    document.body[span.start_byte..span.end_byte].to_owned()
                                }),
                                matched_lines: vec![],
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
    #[cfg(test)]
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
                _ = self.stop.cancelled() => { bail!("SAG Wiki 正在退出") }
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
        if self.standalone || self._instance.is_none() { return; }
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
