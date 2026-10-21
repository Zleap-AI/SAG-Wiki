use super::*;
use sag_client::wire;
use sag_protocol::{BuildInfo, Command, Frame, Output, ServiceInfo};
use std::{os::unix::fs::PermissionsExt, time::Duration};
use tokio::{io::BufReader, net::{UnixListener, UnixStream}, sync::Semaphore};

fn info(app: &App, uid: u32) -> Result<ServiceInfo> {
    let build = BuildInfo::current();
    Ok(ServiceInfo {
        version: build.version, protocol_version: sag_protocol::VERSION,
        build_id: build.build_id, channel: build.channel, pid: std::process::id(),
        executable: std::env::current_exe()?.display().to_string(),
        data_directory: app.data.join("users").join(uid.to_string()).display().to_string(),
    })
}
pub(super) async fn execute(app: &App, uid: u32, operation: Operation, cancel: CancellationToken) -> Result<Output> {
    match &operation {
        Operation::Info => return Ok(Output::Info(info(app, uid)?)),
        Operation::Shutdown => {
            let mut users = app.users.lock().await;
            if let Some(service) = users.remove(&uid) { service.finish().await?; }
            return Ok(Output::Unit);
        }
        Operation::Cli { command: Command::Status, .. } => {
            if !app.users.lock().await.contains_key(&uid) {
                return Ok(Output::Cli(sag_protocol::Response::Status(sag_protocol::Status {
                    protocol_version: sag_protocol::VERSION, connected: false, enabled: false, jev_configured: false,
                })));
            }
        }
        Operation::Cli { .. } => {},
        _ => bail!("CLI 连接不允许执行此操作"),
    }
    let service = app.service(uid).await?;
    let Operation::Cli { path, command } = &operation else { unreachable!() };
    match command {
        Command::Status | Command::Configure { .. } | Command::List | Command::Remove => {},
        _ => app.fnos.require(uid, &path.to_string_lossy(), false).await?,
    }
    if matches!(command, Command::AddWithTypes { .. }) {
        app.fnos.require(uid, &path.join(".sagignore").to_string_lossy(), true).await?;
    }
    if matches!(command, Command::Search { .. } | Command::Match(_)) {
        app.check_paths(uid, &serde_json::to_value(service.store.files()?)?).await?;
    }
    let removing = matches!(command, Command::Remove);
    let sink = EventSink { owner: uuid::Uuid::new_v4().to_string(), request_id: None, cancel, bus: service.events.clone() };
    let output = service.handle(operation, &sink).await.map_err(anyhow::Error::msg)?;
    match &output {
        Output::Cli(sag_protocol::Response::Matches(value)) => {
            for path in value.results.iter().map(|item| &item.absolute_path).chain(value.files.iter().map(|item| &item.absolute_path)) {
                app.fnos.require(uid, path, false).await?;
            }
        }
        Output::Cli(sag_protocol::Response::Search(value)) => {
            for item in &value.results { app.fnos.require(uid, &item.absolute_path, false).await?; }
        }
        _ if removing => {},
        _ => app.check_paths(uid, &serde_json::to_value(&output)?).await?,
    }
    Ok(output)
}
async fn connection(stream: UnixStream, app: App, requests: Arc<Semaphore>, stop: CancellationToken) -> Result<()> {
    let uid = stream.peer_cred()?.uid();
    if uid == 0 || uid == unsafe { libc::geteuid() } { bail!("CLI 必须使用飞牛用户账户"); }
    app.fnos.folders(uid).await?;
    let (read, mut write) = stream.into_split();
    let mut read = BufReader::new(read);
    match tokio::time::timeout(Duration::from_secs(10), wire::read(&mut read)).await?? {
        Some(Frame::Hello { protocol_version, .. }) if protocol_version == sag_protocol::VERSION => {},
        _ => bail!("CLI 协议版本无效"),
    }
    wire::write(&mut write, &Frame::Welcome { info: info(&app, uid)? }).await?;
    loop {
        let frame = tokio::select! { _ = stop.cancelled() => return Ok(()), frame = wire::read(&mut read) => frame? };
        let Some(Frame::Request { id, operation }) = frame else {
            if frame.is_none() { return Ok(()); }
            bail!("CLI 请求无效");
        };
        let cancel = stop.child_token();
        let _guard = cancel.clone().drop_guard();
        let future = async {
            let _permit = requests.acquire().await?;
            execute(&app, uid, operation, cancel.clone()).await
        };
        tokio::pin!(future);
        let result = loop {
            tokio::select! {
                _ = stop.cancelled() => return Ok(()),
                result = &mut future => break result,
                frame = wire::read(&mut read) => match frame? {
                    None => return Ok(()),
                    Some(Frame::Cancel { id: target }) if target == id => { cancel.cancel(); },
                    _ => bail!("请等待当前 CLI 请求完成"),
                }
            }
        };
        wire::write(&mut write, &Frame::Response { id, result: result.map_err(|error| format!("{error:#}")) }).await?;
    }
}
pub async fn run(app: App, socket: PathBuf, stop: CancellationToken) -> Result<()> {
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o666))?;
    let requests = Arc::new(Semaphore::new(20));
    let mut jobs = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = stop.cancelled() => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let app = app.clone(); let requests = requests.clone(); let stop = stop.clone();
                jobs.spawn(async move { connection(stream, app, requests, stop).await });
            }
            Some(result) = jobs.join_next(), if !jobs.is_empty() => {
                if let Err(error) = result? { eprintln!("CLI 连接结束：{error:#}"); }
            }
        }
    }
    while let Some(result) = jobs.join_next().await { if let Err(error) = result? { eprintln!("CLI 连接结束：{error:#}"); } }
    std::fs::remove_file(socket)?;
    Ok(())
}

pub async fn run_command(app: &App, uid: u32, args: &serde_json::Map<String, Value>) -> Result<Value> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
    let directory = args.get("directory").and_then(Value::as_str).context("请选择工作目录")?;
    app.fnos.require(uid, directory, false).await?;
    let command = args.get("command").and_then(Value::as_str).context("请输入 sag 命令")?;
    let mut argv = shlex::split(command).context("命令引号不匹配")?;
    if argv.first().map(String::as_str) != Some("sag") { bail!("仅支持 sag 命令"); }
    argv.remove(0);
    let mut stream = UnixStream::connect(&app.worker_socket).await?;
    let request = json!({"uid":uid,"cli_args":argv,"directory":directory});
    stream.write_all(&serde_json::to_vec(&request)?).await?;
    stream.write_all(b"\n").await?;
    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    tokio::time::timeout(Duration::from_secs(610), (&mut reader).take(1024*1024+1).read_line(&mut response)).await.context("CLI 命令超时")??;
    if response.len() > 1024*1024 { bail!("CLI 输出超过大小限制"); }
    let value: Value = serde_json::from_str(&response).context("CLI 进程返回了无效响应")?;
    if let Some(error) = value.get("error").and_then(Value::as_str) { bail!("{error}"); }
    Ok(value)
}
