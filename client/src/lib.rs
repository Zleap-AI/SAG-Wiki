pub mod wire;
use anyhow::{bail, Context, Result};
use sag_protocol::*;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    io::BufReader,
    net::TcpStream,
    sync::{broadcast, mpsc, oneshot},
};

type Pending = HashMap<String, oneshot::Sender<Result<Output, String>>>;
struct Inner {
    alive: AtomicBool,
    info: ServiceInfo,
    outgoing: mpsc::Sender<Frame>,
    pending: Mutex<Pending>,
    events: broadcast::Sender<EventEnvelope>,
}
#[derive(Clone)]
pub struct Client(Arc<Inner>);
impl Client {
    pub fn is_connected(&self) -> bool {
        self.0.alive.load(Ordering::SeqCst)
    }
    pub fn info(&self) -> &ServiceInfo {
        &self.0.info
    }
    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope> {
        self.0.events.subscribe()
    }
    pub async fn connect(data: &Path) -> Result<Option<Self>> {
        let bytes = match std::fs::read(data.join("sag-service.json")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let endpoint: Endpoint = serde_json::from_slice(&bytes).context("SAG 服务连接记录无效")?;
        if endpoint.protocol_version != VERSION {
            bail!(
                "SAG 服务协议版本 {} 与客户端 {} 不兼容，请升级桌面端或 CLI；现有服务未修改",
                endpoint.protocol_version,
                VERSION
            )
        }
        let stream = match tokio::time::timeout(
            Duration::from_secs(3),
            TcpStream::connect(("127.0.0.1", endpoint.port)),
        )
        .await
        {
            Ok(Ok(stream)) => stream,
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                return Ok(None)
            }
            Ok(Err(error)) => return Err(error.into()),
            Err(error) => return Err(error).context("连接 SAG 服务超时"),
        };
        stream.set_nodelay(true)?;
        let (read, mut write) = stream.into_split();
        let mut read = BufReader::new(read);
        wire::write(
            &mut write,
            &Frame::Hello {
                protocol_version: VERSION,
                token: endpoint.token,
            },
        )
        .await?;
        let frame = tokio::time::timeout(Duration::from_secs(10), wire::read(&mut read))
            .await
            .context("SAG 服务握手超时")??;
        let info = match frame {
            Some(Frame::Welcome { info }) => info,
            _ => bail!("SAG 服务握手失败"),
        };
        if info.protocol_version != VERSION || info.pid != endpoint.pid {
            bail!("SAG 服务身份不匹配，请重新连接")
        }
        let (outgoing, mut incoming) = mpsc::channel(32);
        let (events, _) = broadcast::channel(256);
        let inner = Arc::new(Inner {
            alive: AtomicBool::new(true),
            info,
            outgoing,
            pending: Mutex::new(HashMap::new()),
            events,
        });
        let weak = Arc::downgrade(&inner);
        tokio::spawn(async move {
            while let Some(frame) = incoming.recv().await {
                if let Err(error) = wire::write(&mut write, &frame).await {
                    if let Some(inner) = weak.upgrade() {
                        inner.fail(format!("SAG 服务写入失败：{error}"));
                    }
                    break;
                }
            }
        });
        let weak = Arc::downgrade(&inner);
        tokio::spawn(async move {
            let result = async {
                while let Some(frame) = wire::read(&mut read).await? {
                    let Some(inner) = weak.upgrade() else {
                        break;
                    };
                    match frame {
                        Frame::Response { id, result } => {
                            if let Some(sender) = inner.pending.lock().unwrap().remove(&id) {
                                let _ = sender.send(result);
                            }
                        }
                        Frame::Event { envelope } => {
                            let _ = inner.events.send(envelope);
                        }
                        _ => bail!("SAG 服务返回了无效消息"),
                    }
                }
                Ok::<_, anyhow::Error>(())
            }
            .await;
            if let Some(inner) = weak.upgrade() {
                inner.fail(match result {
                    Ok(()) => "SAG 服务连接已断开".into(),
                    Err(error) => format!("SAG 服务连接失败：{error}"),
                });
            }
        });
        Ok(Some(Self(inner)))
    }
    pub async fn call(&self, operation: Operation, timeout: Duration) -> Result<Output> {
        if !self.is_connected() {
            bail!("SAG 服务连接已关闭")
        }
        let id = uuid::Uuid::new_v4().to_string();
        let (sender, receiver) = oneshot::channel();
        self.0.pending.lock().unwrap().insert(id.clone(), sender);
        if self
            .0
            .outgoing
            .send(Frame::Request {
                id: id.clone(),
                operation,
            })
            .await
            .is_err()
        {
            self.0.pending.lock().unwrap().remove(&id);
            bail!("SAG 服务连接已关闭")
        }
        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(Ok(output))) => Ok(output),
            Ok(Ok(Err(error))) => bail!("{error}"),
            Ok(Err(_)) => bail!("SAG 服务未完成请求"),
            Err(_) => {
                self.0.pending.lock().unwrap().remove(&id);
                let _ = self.0.outgoing.send(Frame::Cancel { id }).await;
                bail!("请求超时，已取消本次请求")
            }
        }
    }
    pub async fn execute(
        &self,
        path: PathBuf,
        command: Command,
        timeout: Duration,
    ) -> Result<Response> {
        match self.call(Operation::Cli { path, command }, timeout).await? {
            Output::Cli(response) => Ok(response),
            _ => bail!("SAG 服务响应类型不匹配"),
        }
    }
}
impl Inner {
    fn fail(&self, message: String) {
        if !self.alive.swap(false, Ordering::SeqCst) {
            return;
        }
        for (_, sender) in self.pending.lock().unwrap().drain() {
            let _ = sender.send(Err(message.clone()));
        }
        let _ = self.events.send(EventEnvelope {
            request_id: None,
            event: ServiceEvent::ServiceDisconnected(message),
        });
    }
}
#[cfg(windows)]
fn prevent_standard_handle_inheritance() -> std::io::Result<()> {
    const HANDLE_FLAG_INHERIT: u32 = 1;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(kind: u32) -> *mut std::ffi::c_void;
        fn SetHandleInformation(handle: *mut std::ffi::c_void, mask: u32, flags: u32) -> i32;
    }
    // Stdio::null selects the daemon's standard streams, but CreateProcess
    // also inherits unrelated inheritable handles. In particular, retaining
    // our original capture pipes prevents callers from observing EOF.
    for kind in [-10_i32, -11, -12] {
        let handle = unsafe { GetStdHandle(kind as u32) };
        if handle.is_null() || handle == -1_isize as *mut std::ffi::c_void {
            continue; // A GUI process may have no standard handles.
        }
        if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}
