use super::*;
use serde_json::{json, Value};

fn fixture() -> (tempfile::TempDir, Arc<CliService>, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("项目 ' quoted");
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("session.rs"),
        "// 中文说明\r\nfn authenticate() {\r\n    validate_permission();\r\n}\r\n",
    )
    .unwrap();
    let root = root.canonicalize().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("test.sqlite")).unwrap());
    store.add_root(&root).unwrap();
    store.scan(root.to_str().unwrap(), |_| {}).unwrap();
    let service = CliService::start(
        store,
        reqwest::Client::new(),
        dir.path().into(),
        Arc::new(tokio::sync::Mutex::new(())),
        lock_instance(dir.path()).unwrap(),
    )
    .unwrap();
    (dir, service, root)
}
async fn send(
    service: &CliService,
    path: &Path,
    command: Command,
    token: Option<&str>,
) -> Response {
    let endpoint: Endpoint =
        serde_json::from_slice(&fs::read(service.data.join("cli-endpoint.json")).unwrap()).unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", endpoint.port))
        .await
        .unwrap();
    let request = Request {
        protocol_version: VERSION,
        token: token.unwrap_or(&endpoint.token).into(),
        path: path.into(),
        command,
    };
    let mut bytes = serde_json::to_vec(&request).unwrap();
    bytes.push(b'\n');
    stream.write_all(&bytes).await.unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).await.unwrap();
    serde_json::from_str(&line).unwrap()
}
fn search(content: bool) -> Command {
    Command::Search {
        query: "authenticate permission\n中文".into(),
        max_results: 10,
        content,
        timeout: 3,
    }
}

