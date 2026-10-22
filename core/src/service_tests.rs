use sag_client::Client;
use sag_protocol::{types::*, *};
use std::{
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
    let result = client.call(Operation::SaveSettings {
        settings: settings.clone(), answer_key: String::new(), jev_key: String::new(), mcp_credentials: None,
    }, Duration::from_secs(5)).await;
    assert!(result.is_err(), "Jev-only auth mode was accepted for the answer model");
    let result = client.call(Operation::TestModel {
        kind: "answer".into(), config: settings.answer, key: String::new(),
    }, Duration::from_secs(5)).await;
    assert!(result.err().unwrap().to_string().contains("问答模型"));
    let Output::Settings(saved) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert_eq!(saved.answer.url, previous_url);
    assert_eq!(crate::models::secret(&dir.path().join("data"), "answer").unwrap(), "previous-answer-key");
    stop(&client, server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invalid_local_jev_settings_do_not_remove_the_saved_key() {
    let (dir, client, server) = start().await;
    crate::models::save_secret(&dir.path().join("data"), "jev", "previous-jev-key").unwrap();
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    settings.jev.auth_mode = ModelAuthMode::None;
    settings.retrieval.candidate_k = 0;
    let result = client.call(Operation::SaveSettings {
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
                } else {
                    serde_json::json!({"content":"完成"})
                };
                let data = format!(
                    "data: {}\n\ndata: [DONE]\n\n",
                    serde_json::json!({"choices":[{"delta":delta}]})
                );
                let response=format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",data.len(),data);
                let _ = stream.write_all(response.as_bytes()).await;
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
        Operation::SaveSettings {
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
    assert_eq!(summary.text, "完成");
    assert_eq!(summary.evidence.len(), 6);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let ServiceEvent::QueryProgress(progress) = events.recv().await.unwrap().event {
                if progress.id == "summary" {
                    assert_eq!(progress.delta.as_deref(), Some("完成")); break;
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
    settings.search_summary_enabled = false;
    call(&client, Operation::SaveSettings { settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
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
    assert_eq!(settings.conversion_workers, 1);
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
    tokio::time::sleep(Duration::from_millis(400)).await;
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
    call(&client, Operation::SaveSettings { settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    let folder = dir.path().join("project");
    std::fs::create_dir_all(folder.join("papers")).unwrap();
    std::fs::create_dir_all(folder.join("sibling")).unwrap();
    std::fs::write(folder.join("papers/note.md"), "paper evidence").unwrap();
    std::fs::write(folder.join("papers/main.py"), "print('code')").unwrap();
    std::fs::write(folder.join("sibling/note.md"), "sibling evidence").unwrap();
    let initial = FolderIndexOptions { extensions: None, file_types: FileTypes { markdown: true, text: false, code: false, config: false }, convert_other_formats: false, conversion_formats: None };
    let Output::Text(root) = call(&client, Operation::AddRootWithOptions { path: folder.to_string_lossy().into_owned(), options: initial.clone() }).await else { panic!("root") };
    let child = format!("{root}/papers");
    let changed = FolderIndexOptions { file_types: FileTypes { markdown: false, code: true, ..initial.file_types.clone() }, ..initial.clone() };
    for (expected, name) in [(initial, "note.md"), (changed.clone(), "main.py")] {
        if name == "main.py" {
            std::fs::write(folder.join("sibling/note.md"), [0xff, 0xfe, 0x00]).unwrap();
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
        assert!(files.iter().any(|file| file.path == format!("{child}/{name}")));
        assert!(files.iter().any(|file| file.path.ends_with("sibling/note.md")));
    }
    assert!(folder.join("papers/note.md").exists());
    assert!(folder.join("papers/main.py").exists());
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
    call(&client, Operation::SaveSettings { settings: settings.clone(), answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    settings.conversion_formats = Some(vec!["mp3".into()]);
    let rejected = client.call(Operation::SaveSettings { settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }, Duration::from_secs(5)).await;
    assert!(rejected.is_err(), "a document runtime must not authorize a new media selection");
    let message = rejected.err().unwrap().to_string();
    assert!(message.contains("音视频") || message.contains("媒体") || message.contains("media"), "{message}");
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert_eq!(settings.conversion_formats, Some(vec!["pdf".into()]));
    // An unavailable component must not prevent saving unrelated model choices.
    std::fs::remove_dir_all(&home).unwrap();
    settings.answer.reasoning_effort = Some(ReasoningEffort::High);
    call(&client, Operation::SaveSettings { settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    let Output::Settings(mut settings) = call(&client, Operation::GetSettings).await else { panic!("settings") };
    assert_eq!(settings.answer.reasoning_effort, Some(ReasoningEffort::High));
    settings.convert_other_formats = false;
    settings.conversion_formats = Some(vec!["mp3".into()]);
    call(&client, Operation::SaveSettings { settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
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
    call(&client, Operation::SaveSettings { settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
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
    settings.conversion_formats = Some(vec![]);
    settings.watch = false;
    call(&client, Operation::SaveSettings { settings: settings.clone(), answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    let folder = dir.path().join("selected-cache");
    std::fs::create_dir(&folder).unwrap();
    let source = folder.join("cached.pdf");
    std::fs::write(&source, "original").unwrap();
    let output = crate::docling::output_path(&folder, &source).unwrap();
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    std::fs::write(&output, "cachedselectedevidence.").unwrap();
    let ownership = std::collections::BTreeMap::from([("cached.pdf", (crate::docling::fingerprint(&source).unwrap(), crate::docling::fingerprint(&output).unwrap()))]);
    std::fs::write(output.parent().unwrap().join(".sag-converted.json"), serde_json::to_vec(&ownership).unwrap()).unwrap();
    let Output::Text(root) = call(&client, Operation::AddRoot { path: folder.to_string_lossy().into_owned() }).await else { panic!("root") };
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(call(&client, Operation::ListRoots).await, Output::Roots(roots) if roots.iter().any(|item| item.path == root && item.status == "ready")) { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    assert!(matches!(call(&client, Operation::ListFiles).await, Output::Files(files) if files.is_empty()));
    settings.conversion_formats = Some(vec!["pdf".into()]);
    call(&client, Operation::SaveSettings { settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(call(&client, Operation::ListFiles).await, Output::Files(files) if files.len() == 1 && files[0].path.ends_with("cached.pdf.md")) { break; }
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
    call(&client, Operation::SaveSettings { settings, answer_key: String::new(), jev_key: String::new(), mcp_credentials: None }).await;
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
