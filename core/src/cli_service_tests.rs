use super::*;
use serde_json::{json, Value};

fn fixture() -> (tempfile::TempDir, Arc<CliService>, PathBuf) {
    let (dir, service, root, rx) = fixture_with_queue();
    let store = service.store.clone();
    std::thread::spawn(move || {
        for request in rx { crate::indexer::apply_request(&store, request, |_| {}).unwrap(); }
    });
    (dir, service, root)
}
fn fixture_with_queue() -> (tempfile::TempDir, Arc<CliService>, PathBuf, std::sync::mpsc::Receiver<crate::indexer::IndexRequest>) {
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
    let mut settings = store.settings().unwrap();
    settings.file_types.code = true;
    store.save_settings(&settings).unwrap();
    store.add_root(&root).unwrap();
    store.scan(root.to_str().unwrap(), |_| {}).unwrap();
    let watcher = Arc::new(std::sync::Mutex::new(notify::recommended_watcher(|_: notify::Result<notify::Event>| {}).unwrap()));
    let (tx, rx) = std::sync::mpsc::channel();
    let add_store = store.clone();
    let add_root = Arc::new(move |path: &Path| crate::indexer::add_root(&add_store, &watcher, &tx, path));
    let remove_store = store.clone();
    let remove_root = Arc::new(move |path: &Path| remove_store.remove_path(path));
    let service = CliService::start(
        store,
        reqwest::Client::new(),
        dir.path().into(),
        lock_instance(dir.path()).unwrap(),
        add_root,
        remove_root,
    )
    .unwrap();
    (dir, service, root, rx)
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
async fn check_and_list_report_pending_indexes_and_disabled_access_without_models() {
    let (_dir, service, root) = fixture();
    service.store.connection().execute("UPDATE roots SET status='indexing'", []).unwrap();
    service.configure(Access { enabled: false }).unwrap();
    match send(&service, &root, Command::Check, None).await {
        Response::Checked(output) => {
            assert_eq!(output.coverage, IndexCoverage::Included);
            assert!(!output.index_ready);
            assert!(!output.enabled);
            assert_eq!(output.indexed_files, 1);
            assert_eq!(output.roots[0].status, "indexing");
        }
        response => panic!("{response:?}"),
    }
    match send(&service, &root, Command::List, None).await {
        Response::Roots(roots) => {
            assert_eq!(roots.len(), 1);
            assert_eq!(roots[0].path, root.to_str().unwrap());
            assert_eq!(roots[0].count, 1);
        }
        response => panic!("{response:?}"),
    }
}

#[tokio::test]
async fn status_contains_only_service_configuration_and_does_not_require_a_directory() {
    let (_dir, service, root) = fixture();
    let expected = json!({
        "protocol_version": VERSION,
        "connected": true,
        "enabled": true,
        "jev_configured": false,
    });
    for path in [&root, &root.join("not-created")] {
        let Response::Status(status) = send(&service, path, Command::Status, None).await else {
            panic!("status failed");
        };
        assert_eq!(serde_json::to_value(status).unwrap(), expected);
    }
}

#[tokio::test]
async fn local_matching_socket_needs_no_model_and_keeps_access_checks() {
    let (_dir, service, root) = fixture();
    let command = || Command::Match(MatchRequest {
        patterns: vec![ r"^fn auth.*\(\) \{$".into()], mode: MatchMode::Regex,
        max_results: Some(10), content: true, timeout: 3, options: MatchOptions::default(),
    });
    match send(&service, &root, command(), None).await {
        Response::Matches(output) => {
            assert_eq!(output.results.len(), 1);
            assert_eq!(output.results[0].start_line, 2);
            assert_eq!(output.results[0].content.as_deref(), Some("fn authenticate() {"));
        }
        r => panic!("{r:?}"),
    }
    match send(&service, &root, command(), Some("wrong-token")).await {
        Response::Error(error) => assert!(error.contains("令牌")),
        r => panic!("{r:?}"),
    }
    service.configure(Access { enabled: false }).unwrap();
    match send(&service, &root, command(), None).await {
        Response::Error(error) => assert!(error.contains("未启用")),
        r => panic!("{r:?}"),
    }
}

#[tokio::test]
async fn socket_status_reports_disabled_and_search_uses_automatic_scope() {
    let (dir, service, root) = fixture();
    service.configure(Access { enabled: false }).unwrap();
    match send(&service, &root, Command::Status, None).await {
        Response::Status(s) => {
            assert!(!s.enabled);
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
            enabled: true
        })
        .unwrap();
    match send(&service, &root, search(false), Some("incorrect")).await {
        Response::Error(e) => assert!(e.contains("令牌")),
        r => panic!("{r:?}"),
    }
    let unindexed = dir.path().join("unindexed");
    fs::create_dir(&unindexed).unwrap();
    match send(&service, &unindexed, search(false), None).await {
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
async fn twenty_searches_reach_the_model_before_any_response_returns() {
    let (_dir, service, root) = fixture();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut settings = service.store.settings().unwrap();
    settings.jev.url = format!("http://{}/decisions", listener.local_addr().unwrap());
    settings.retrieval.max_hops = 0;
    service.store.save_settings(&settings).unwrap();
    models::save_secret(&service.data, "jev", "test-only").unwrap();
    let mut clients = vec![];
    for _ in 0..20 {
        let service = service.clone();
        let root = root.clone();
        clients.push(tokio::spawn(async move { send(&service, &root, search(true), None).await }));
    }
    let mut requests = vec![];
    for _ in 0..20 {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept()).await
            .expect("all twenty searches must run before a model response is released").unwrap();
        let payload = model_request(&mut stream).await;
        requests.push((stream, payload));
    }
    for (mut stream, payload) in requests {
        let answers: serde_json::Map<String, Value> = payload["questions"].as_object().unwrap()
            .keys().map(|key| (key.clone(), json!({"type":"noul","noul":0.9}))).collect();
        let body = json!({"answers":answers}).to_string();
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    }
    for client in clients {
        let Response::Search(output) = client.await.unwrap() else { panic!("search failed") };
        assert_eq!(output.results.len(), 1);
        assert!(output.results[0].content.as_deref().unwrap().contains("validate_permission"));
    }
    service.shutdown();
}
#[tokio::test]
async fn standalone_search_rechecks_access_changed_by_another_core() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("session.txt"), "Session authentication checks permissions.").unwrap();
    let service = CliService::local(dir.path().into()).unwrap();
    let registered = service.store.add_root(&root).unwrap();
    service.store.scan(&registered, |_| {}).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut settings = service.store.settings().unwrap();
    settings.jev.url = format!("http://{}/decisions", listener.local_addr().unwrap());
    settings.retrieval.max_hops = 0;
    service.store.save_settings(&settings).unwrap();
    models::save_secret(dir.path(), "jev", "test-only").unwrap();
    let task = tokio::spawn(async move { service.run_local(root, Command::Search {
        query: "How does session authentication check permissions?".into(),
        max_results: 1, content: true, timeout: 3,
    }, CancellationToken::new()).await });
    let (mut stream, _) = listener.accept().await.unwrap();
    let payload = model_request(&mut stream).await;
    let other = CliService::local(dir.path().into()).unwrap();
    other.configure(Access { enabled: false }).unwrap();
    drop(other);
    let answers: serde_json::Map<String, Value> = payload["questions"].as_object().unwrap().keys()
        .map(|key| (key.clone(), json!({"type":"noul","noul":0.9}))).collect();
    let body = json!({"answers":answers}).to_string();
    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    assert!(task.await.unwrap().unwrap_err().to_string().contains("未启用"));
}
#[tokio::test]
async fn semantic_limit_keeps_every_source_span_of_selected_evidence() {
    let (_dir, service, root) = fixture();
    fs::remove_file(root.join("session.rs")).unwrap();
    fs::write(root.join("session.md"), "Session access requires a valid account.\n\nAuthentication checks permission.\n\nAccess denied returns an error.\n").unwrap();
    service.store.scan(root.to_str().unwrap(), |_| {}).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut settings = service.store.settings().unwrap();
    settings.jev.url = format!("http://{}/decisions", listener.local_addr().unwrap());
    settings.retrieval.max_hops = 0;
    service.store.save_settings(&settings).unwrap();
    models::save_secret(&service.data, "jev", "test-only").unwrap();
    service.configure(Access { enabled: true }).unwrap();
    let model = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let payload = model_request(&mut stream).await;
            let answers: serde_json::Map<String, Value> = payload["questions"].as_object().unwrap()
                .keys().map(|key| (key.clone(), json!({"type":"noul","noul":0.9}))).collect();
            let body = json!({"answers":answers}).to_string();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });
    for content in [true, false] {
        let response = send(&service, &root, Command::Search {
            query: "How does session authentication check permission and report denied access?".into(),
            max_results: 1, content, timeout: 3,
        }, None).await;
        let Response::Search(output) = response else { panic!("{response:?}") };
        assert!(!output.truncated);
        assert_eq!(output.results.iter().map(|r| (r.start_line, r.end_line)).collect::<Vec<_>>(), vec![(1, 1), (3, 3), (5, 5)]);
        assert!(output.results.iter().all(|r| r.path == "./session.md"));
        let passages = output.results.iter().map(|r| r.content.as_deref()).collect::<Vec<_>>();
        assert_eq!(passages, if content {
            vec![Some("Session access requires a valid account."), Some("Authentication checks permission."), Some("Access denied returns an error.")]
        } else { vec![None, None, None] });
    }
    model.await.unwrap();
    service.shutdown();
}
#[tokio::test]
async fn semantic_results_keep_headings_without_repeating_overlap() {
    let (dir, service, root) = fixture();
    fs::remove_file(root.join("session.rs")).unwrap();
    let body = (0..12).map(|i| format!("Session marker-{i:02} requires permission. {} End-{i:02}.", "Production policy applies. ".repeat(5))).collect::<Vec<_>>().join("\n\n");
    fs::write(root.join("session.md"), format!("# Deploy\n## Production\n{body}")).unwrap();
    service.store.scan(root.to_str().unwrap(), |_| {}).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut settings = service.store.settings().unwrap();
    settings.jev.url = format!("http://{}/decisions", listener.local_addr().unwrap());
    settings.retrieval.max_hops = 0;
    service.store.save_settings(&settings).unwrap();
    models::save_secret(dir.path(), "jev", "test-only").unwrap();
    let model = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let payload = model_request(&mut stream).await;
        assert!(payload["questions"].as_object().unwrap().values().all(|q| q["instructions"]["candidate_heading"] == "Deploy / Production"));
        let answers: serde_json::Map<String, Value> = payload["questions"].as_object().unwrap().keys().map(|key| (key.clone(), json!({"type":"noul","noul":0.9}))).collect();
        let body = json!({"answers":answers}).to_string();
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    });
    let Response::Search(output) = send(&service, &root, Command::Search {query:"Session production permission".into(),max_results:100,content:true,timeout:3}, None).await else { panic!("search failed") };
    model.await.unwrap();
    assert!(output.results.iter().all(|r| r.heading == "Deploy / Production" && r.path == "./session.md"));
    let shown = output.results.iter().map(|r| r.content.as_deref().unwrap()).collect::<Vec<_>>().join("\n");
    for i in 0..12 { assert_eq!(shown.matches(&format!("End-{i:02}.")).count(), 1, "overlapping source was repeated"); }
    service.shutdown();
}