#[tokio::test]
async fn socket_status_reports_disabled_and_search_checks_authorization() {
    let (dir, service, root) = fixture();
    match send(&service, &root, Command::Status, None).await {
        Response::Status(s) => {
            assert!(!s.enabled);
            assert!(!s.allowed);
            assert!(!s.jev_configured)
        }
        r => panic!("{r:?}"),
    }
    match send(&service, &root, search(false), None).await {
        Response::Error(e) => assert!(e.contains("未启用")),
        r => panic!("{r:?}"),
    }
    service
        .configure(Access {
            enabled: true,
            roots: vec![root.to_str().unwrap().into()],
        })
        .unwrap();
    match send(&service, &root, search(false), Some("incorrect")).await {
        Response::Error(e) => assert!(e.contains("令牌")),
        r => panic!("{r:?}"),
    }
    match send(&service, dir.path(), search(false), None).await {
        Response::Error(e) => assert!(e.contains("文件夹")),
        r => panic!("{r:?}"),
    }
    match send(&service, &root, search(false), None).await {
        Response::Error(e) => assert!(e.contains("JEV 未配置")),
        r => panic!("{r:?}"),
    }
    assert!(lock_instance(dir.path()).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(dir.path().join("cli-endpoint.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    service.shutdown();
    assert!(!dir.path().join("cli-endpoint.json").exists());
}

async fn model_request(stream: &mut TcpStream) -> Value {
    let mut read = BufReader::new(stream);
    let mut length = 0;
    loop {
        let mut line = String::new();
        read.read_line(&mut line).await.unwrap();
        if line == "\r\n" {
            break;
        }
        if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = n.trim().parse::<usize>().unwrap();
        }
    }
    let mut body = vec![0; length];
    read.read_exact(&mut body).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}
#[tokio::test]
async fn socket_search_reuses_retrieval_and_returns_exact_crlf_passage() {
    let (_dir, service, root) = fixture();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut settings = service.store.settings().unwrap();
    settings.jev.url = format!("http://{}/decisions", listener.local_addr().unwrap());
    settings.retrieval.max_hops = 0;
    service.store.save_settings(&settings).unwrap();
    models::save_secret(&service.data, "jev", "test-only").unwrap();
    service
        .configure(Access {
            enabled: true,
            roots: vec![root.to_str().unwrap().into()],
        })
        .unwrap();
    let model = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let payload = model_request(&mut stream).await;
            let answers: serde_json::Map<String, Value> = payload["questions"]
                .as_object()
                .unwrap()
                .keys()
                .map(|key| (key.clone(), json!({"type":"noul","noul":0.9})))
                .collect();
            let body = json!({"answers":answers}).to_string();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        }
    });
    let plain = send(&service, &root, search(false), None).await;
    let content = send(&service, &root, search(true), None).await;
    let (Response::Search(a), Response::Search(b)) = (plain, content) else {
        panic!("search failed")
    };
    assert!(!a.results.is_empty());
    assert_eq!(a.results.len(), b.results.len());
    for (a, b) in a.results.iter().zip(&b.results) {
        assert_eq!(
            (&a.absolute_path, a.start_line, a.end_line),
            (&b.absolute_path, b.start_line, b.end_line)
        );
        assert!(a.content.is_none());
        let file = fs::read_to_string(&b.absolute_path).unwrap();
        let expected = file
            .lines()
            .skip(b.start_line - 1)
            .take(b.end_line - b.start_line + 1)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            b.content.as_ref().unwrap().replace("\r\n", "\n").trim_end(),
            expected
        );
    }
    model.await.unwrap();
}
#[tokio::test]
async fn disconnect_and_timeout_release_the_query_without_cancelling_ui() {
    let (_dir, service, root) = fixture();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut settings = service.store.settings().unwrap();
    settings.jev.url = format!("http://{}/decisions", listener.local_addr().unwrap());
    service.store.save_settings(&settings).unwrap();
    models::save_secret(&service.data, "jev", "test-only").unwrap();
    service
        .configure(Access {
            enabled: true,
            roots: vec![root.to_str().unwrap().into()],
        })
        .unwrap();
    let endpoint: Endpoint =
        serde_json::from_slice(&fs::read(service.data.join("cli-endpoint.json")).unwrap()).unwrap();
    let mut cli = TcpStream::connect(("127.0.0.1", endpoint.port))
        .await
        .unwrap();
    let request = Request {
        protocol_version: VERSION,
        token: endpoint.token,
        path: root.clone(),
        command: search(true),
    };
    cli.write_all(format!("{}\n", serde_json::to_string(&request).unwrap()).as_bytes())
        .await
        .unwrap();
    let (mut remote, _) = listener.accept().await.unwrap();
    model_request(&mut remote).await;
    drop(cli);
    let guard = tokio::time::timeout(Duration::from_secs(1), service.query_lock.lock())
        .await
        .unwrap();
    drop(guard);
    let mut eof = [0];
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), remote.read(&mut eof))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    let cli = send(
        &service,
        &root,
        Command::Search {
            query: "authenticate".into(),
            max_results: 10,
            content: false,
            timeout: 1,
        },
        None,
    );
    let response = cli.await;
    match response {
        Response::Error(e) => assert!(e.contains("超时")),
        r => panic!("{r:?}"),
    }
    assert!(service.query_lock.try_lock().is_ok());
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_escape_and_oversized_requests_are_rejected() {
    let (dir, service, root) = fixture();
    let outside = dir.path().join("outside");
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
    service
        .configure(Access {
            enabled: true,
            roots: vec![root.to_str().unwrap().into()],
        })
        .unwrap();
    match send(&service, &root.join("escape"), search(false), None).await {
        Response::Error(e) => assert!(e.contains("文件夹")),
        r => panic!("{r:?}"),
    }
    let endpoint: Endpoint =
        serde_json::from_slice(&fs::read(service.data.join("cli-endpoint.json")).unwrap()).unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", endpoint.port))
        .await
        .unwrap();
    stream
        .write_all(&vec![b'x'; MAX_REQUEST as usize + 1])
        .await
        .unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).await.unwrap();
    match serde_json::from_str::<Response>(&line).unwrap() {
        Response::Error(e) => assert!(e.contains("长度上限")),
        r => panic!("{r:?}"),
    }
}

#[tokio::test]
async fn removing_a_root_revokes_access_and_settings_remain_editable() {
    let (_dir, service, root) = fixture();
    let path = root.to_str().unwrap();
    service
        .configure(Access {
            enabled: true,
            roots: vec![path.into()],
        })
        .unwrap();
    service.store.remove_root(path).unwrap();
    service.revoke_root(path).unwrap();
    assert!(service.access().roots.is_empty());
    service.configure(Access::default()).unwrap();
    assert!(!service.access().enabled);
}

#[tokio::test]
async fn saturated_connections_do_not_stop_the_listener() {
    let (_dir, service, root) = fixture();
    let endpoint: Endpoint =
        serde_json::from_slice(&fs::read(service.data.join("cli-endpoint.json")).unwrap()).unwrap();
    let mut occupied = vec![];
    for _ in 0..20 {
        occupied.push(
            TcpStream::connect(("127.0.0.1", endpoint.port))
                .await
                .unwrap(),
        );
    }
    for _ in 0..10 {
        let stream = TcpStream::connect(("127.0.0.1", endpoint.port))
            .await
            .unwrap();
        drop(stream);
    }
    drop(occupied);
    // Closing idle clients lets their handlers release the slots before a status request.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let reply = send(&service, &root, Command::Status, None).await;
    assert!(matches!(reply, Response::Status(_)), "{reply:?}");
}
