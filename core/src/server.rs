use crate::{
    cli_service,
    service::{EventSink, Service},
};
use anyhow::{bail, Context, Result};
use sag_client::wire;
use sag_protocol::*;
use std::{
    collections::HashMap,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::BufReader,
    net::{TcpListener, TcpStream},
    sync::Semaphore,
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

pub async fn run(data: PathBuf) -> Result<()> {
    run_until(data, CancellationToken::new()).await
}
pub async fn run_until(data: PathBuf, shutdown: CancellationToken) -> Result<()> {
    std::fs::create_dir_all(&data)?;
    let _instance = cli_service::lock_instance(&data)?;
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let service = Service::open(data.clone())?;
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
    let mut file = tempfile::NamedTempFile::new_in(&data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(windows)]
    {
        let username = std::env::var("USERNAME").context("无法获取当前用户")?;
        let result = std::process::Command::new("icacls")
            .arg(file.path())
            .args(["/inheritance:r", "/grant:r", &format!("{username}:F")])
            .output()?;
        if !result.status.success() {
            bail!("无法保护 SAG 服务连接记录")
        }
    }
    file.write_all(&serde_json::to_vec(&endpoint)?)?;
    file.persist(data.join("sag-service.json"))?;
    let requests = Arc::new(Semaphore::new(20));
    let connections = Arc::new(Semaphore::new(64));
    let mut clients = JoinSet::new();
    loop {
        tokio::select! {
            _=shutdown.cancelled()=>{service.stop.cancel();break;},
            _=service.stop.cancelled()=>break,
            accepted=listener.accept()=>{
                let (stream,_)=accepted?;
                let Ok(permit)=connections.clone().try_acquire_owned() else {continue};
                let service=service.clone();let token=token.clone();let requests=requests.clone();
                clients.spawn(async move {
                    let _permit=permit;
                    if let Err(error)=serve(stream,service,token,requests).await {eprintln!("SAG 连接结束：{error:#}");}
                });
            }
            completed=clients.join_next(),if !clients.is_empty()=>{
                if let Some(Err(error))=completed {eprintln!("SAG 连接任务失败：{error}");}
            }
        }
    }
    while let Some(result) = clients.join_next().await {
        result.context("停止 SAG 客户端连接失败")?;
    }
    service.finish().await?;
    let path = data.join("sag-service.json");
    match std::fs::read(&path) {
        Ok(bytes) => {
            let current: Endpoint = serde_json::from_slice(&bytes)?;
            if current.pid == std::process::id() && current.token == token {
                std::fs::remove_file(path)?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

async fn serve(
    stream: TcpStream,
    service: Arc<Service>,
    token: String,
    requests: Arc<Semaphore>,
) -> Result<()> {
    stream.set_nodelay(true)?;
    let (read, write) = stream.into_split();
    let mut read = BufReader::new(read);
    let write = Arc::new(tokio::sync::Mutex::new(write));
    match tokio::time::timeout(Duration::from_secs(10), wire::read(&mut read))
        .await
        .context("认证超时")??
    {
        Some(Frame::Hello {
            protocol_version,
            token: provided,
        }) if protocol_version == VERSION && provided == token => {}
        _ => bail!("协议版本或访问令牌无效"),
    }
    wire::write(
        &mut *write.lock().await,
        &Frame::Welcome {
            info: service.info()?,
        },
    )
    .await?;
    let owner = uuid::Uuid::new_v4().to_string();
    let cancelled = service.stop.child_token();
    let _guard = cancelled.clone().drop_guard();
    let mut events = service.events.subscribe();
    let event_write = write.clone();
    let event_owner = owner.clone();
    let event_cancel = cancelled.clone();
    let event_task = tokio::spawn(async move {
        loop {
            let event =
                tokio::select! {_=event_cancel.cancelled()=>break,event=events.recv()=>event};
            match event {
                Ok((target, envelope))
                    if target.as_ref().is_none_or(|target| target == &event_owner) =>
                {
                    let written = tokio::select! {
                        _ = event_cancel.cancelled() => break,
                        result = async { wire::write(&mut *event_write.lock().await, &Frame::Event { envelope }).await } => result,
                    };
                    if let Err(error) = written {
                        eprintln!("SAG 事件传输失败：{error}");
                        event_cancel.cancel();
                        break;
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    eprintln!("SAG 事件流结束：{error}");
                    event_cancel.cancel();
                    break;
                }
            }
        }
    });
    let running = Arc::new(Mutex::new(HashMap::<String, CancellationToken>::new()));
    let mut jobs = JoinSet::new();
    let result=async {
        'connection: loop {
            let next = wire::read(&mut read);
            tokio::pin!(next);
            let frame = loop {
                tokio::select! {
                    _=cancelled.cancelled()=>break 'connection,
                    frame=&mut next=>break frame?,
                    job=jobs.join_next(),if !jobs.is_empty()=>{job.unwrap().context("SAG 请求任务失败")?;},
                }
            };
            match frame {
                None=>break,
                Some(Frame::Cancel{id})=>{if let Some(token)=running.lock().unwrap().get(&id){token.cancel();}},
                Some(Frame::Request{id,operation})=>{
                    if running.lock().unwrap().contains_key(&id){bail!("请求 ID 重复")}
                    if jobs.len()>=32 {bail!("单个客户端请求数量超过上限")}
                    let cancel=cancelled.child_token();
                    running.lock().unwrap().insert(id.clone(),cancel.clone());
                    let service=service.clone();let write=write.clone();let running=running.clone();let requests=requests.clone();let connection_cancel=cancelled.clone();
                    let sink=EventSink{owner:owner.clone(),request_id:Some(id.clone()),cancel:cancel.clone(),bus:service.events.clone()};
                    jobs.spawn(async move {
                        let costly=matches!(&operation,Operation::RunQuery{..}|Operation::ExploreGraph{..}|Operation::ExploreNeighbors{..}|Operation::Cli{command:Command::Search{..}|Command::Match(_),..});
                        let shutdown=matches!(operation,Operation::Shutdown);
                        let result=async {
                            let _permit=if costly {
                                Some(tokio::select!{_=cancel.cancelled()=>return Err("已停止".into()),permit=requests.acquire_owned()=>permit.map_err(|e|e.to_string())?})
                            }else{None};
                            service.handle(operation,&sink).await
                        }.await;
                        running.lock().unwrap().remove(&id);
                        let written=tokio::select! {
                            _=connection_cancel.cancelled()=>return,
                            result=async {wire::write(&mut *write.lock().await,&Frame::Response{id,result}).await}=>result,
                        };
                        if shutdown {service.stop.cancel();}
                        if let Err(error)=written {eprintln!("SAG 响应传输失败：{error}");connection_cancel.cancel();}
                    });
                }
                _=>bail!("客户端消息类型无效"),
            }
        }
        Ok::<_,anyhow::Error>(())
    }.await;
    cancelled.cancel();
    while let Some(job) = jobs.join_next().await {
        job.context("清理 SAG 请求失败")?;
    }
    event_task.await.context("清理 SAG 事件任务失败")?;
    result
}
