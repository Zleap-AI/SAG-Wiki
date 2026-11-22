use sag_client::Client;
use sag_protocol::{types::*, *};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn conversation_rename_is_not_overwritten_by_an_active_answer() {
    let (_dir, client, server) = start().await;
    let started_at = crate::store::now();
    let hold = Arc::new(AtomicBool::new(true));
    let arrived = Arc::new(AtomicUsize::new(0));
    let (url, model) = model_server(false, hold.clone(), arrived.clone()).await;
    configure(&client, url, false).await;
    let caller = client.clone();
    let active = tokio::spawn(async move {
        caller
            .call(
                Operation::RunQuery {
                    query: query("rename"),
                },
                Duration::from_secs(10),
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        while arrived.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let caller = client.clone();
    let rename = tokio::spawn(async move {
        caller
            .call(
                Operation::EditConversation {
                    id: "rename".into(),
                    title: Some("用户重命名".into()),
                    deleted: None,
                },
                Duration::from_secs(10),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    hold.store(false, Ordering::SeqCst);
    assert!(active.await.unwrap().is_ok());
    assert!(rename.await.unwrap().is_ok());
    let Output::Conversations(conversations) = call(&client, Operation::ListConversations).await
    else {
        panic!("conversations")
    };
    assert_eq!(conversations[0].title, "用户重命名");
    let created_at = conversations[0].messages[0].created_at.unwrap();
    assert!(created_at >= started_at && created_at <= conversations[0].updated);
    stop(&client, server).await;
    model.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunked_request_survives_another_request_finishing_on_the_same_connection() {
    let (dir, client, server) = start().await;
    let hold = Arc::new(AtomicBool::new(true));
    let arrived = Arc::new(AtomicUsize::new(0));
    let (url, model) = model_server(false, hold.clone(), arrived.clone()).await;
    configure(&client, url, false).await;
    let endpoint: Endpoint =
        serde_json::from_slice(&std::fs::read(dir.path().join("data/sag-service.json")).unwrap())
            .unwrap();
    let stream = tokio::net::TcpStream::connect(("127.0.0.1", endpoint.port))
        .await
        .unwrap();
    let (read, mut write) = stream.into_split();
    let mut read = tokio::io::BufReader::new(read);
    sag_client::wire::write(
        &mut write,
        &Frame::Hello {
            protocol_version: VERSION,
            token: endpoint.token,
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        sag_client::wire::read(&mut read).await.unwrap(),
        Some(Frame::Welcome { .. })
    ));
    sag_client::wire::write(
        &mut write,
        &Frame::Request {
            id: "active".into(),
            operation: Operation::RunQuery {
                query: query("active"),
            },
        },
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while arrived.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let request = serde_json::to_string(&Frame::Request {
        id: "chunked".into(),
        operation: Operation::Info,
    })
    .unwrap();
    let middle = request.len() / 2;
    let first = format!(
        "{}\n",
        serde_json::json!({"data": &request[..middle], "last": false})
    );
    write.write_all(first.as_bytes()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    hold.store(false, Ordering::SeqCst);
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), sag_client::wire::read(&mut read))
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if matches!(frame, Frame::Response { ref id, .. } if id == "active") {
            break;
        }
    }
    let last = format!(
        "{}\n",
        serde_json::json!({"data": &request[middle..], "last": true})
    );
    write.write_all(last.as_bytes()).await.unwrap();
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), sag_client::wire::read(&mut read))
            .await
            .unwrap()
            .unwrap()
            .expect("connection must keep the partially received request");
        if let Frame::Response { id, result } = frame {
            assert_eq!(id, "chunked");
            assert!(matches!(result.unwrap(), Output::Info(_)));
            break;
        }
    }
    drop(write);
    drop(read);
    stop(&client, server).await;
    model.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn request_timeout_preserves_other_clients_and_the_connection() {
    let (dir, client, server) = start().await;
    let hold = Arc::new(AtomicBool::new(true));
    let arrived = Arc::new(AtomicUsize::new(0));
    let (url, model) = model_server(false, hold.clone(), arrived.clone()).await;
    configure(&client, url, false).await;
    let other = Client::connect(&dir.path().join("data"))
        .await
        .unwrap()
        .unwrap();
    let caller = client.clone();
    let timed = tokio::spawn(async move {
        caller
            .call(
                Operation::RunQuery {
                    query: query("timeout"),
                },
                Duration::from_millis(200),
            )
            .await
    });
    let caller = other.clone();
    let successful = tokio::spawn(async move {
        caller
            .call(
                Operation::RunQuery {
                    query: query("unaffected"),
                },
                Duration::from_secs(10),
            )
            .await
    });
    let error = timed.await.unwrap().err().unwrap();
    assert!(error.to_string().contains("超时"));
    hold.store(false, Ordering::SeqCst);
    assert!(successful.await.unwrap().is_ok());
    assert!(matches!(
        call(&client, Operation::Info).await,
        Output::Info(_)
    ));
    stop(&client, server).await;
    model.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queued_conversation_request_cancels_without_waiting_for_the_active_request() {
    let (_dir, client, server) = start().await;
    let hold = Arc::new(AtomicBool::new(true));
    let arrived = Arc::new(AtomicUsize::new(0));
    let (url, model) = model_server(false, hold.clone(), arrived.clone()).await;
    configure(&client, url, false).await;
    let first = client.clone();
    let active = tokio::spawn(async move {
        first
            .call(
                Operation::RunQuery {
                    query: query("same-session"),
                },
                Duration::from_secs(15),
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        while arrived.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let second = client.clone();
    let mut queued = query("queued");
    queued.conversation_id = Some("same-session".into());
    let waiting = tokio::spawn(async move {
        second
            .call(
                Operation::RunQuery { query: queued },
                Duration::from_secs(15),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    call(
        &client,
        Operation::CancelQuery {
            id: "queued".into(),
        },
    )
    .await;
    let result = tokio::time::timeout(Duration::from_secs(1), waiting).await;
    hold.store(false, Ordering::SeqCst);
    assert!(result
        .expect("queued request should cancel immediately")
        .unwrap()
        .is_err());
    assert!(active.await.unwrap().is_ok());
    stop(&client, server).await;
    model.abort();
}

async fn start() -> (
    tempfile::TempDir,
    Client,
    tokio::task::JoinHandle<anyhow::Result<()>>,
) {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let task = tokio::spawn(crate::server::run(data.clone()));
    let client = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(client) = Client::connect(&data).await.unwrap() {
                break client;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    (dir, client, task)
}
async fn call(client: &Client, operation: Operation) -> Output {
    client
        .call(operation, Duration::from_secs(20))
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_svg_image_uses_image_data_url_and_folder_authorization() {
    let (dir, client, server) = start().await;
    let notes = dir.path().join("notes");
    std::fs::create_dir(&notes).unwrap();
    let logo = notes.join("logo.svg");
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"10\" height=\"10\"/></svg>";
    std::fs::write(&logo, svg).unwrap();
    call(&client, Operation::AddRoot { path: notes.display().to_string() }).await;
    let Output::Text(image) = call(&client, Operation::LoadImage { path: logo.display().to_string() }).await else { panic!("image") };
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(image.strip_prefix("data:image/svg+xml;base64,").unwrap()).unwrap();
    assert_eq!(bytes, svg.as_bytes());
    let outside = dir.path().join("outside.svg");
    std::fs::write(&outside, svg).unwrap();
    let result = client.call(Operation::LoadImage { path: outside.display().to_string() }, Duration::from_secs(5)).await;
    assert!(matches!(result, Err(error) if error.to_string().contains("不在已添加的文件夹内")));
    stop(&client, server).await;
}
async fn stop(client: &Client, task: tokio::task::JoinHandle<anyhow::Result<()>>) {
    call(client, Operation::Shutdown).await;
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn jev_auth_mode_cannot_disable_answer_provider_key_protection() {
    let (dir, client, server) = start().await;
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    let previous_url = settings.answer.url.clone();
    crate::models::save_secret(&dir.path().join("data"), "answer", "previous-answer-key").unwrap();
    settings.answer = serde_json::from_value(serde_json::json!({
        "url":"http://127.0.0.1:1/v1", "model":"local-model", "auth_mode":"none"
    })).unwrap();
    let result = client.call(Operation::SaveSettings { mineru_key:String::new(),
        settings: settings.clone(), answer_key: String::new(), jev_key: String::new(), mcp_credentials: None,
    }, Duration::from_secs(5)).await;
    assert!(result.is_err(), "Jev-only auth mode was accepted for the answer model");
    let result = client.call(Operation::TestModel {
        kind: "answer".into(), config: settings.answer, key: String::new(), model_proxy: None,
    }, Duration::from_secs(5)).await;
    assert!(result.err().unwrap().to_string().contains("问答模型"));
    let Output::Settings(saved) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert_eq!(saved.answer.url, previous_url);
    assert_eq!(crate::models::secret(&dir.path().join("data"), "answer").unwrap(), "previous-answer-key");
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn answer_model_catalog_uses_the_configured_provider_and_protects_its_saved_key() {
    let (dir, client, server) = start().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    settings.answer.url = format!("{origin}/proxy/v1/chat/completions");
    call(&client, Operation::SaveSettings {
        settings: settings.clone(), answer_key: "fixture-saved-key".into(), jev_key: String::new(),
        mineru_key: String::new(), mcp_credentials: None,
    }).await;
    let fixture = tokio::spawn(async move {
        let body = r#"{"data":[{"id":"test-b"},{"id":"test-a"},{"id":"test-b"}]}"#;
        for (path, key) in [("/proxy/v1/models", "fixture-saved-key"), ("/proxy/v1/models?project=demo", "fixture-draft-key")] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(stream.read_u8().await.unwrap());
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            assert!(request.starts_with(&format!("get {path} http/1.1\r\n")));
            assert!(request.contains(&format!("\r\nauthorization: bearer {key}\r\n")));
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });
    let fetch = |config, key: &str| client.call(Operation::ListAnswerModels {config, key: key.into(), model_proxy: None}, Duration::from_secs(5));
    let Output::Strings(models) = fetch(settings.answer.clone(), "").await.unwrap() else { panic!("model list") };
    assert_eq!(models, ["test-a", "test-b"]);
    let mut config = settings.answer;
    config.url = format!("{origin}/proxy/v1/responses?project=demo");
    assert!(matches!(fetch(config.clone(), "fixture-draft-key").await.unwrap(), Output::Strings(_)));
    config.url = "http://127.0.0.1:1/v1".into();
    let error = fetch(config, "").await.err().unwrap().to_string();
    assert!(error.contains("切换供应商后"));
    assert_eq!(crate::models::secret(&dir.path().join("data"), "answer").unwrap(), "fixture-saved-key");
    fixture.await.unwrap();
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn model_proxy_routes_draft_and_saved_connections_for_both_model_kinds() {
    let (_dir, client, server) = start().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let model_proxy = ModelProxy {
        mode: crate::types::ProxyMode::Custom,
        url: format!("http://{}", listener.local_addr().unwrap()),
        ..Default::default()
    };
    let fixture = tokio::spawn(async move {
        let body = r#"{"choices":[{"message":{"content":"OK"}}],"answers":{"test":{"noul":1}}}"#;
        for path in ["chat/completions", "systemone"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                header.push(stream.read_u8().await.unwrap());
            }
            let header = String::from_utf8(header).unwrap();
            assert!(header.starts_with(&format!("POST http://192.0.2.10/v1/{path} HTTP/1.1\r\n")));
            let length: usize = header.lines().find_map(|line| line.strip_prefix("content-length: ")).unwrap().parse().unwrap();
            stream.read_exact(&mut vec![0; length]).await.unwrap();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    settings.answer.url = "http://192.0.2.10/v1".into();
    settings.jev.url = "http://192.0.2.10/v1/systemone".into();
    assert!(matches!(call(&client, Operation::TestModel {
        kind: "answer".into(), config: settings.answer.clone(), key: "fixture-key".into(), model_proxy: Some(model_proxy.clone()),
    }).await, Output::Text(result) if result == "连接成功"));
    let Output::Settings(before_save) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert!(before_save.model_proxy.url.is_empty());
    settings.model_proxy = model_proxy;
    call(&client, Operation::SaveSettings {
        settings: settings.clone(), answer_key: "fixture-key".into(), jev_key: "fixture-key".into(), mineru_key: String::new(), mcp_credentials: None,
    }).await;
    assert!(matches!(call(&client, Operation::TestModel {
        kind: "jev".into(), config: settings.jev, key: String::new(), model_proxy: None,
    }).await, Output::Text(result) if result == "连接成功"));
    fixture.await.unwrap();
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remote_mcp_connection_test_uses_the_saved_proxy() {
    let (_dir, client, server) = start().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    settings.model_proxy.mode = crate::types::ProxyMode::Custom;
    settings.model_proxy.url = format!("http://{}", listener.local_addr().unwrap());
    call(&client, Operation::SaveSettings {
        settings, answer_key: String::new(), jev_key: String::new(), mineru_key: String::new(), mcp_credentials: None,
    }).await;
    let fixture = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            header.push(stream.read_u8().await.unwrap());
        }
        assert!(header.starts_with(b"CONNECT 192.0.2.10:443 HTTP/1.1\r\n"));
        stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
    });
    let result = client.call(Operation::TestMcp {
        server: McpServer { id: "proxy-test".into(), name: "Proxy test".into(), url: "https://192.0.2.10/mcp".into(), ..Default::default() },
        credentials: Some(McpCredentials::default()),
    }, Duration::from_secs(5)).await;
    tokio::time::timeout(Duration::from_secs(5), fixture).await.unwrap().unwrap();
    assert!(result.is_err());
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invalid_local_jev_settings_do_not_remove_the_saved_key() {
    let (dir, client, server) = start().await;
    crate::models::save_secret(&dir.path().join("data"), "jev", "previous-jev-key").unwrap();
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    settings.jev.auth_mode = ModelAuthMode::None;
    settings.retrieval.candidate_k = 0;
    let result = client.call(Operation::SaveSettings { mineru_key:String::new(),
        settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None,
    }, Duration::from_secs(5)).await;
    assert!(result.is_err());
    assert_eq!(crate::models::secret(&dir.path().join("data"), "jev").unwrap(), "previous-jev-key");
    let Output::Settings(saved) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert_eq!(saved.jev.auth_mode, ModelAuthMode::Bearer);
    stop(&client, server).await;
}
fn query(id: &str) -> Query {
    Query {
        id: id.into(),
        kind: "ask".into(),
        question: format!("测试 {id}"),
        scope: vec![],
        references: vec![],
        model: "test".into(),
        conversation_id: None,
        replace_from: None,
        page: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn help_uses_only_bundled_documents_and_reads_citations_by_path() {
    let (dir, client, server) = start().await;
    let private = dir.path().join("notes");
    std::fs::create_dir(&private).unwrap();
    let file = private.join("private.md");
    std::fs::write(&file, "# Private\n\nxyqzxxyalone987").unwrap();
    call(&client, Operation::AddRoot { path: private.display().to_string() }).await;
    let Output::Settings(settings) = call(&client, Operation::GetSettings).await else {
        panic!("settings")
    };
    let mut request = query("built-in-help");
    request.kind = "ask_help".into();
    request.question = "how do I add a folder?".into();
    request.model = settings.answer.model;
    let Output::Query(output) = call(&client, Operation::RunQuery { query: request.clone() }).await else {
        panic!("query output")
    };
    assert!(!output.trace.is_empty());
    let conversation = output.conversation.unwrap();
    let message = &conversation.messages[0];
    assert!(message.help);
    assert_eq!(message.status, "complete");
    assert!(message.answer.contains("[1]"));
    assert!(message.tool_calls.is_empty());
    assert!(!message.evidence.is_empty());
    assert!(message.evidence.iter().all(|candidate| candidate.path.starts_with("sag-help/")));
    let source = &message.evidence[0];
    let Output::Document(document) = call(&client, Operation::ReadHelpDocument { path: source.path.clone() }).await else {
        panic!("bundled document")
    };
    assert_eq!(document.hash, source.hash);
    assert!(!source.source_spans.is_empty());
    assert!(source.source_spans.iter().all(|span|
        !document.body[span.start_byte..span.end_byte].trim().is_empty()));
    assert!(client.call(Operation::ReadHelpDocument { path: file.display().to_string() }, Duration::from_secs(2)).await.is_err());

    request.id = "help-private".into();
    request.question = "xyqzxxyalone987".into();
    let Output::Query(output) = call(&client, Operation::RunQuery { query: request.clone() }).await else {
        panic!("query output")
    };
    assert!(output.results.is_empty());
    assert!(output.conversation.unwrap().messages[0].evidence.is_empty());

    let hold = Arc::new(AtomicBool::new(false));
    let (url, model) = model_server(false, hold.clone(), Arc::new(AtomicUsize::new(0))).await;
    configure(&client, url, true).await;
    request.id = "help-with-model".into();
    request.model = "test".into();
    request.question = "how do I add a folder? (built-in help fixture)".into();
    let mut events = client.subscribe();
    let caller = client.clone();
    let query = request.clone();
    let active = tokio::spawn(async move { call(&caller, Operation::RunQuery { query }).await });
    let first_text = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let ServiceEvent::QueryProgress(progress) = events.recv().await.unwrap().event {
                if progress.id == "help-with-model" && progress.delta.is_some() {
                    break progress;
                }
            }
        }
    }).await;
    let finished = active.is_finished();
    hold.store(false, Ordering::SeqCst);
    let first_text = first_text.expect("help must send answer text before the model finishes");
    assert!(!finished);
    assert_eq!(first_text.delta.as_deref(), Some("添加文件夹 [1]"));
    assert!(!first_text.replace);
    let Output::Query(output) = active.await.unwrap() else {
        panic!("query output")
    };
    let message = &output.conversation.unwrap().messages[0];
    assert_eq!(message.answer, "添加文件夹 [1]");
    assert_eq!(message.tool_calls.len(), 2);
    assert!(message.evidence.iter().all(|candidate| candidate.path.starts_with("sag-help/")));
    model.abort();

    request.id = "help-with-reference".into();
    request.references = vec![file.display().to_string()];
    assert!(client.call(Operation::RunQuery { query: request }, Duration::from_secs(2)).await.is_err());
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remove_all_roots_notifies_clients_and_can_add_folders_again() {
    let (dir, client, server) = start().await;
    let notes = dir.path().join("notes");
    std::fs::create_dir(&notes).unwrap();
    let source = notes.join("kept.txt");
    std::fs::write(&source, "preserved document").unwrap();
    call(&client, Operation::AddRoot {path:notes.display().to_string()}).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let Output::Roots(roots) = call(&client, Operation::ListRoots).await else { panic!("roots") };
            if roots[0].status == "ready" { break; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.unwrap();
    let mut events = client.subscribe();
    assert!(matches!(call(&client, Operation::RemoveAllRoots).await, Output::Unit));
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(events.recv().await.unwrap().event, ServiceEvent::DataChanged) { break; }
        }
    }).await.unwrap();
    let Output::Snapshot(snapshot) = call(&client, Operation::Snapshot).await else { panic!("snapshot") };
    assert!(snapshot.roots.is_empty());
    assert!(snapshot.files.is_empty());
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "preserved document");
    call(&client, Operation::AddRoot {path:notes.display().to_string()}).await;
    let Output::Roots(roots) = call(&client, Operation::ListRoots).await else { panic!("roots") };
    assert_eq!(roots.len(), 1);
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn clear_all_data_removes_settings_secrets_drafts_and_notifies_all_views() {
    let (dir, client, server) = start().await;
    configure(&client, "https://custom.example/v1".into(), true).await;
    call(&client, Operation::SetAccess {access:Access {enabled:false}}).await;
    crate::models::save_secret(&dir.path().join("data"), "jev", "test-jev-key").unwrap();
    crate::models::save_mcp_credentials(&dir.path().join("data"), "unused", &McpCredentials {
        headers: [("Authorization".into(), "test-mcp-key".into())].into(), ..Default::default()
    }).unwrap();
    let notes = dir.path().join("notes");
    std::fs::create_dir(&notes).unwrap();
    let source = notes.join("source.txt");
    std::fs::write(&source, "original file").unwrap();
    call(&client, Operation::AddRoot { path: notes.display().to_string() }).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let Output::Roots(roots) = call(&client, Operation::ListRoots).await else { panic!("roots") };
            if roots[0].status == "ready" { break; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.unwrap();
    call(&client, Operation::PutDraft {draft:Draft {path:source.display().to_string(),name:"source.txt".into(),body:"draft contents".into(),hash:"original".into(),is_new:false}}).await;
    let mut events = client.subscribe();
    assert!(matches!(call(&client, Operation::ClearAllData).await, Output::Unit));
    tokio::time::timeout(Duration::from_secs(5), async {
        let (mut data, mut history, mut settings) = (false, false, false);
        while !data || !history || !settings {
            match events.recv().await.unwrap().event {
                ServiceEvent::DataChanged => data = true,
                ServiceEvent::HistoryChanged => history = true,
                ServiceEvent::SettingsChanged => settings = true,
                _ => {}
            }
        }
    }).await.unwrap();
    let Output::Snapshot(snapshot) = call(&client, Operation::Snapshot).await else { panic!("snapshot") };
    assert!(snapshot.roots.is_empty());
    assert!(snapshot.files.is_empty());
    let Output::Conversations(history) = call(&client, Operation::ListConversations).await else { panic!("history") };
    assert!(history.is_empty());
    let Output::Settings(settings) = call(&client, Operation::GetSettings).await else {panic!("settings")};
    assert_eq!(serde_json::to_value(settings).unwrap(), serde_json::to_value(Settings::default()).unwrap());
    let Output::Drafts(drafts) = call(&client, Operation::ListDrafts).await else {panic!("drafts")};
    assert!(drafts.is_empty());
    let credentials:serde_json::Value=serde_json::from_slice(&std::fs::read(dir.path().join("data/credentials.json")).unwrap()).unwrap();
    assert_eq!(credentials,serde_json::json!({}));
    let Output::Access(access) = call(&client, Operation::GetAccess).await else {panic!("access")};
    assert_eq!(access.enabled,Access::default().enabled);
    assert_eq!(std::fs::read_to_string(source).unwrap(), "original file");
    stop(&client, server).await;
    let reopened = crate::store::Store::open(&dir.path().join("data/sag.sqlite")).unwrap();
    assert!(reopened.drafts().unwrap().is_empty());
    assert_eq!(serde_json::to_value(reopened.settings().unwrap()).unwrap(), serde_json::to_value(Settings::default()).unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_service_preserves_large_documents_and_index_events() {
    let (dir, client, server) = start().await;
    let notes = dir.path().join("notes");
    std::fs::create_dir(&notes).unwrap();
    let html = notes.join("large.html");
    let body = format!(
        "<html><body>visible needle<img src='data:image/png;base64,{}'></body></html>",
        "A".repeat(18 * 1024 * 1024)
    );
    std::fs::write(&html, &body).unwrap();
    let mut events = client.subscribe();
    call(
        &client,
        Operation::AddRoot {
            path: notes.display().to_string(),
        },
    )
    .await;
    let output = call(
        &client,
        Operation::ReadDocument {
            path: html.display().to_string(),
        },
    )
    .await;
    let Output::Document(document) = output else {
        panic!("expected document")
    };
    assert_eq!(document.body, body);
    let markdown = notes.join("editable.md");
    std::fs::write(&markdown, "old text").unwrap();
    let Output::Document(editable) = call(
        &client,
        Operation::ReadDocument {
            path: markdown.display().to_string(),
        },
    )
    .await
    else {
        panic!("document")
    };
    call(
        &client,
        Operation::SaveDocument {
            path: editable.path,
            body: "changed needle".into(),
            expected_hash: Some(editable.hash),
        },
    )
    .await;
    assert_eq!(
        std::fs::read_to_string(&markdown).unwrap(),
        "changed needle"
    );
    let changed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if matches!(
                events.recv().await.unwrap().event,
                ServiceEvent::DataChanged
            ) {
                break;
            }
        }
    })
    .await;
    assert!(changed.is_ok());
    let other = Client::connect(&dir.path().join("data"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(client.info().pid, other.info().pid);
    drop(client);
    let Output::Document(document) = call(
        &other,
        Operation::ReadDocument {
            path: html.display().to_string(),
        },
    )
    .await
    else {
        panic!("expected document")
    };
    assert_eq!(document.body, body);
    stop(&other, server).await;
}

async fn model_server(
    required: bool,
    hold: Arc<AtomicBool>,
    arrived: Arc<AtomicUsize>,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let hold = hold.clone();
            let arrived = arrived.clone();
            tokio::spawn(async move {
                let mut data = vec![];
                let mut buf = [0; 4096];
                let body = loop {
                    let n = stream.read(&mut buf).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    data.extend_from_slice(&buf[..n]);
                    if let Some(end) = data.windows(4).position(|p| p == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&data[..end]);
                        let length = header
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|v| v.parse::<usize>().ok())
                            })
                            .unwrap();
                        if data.len() >= end + 4 + length {
                            break serde_json::from_slice::<serde_json::Value>(
                                &data[end + 4..end + 4 + length],
                            )
                            .unwrap();
                        }
                    }
                };
                arrived.fetch_add(1, Ordering::SeqCst);
                while hold.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                let delta = if required
                    && !body["messages"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|message| message["role"] == "tool")
                {
                    serde_json::json!({"tool_calls":[{"index":0,"id":"approval-call","type":"function","function":{"name":"mcp_0_0","arguments":"{\"text\":\"hello\"}"}}]})
                } else if body["model"] == "summary-test" {
                    serde_json::json!({"content":"独立总结"})
                } else if let Some(messages) = body["messages"].as_array().filter(|messages| messages.iter()
                    .any(|message| message["role"] == "user" && message["content"].as_str()
                        .is_some_and(|input| input.contains("built-in help fixture"))))
                {
                    let tools = body["tools"].as_array().unwrap();
                    assert_eq!(tools.len(), 1);
                    assert_eq!(tools[0]["function"]["name"], "sag2_search");
                    let results: Vec<serde_json::Value> = messages.iter().filter(|message| message["role"] == "tool")
                        .map(|message| serde_json::from_str(message["content"].as_str().unwrap()).unwrap()).collect();
                    for result in &results {
                        assert!(result["sources"].as_array().unwrap().iter().all(|source|
                            source["path"].as_str().unwrap().starts_with("sag-help/")));
                    }
                    if results.is_empty() {
                        assert_eq!(body["tool_choice"], "auto");
                        serde_json::json!({"tool_calls":[{"index":0,"id":"start-help","type":"function","function":{"name":"sag2_search","arguments":"{\"query\":\"how do I add a folder?\"}"}}]})
                    } else if results.len() == 1 {
                        assert!(!results[0]["sources"].as_array().unwrap().is_empty());
                        serde_json::json!({"tool_calls":[{"index":0,"id":"more-help","type":"function","function":{"name":"sag2_search","arguments":"{\"query\":\"xyqzxxyalone987\"}"}}]})
                    } else {
                        assert!(results[1]["sources"].as_array().unwrap().is_empty());
                        serde_json::json!({"content":"添加文件夹 [1]"})
                    }
                } else {
                    serde_json::json!({"content":"完成"})
                };
                let help_answer = delta["content"] == "添加文件夹 [1]";
                let first_event = format!("data: {}\n\n", serde_json::json!({"choices":[{"delta":delta}]}));
                let end = format!("data: {}\n\ndata: [DONE]\n\n",
                    serde_json::json!({"choices":[],"usage":{"prompt_tokens":742,"completion_tokens":18}}));
                if help_answer {
                    hold.store(true, Ordering::SeqCst);
                }
                let response=format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",first_event.len() + end.len(),first_event);
                let _ = stream.write_all(response.as_bytes()).await;
                while help_answer && hold.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                let _ = stream.write_all(end.as_bytes()).await;
            });
        }
    });
    (format!("http://{address}/v1"), task)
}
async fn configure(client: &Client, url: String, mcp: bool) {
    let Output::Settings(mut settings) = call(client, Operation::GetSettings).await else {
        panic!("settings")
    };
    settings.answer.url = url;
    settings.answer.model = "test".into();
    settings.answer.models = vec!["test".into()];
    settings.timeout_secs = 10;
    if mcp {
        let server = crate::mcp::tests::local_server();
        settings.model_profiles.insert(
            "test".into(),
            ModelProfile {
                sag2_enabled: false,
                mcp_servers: vec![server.id.clone()],
                ..Default::default()
            },
        );
        settings.mcp_servers = vec![server];
    }
    call(
        client,
        Operation::SaveSettings { mineru_key:String::new(),
            settings,
            answer_key: "test-key".into(),
            jev_key: String::new(),
            mcp_credentials: None,
        },
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn twenty_requests_overlap_and_cancellation_is_owned_by_one_client() {
    let (dir, client, server) = start().await;
    let hold = Arc::new(AtomicBool::new(true));
    let arrived = Arc::new(AtomicUsize::new(0));
    let (url, model) = model_server(false, hold.clone(), arrived.clone()).await;
    configure(&client, url, false).await;
    let other = Client::connect(&dir.path().join("data"))
        .await
        .unwrap()
        .unwrap();
    let mut jobs = tokio::task::JoinSet::new();
    for i in 0..20 {
        let client = client.clone();
        jobs.spawn(async move {
            (
                i,
                client
                    .call(
                        Operation::RunQuery {
                            query: query(&format!("request-{i}")),
                        },
                        Duration::from_secs(15),
                    )
                    .await,
            )
        });
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        while arrived.load(Ordering::SeqCst) < 20 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("all 20 model requests must arrive before any response");
    call(
        &other,
        Operation::CancelQuery {
            id: "request-0".into(),
        },
    )
    .await;
    assert_eq!(arrived.load(Ordering::SeqCst), 20);
    call(
        &client,
        Operation::CancelQuery {
            id: "request-0".into(),
        },
    )
    .await;
    hold.store(false, Ordering::SeqCst);
    let mut completed = 0;
    let mut cancelled = 0;
    while let Some(job) = jobs.join_next().await {
        let (i, result) = job.unwrap();
        if i == 0 {
            assert!(result.is_err());
            cancelled += 1;
        } else {
            let Output::Query(output) = result.unwrap() else {
                panic!("query")
            };
            assert_eq!(
                output.conversation.unwrap().messages.last().unwrap().answer,
                "完成"
            );
            completed += 1;
        }
    }
    assert_eq!((completed, cancelled), (19, 1));
    stop(&client, server).await;
    model.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tool_approval_is_delivered_only_to_the_requesting_client() {
    let (dir, client, server) = start().await;
    let (url, model) = model_server(
        true,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicUsize::new(0)),
    )
    .await;
    configure(&client, url, true).await;
    let other = Client::connect(&dir.path().join("data"))
        .await
        .unwrap()
        .unwrap();
    let mut own = client.subscribe();
    let mut foreign = other.subscribe();
    let asking = client.clone();
    let answer = tokio::spawn(async move {
        asking
            .call(
                Operation::RunQuery {
                    query: query("approval"),
                },
                Duration::from_secs(15),
            )
            .await
    });
    let approval = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let ServiceEvent::ToolApproval(value) = own.recv().await.unwrap().event {
                break value;
            }
        }
    })
    .await
    .unwrap();
    while let Ok(event) = foreign.try_recv() {
        assert!(!matches!(event.event, ServiceEvent::ToolApproval(_)));
    }
    assert!(other
        .call(
            Operation::ResolveToolApproval {
                id: approval.id.clone(),
                allow: true
            },
            Duration::from_secs(5)
        )
        .await
        .is_err());
    call(
        &client,
        Operation::ResolveToolApproval {
            id: approval.id,
            allow: false,
        },
    )
    .await;
    let Output::Query(output) = answer.await.unwrap().unwrap() else {
        panic!("query")
    };
    assert!(output.conversation.unwrap().messages[0].tool_calls[0].denied);
    stop(&client, server).await;
    model.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn clear_conversations_rejects_active_answers_and_removes_hidden_history() {
    let (_dir, client, server) = start().await;
    let hold = Arc::new(AtomicBool::new(true));
    let arrived = Arc::new(AtomicUsize::new(0));
    let (url, model) = model_server(false, hold.clone(), arrived.clone()).await;
    configure(&client, url, false).await;
    let caller = client.clone();
    let active = tokio::spawn(async move { caller.call(Operation::RunQuery {query:query("clear-history")}, Duration::from_secs(10)).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while arrived.load(Ordering::SeqCst) == 0 { tokio::time::sleep(Duration::from_millis(5)).await; }
    }).await.unwrap();
    let error = client.call(Operation::ClearConversations, Duration::from_secs(5)).await.err().unwrap();
    assert!(error.to_string().contains("请先停止正在进行的问答"));
    let error = client.call(Operation::ClearAllData, Duration::from_secs(5)).await.err().unwrap();
    assert!(error.to_string().contains("请先停止正在进行的问答"));
    hold.store(false, Ordering::SeqCst);
    active.await.unwrap().unwrap();
    call(&client, Operation::EditConversation {id:"clear-history".into(),title:None,deleted:Some(true)}).await;
    assert!(matches!(call(&client, Operation::ClearConversations).await,Output::Unit));
    let Output::Conversations(history)=call(&client,Operation::ListConversations).await else {panic!("history")};
    assert!(history.is_empty());
    let Output::Query(output)=call(&client,Operation::RunQuery {query:query("new-after-clear")}).await else {panic!("query")};
    assert_eq!(output.conversation.unwrap().id,"new-after-clear");
    stop(&client,server).await;
    model.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn search_summary_streams_without_history_and_respects_limit_disable_and_cancel() {
    let (dir, client, server) = start().await;
    let hold = Arc::new(AtomicBool::new(false));
    let arrived = Arc::new(AtomicUsize::new(0));
    let (url, model) = model_server(false, hold.clone(), arrived.clone()).await;
    configure(&client, url, false).await;
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert!(settings.search_summary_model.is_empty());
    settings.answer.models.push("summary-test".into());
    settings.search_summary_model = "summary-test".into();
    call(&client, Operation::SaveSettings { mineru_key:String::new(), settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    let notes = dir.path().join("notes");
    std::fs::create_dir(&notes).unwrap();
    for index in 0..6 {
        std::fs::write(notes.join(format!("{index}.txt")), format!("collaboration evidence {index}")).unwrap();
    }
    call(&client, Operation::AddRoot { path: notes.display().to_string() }).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let Output::Roots(roots) = call(&client, Operation::ListRoots).await else { panic!("roots") };
            if roots[0].status == "ready" { break; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.unwrap();
    let mut search = query("search");
    search.kind = "search".into();
    search.question = "collaboration".into();
    let Output::Query(output) = call(&client, Operation::RunQuery { query: search }).await else { panic!("query") };
    assert_eq!(output.results.len(), 6);
    let request = |id: &str, count| Operation::SummarizeSearch {
        id: id.into(), question: "How is collaboration described?".into(),
        results: output.results[..count].to_vec(),
    };
    assert!(client.call(request("empty", 0), Duration::from_secs(5)).await.err().unwrap().to_string().contains("需要搜索结果"));
    assert_eq!(arrived.load(Ordering::SeqCst), 0);
    let mut events = client.subscribe();
    let Output::SearchSummary(summary) = call(&client, request("summary", 6)).await else { panic!("summary") };
    assert_eq!(summary.text, "独立总结");
    assert_eq!(summary.evidence.len(), 6);
    assert_eq!(summary.context.input_tokens, Some(742));
    assert_eq!(summary.context.output_tokens, Some(18));
    assert!(summary.context.estimated_input > 0);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let ServiceEvent::QueryProgress(progress) = events.recv().await.unwrap().event {
                if progress.id == "summary" {
                    assert_eq!(progress.delta.as_deref(), Some("独立总结")); break;
                }
            }
        }
    }).await.unwrap();
    assert!(matches!(call(&client, Operation::ListConversations).await, Output::Conversations(history) if history.is_empty()));
    hold.store(true, Ordering::SeqCst);
    let caller = client.clone();
    let pending = request("cancel-summary", 5);
    let active = tokio::spawn(async move { caller.call(pending, Duration::from_secs(10)).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while arrived.load(Ordering::SeqCst) < 2 { tokio::time::sleep(Duration::from_millis(5)).await; }
    }).await.unwrap();
    call(&client, Operation::CancelQuery { id: "cancel-summary".into() }).await;
    assert!(tokio::time::timeout(Duration::from_secs(1), active).await.unwrap().unwrap().is_err());
    hold.store(false, Ordering::SeqCst);
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert!(settings.search_summary_enabled);
    assert_eq!(settings.answer.model, "test");
    assert_eq!(settings.search_summary_model, "summary-test");
    settings.search_summary_enabled = false;
    call(&client, Operation::SaveSettings { mineru_key:String::new(), settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    assert!(client.call(request("disabled", 5), Duration::from_secs(5)).await.err().unwrap().to_string().contains("已关闭"));
    assert_eq!(arrived.load(Ordering::SeqCst), 2);
    let Output::Settings(settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert!(!settings.search_summary_enabled);
    stop(&client, server).await;
    model.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn index_details_support_pause_resume_and_shutdown_over_rpc() {
    let (dir, client, server) = start().await;
    let Output::Settings(settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert_eq!(settings.conversion_workers, 4);
    call(&client, Operation::SetConversionWorkers { workers: 2 }).await;
    let Output::Settings(settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert_eq!(settings.conversion_workers, 2);
    assert!(matches!(call(&client, Operation::SetIndexPaused { paused: true }).await,
        Output::IndexStatus(status) if status.paused));
    let root = dir.path().join("notes");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("meeting.md"), "meeting evidence").unwrap();
    call(&client, Operation::AddRoot { path: root.to_string_lossy().into_owned() }).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let Output::IndexStatus(status) = call(&client, Operation::GetIndexStatus).await else { panic!("index status") };
            if !status.tasks.is_empty() {
                assert!(status.paused);
                assert_eq!(status.tasks[0].path, root.canonicalize().unwrap().to_string_lossy());
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    assert!(matches!(call(&client, Operation::ListFiles).await, Output::Files(files) if files.is_empty()));
    call(&client, Operation::SetIndexPaused { paused: false }).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(call(&client, Operation::ListFiles).await, Output::Files(files) if files.len()==1) { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let Output::IndexHistory(records) = call(&client, Operation::GetIndexHistory).await else { panic!("index history") };
            if !records.is_empty() {
                assert_eq!(records[0].status, "ready");
                assert_eq!(records[0].completed, 1);
                let Output::IndexHistoryLog(log) = call(&client, Operation::GetIndexHistoryLog { id: records[0].id }).await else { panic!("index history log") };
                let file: sag_protocol::IndexFileLog = serde_json::from_str(log.raw.as_ref().unwrap().lines().next().unwrap()).unwrap();
                assert!(file.path.ends_with("meeting.md"));
                assert!(matches!(call(&client, Operation::AuthorizeIndexHistoryFile { id: records[0].id, path: file.path.clone() }).await, Output::Text(path) if path == file.path));
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    call(&client, Operation::ClearIndexHistory).await;
    assert!(matches!(call(&client, Operation::GetIndexHistory).await, Output::IndexHistory(records) if records.is_empty()));
    assert!(matches!(call(&client, Operation::ListFiles).await, Output::Files(files) if files.len()==1));
    call(&client, Operation::SetIndexPaused { paused: true }).await;
    call(&client, Operation::ReindexAll).await;
    // ReindexAll queues work; wait for the worker to enter its paused scan
    // before testing that clearing history preserves that active task.
    let root_path = root.canonicalize().unwrap().to_string_lossy().into_owned();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let Output::IndexStatus(status) = call(&client, Operation::GetIndexStatus).await else { panic!("index status") };
            assert!(status.paused);
            if status.tasks.iter().any(|task| task.path == root_path && task.stage == "scanning") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("queued reindex should reach its paused scan");
    call(&client, Operation::ClearIndexHistory).await;
    assert!(matches!(call(&client, Operation::GetIndexStatus).await, Output::IndexStatus(status) if status.paused && !status.tasks.is_empty()));
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn folder_index_options_apply_through_rpc_and_rebuild_only_the_requested_root() {
    let (dir, client, server) = start().await;
    let folder = dir.path().join("local-rules");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("note.md"), "documentation").unwrap();
    std::fs::write(folder.join("main.py"), "print('local settings')").unwrap();
    let initial = FolderIndexOptions { extensions: None, file_types: FileTypes { markdown: true, text: false, code: false, config: false }, convert_other_formats: false, conversion_formats: None };
    let Output::Text(root) = call(&client, Operation::AddRootWithOptions { path: folder.to_string_lossy().into_owned(), options: initial.clone() }).await else { panic!("root") };
    for (options, name) in [
        (initial.clone(), "note.md"),
        (FolderIndexOptions { file_types: FileTypes { markdown: false, code: true, ..initial.file_types.clone() }, ..initial.clone() }, "main.py"),
    ] {
        if name == "main.py" {
            call(&client, Operation::SaveFolderIndexOptions { path: root.clone(), options: options.clone(), rebuild: true }).await;
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let Output::Roots(roots) = call(&client, Operation::ListRoots).await else { panic!("roots") };
                if roots.iter().any(|item| item.path == root && item.status == "ready") { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        let Output::Files(files) = call(&client, Operation::ListFiles).await else { panic!("files") };
        assert_eq!(files.len(), 1);
        assert!(files[0].path.ends_with(name));
        let Output::FolderIndexOptions(saved) = call(&client, Operation::GetFolderIndexOptions { path: root.clone() }).await else { panic!("options") };
        assert_eq!(saved, options);
    }
    assert!(folder.join("note.md").exists());
    assert!(folder.join("main.py").exists());
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn nested_folder_settings_reindex_only_the_selected_subdirectory_over_rpc() {
    let (dir, client, server) = start().await;
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    settings.watch = false;
    call(&client, Operation::SaveSettings { mineru_key:String::new(), settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    let folder = dir.path().join("project");
    std::fs::create_dir_all(folder.join("papers")).unwrap();
    std::fs::create_dir_all(folder.join("sibling")).unwrap();
    std::fs::write(folder.join("papers").join("note.md"), "paper evidence").unwrap();
    std::fs::write(folder.join("papers").join("main.py"), "print('code')").unwrap();
    std::fs::write(folder.join("sibling").join("note.md"), "sibling evidence").unwrap();
    let initial = FolderIndexOptions { extensions: None, file_types: FileTypes { markdown: true, text: false, code: false, config: false }, convert_other_formats: false, conversion_formats: None };
    let Output::Text(root) = call(&client, Operation::AddRootWithOptions { path: folder.to_string_lossy().into_owned(), options: initial.clone() }).await else { panic!("root") };
    let child = Path::new(&root).join("papers").to_string_lossy().into_owned();
    let changed = FolderIndexOptions { file_types: FileTypes { markdown: false, code: true, ..initial.file_types.clone() }, ..initial.clone() };
    for (expected, name) in [(initial, "note.md"), (changed.clone(), "main.py")] {
        if name == "main.py" {
            std::fs::write(folder.join("sibling").join("note.md"), [0xff, 0xfe, 0x00]).unwrap();
            call(&client, Operation::SaveFolderIndexOptions { path: child.clone(), options: changed.clone(), rebuild: true }).await;
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let Output::Roots(roots) = call(&client, Operation::ListRoots).await else { panic!("roots") };
                if roots.iter().any(|item| item.path == root && item.status == "ready") { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        let Output::FolderIndexOptions(saved) = call(&client, Operation::GetFolderIndexOptions { path: child.clone() }).await else { panic!("options") };
        assert_eq!(saved, expected);
        let Output::Files(files) = call(&client, Operation::ListFiles).await else { panic!("files") };
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|file| Path::new(&file.path) == Path::new(&child).join(name).as_path()));
        assert!(files.iter().any(|file| Path::new(&file.path) == Path::new(&root).join("sibling").join("note.md").as_path()));
    }
    assert!(folder.join("papers").join("note.md").exists());
    assert!(folder.join("papers").join("main.py").exists());
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn conversion_profile_settings_validate_changed_formats_without_blocking_other_saves() {
    let (dir, client, server) = start().await;
    let home = dir.path().join("data/docling");
    std::fs::create_dir_all(home.join("bin")).unwrap();
    let binary = if cfg!(windows) { "docling.exe" } else { "docling" };
    std::fs::write(home.join("bin").join(binary), "fixture").unwrap();
    std::fs::write(home.join("ready"), "2.140.0").unwrap();
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    settings.convert_other_formats = true;
    settings.conversion_formats = Some(vec!["pdf".into()]);
    call(&client, Operation::SaveSettings { mineru_key:String::new(), settings: settings.clone(), answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    settings.conversion_formats = Some(vec!["mp3".into()]);
    let rejected = client.call(Operation::SaveSettings { mineru_key:String::new(), settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }, Duration::from_secs(5)).await;
    assert!(rejected.is_err(), "a document runtime must not authorize a new media selection");
    let message = rejected.err().unwrap().to_string();
    assert!(message.contains("音视频") || message.contains("媒体") || message.contains("media"), "{message}");
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert_eq!(settings.conversion_formats, Some(vec!["pdf".into()]));
    // An unavailable component must not prevent saving unrelated model choices.
    std::fs::remove_dir_all(&home).unwrap();
    settings.answer.reasoning_effort = Some(ReasoningEffort::High);
    call(&client, Operation::SaveSettings { mineru_key:String::new(), settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert_eq!(settings.answer.reasoning_effort, Some(ReasoningEffort::High));
    settings.convert_other_formats = false;
    settings.conversion_formats = Some(vec!["mp3".into()]);
    call(&client, Operation::SaveSettings { mineru_key:String::new(), settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    assert!(!home.exists(), "saving settings must never download a component");
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn folder_conversion_profile_validation_precedes_rule_writes_and_rebuilds() {
    let (dir, client, server) = start().await;
    let folder = dir.path().join("media-rules");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("note.md"), "preservedfolderindex").unwrap();
    std::fs::write(folder.join(".sagignore"), "private-*\n").unwrap();
    let selected = FolderIndexOptions { extensions: None, file_types: FileTypes::default(), convert_other_formats: true, conversion_formats: Some(vec!["mp3".into()]) };
    let rejected = client.call(Operation::AddRootWithOptions { path: folder.to_string_lossy().into_owned(), options: selected.clone() }, Duration::from_secs(5)).await;
    assert!(rejected.is_err(), "explicit media options require a media component");
    assert_eq!(std::fs::read_to_string(folder.join(".sagignore")).unwrap(), "private-*\n");
    assert!(matches!(call(&client, Operation::ListRoots).await, Output::Roots(roots) if roots.is_empty()));
    let Output::Text(root) = call(&client, Operation::AddRoot { path: folder.to_string_lossy().into_owned() }).await else { panic!("root") };
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(call(&client, Operation::ListRoots).await, Output::Roots(roots) if roots.iter().any(|item| item.path == root && item.status == "ready")) { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    let rejected = client.call(Operation::SaveFolderIndexOptions { path: root.clone(), options: selected, rebuild: true }, Duration::from_secs(5)).await;
    assert!(rejected.is_err(), "rebuild validation must precede deleting the old index");
    assert_eq!(std::fs::read_to_string(folder.join(".sagignore")).unwrap(), "private-*\n");
    assert!(matches!(call(&client, Operation::ListFiles).await, Output::Files(files) if files.len() == 1 && files[0].path.ends_with("note.md")));
    let Output::FolderIndexOptions(mut options) = call(&client, Operation::GetFolderIndexOptions { path: root.clone() }).await else { panic!("options") };
    assert!(!options.convert_other_formats);
    assert!(!dir.path().join("data/docling").exists(), "folder operations must never download components");
    let home = dir.path().join("data/docling");
    std::fs::create_dir_all(home.join("bin")).unwrap();
    std::fs::write(home.join("bin").join(if cfg!(windows) { "docling.exe" } else { "docling" }), "fixture").unwrap();
    std::fs::write(home.join("ready"), "2.140.0").unwrap();
    options.convert_other_formats = true;
    options.conversion_formats = Some(vec!["pdf".into()]);
    call(&client, Operation::SaveFolderIndexOptions { path: root.clone(), options: options.clone(), rebuild: false }).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(call(&client, Operation::ListRoots).await, Output::Roots(roots) if roots.iter().any(|item| item.path == root && item.status == "ready")) { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    std::fs::remove_dir_all(&home).unwrap();
    options.file_types.code = true;
    call(&client, Operation::SaveFolderIndexOptions { path: root.clone(), options, rebuild: true }).await;
    assert!(!home.exists(), "unrelated folder changes must not recheck or install an unchanged profile selection");
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inherited_legacy_conversion_options_report_missing_media_during_scan_without_installing() {
    let (dir, client, server) = start().await;
    // Simulate an existing library preference from a prior version, without an installed runtime.
    let store = crate::store::Store::open(&dir.path().join("data/sag.sqlite")).unwrap();
    let mut settings = store.settings().unwrap();
    settings.convert_other_formats = true;
    settings.conversion_formats = None;
    store.save_settings(&settings).unwrap();
    drop(store);
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    settings.answer.reasoning_effort = Some(ReasoningEffort::High);
    call(&client, Operation::SaveSettings { mineru_key:String::new(), settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    let folder = dir.path().join("legacy-media");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("note.md"), "nativeevidence").unwrap();
    std::fs::write(folder.join("recording.mp3"), "small fake audio").unwrap();
    let Output::Text(root) = call(&client, Operation::AddRoot { path: folder.to_string_lossy().into_owned() }).await else { panic!("root") };
    let issue = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let Output::Roots(roots) = call(&client, Operation::ListRoots).await else { panic!("roots") };
            if let Some(item) = roots.iter().find(|item| item.path == root && item.status == "partial") { break item.error.clone(); }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap().expect("the missing media profile must be reported for the source");
    assert!(issue.contains("音视频") || issue.contains("媒体") || issue.contains("media"), "{issue}");
    assert!(matches!(call(&client, Operation::ListFiles).await, Output::Files(files) if files.len() == 1 && files[0].path.ends_with("note.md")));
    assert!(!dir.path().join("data/docling").exists(), "ordinary scans must never install profiles");
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn changing_only_conversion_formats_rescans_cached_selected_sources() {
    let (dir, client, server) = start().await;
    let home = dir.path().join("data/docling");
    std::fs::create_dir_all(home.join("bin")).unwrap();
    std::fs::write(home.join("bin").join(if cfg!(windows) { "docling.exe" } else { "docling" }), "fixture").unwrap();
    std::fs::write(home.join("ready"), "2.140.0").unwrap();
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    settings.convert_other_formats = true;
    settings.conversion_storage = sag_protocol::types::ConversionStorage::Library;
    settings.conversion_formats = Some(vec![]);
    settings.watch = false;
    call(&client, Operation::SaveSettings { mineru_key:String::new(), settings: settings.clone(), answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    let folder = dir.path().join("selected-cache");
    std::fs::create_dir(&folder).unwrap();
    let source = folder.join("cached.pages");
    std::fs::write(&source, "original").unwrap();
    let output = crate::docling::output_path(&folder, &source).unwrap();
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    std::fs::write(&output, "cachedselectedevidence.").unwrap();
    let ownership = std::collections::BTreeMap::from([("cached.pages", (crate::docling::fingerprint(&source).unwrap(), crate::docling::fingerprint(&output).unwrap()))]);
    std::fs::write(output.parent().unwrap().join(".sag-converted.json"), serde_json::to_vec(&ownership).unwrap()).unwrap();
    let Output::Text(root) = call(&client, Operation::AddRoot { path: folder.to_string_lossy().into_owned() }).await else { panic!("root") };
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(call(&client, Operation::ListRoots).await, Output::Roots(roots) if roots.iter().any(|item| item.path == root && item.status == "ready")) { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    assert!(matches!(call(&client, Operation::ListFiles).await, Output::Files(files) if files.is_empty()));
    settings.conversion_formats = Some(vec!["pages".into()]);
    call(&client, Operation::SaveSettings { mineru_key:String::new(), settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(call(&client, Operation::ListFiles).await, Output::Files(files) if files.len() == 1 && files[0].path.ends_with("cached.pages.md")) { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("changing the conversion selection must schedule a scan without a watcher event");
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn uninstall_docling_over_rpc_updates_status_and_default() {
    let (dir, client, server) = start().await;
    let home = dir.path().join("data/docling");
    std::fs::create_dir_all(home.join("bin")).unwrap();
    let binary = if cfg!(windows) { "docling.exe" } else { "docling" };
    std::fs::write(home.join("bin").join(binary), "fixture").unwrap();
    std::fs::write(home.join("ready"), "2.140.0").unwrap();
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    settings.convert_other_formats = true;
    call(&client, Operation::SaveSettings { mineru_key:String::new(), settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    let Output::Docling(before) = call(&client, Operation::DoclingStatus).await else { panic!("docling") };
    assert!(before.installed);
    let Output::Docling(after) = call(&client, Operation::UninstallDocling).await else { panic!("docling") };
    assert!(!after.installed);
    assert_eq!(after.stage, "uninstalled");
    assert!(!home.exists());
    let Output::Settings(settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert!(!settings.convert_other_formats);
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn running_service_accepts_a_desktop_component_source_without_restart() {
    let (dir, client, server) = start().await;
    let component = dir.path().join("desktop-resources/docling-component");
    std::fs::create_dir_all(&component).unwrap();
    // Deliberately invalid payload: installation must reach this registered
    // source, independently of the environment inherited at service startup.
    std::fs::write(component.join("manifest.json"), "{}").unwrap();
    assert!(matches!(call(&client, Operation::RegisterDoclingComponent {
        path: component.to_string_lossy().into_owned(),
    }).await, Output::Unit));
    call(&client, Operation::InstallDocling).await;
    let status = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let Output::Docling(status) = call(&client, Operation::DoclingStatus).await else { panic!("docling") };
            if !status.running { break status; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    let message = status.error.expect("invalid component must be rejected");
    assert!(!message.contains("未包含预构建文档组件"), "registered component was ignored: {message}");
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn save_note_writes_an_anchored_file_and_tracks_source_changes() {
    use sag_protocol::types::{Candidate, Message, NoteSourceState, SourceSpan};
    let (dir, client, server) = start().await;
    let root = dir.path().join("kb");
    std::fs::create_dir_all(root.join("notes")).unwrap();
    let passage = "fn check_permission(user: &User) -> bool {\n    user.roles.contains(&Role::Admin)\n}";
    let source = root.join("guard.md");
    std::fs::write(&source, format!("# guard\n\n{passage}\n")).unwrap();
    let root = root.canonicalize().unwrap();
    call(&client, Operation::AddRoot { path: root.to_string_lossy().into_owned() }).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Output::Roots(roots) = call(&client, Operation::ListRoots).await {
                if roots.iter().any(|r| r.status == "ready" && r.count == 1) { break; }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }).await.unwrap();
    let raw = std::fs::read(&source).unwrap();
    let body = String::from_utf8(raw.clone()).unwrap();
    let start = body.find(passage).unwrap();
    let candidate = Candidate {
        id: 1, document_id: 1, path: source.canonicalize().unwrap().to_string_lossy().into_owned(), source_path: None, title: "guard.md".into(), heading: "guard".into(),
        content: passage.into(), start_line: 3, end_line: 5, source_ranges: vec![(start, start + passage.len())],
        source_spans: vec![SourceSpan { start_byte: start, end_byte: start + passage.len(), start_line: 3, start_column: 1, end_line: 5, end_column: 1 }],
        bm25: 0., score: 1., previous_score: None, order: 0, hash: crate::store::hash(&raw),
    };
    let message = Message {
        help: false,
        created_at: Some(1_761_370_800_000), question: "权限在哪里校验？".into(), answer: "在 guard 里 [1]，再提一次 [1]。".into(),
        reasoning: String::new(), reasoning_blocks: vec![], evidence: vec![candidate], model: "test".into(),
        scope: vec![root.to_string_lossy().into_owned()], references: vec![], status: "complete".into(), error: None,
        retrieval_question: "权限在哪里校验？".into(), tool_calls: vec![], context: None, note_path: None,
    };
    let conversation = sag_protocol::types::Conversation { id: "c1".into(), title: "t".into(), messages: vec![message], updated: 0, deleted: false };
    // Seed the conversation directly; RunQuery would need a model.
    let Output::Info(_) = call(&client, Operation::Info).await else { panic!("info") };
    let data = dir.path().join("data");
    let store = crate::store::Store::open(&data.join("sag.sqlite")).unwrap();
    store.put_conversation(&conversation).unwrap();
    drop(store);

    let target = root.join("notes");
    let Output::Note(detail) = call(&client, Operation::SaveNote {
        conversation_id: "c1".into(), message_index: 0, target: Some(target.to_string_lossy().into_owned()), file_name: None, expected_hash: None, remember: true,
    }).await else { panic!("note") };
    assert_eq!(detail.summary.state, NoteSourceState::Intact);
    assert_eq!(detail.sources.len(), 1);
    let note_path = std::path::PathBuf::from(&detail.summary.path);
    assert_eq!(note_path.file_name().unwrap().to_string_lossy(), "权限在哪里校验.md");
    let written = std::fs::read_to_string(&note_path).unwrap();
    assert!(written.starts_with("---\nsag_note: 1\n"));
    assert!(written.contains("path: \"../guard.md\""));
    assert!(written.contains("在 guard 里 [1]，再提一次 [1]。"));
    let Output::Conversations(conversations) = call(&client, Operation::ListConversations).await else { panic!("conversations") };
    assert_eq!(conversations[0].messages[0].note_path.as_deref(), Some(detail.summary.path.as_str()));
    let Output::Settings(settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert_eq!(settings.notes.folder.as_deref(), Some(target.canonicalize().unwrap().to_str().unwrap()));

    // Edit the source: the watcher re-indexes it and the anchor becomes "changed".
    std::fs::write(&source, "# guard\n\nfn check_permission(user: &User, action: Action) -> bool {\n    user.roles.contains(&Role::Admin) || action.is_public()\n}\n").unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Output::Note(detail) = call(&client, Operation::GetNote { path: detail.summary.path.clone() }).await {
                if detail.sources[0].state == NoteSourceState::Changed { break; }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.unwrap();
    // VerifyNotes reads the disk even before the watcher catches up.
    std::fs::remove_file(&source).unwrap();
    let Output::Notes(summaries) = call(&client, Operation::VerifyNotes { paths: vec![detail.summary.path.clone()] }).await else { panic!("verify") };
    assert_eq!(summaries[0].state, NoteSourceState::Missing);
    let Output::Notes(all) = call(&client, Operation::ListNotes { scope: None }).await else { panic!("list") };
    assert_eq!(all.len(), 1);
    stop(&client, server).await;
}

#[tokio::test]
async fn local_skill_http_serves_bundled_files_without_affecting_rpc() {
    let (dir, client, server) = start().await;
    let endpoint: Endpoint = serde_json::from_slice(
        &std::fs::read(dir.path().join("data/sag-service.json")).unwrap(),
    ).unwrap();
    let base = format!("http://127.0.0.1:{}", endpoint.port);
    let http = reqwest::Client::new();
    for (path, expected) in [
        ("/sag-wiki.md", include_str!("../../cli/skills/sag/SKILL.md")),
        ("/SKILL.md", include_str!("../../cli/skills/sag/SKILL.md")),
        ("/references/setup.md", include_str!("../../cli/skills/sag/references/setup.md")),
        ("/references/parameters.md", include_str!("../../cli/skills/sag/references/parameters.md")),
        ("/references/ignores.md", include_str!("../../cli/skills/sag/references/ignores.md")),
    ] {
        let response = http.get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["content-type"], "text/markdown; charset=utf-8");
        assert_eq!(response.text().await.unwrap(), expected);
    }
    let head = http.head(format!("{base}/sag-wiki.md")).send().await.unwrap();
    assert_eq!(head.status(), 200);
    assert_eq!(head.headers()["content-length"], include_str!("../../cli/skills/sag/SKILL.md").len().to_string());
    assert!(head.bytes().await.unwrap().is_empty());
    for path in ["/credentials.json", "/references/other.md", "/sag-service.json"] {
        assert_eq!(http.get(format!("{base}{path}")).send().await.unwrap().status(), 404);
    }
    assert_eq!(http.post(format!("{base}/sag-wiki.md")).send().await.unwrap().status(), 405);
    assert!(matches!(call(&client, Operation::Info).await, Output::Info(_)));
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn import_invalid_offline_component_fails_locally_without_registering_a_source() {
    let (dir, client, server) = start().await;
    let component = dir.path().join("offline-component");
    std::fs::create_dir_all(&component).unwrap();
    std::fs::write(component.join("manifest.json"), serde_json::to_vec(&serde_json::json!({
        "schema": 0, "component_version": "", "profile": "", "platform": "", "python": "",
        "site_packages": "", "models": "", "files": []
    })).unwrap()).unwrap();
    let operation: Operation = serde_json::from_value(serde_json::json!({
        "operation": "import_docling_component", "args": { "path": component }
    })).unwrap();
    call(&client, operation).await;
    let status = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let Output::Docling(status) = call(&client, Operation::DoclingStatus).await else { panic!("docling") };
            if !status.running { break status; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    assert_eq!(status.requested_profiles, vec![sag_protocol::DoclingProfile::Documents]);
    assert!(status.error.unwrap().contains("组件版本、文档能力或平台与当前应用不匹配"));
    assert!(!status.installed);
    assert!(!dir.path().join("data/docling/uv").exists());
    assert!(!dir.path().join("data/docling/uv.exe").exists());
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mineru_settings_keep_provider_secrets_separate_and_validate_before_saving() {
    use sag_protocol::mineru::{MineruConfig, MineruProvider};
    use sag_protocol::types::DocumentParser;
    let (dir, client, server) = start().await;
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert!(!settings.mineru.enabled);
    settings.mineru.enabled = false;
    call(&client, Operation::SaveSettings { settings:settings.clone(), answer_key:String::new(),jev_key:String::new(),mineru_key:"302-test-secret".into(),mcp_credentials:None }).await;
    let Output::Settings(saved) = call(&client,Operation::GetSettings).await else {panic!("settings")};
    assert!(saved.mineru.has_key);
    assert!(saved.mineru.enabled);
    assert!(!serde_json::to_string(&saved).unwrap().contains("302-test-secret"));
    settings.mineru.base_url = "https://mineru.example.com/proxy".into();
    call(&client, Operation::SaveSettings { settings:settings.clone(), answer_key:String::new(),jev_key:String::new(),mineru_key:String::new(),mcp_credentials:None }).await;
    let Output::Settings(saved) = call(&client,Operation::GetSettings).await else {panic!("settings")};
    assert_eq!(saved.mineru.base_url(), "https://mineru.example.com/proxy");
    assert!(saved.mineru.has_key && saved.mineru.enabled);
    assert_ne!(saved.mineru.content_identity(), MineruConfig::default().content_identity());
    settings.document_parser = Some(DocumentParser::Docling);
    call(&client, Operation::SaveSettings { settings:settings.clone(), answer_key:String::new(),jev_key:String::new(),mineru_key:String::new(),mcp_credentials:None }).await;
    let Output::Settings(saved) = call(&client,Operation::GetSettings).await else {panic!("settings")};
    assert_eq!(saved.document_parser, Some(DocumentParser::Docling));
    assert!(saved.mineru.has_key && !saved.mineru.enabled);
    // An older client omits the preference; saving a key must not change the selected parser.
    settings.document_parser = None;
    call(&client, Operation::SaveSettings { settings:settings.clone(), answer_key:String::new(),jev_key:String::new(),mineru_key:"302-test-secret".into(),mcp_credentials:None }).await;
    let Output::Settings(saved) = call(&client,Operation::GetSettings).await else {panic!("settings")};
    assert_eq!(saved.document_parser, Some(DocumentParser::Docling));
    assert!(saved.mineru.has_key && !saved.mineru.enabled);
    settings.document_parser = Some(DocumentParser::Mineru);
    call(&client, Operation::SaveSettings { settings:settings.clone(), answer_key:String::new(),jev_key:String::new(),mineru_key:String::new(),mcp_credentials:None }).await;
    let Output::Settings(saved) = call(&client,Operation::GetSettings).await else {panic!("settings")};
    assert!(saved.mineru.has_key && saved.mineru.enabled);
    settings.mineru.base_url.clear();
    settings.mineru.provider=MineruProvider::Official;
    let result=client.call(Operation::SaveSettings {settings:settings.clone(),answer_key:String::new(),jev_key:String::new(),mineru_key:String::new(),mcp_credentials:None},Duration::from_secs(5)).await;
    assert!(result.is_ok());
    let Output::Settings(saved) = call(&client,Operation::GetSettings).await else {panic!("settings")};
    assert_eq!(saved.document_parser, Some(DocumentParser::Mineru));
    assert!(!saved.mineru.enabled);
    call(&client, Operation::SaveSettings { settings:settings.clone(), answer_key:String::new(),jev_key:String::new(),mineru_key:"official-test-secret".into(),mcp_credentials:None }).await;
    let data=dir.path().join("data");
    assert_eq!(crate::mineru::key(&data,&MineruConfig::default()).unwrap(),"302-test-secret");
    assert_eq!(crate::mineru::key(&data,&settings.mineru).unwrap(),"official-test-secret");
    call(&client,Operation::ClearMineruKey {config:settings.mineru.clone()}).await;
    assert!(!crate::mineru::has_key(&data,&settings.mineru).unwrap());
    assert!(crate::mineru::has_key(&data,&MineruConfig::default()).unwrap());
    stop(&client,server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chat_import_uses_remembered_folder_through_service() {
    let (dir, client, server) = start().await;
    let transcripts = dir.path().join("transcripts");
    std::fs::create_dir(&transcripts).unwrap();
    let source = transcripts.join("conversation.jsonl");
    std::fs::write(&source, [
        serde_json::json!({"type":"user","sessionId":"service-chat","uuid":"u","timestamp":"2026-11-15T01:00:00Z","cwd":"/demo/project","message":{"role":"user","content":"chatimportmarker question"}}),
        serde_json::json!({"type":"assistant","sessionId":"service-chat","uuid":"a","timestamp":"2026-11-15T01:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":"Answer from imported chat"},{"type":"tool_use","name":"Read","input":{"path":"hidden-tool-marker"}}]}}),
    ].into_iter().map(|v|format!("{v}\n")).collect::<String>()).unwrap();
    let Output::ChatCatalog(catalog) = call(&client, Operation::ListChatSessions { source:ChatSource::Claude, source_root:Some(transcripts.display().to_string()), query:Default::default() }).await else {panic!("catalog response")};
    assert_eq!(catalog.sessions.len(),1);
    let Output::ChatPreview(preview) = call(&client, Operation::PreviewChatImport { source:ChatSource::Claude, source_root:catalog.source_root.clone(), paths:catalog.sessions[0].paths.clone(), include_tools:false }).await else {panic!("preview response")};
    assert!(!preview.body.contains("hidden-tool-marker"));
    let folder = dir.path().join("Custom Chats");
    std::fs::create_dir(&folder).unwrap();
    let Output::Text(saved) = call(&client, Operation::SetChatImportFolder { path: folder.display().to_string() }).await else { panic!("saved folder") };
    let Output::Text(remembered) = call(&client, Operation::GetChatImportFolder).await else { panic!("remembered folder") };
    assert_eq!(saved, remembered);
    let Output::ChatImports(imported) = call(&client, Operation::ImportChatSessions {source:ChatSource::Claude, source_root:catalog.source_root.clone(), sessions:catalog.sessions.clone(), include_tools:false}).await else {panic!("import response")};
    assert_eq!(imported[0].status, "imported");
    assert_eq!(Path::new(&imported[0].path).parent().unwrap(), Path::new(&saved).join("claude code"));
    assert_eq!(std::fs::read_to_string(&imported[0].path).unwrap(), preview.body);
    let Output::ChatImports(repeated) = call(&client, Operation::ImportChatSessions {source:ChatSource::Claude, source_root:catalog.source_root, sessions:catalog.sessions, include_tools:true}).await else {panic!("repeat response")};
    assert_eq!(repeated[0].path, imported[0].path);
    assert_eq!(repeated[0].status, "updated");
    call(&client,Operation::Shutdown).await;
    server.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn proxy_password_is_stored_with_the_model_keys_and_never_returned() {
    use crate::types::{ModelProxy, ProxyMode};
    let (dir, client, server) = start().await;
    let data = dir.path().join("data");
    let save = |settings: Settings| client.call(Operation::SaveSettings {
        settings, answer_key: String::new(), jev_key: String::new(), mineru_key: String::new(), mcp_credentials: None,
    }, Duration::from_secs(5));
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert_eq!(settings.model_proxy, ModelProxy::default());
    settings.model_proxy = ModelProxy {
        mode: ProxyMode::Custom,
        url: "127.0.0.1:7890".into(),
        username: " alice ".into(),
        bypass: ".corp.example".into(),
        has_password: false,
        password: Some("secret".into()),
        stored_password: None,
    };
    save(settings.clone()).await.unwrap();
    assert_eq!(crate::models::proxy_password(&data).unwrap().as_deref(), Some("secret"));
    let Output::Settings(saved) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert_eq!(saved.model_proxy.username, "alice");
    assert!(saved.model_proxy.has_password);
    assert_eq!(saved.model_proxy.password, None, "the password never crosses the protocol");
    let preferences: String = rusqlite::Connection::open(data.join("sag.sqlite")).unwrap()
        .query_row("SELECT value FROM preferences WHERE id=1", [], |row| row.get(0)).unwrap();
    assert!(!preferences.contains("secret"), "the preferences row must not hold the password");

    // Leaving the password blank keeps it; so does clearing only the username by mistake.
    save(saved.clone()).await.unwrap();
    assert_eq!(crate::models::proxy_password(&data).unwrap().as_deref(), Some("secret"));
    let mut slipped = saved.clone();
    slipped.model_proxy.username.clear();
    save(slipped).await.unwrap();
    assert_eq!(crate::models::proxy_password(&data).unwrap().as_deref(), Some("secret"));
    // A password typed without a username is refused instead of being thrown away.
    let mut nameless = saved.clone();
    nameless.model_proxy.username.clear();
    nameless.model_proxy.password = Some("other".into());
    let Err(error) = save(nameless).await else { panic!("a password without a username must be refused") };
    assert!(error.to_string().contains("请填写代理用户名"), "{error}");
    // Turning authentication off sends an empty password and removes the stored one.
    let mut anonymous = saved.clone();
    anonymous.model_proxy.username.clear();
    anonymous.model_proxy.password = Some(String::new());
    save(anonymous).await.unwrap();
    assert_eq!(crate::models::proxy_password(&data).unwrap(), None);
    let Output::Settings(saved) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert!(!saved.model_proxy.has_password);

    // Invalid addresses are rejected before anything is written; direct mode ignores the address.
    let mut invalid = saved.clone();
    invalid.model_proxy.url = "ftp://proxy".into();
    assert!(save(invalid).await.is_err());
    let mut direct = saved.clone();
    direct.model_proxy.mode = ProxyMode::None;
    direct.answer.url = "http://127.0.0.1:9/v1".into();
    save(direct.clone()).await.unwrap();
    let Err(error) = client.call(Operation::TestProxyConnection {
        proxy: direct.model_proxy,
    }, Duration::from_secs(20)).await else { panic!("a closed port must fail the probe") };
    assert!(error.to_string().contains("无法连接 127.0.0.1"), "{error}");
    stop(&client, server).await;
}
