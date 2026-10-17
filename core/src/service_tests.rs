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
                let routing = body.get("tools").is_none();
                let delta = if routing {
                    arrived.fetch_add(1, Ordering::SeqCst);
                    while hold.load(Ordering::SeqCst) {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    serde_json::json!({"content":serde_json::json!({"requires_tool":required}).to_string()})
                } else if required
                    && !body["messages"]
                        .to_string()
                        .contains("external_tool_results")
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
