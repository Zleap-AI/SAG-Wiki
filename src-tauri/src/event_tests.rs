use super::*;
use sag_client::wire;
use sag_protocol::{Endpoint, EventEnvelope, Frame, ServiceInfo, Snapshot};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    test::{mock_builder, mock_context, noop_assets},
    Listener,
};
use tokio::{
    io::BufReader,
    net::TcpListener,
    sync::{mpsc, Notify},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn streaming_recovers_after_a_failed_reconnect_and_successful_request() {
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let info = ServiceInfo {
        version: Some(protocol::BUILD_VERSION.into()),
        protocol_version: protocol::VERSION,
        build_id: protocol::BUILD_ID.into(),
        channel: protocol::BUILD_CHANNEL.into(),
        pid: std::process::id(),
        executable: String::new(),
        data_directory: dir.path().display().to_string(),
    };
    std::fs::write(
        dir.path().join("sag-service.json"),
        serde_json::to_vec(&Endpoint {
            protocol_version: protocol::VERSION,
            port: listener.local_addr().unwrap().port(),
            pid: info.pid,
            token: "streaming-test".into(),
        })
        .unwrap(),
    )
    .unwrap();
    let refuse = Arc::new(AtomicBool::new(false));
    let disconnect = Arc::new(Notify::new());
    let server_refuse = refuse.clone();
    let server_disconnect = disconnect.clone();
    let server = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            if server_refuse.load(Ordering::SeqCst) {
                continue;
            }
            let info = info.clone();
            let disconnect = server_disconnect.clone();
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut read = BufReader::new(read);
                assert!(matches!(
                    wire::read(&mut read).await.unwrap(),
                    Some(Frame::Hello { .. })
                ));
                wire::write(&mut write, &Frame::Welcome { info: info.clone() })
                    .await
                    .unwrap();
                loop {
                    let frame = tokio::select! {
                        _ = disconnect.notified() => break,
                        frame = wire::read(&mut read) => frame.unwrap(),
                    };
                    let Some(Frame::Request { id, operation }) = frame else {
                        break;
                    };
                    let result = match operation {
                        Operation::Snapshot => Output::Snapshot(Snapshot {
                            roots: vec![],
                            files: vec![],
                            settings: Settings::default(),
                            conversations: vec![],
                            drafts: vec![],
                        }),
                        Operation::Info => Output::Info(info.clone()),
                        _ => panic!("unexpected test request"),
                    };
                    wire::write(
                        &mut write,
                        &Frame::Response {
                            id,
                            result: Ok(result),
                        },
                    )
                    .await
                    .unwrap();
                    wire::write(
                        &mut write,
                        &Frame::Event {
                            envelope: EventEnvelope {
                                request_id: None,
                                event: ServiceEvent::QueryProgress(Progress {
                                    id: "streaming-test".into(),
                                    stage: "正在生成回答".into(),
                                    delta: Some("片段".into()),
                                    reasoning_delta: None,
                                    reasoning_block: None,
                                    replace: false,
                                    count: 0,
                                    tool: None,
                                }),
                            },
                        },
                    )
                    .await
                    .unwrap();
                }
            });
        }
    });
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    let (progress_tx, mut progress) = mpsc::unbounded_channel();
    app.listen("query-progress", move |_| {
        progress_tx.send(()).unwrap();
    });
    let (errors_tx, mut errors) = mpsc::unbounded_channel();
    app.listen("app-error", move |event| {
        errors_tx.send(event.payload().to_owned()).unwrap();
    });
    let state = Arc::new(AppState {
        client: Mutex::new(Client::connect(dir.path()).await.unwrap().unwrap()),
        reconnect: tokio::sync::Mutex::new(()),
        app: app.handle().clone(),
        data_dir: dir.path().into(),
        service_binary: PathBuf::new(),
        cli_install_error: Mutex::new(None),
    });
    let forwarding = forward_events(state.clone());
    tokio::time::timeout(Duration::from_secs(3), progress.recv())
        .await
        .unwrap()
        .unwrap();
    refuse.store(true, Ordering::SeqCst);
    disconnect.notify_waiters();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !errors.recv().await.unwrap().contains("SAG 服务重连失败") {}
    })
    .await
    .unwrap();
    refuse.store(false, Ordering::SeqCst);
    rpc(&state, Operation::Info).await.unwrap();
    let restored = tokio::time::timeout(Duration::from_secs(3), progress.recv()).await;
    forwarding.abort();
    server.abort();
    assert!(
        restored.is_ok(),
        "ordinary requests recovered, but streaming stayed disconnected"
    );
}
