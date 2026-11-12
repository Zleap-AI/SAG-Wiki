use super::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// Real HTTP/SSE: headers and partial content arrive before the delayed body.
struct Reply {
    status: u16,
    delay_ms: u64,
    text: &'static str,
    complete: bool,
    truncated: bool,
    finish_reason: &'static str,
}
fn ok(text: &'static str) -> Reply {
    Reply {
        status: 200,
        delay_ms: 0,
        text,
        complete: true,
        truncated: false,
        finish_reason: "stop",
    }
}
async fn server(replies: Vec<Reply>) -> (ModelConfig, Arc<Mutex<Vec<Value>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = ModelConfig {
        url: format!("http://{}", listener.local_addr().unwrap()),
        model: "test".into(),
        ..Default::default()
    };
    let requests = Arc::new(Mutex::new(vec![]));
    let captured = requests.clone();
    tokio::spawn(async move {
        for reply in replies {
            let Ok(Ok((mut socket, _))) =
                tokio::time::timeout(Duration::from_secs(8), listener.accept()).await
            else {
                return;
            };
            let mut bytes = vec![];
            let mut buf = [0; 4096];
            let request = loop {
                let n = socket.read(&mut buf).await.unwrap();
                if n == 0 {
                    return;
                }
                bytes.extend_from_slice(&buf[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let len = String::from_utf8_lossy(&bytes[..end])
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|n| n.parse::<usize>().ok())
                        })
                        .unwrap();
                    if bytes.len() >= end + 4 + len {
                        break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + len])
                            .unwrap();
                    }
                }
            };
            captured.lock().unwrap().push(request);
            tokio::spawn(async move {
                let partial = "data: {\"choices\":[{\"delta\":{\"content\":\"partial-\"}}]}\n\n";
                let rest = format!(
                    "data: {}\n\ndata: [DONE]\n\n",
                    json!({"choices":[{"delta":{"content":reply.text},"finish_reason":reply.finish_reason}]})
                );
                let length = if reply.truncated {
                    partial.len() + 1024
                } else if reply.complete {
                    partial.len() + rest.len()
                } else {
                    partial.len()
                };
                let header = format!("HTTP/1.1 {} Test\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", reply.status, length);
                let _ = socket.write_all(header.as_bytes()).await;
                let _ = socket.write_all(partial.as_bytes()).await;
                tokio::time::sleep(Duration::from_millis(reply.delay_ms)).await;
                if reply.complete {
                    let _ = socket.write_all(rest.as_bytes()).await;
                }
            });
        }
    });
    (config, requests)
}
fn runtime(retries: usize) -> sag_protocol::runtime::RuntimeSettings {
    let mut r = sag_protocol::runtime::RuntimeSettings::default();
    r.retry_count = retries as _;
    r.retry_base_secs = 0;
    r.retry_max_secs = 0;
    r.retry_total_secs = 5;
    r
}
async fn run(
    config: &ModelConfig,
    messages: &[Value],
    runtime: &sag_protocol::runtime::RuntimeSettings,
    cancel: &CancellationToken,
) -> Result<(String, Option<usize>, Option<usize>)> {
    summarize(
        &reqwest::Client::new(),
        config,
        "test-only",
        messages,
        32000,
        2048,
        1,
        runtime,
        cancel,
        |_| {},
    )
    .await
}
fn input(request: &Value) -> Value {
    serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn summary_recovers_body_timeout_without_reusing_partial_output() {
    let (config, requests) = server(vec![
        Reply {
            delay_ms: 1200,
            ..ok("late")
        },
        ok("recovered"),
    ])
    .await;
    let result = run(
        &config,
        &[json!({"role":"user","content":"任务与证据"})],
        &runtime(1),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result.0, "partial-recovered");
    assert_eq!(requests.lock().unwrap().len(), 2);
}
#[tokio::test]
async fn summary_retries_incomplete_sse_instead_of_accepting_partial_summary() {
    let (config, requests) = server(vec![
        Reply {
            complete: false,
            ..ok("unused")
        },
        ok("complete"),
    ])
    .await;
    let result = run(
        &config,
        &[json!({"role":"user","content":"task"})],
        &runtime(1),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result.0, "partial-complete");
    assert_eq!(requests.lock().unwrap().len(), 2);
}
#[tokio::test]
async fn summary_bounds_work_and_resumes_the_failed_chunk() {
    let (config, requests) = server(vec![
        ok("checkpoint"),
        Reply {
            delay_ms: 1200,
            ..ok("late")
        },
        Reply {
            delay_ms: 1200,
            ..ok("late")
        },
        ok("recovered"),
        ok("next"),
        ok("end"),
        ok("end"),
    ])
    .await;
    let messages = vec![json!({"role":"user","content":"保留全部证据和Unicode🦀。".repeat(1000)})];
    run(&config, &messages, &runtime(2), &CancellationToken::new())
        .await
        .unwrap();
    let requests = requests.lock().unwrap();
    assert!(requests.len() >= 4);
    for request in requests.iter() {
        assert!(tokens(request["messages"].as_array().unwrap(), &[]) <= 8192);
    }
    assert_eq!(
        input(&requests[1])["previous_summary"],
        "partial-checkpoint"
    );
    assert_eq!(
        input(&requests[3])["previous_summary"],
        "partial-checkpoint"
    );
    let failed = input(&requests[1])["transcript_part"]
        .as_str()
        .unwrap()
        .to_owned();
    let retry = input(&requests[3])["transcript_part"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(failed.starts_with(&retry) && retry.len() < failed.len());
    let restored = requests
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 1 && *i != 2)
        .map(|(_, r)| input(r)["transcript_part"].as_str().unwrap().to_owned())
        .collect::<String>();
    assert_eq!(restored, messages[0].to_string());
}
#[tokio::test]
async fn summary_retry_limit_and_cancellation_are_respected() {
    let (config, requests) = server(vec![
        Reply {
            delay_ms: 1200,
            ..ok("late")
        },
        Reply {
            delay_ms: 1200,
            ..ok("late")
        },
    ])
    .await;
    let error = run(
        &config,
        &[json!({"content":"task"})],
        &runtime(1),
        &CancellationToken::new(),
    )
    .await
    .err()
    .unwrap();
    assert!(format!("{error:#}").contains("timed out"));
    assert_eq!(requests.lock().unwrap().len(), 2);
    let (config, requests) = server(vec![Reply {
        delay_ms: 1200,
        ..ok("late")
    }])
    .await;
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        token.cancel();
    });
    let error = run(&config, &[json!({"content":"task"})], &runtime(3), &cancel)
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("已停止"));
    assert_eq!(requests.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn summary_does_not_retry_authentication_errors() {
    let (config, requests) = server(vec![Reply {
        status: 401,
        ..ok("unauthorized")
    }])
    .await;
    assert!(run(
        &config,
        &[json!({"content":"task"})],
        &runtime(3),
        &CancellationToken::new()
    )
    .await
    .is_err());
    assert_eq!(requests.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn summary_total_deadline_bounds_body_retries() {
    let (config, requests) = server(vec![
        Reply {
            delay_ms: 1200,
            ..ok("late")
        },
        Reply {
            delay_ms: 1200,
            ..ok("late")
        },
    ])
    .await;
    let mut r = runtime(10);
    r.retry_total_secs = 1;
    let started = std::time::Instant::now();
    assert!(run(
        &config,
        &[json!({"content":"task"})],
        &r,
        &CancellationToken::new()
    )
    .await
    .is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(requests.lock().unwrap().len(), 1);
}

#[test]
fn summary_uses_supported_output_and_reasoning_parameters_only() {
    let config = ModelConfig {
        url: "https://api.deepseek.com".into(),
        model: "deepseek-v4-pro".into(),
        reasoning_effort: Some(crate::types::ReasoningEffort::Max),
        ..Default::default()
    };
    let payload = models::summary_payload(&config, vec![], 512).unwrap();
    assert_eq!(payload["reasoning_effort"], "none");
    assert_eq!(payload["max_tokens"], 512);
    assert_eq!(
        config.reasoning_effort,
        Some(crate::types::ReasoningEffort::Max)
    );
    let unknown = ModelConfig {
        url: "http://127.0.0.1".into(),
        model: "custom".into(),
        ..Default::default()
    };
    let payload = models::summary_payload(&unknown, vec![], 512).unwrap();
    assert!(payload.get("reasoning_effort").is_none());
    assert!(payload.get("max_tokens").is_none());
}

#[tokio::test]
async fn summary_timeout_setting_still_controls_body_reads() {
    for timeout in [1, 3] {
        let (config, requests) = server(vec![Reply {
            delay_ms: 1200,
            ..ok("complete")
        }])
        .await;
        let result = summarize(
            &reqwest::Client::new(),
            &config,
            "test-only",
            &[json!({"role":"user","content":"same task"})],
            32000,
            2048,
            timeout,
            &runtime(0),
            &CancellationToken::new(),
            |_| {},
        )
        .await;
        if timeout == 1 {
            assert!(format!("{:#}", result.err().unwrap()).contains("timed out"));
        } else {
            assert_eq!(result.unwrap().0, "partial-complete");
        }
        assert_eq!(requests.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn summary_recovery_completes_the_answer_and_preserves_history() {
    let mut replies = vec![Reply {
        delay_ms: 1200,
        ..ok("late")
    }];
    replies.extend((0..20).map(|_| ok("完整摘要或最终回答")));
    let (config, requests) = server(replies).await;
    let previous: crate::types::Message = serde_json::from_value(json!({
        "question":"用户原始要求", "answer":"需要保留的事实。".repeat(1000), "evidence":[],
        "model":"test", "scope":[], "references":[], "status":"done", "error":null,
        "retrieval_question":""
    }))
    .unwrap();
    let mut current = previous.clone();
    current.answer.clear();
    current.question = "继续".into();
    let original = previous.answer.clone();
    let r = runtime(1);
    let mut progress = vec![];
    super::super::answer(
        &reqwest::Client::new(),
        &config,
        "test-only",
        &mut current,
        std::slice::from_ref(&previous),
        4096,
        512,
        1,
        &CancellationToken::new(),
        super::super::AgentOptions {
            runtime: Some(&r),
            ..Default::default()
        },
        |_| async { panic!("summary recovery must not repeat tools") },
        |_, _| async { panic!("no external action") },
        |status, _, _, _| progress.push(status.to_owned()),
    )
    .await
    .unwrap();
    assert_eq!(current.answer, "partial-完整摘要或最终回答");
    assert!(current.context.as_ref().unwrap().summary.is_some());
    assert_eq!(previous.answer, original);
    assert!(progress.iter().any(|s| s.contains("正在重试上下文压缩")));
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests[0], requests[1],
        "first retry repeats only the failed summary"
    );
    assert!(requests.last().unwrap().get("tools").is_some());
}

#[tokio::test]
async fn summary_retries_premature_http_body_eof() {
    let (config, requests) = server(vec![
        Reply {
            complete: false,
            truncated: true,
            ..ok("unused")
        },
        ok("complete"),
    ])
    .await;
    let result = run(
        &config,
        &[json!({"role":"user","content":"task"})],
        &runtime(1),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result.0, "partial-complete");
    assert_eq!(requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn summary_recovers_output_limit_without_using_a_truncated_draft() {
    let (config, requests) = server(vec![
        Reply {
            finish_reason: "length",
            ..ok("truncated")
        },
        ok("complete"),
    ])
    .await;
    let result = run(
        &config,
        &[json!({"role":"user","content":"all facts"})],
        &runtime(1),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result.0, "partial-complete");
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        input(&requests[0]),
        input(&requests[1]),
        "retry must keep the full original input, not a truncated draft"
    );
    assert_ne!(requests[0]["messages"][0], requests[1]["messages"][0]);
}

#[tokio::test]
async fn summary_output_limit_recovery_is_bounded() {
    let (config, requests) = server(vec![
        Reply {
            finish_reason: "length",
            ..ok("truncated")
        },
        Reply {
            finish_reason: "length",
            ..ok("truncated")
        },
    ])
    .await;
    assert!(run(
        &config,
        &[json!({"role":"user","content":"facts"})],
        &runtime(10),
        &CancellationToken::new()
    )
    .await
    .is_err());
    assert_eq!(requests.lock().unwrap().len(), 2);
}

#[test]
fn summary_proactive_compaction_keeps_room_for_the_complete_question() {
    let question = json!({"role":"user","content":"question".repeat(1000)});
    let messages = vec![
        json!({"role":"system","content":"system"}),
        question.clone(),
    ];
    let used = tokens(&messages, &[]);
    let budget = used + used / 20;
    assert!(used > trigger_budget(budget));
    assert!(
        !should_compact(&messages, &question, &[], budget),
        "no older records to compress"
    );
    let mut messages = messages;
    messages.push(json!({"role":"assistant","content":"answer"}));
    assert!(
        !should_compact(&messages, &question, &[], budget),
        "do not squeeze out the question for headroom"
    );
    assert!(
        should_compact(&messages, &question, &[], used / 2),
        "actual overflow still requires compaction"
    );
}
