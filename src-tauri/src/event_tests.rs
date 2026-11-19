use super::*;
use sag_client::wire;
use sag_protocol::{Endpoint, EventEnvelope, Frame, ServiceInfo, Snapshot};
use std::path::Path;
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

fn quitting_state<R: tauri::Runtime>(app: &tauri::App<R>, data: &Path) -> Arc<AppState<R>> {
    Arc::new(AppState {
        client: Mutex::new(Client::disconnected()),
        reconnect: tokio::sync::Mutex::new(()),
        app: app.handle().clone(),
        data_dir: data.into(),
        service_binary: data.join("must-not-start"),
        cli_install_error: Mutex::new(None),
        quitting: AtomicBool::new(false),
    })
}

async fn shutdown_server(
    data: &Path,
    reject: bool,
) -> (tokio::task::JoinHandle<()>, Arc<Notify>, Arc<Notify>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let instance = std::fs::OpenOptions::new().create(true).truncate(false)
        .read(true).write(true).open(data.join("cli-instance.lock")).unwrap();
    instance.lock().unwrap();
    let endpoint = data.join("sag-service.json");
    let info = ServiceInfo {
        version: Some(protocol::BUILD_VERSION.into()), protocol_version: protocol::VERSION,
        build_id: protocol::BUILD_ID.into(), channel: protocol::BUILD_CHANNEL.into(),
        pid: std::process::id(), executable: String::new(), data_directory: data.display().to_string(),
    };
    std::fs::write(&endpoint, serde_json::to_vec(&Endpoint {
        protocol_version: protocol::VERSION, port: listener.local_addr().unwrap().port(),
        pid: info.pid, token: "quit-test".into(),
    }).unwrap()).unwrap();
    let requested = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let notified = requested.clone();
    let finish = release.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut read = BufReader::new(read);
        assert!(matches!(wire::read(&mut read).await.unwrap(), Some(Frame::Hello { .. })));
        wire::write(&mut write, &Frame::Welcome { info }).await.unwrap();
        let Some(Frame::Request { id, operation: Operation::Shutdown }) = wire::read(&mut read).await.unwrap() else {
            panic!("quit did not request service shutdown");
        };
        let result = if reject { Err("stop refused".into()) } else { Ok(Output::Unit) };
        wire::write(&mut write, &Frame::Response { id, result }).await.unwrap();
        notified.notify_one();
        finish.notified().await;
        std::fs::remove_file(endpoint).unwrap();
        drop(instance);
    });
    (server, requested, release)
}

#[tokio::test]
async fn quitting_without_a_service_does_not_start_one() {
    let dir = tempfile::tempdir().unwrap();
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    let state = quitting_state(&app, dir.path());
    stop_background(&state).await.unwrap();
    assert!(state.quitting.load(Ordering::Relaxed));
    assert!(rpc(&state, Operation::Info).await.unwrap_err().contains("正在退出"));
    assert!(!dir.path().join("sag-service.json").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quitting_waits_for_service_release_and_blocks_automatic_restart() {
    // Repeated shutdowns catch Windows endpoint deletion racing the status check.
    for _ in 0..32 {
        let dir = tempfile::tempdir().unwrap();
        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        let state = quitting_state(&app, dir.path());
        let (server, requested, release) = shutdown_server(dir.path(), false).await;
        let copy = state.clone();
        let quitting = tokio::spawn(async move { stop_background(&copy).await });
        tokio::time::timeout(Duration::from_secs(3), requested.notified()).await.unwrap();
        assert!(!quitting.is_finished(), "quit completed before the service released its instance");
        assert!(rpc(&state, Operation::Info).await.unwrap_err().contains("正在退出"));
        release.notify_one();
        tokio::time::timeout(Duration::from_secs(3), quitting).await.unwrap().unwrap().unwrap();
        server.await.unwrap();
        assert!(!dir.path().join("sag-service.json").exists());
        assert!(rpc(&state, Operation::Info).await.unwrap_err().contains("正在退出"));
    }
}

#[tokio::test]
async fn failed_quit_reports_the_error_and_allows_retry() {
    let dir = tempfile::tempdir().unwrap();
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    let state = quitting_state(&app, dir.path());
    let (server, _, release) = shutdown_server(dir.path(), true).await;
    assert!(stop_background(&state).await.unwrap_err().contains("stop refused"));
    assert!(!state.quitting.load(Ordering::Relaxed));
    release.notify_one();
    server.await.unwrap();
}

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
    let settings_requested = Arc::new(Notify::new());
    let settings_release = Arc::new(Notify::new());
    let first_snapshot = Arc::new(AtomicBool::new(true));
    let server_settings_requested = settings_requested.clone();
    let server_settings_release = settings_release.clone();
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
            let settings_requested = server_settings_requested.clone();
            let settings_release = server_settings_release.clone();
            let first_snapshot = first_snapshot.clone();
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
                    let snapshot = matches!(&operation, Operation::Snapshot);
                    let result = match operation {
                        Operation::Snapshot => Output::Snapshot(Snapshot {
                            roots: vec![],
                            files: vec![],
                            settings: Settings {
                                language: if first_snapshot.load(Ordering::SeqCst) {
                                    protocol::locales::default_interface_language().into()
                                } else {
                                    "ja".into()
                                },
                                ..Settings::default()
                            },
                            conversations: vec![],
                            drafts: vec![],
                        }),
                        Operation::Info => Output::Info(info.clone()),
                        Operation::GetSettings => {
                            settings_requested.notify_one();
                            settings_release.notified().await;
                            Output::Settings(Settings {
                                language: "ja".into(),
                                ..Settings::default()
                            })
                        }
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
                    if !snapshot {
                        continue;
                    }
                    if first_snapshot.swap(false, Ordering::SeqCst) {
                        wire::write(
                            &mut write,
                            &Frame::Event {
                                envelope: EventEnvelope {
                                    request_id: None,
                                    event: ServiceEvent::SettingsChanged,
                                },
                            },
                        )
                        .await
                        .unwrap();
                    }
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
    let (language_tx, mut language) = mpsc::unbounded_channel();
    app.listen("interface-language-changed", move |event| {
        language_tx.send(event.payload().to_owned()).unwrap();
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
        quitting: AtomicBool::new(false),
    });
    let forwarding = forward_events(state.clone());
    tokio::time::timeout(Duration::from_secs(3), settings_requested.notified())
        .await
        .expect("language refresh requested settings");
    tokio::time::timeout(Duration::from_secs(1), progress.recv())
        .await
        .expect("progress streams even while the language refresh is pending")
        .unwrap();
    settings_release.notify_one();
    tokio::time::timeout(Duration::from_secs(3), async {
        while language.recv().await.unwrap() != r#""ja""# {}
    })
    .await
    .expect("the language change is still applied");
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
    assert!(
        language.try_recv().is_err(),
        "reconnecting with the same language should not reapply it"
    );
    forwarding.abort();
    server.abort();
    assert!(
        restored.is_ok(),
        "ordinary requests recovered, but streaming stayed disconnected"
    );
}