#[tokio::test]
async fn parent_scope_search_reuses_retrieval_and_returns_exact_crlf_passage() {
    let (_dir, service, root) = fixture();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("outside.rs"), "fn authenticate() { validate_permission(); }").unwrap();
    let outside_root = service.store.add_root(outside.path()).unwrap();
    service.store.scan(&outside_root, |_| {}).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut settings = service.store.settings().unwrap();
    settings.jev.url = format!("http://{}/decisions", listener.local_addr().unwrap());
    settings.retrieval.max_hops = 0;
    service.store.save_settings(&settings).unwrap();
    models::save_secret(&service.data, "jev", "test-only").unwrap();
    service
        .configure(Access {
            enabled: true
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
    let plain = send(&service, root.parent().unwrap(), search(false), None).await;
    let content = send(&service, root.parent().unwrap(), search(true), None).await;
    let (Response::Search(a), Response::Search(b)) = (plain, content) else {
        panic!("search failed")
    };
    assert!(!a.results.is_empty());
    assert!(a.results.iter().all(|item| Path::new(&item.absolute_path).starts_with(&root)));
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
async fn disconnect_and_timeout_only_cancel_their_own_parallel_queries() {
    let (_dir, service, root) = fixture();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut settings = service.store.settings().unwrap();
    settings.jev.url = format!("http://{}/decisions", listener.local_addr().unwrap());
    service.store.save_settings(&settings).unwrap();
    models::save_secret(&service.data, "jev", "test-only").unwrap();
    service
        .configure(Access {
            enabled: true
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
    let peer_service = service.clone();
    let peer_root = root.clone();
    let peer = tokio::spawn(async move { send(&peer_service, &peer_root, search(true), None).await });
    let (mut peer_remote, _) = tokio::time::timeout(Duration::from_secs(1), listener.accept()).await.unwrap().unwrap();
    let peer_payload = model_request(&mut peer_remote).await;
    drop(cli);
    let mut eof = [0];
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), remote.read(&mut eof))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    let answers: serde_json::Map<String, Value> = peer_payload["questions"].as_object().unwrap().keys()
        .map(|key| (key.clone(), json!({"type":"noul","noul":0.9}))).collect();
    let body = json!({"answers":answers}).to_string();
    peer_remote.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    assert!(matches!(peer.await.unwrap(), Response::Search(_)));
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
            enabled: true
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
async fn removing_a_root_removes_it_from_automatic_scope() {
    let (_dir, service, root) = fixture();
    let path = root.to_str().unwrap();
    service
        .configure(Access {
            enabled: true
        })
        .unwrap();
    let child = root.join("nested");
    fs::create_dir(&child).unwrap();
    service.configure(Access { enabled: true }).unwrap();
    service.store.remove_root(path).unwrap();
    assert_eq!(service.check(&root).unwrap().coverage, IndexCoverage::None);
    service.configure(Access { enabled: false }).unwrap();
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

#[tokio::test]
async fn add_registers_indexes_and_enables_discovery() {
    let (dir, service, root) = fixture();
    let new = dir.path().join("新项目");
    fs::create_dir(&new).unwrap();
    let new = new.canonicalize().unwrap();
    fs::write(new.join("notes.md"), "# 新项目\n索引内容").unwrap();
    service.configure(Access { enabled:false }).unwrap();
    assert!(matches!(send(&service,&new,Command::Add,None).await,Response::Error(_)));
    assert!(!service.store.roots().unwrap().iter().any(|r| Path::new(&r.path)==new));
    service.configure(Access { enabled:true }).unwrap();
    assert!(matches!(send(&service,&new,Command::Add,Some("bad-token")).await,Response::Error(_)));
    assert!(!service.store.roots().unwrap().iter().any(|r| Path::new(&r.path)==new));
    assert!(matches!(send(&service,&new,Command::Add,None).await,Response::Added(_)));
    for _ in 0..100 {
        if service.index_ready(&new).unwrap() { break; }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(service.index_ready(&new).unwrap());
    assert!(service.store.document(&new.join("notes.md")).is_ok());
    assert_eq!(service.check(&new).unwrap().coverage, IndexCoverage::Included);
    assert!(matches!(send(&service,&new,Command::Add,None).await,Response::Added(_)));
    assert_eq!(service.store.roots().unwrap().iter().filter(|r| Path::new(&r.path)==new).count(),1);
    let child = root.join("子目录");
    fs::create_dir(&child).unwrap();
    assert!(matches!(send(&service,&child,Command::Add,None).await,Response::Added(_)));
    assert_eq!(service.check(&child).unwrap().coverage, IndexCoverage::Included);
    assert_eq!(service.check(&root).unwrap().coverage, IndexCoverage::Included);
    assert!(matches!(send(&service,&root.join("session.rs"),Command::Add,None).await,Response::Error(_)));
    assert!(matches!(send(&service,&dir.path().join("missing"),Command::Add,None).await,Response::Error(_)));
}


#[tokio::test]
async fn offline_roots_do_not_block_add_or_disabling_cli() {
    let (dir, service, root, _queue) = fixture_with_queue();
    service.configure(Access { enabled: true }).unwrap();
    fs::rename(&root, root.with_file_name("moved")).unwrap();
    let new = dir.path().join("new");
    fs::create_dir(&new).unwrap();
    assert!(matches!(send(&service,&new,Command::Add,None).await,Response::Added(_)));
    assert_eq!(service.check(&new).unwrap().coverage, IndexCoverage::Included);
    let mut access = service.access();
    access.enabled = false;
    service.configure(access).unwrap();
    assert!(!service.access().enabled);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parallel_adds_preserve_roots_and_queued_index_is_not_ready() {
    let (dir, service, existing, _queue) = fixture_with_queue();
    service.configure(Access { enabled: true }).unwrap();
    let Response::Added(existing_result) = send(&service,&existing,Command::Add,None).await else { panic!("add failed") };
    assert_eq!(existing_result.index_status,"pending");
    assert!(!service.index_ready(&existing).unwrap());
    let root = dir.path().join("outer");
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let child = root.join("child");
    fs::create_dir(&child).unwrap();
    let (a,b) = tokio::join!(send(&service,&root,Command::Add,None),send(&service,&child,Command::Add,None));
    for result in [a,b] {
        let Response::Added(output) = result else { panic!("add failed: {result:?}") };
        assert_eq!(output.index_status,"pending");
    }
    assert_eq!(service.check(&root).unwrap().coverage, IndexCoverage::Included);
    assert_eq!(service.check(&child).unwrap().coverage, IndexCoverage::Included);
    assert!(!service.index_ready(&root).unwrap());
}

#[tokio::test]
async fn new_install_is_enabled_and_parent_scope_discovers_indexed_children() {
    let (dir, service, root) = fixture();
    assert!(service.status().unwrap().enabled, "new users should not need to enable CLI");
    let check = service.check(dir.path()).unwrap();
    assert_eq!(check.coverage, IndexCoverage::Partial);
    assert!(check.index_ready, "a parent directory should discover indexed children");
    let unindexed = dir.path().join("unindexed");
    fs::create_dir(&unindexed).unwrap();
    assert_eq!(service.check(&unindexed).unwrap().coverage, IndexCoverage::None);
    assert_eq!(service.check(&root).unwrap().coverage, IndexCoverage::Included);
    service.configure(Access { enabled: false }).unwrap();
    assert!(matches!(send(&service,&root,search(false),None).await,Response::Error(_)));
}

#[tokio::test]
async fn remove_unregisters_only_the_requested_root_and_preserves_local_files() {
    let (dir, service, root, _queue) = fixture_with_queue();
    let original = fs::read(root.join("session.rs")).unwrap();
    let child = root.join("child");
    fs::create_dir(&child).unwrap();
    fs::write(child.join("notes.md"), b"keep nested file").unwrap();
    let other = dir.path().join("other");
    fs::create_dir(&other).unwrap();
    assert!(matches!(send(&service, &other, Command::Add, None).await, Response::Added(_)));

    service.configure(Access { enabled: false }).unwrap();
    assert!(matches!(send(&service, &root, Command::Remove, None).await, Response::Error(_)));
    service.configure(Access { enabled: true }).unwrap();
    assert!(matches!(send(&service, &root, Command::Remove, Some("bad-token")).await, Response::Error(_)));
    assert!(matches!(send(&service, &root.join("session.rs"), Command::Remove, None).await, Response::Error(_)));
    assert_eq!(service.check(&root).unwrap().coverage, IndexCoverage::Included);

    let Response::Removed { path } = send(&service, &root, Command::Remove, None).await else { panic!("remove failed") };
    assert_eq!(path, root.to_str().unwrap());
    let Response::Checked(check) = send(&service, &root, Command::Check, None).await else { panic!("check failed") };
    assert_eq!(check.coverage, IndexCoverage::None);
    assert_eq!(service.check(&other).unwrap().coverage, IndexCoverage::Included);
    assert!(matches!(send(&service, &root, search(true), None).await, Response::Error(_)));
    assert_eq!(fs::read(root.join("session.rs")).unwrap(), original);
    assert_eq!(fs::read(child.join("notes.md")).unwrap(), b"keep nested file");
    assert!(matches!(send(&service, &root, Command::Remove, None).await, Response::Error(_)));
}

#[tokio::test]
async fn remove_accepts_an_offline_registered_path_and_keeps_moved_files() {
    let (dir, service, root, _queue) = fixture_with_queue();
    let moved = root.with_file_name("moved");
    let original = fs::read(root.join("session.rs")).unwrap();
    fs::rename(&root, &moved).unwrap();
    let Response::Removed { path } = send(&service, &root, Command::Remove, None).await else { panic!("offline remove failed") };
    assert_eq!(path, root.to_str().unwrap());
    let Response::Checked(check) = send(&service, dir.path(), Command::Check, None).await else { panic!("check failed") };
    assert_eq!(check.coverage, IndexCoverage::None);
    assert_eq!(fs::read(moved.join("session.rs")).unwrap(), original);
}

#[tokio::test]
async fn removed_subdirectory_stays_out_of_index_until_added_again() {
    let (dir, service, root, queue) = fixture_with_queue();
    let child = root.join("excluded");
    fs::create_dir(&child).unwrap();
    let file = child.join("notes.md");
    fs::write(&file, "subdirectory content").unwrap();
    assert!(matches!(send(&service, &child, Command::Add, None).await, Response::Added(_)));
    for request in queue.try_iter() { crate::indexer::apply_request(&service.store, request, |_| {}).unwrap(); }
    assert!(service.store.files().unwrap().iter().any(|item| item.path == file.to_str().unwrap()));

    assert!(matches!(send(&service, &child, Command::Remove, None).await, Response::Removed { .. }));
    assert_eq!(service.check(&child).unwrap().coverage, IndexCoverage::Excluded);
    assert_eq!(service.check(&root).unwrap().coverage, IndexCoverage::Included);
    assert!(!service.store.entries(&root, true).unwrap().iter().any(|item| item.path == child.to_str().unwrap()));
    assert!(service.store.document(&file).is_err());
    assert!(!service.store.files().unwrap().iter().any(|item| item.path == file.to_str().unwrap()));
    fs::write(&file, "changed local content").unwrap();
    assert!(matches!(send(&service, &root, Command::Add, None).await, Response::Added(_)));
    for request in queue.try_iter() { crate::indexer::apply_request(&service.store, request, |_| {}).unwrap(); }
    service.store.update_paths(root.to_str().unwrap(), &[file.clone(), child.clone()], |_| {}).unwrap();
    assert!(!service.store.files().unwrap().iter().any(|item| item.path == file.to_str().unwrap()));
    let reopened = Store::open(&dir.path().join("test.sqlite")).unwrap();
    assert!(reopened.document(&file).is_err());
    assert_eq!(fs::read_to_string(&file).unwrap(), "changed local content");

    assert!(matches!(send(&service, &child, Command::Add, None).await, Response::Added(_)));
    for request in queue.try_iter() { crate::indexer::apply_request(&service.store, request, |_| {}).unwrap(); }
    assert_eq!(service.check(&child).unwrap().coverage, IndexCoverage::Included);
    assert!(service.store.files().unwrap().iter().any(|item| item.path == file.to_str().unwrap()));
    assert_eq!(service.store.document(&file).unwrap().body, "changed local content");
}

#[tokio::test]
async fn project_ignore_files_control_initial_and_incremental_indexing() {
    let (_dir, service, root, queue) = fixture_with_queue();
    fs::create_dir(root.join("generated")).unwrap();
    fs::create_dir(root.join("nested")).unwrap();
    for name in ["drop.md", "drop-keep.md", "keep.secret.md", "internal.private.md", "generated/output.md", "nested/no.md", "nested/ok.md"] {
        fs::write(root.join(name), "ignore fixture content").unwrap();
    }
    fs::write(root.join(".gitignore"), "generated/\ndrop*.md\n!drop-keep.md\n*.secret.md\n").unwrap();
    fs::write(root.join(".sagignore"), "*.private.md\n!keep.secret.md\n").unwrap();
    fs::write(root.join("nested/.gitignore"), "*.md\n!ok.md\n").unwrap();
    assert!(matches!(send(&service, &root, Command::Add, None).await, Response::Added(_)));
    for request in queue.try_iter() { crate::indexer::apply_request(&service.store, request, |_| {}).unwrap(); }
    let paths: Vec<_> = service.store.files().unwrap().into_iter().map(|file| file.path).collect();
    for name in ["drop.md", "internal.private.md", "generated/output.md", "nested/no.md"] {
        assert!(!paths.contains(&root.join(name).to_str().unwrap().to_owned()), "{name} should be ignored");
    }
    for name in ["drop-keep.md", "keep.secret.md", "nested/ok.md"] {
        assert!(paths.contains(&root.join(name).to_str().unwrap().to_owned()), "{name} should be indexed");
    }
    fs::write(root.join("drop.md"), "modified ignored file").unwrap();
    service.store.update_paths(root.to_str().unwrap(), &[root.join("drop.md")], |_| {}).unwrap();
    assert!(!service.store.files().unwrap().iter().any(|file| file.path.ends_with("/drop.md")));
    assert_eq!(fs::read_to_string(root.join("drop.md")).unwrap(), "modified ignored file");
}

#[tokio::test]
async fn changing_ignore_files_reconciles_existing_index_and_restores_unignored_files() {
    let (_dir, service, root, queue) = fixture_with_queue();
    let file = root.join("changing.md");
    fs::write(&file, "content remains on disk").unwrap();
    send(&service, &root, Command::Add, None).await;
    for request in queue.try_iter() { crate::indexer::apply_request(&service.store, request, |_| {}).unwrap(); }
    assert!(service.store.files().unwrap().iter().any(|item| item.path == file.to_str().unwrap()));
    let rules = root.join(".sagignore");
    for (contents, present) in [(Some("changing.md\n"), false), (Some(""), true), (Some("changing.md\n"), false), (None, true)] {
        let event = if let Some(contents) = contents {
            fs::write(&rules, contents).unwrap();
            notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
        } else {
            fs::remove_file(&rules).unwrap();
            notify::Event::new(notify::EventKind::Remove(notify::event::RemoveKind::File))
        }.add_path(rules.clone());
        for request in crate::indexer::event_requests(&event, &service.store.roots().unwrap(), &service.store.settings().unwrap()) {
            crate::indexer::apply_request(&service.store, request, |_| {}).unwrap();
        }
        assert_eq!(service.store.files().unwrap().iter().any(|item| item.path == file.to_str().unwrap()), present);
    }
    assert_eq!(fs::read_to_string(file).unwrap(), "content remains on disk");
}

#[tokio::test]
async fn malformed_ignore_rules_preserve_existing_index_and_report_the_error() {
    let (_dir, service, root, queue) = fixture_with_queue();
    let original: Vec<_> = service.store.files().unwrap().into_iter().map(|file| file.path).collect();
    let rules = root.join(".sagignore");
    fs::write(&rules, "[z-a]\n").unwrap();
    send(&service, &root, Command::Add, None).await;
    let error = crate::indexer::apply_request(&service.store, queue.recv().unwrap(), |_| {}).unwrap_err();
    assert!(error.to_string().contains(".sagignore"), "{error}");
    assert!(!service.index_ready(&root).unwrap());
    let after: Vec<_> = service.store.files().unwrap().into_iter().map(|file| file.path).collect();
    assert_eq!(after, original);
    fs::write(&rules, "").unwrap();
    send(&service, &root, Command::Add, None).await;
    crate::indexer::apply_request(&service.store, queue.recv().unwrap(), |_| {}).unwrap();
    assert!(service.index_ready(&root).unwrap());
}

#[cfg(unix)]
#[tokio::test]
async fn failed_key_update_does_not_pair_new_provider_with_old_key() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, service, root, _queue) = fixture_with_queue();
    models::save_secret(dir.path(), "jev", "old-provider-key").unwrap();
    let previous = service.store.settings().unwrap().jev.url;
    // SQLite has an open WAL, but creating the credential replacement is denied.
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let result = service.run_local(root, Command::Configure {
        enable: false,
        url: Some("https://different-provider.example/decisions".into()),
        model: None, key: Some("new-provider-key".into()), auth_mode: None,
    }, CancellationToken::new()).await;
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    let current = service.store.settings().unwrap().jev.url;
    assert!(current == previous || !models::has_secret(dir.path(), "jev").unwrap());
}

#[tokio::test]
async fn invalid_local_jev_cli_configuration_preserves_the_previous_key() {
    let (dir, service, root) = fixture();
    models::save_secret(dir.path(), "jev", "previous-jev-key").unwrap();
    let result = service.run_local(root, Command::Configure {
        enable: false, url: None, model: Some(String::new()), key: None,
        auth_mode: Some(crate::types::ModelAuthMode::None),
    }, CancellationToken::new()).await;
    assert!(matches!(result, Err(_) | Ok(Response::Error(_))));
    assert_eq!(models::secret(dir.path(), "jev").unwrap(), "previous-jev-key");
    assert_eq!(service.store.settings().unwrap().jev.auth_mode, crate::types::ModelAuthMode::Bearer);
}