pub fn spawn_service(data: &Path, executable: &Path) -> Result<()> {
    let mut command = std::process::Command::new(executable);
    command
        .args(["service", "serve"])
        .env("SAG_DATA_DIR", data)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        prevent_standard_handle_inheritance().context("无法隔离 SAG 后台服务的标准输入输出")?;
        command.creation_flags(0x00000008 | 0x00000200);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("无法启动 SAG 服务：{}", executable.display()))?;
    std::thread::spawn(move || {
        if let Err(error) = child.wait() {
            eprintln!("SAG 服务进程等待失败：{error}");
        }
    });
    Ok(())
}
pub async fn ensure(data: &Path, executable: &Path) -> Result<Client> {
    ensure_build(data, executable, &BuildInfo::current()).await
}

async fn ensure_build(data: &Path, executable: &Path, current: &BuildInfo) -> Result<Client> {
    let connected = Client::connect(data).await?;
    if let Some(client) = &connected {
        let running = BuildInfo::from(client.info());
        running.check_protocol(current)?;
        if running.build_id == current.build_id && current.channel != "release" {
            return Ok(client.clone());
        }
    }
    let output = std::process::Command::new(executable)
        .arg("--build-info")
        .output()
        .context("无法检查 SAG 服务构建")?;
    if !output.status.success() {
        bail!(
            "无法检查 SAG 服务构建：{}",
            String::from_utf8_lossy(&output.stderr)
        )
    }
    let identity: BuildInfo =
        serde_json::from_slice(&output.stdout).context("SAG 服务文件缺少构建身份，请重新构建")?;
    identity.check_compatible(current)?;
    if let Some(client) = connected {
        let running = BuildInfo::from(client.info());
        if current.channel == "release"
            && compare_versions(running.version()?, identity.version()?)?
                != std::cmp::Ordering::Less
        {
            running.check_compatible(current)?;
            return Ok(client);
        }
        client
            .call(Operation::Shutdown, Duration::from_secs(10))
            .await?;
        drop(client);
        wait_stopped(data).await?;
    }
    std::fs::create_dir_all(data)?;
    spawn_service(data, executable)?;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(client) = Client::connect(data).await? {
            BuildInfo::from(client.info()).check_compatible(&identity)?;
            return Ok(client);
        }
    }
    bail!("SAG 服务未能启动。请关闭使用此知识库的旧版本，或运行 sag service serve 查看原因。")
}
pub async fn wait_stopped(data: &Path) -> Result<()> {
    for _ in 0..200 {
        if !data.join("sag-service.json").try_exists()? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    bail!("SAG 服务停止超时，连接记录仍存在")
}

#[cfg(all(test, unix))]
mod version_tests;
