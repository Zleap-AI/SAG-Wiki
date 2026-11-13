use super::*;

// A complete Responses endpoint opts in without changing existing provider settings.
pub(super) fn enabled(config: &ModelConfig) -> bool {
    reqwest::Url::parse(config.url.trim())
        .is_ok_and(|url| url.path().trim_end_matches('/').ends_with("/responses"))
}

fn request_payload(payload: &Value) -> Value {
    let mut input = vec![];
    for message in payload["messages"].as_array().unwrap() {
        if let Some(output) = message["responses_output"].as_array() {
            // Keep encrypted reasoning and function calls in their original order.
            input.extend(output.iter().cloned());
        } else if message["role"] == "tool" {
            input.push(json!({"type":"function_call_output","call_id":message["tool_call_id"],"output":message["content"]}));
        } else {
            if !message["content"].is_null() {
                input.push(json!({"role":message["role"],"content":message["content"]}));
            }
            if let Some(calls) = message["tool_calls"].as_array() {
                for call in calls {
                    input.push(json!({"type":"function_call","call_id":call["id"],"name":call["function"]["name"],"arguments":call["function"]["arguments"]}));
                }
            }
        }
    }
    let mut request = json!({
        "model":payload["model"], "input":input, "stream":true, "store":false,
        "include":["reasoning.encrypted_content"], "reasoning":{"summary":"auto"}
    });
    if let Some(effort) = payload.get("reasoning_effort") {
        request["reasoning"]["effort"] = effort.clone();
    }
    if let Some(tools) = payload["tools"].as_array() {
        request["tools"] = json!(tools
            .iter()
            .map(|tool| {
                let mut definition = tool["function"].clone();
                definition["type"] = json!("function");
                // Existing tool schemas use optional fields; do not let Responses normalize them to required fields.
                if definition.get("strict").is_none() {
                    definition["strict"] = json!(false);
                }
                definition
            })
            .collect::<Vec<_>>());
    }
    if let Some(limit) = payload.get("max_tokens") {
        request["max_output_tokens"] = limit.clone();
    }
    if let Some(format) = payload.get("response_format") {
        request["text"] = json!({"format":format});
    }
    for field in ["tool_choice", "parallel_tool_calls", "temperature", "top_p"] {
        if let Some(value) = payload.get(field) {
            request[field] = value.clone();
        }
    }
    request
}

pub(super) async fn turn(
    client: &Client,
    config: &ModelConfig,
    key: &str,
    payload: &Value,
    timeout: u64,
    runtime: &RuntimeSettings,
    cancel: &CancellationToken,
    mut emit: impl FnMut(ChatDelta<'_>),
    require_complete: bool,
) -> Result<ChatTurn> {
    let response = request_with_runtime(
        client,
        &endpoint(config, false),
        key,
        &request_payload(payload),
        timeout,
        runtime,
        cancel,
    )
    .await?;
    let mut chunks = response.bytes_stream();
    let mut pending = vec![];
    let mut data = String::new();
    let mut text = String::new();
    loop {
        let chunk = tokio::select! { _ = cancel.cancelled() => return Err(cancelled()), next = chunks.next() => next };
        let Some(chunk) = chunk else {
            if require_complete {
                return Err(IncompleteSummary.into());
            }
            bail!("Responses 流在完成事件前断开，已保留当前内容")
        };
        pending.extend_from_slice(&chunk?);
        while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
            let line = String::from_utf8(pending.drain(..=end).collect())
                .context("Responses 流不是有效的 UTF-8")?;
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(part) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(part.strip_prefix(' ').unwrap_or(part));
                continue;
            }
            if !line.is_empty() || data.is_empty() {
                continue;
            }
            let event: Value =
                serde_json::from_str(&data).context("Responses 流式事件格式不正确")?;
            data.clear();
            match event["type"].as_str() {
                Some("response.output_text.delta" | "response.refusal.delta") => {
                    let delta = event["delta"]
                        .as_str()
                        .context("Responses 文本事件缺少 delta")?;
                    text.push_str(delta);
                    emit(ChatDelta::Content(delta));
                }
                Some("response.reasoning_summary_text.delta") => {
                    let delta = event["delta"]
                        .as_str()
                        .context("Responses 推理事件缺少 delta")?;
                    emit(ChatDelta::Reasoning(delta));
                }
                Some("response.completed") => {
                    let response = &event["response"];
                    anyhow::ensure!(
                        response["status"] == "completed",
                        "Responses 未正常完成：{}",
                        response["status"]
                    );
                    let output = response["output"]
                        .as_array()
                        .context("Responses 完成事件缺少 output")?;
                    let mut calls = vec![];
                    for item in output.iter().filter(|item| item["type"] == "function_call") {
                        for field in ["call_id", "name", "arguments"] {
                            anyhow::ensure!(
                                item[field].as_str().is_some_and(|value| !value.is_empty()),
                                "Responses 工具调用缺少 {field}"
                            );
                        }
                        calls.push(json!({"id":item["call_id"],"type":"function","function":{"name":item["name"],"arguments":item["arguments"]}}));
                    }
                    anyhow::ensure!(
                        !text.is_empty() || !calls.is_empty(),
                        "模型没有返回回答内容"
                    );
                    return Ok(ChatTurn {
                        text,
                        tool_calls: calls,
                        response_output: output.clone(),
                        input_tokens: response["usage"]["input_tokens"]
                            .as_u64()
                            .map(|n| n as usize),
                        output_tokens: response["usage"]["output_tokens"]
                            .as_u64()
                            .map(|n| n as usize),
                    });
                }
                Some("response.failed" | "error") => {
                    let detail = if event["type"] == "error" {
                        &event
                    } else {
                        &event["response"]["error"]
                    };
                    return Err(anyhow::Error::new(RequestError {
                        status: 400,
                        detail: redact(&detail.to_string(), key),
                    })
                    .context("Responses 模型生成失败"));
                }
                Some("response.incomplete") => {
                    let details = &event["response"]["incomplete_details"];
                    if details["reason"] == "max_output_tokens" {
                        if require_complete {
                            return Err(SummaryOutputLimit.into());
                        }
                        bail!("模型服务因长度限制结束了输出（max_output_tokens），已保留当前内容；可重试或继续提问");
                    }
                    bail!("Responses 输出未完成，已保留当前内容：{}", details);
                }
                _ => {}
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    fn event(value: Value) -> String {
        format!(
            "event: {}\r\ndata: {value}\r\n\r\n",
            value["type"].as_str().unwrap()
        )
    }
    fn completed(output: Value) -> String {
        event(
            json!({"type":"response.completed","response":{"status":"completed","output":output,"usage":{"input_tokens":42,"output_tokens":9}}}),
        )
    }
    fn server(bodies: Vec<String>) -> (ModelConfig, std::thread::JoinHandle<Vec<Value>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = ModelConfig {
            url: format!(
                "http://{}/v1/responses?route=test",
                listener.local_addr().unwrap()
            ),
            model: "gpt-6-astra".into(),
            ..Default::default()
        };
        let handle = std::thread::spawn(move || {
            bodies.into_iter().map(|body| {
            let (mut socket, _) = listener.accept().unwrap();
            socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            let mut bytes=vec![];
            let mut part=[0;4096];
            let payload = loop {
                let count=socket.read(&mut part).unwrap();
                assert!(count>0);
                bytes.extend_from_slice(&part[..count]);
                if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n") {
                    let header=String::from_utf8_lossy(&bytes[..end]);
                    assert!(header.starts_with("POST /v1/responses?route=test HTTP/1.1"));
                    let length=header.lines().find_map(|line|line.to_lowercase().strip_prefix("content-length: ").and_then(|n|n.parse::<usize>().ok())).unwrap();
                    if bytes.len()>=end+4+length { break serde_json::from_slice(&bytes[end+4..end+4+length]).unwrap(); }
                }
            };
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
            // Split both JSON events and Chinese UTF-8 characters across writes.
            for chunk in body.as_bytes().chunks(2) { socket.write_all(chunk).unwrap(); }
            payload
        }).collect()
        });
        (config, handle)
    }

    #[test]
    fn only_complete_responses_urls_opt_in() {
        let mut config = ModelConfig::default();
        for (url, expected) in [
            ("https://example.com/v1", false),
            ("https://example.com/v1/chat/completions", false),
            ("https://example.com/v1/responses/?a=b", true),
        ] {
            config.url = url.into();
            assert_eq!(enabled(&config), expected);
        }
        assert_eq!(
            endpoint(&config, false),
            "https://example.com/v1/responses?a=b"
        );
        config.url = "https://example.com/v1".into();
        assert_eq!(
            endpoint(&config, false),
            "https://example.com/v1/chat/completions"
        );
        assert_eq!(endpoint(&config, true), "https://example.com/v1/systemone");
    }

    #[test]
    fn converts_tools_reasoning_and_json_format_without_chat_fields() {
        let config = ModelConfig {
            url: "https://example.com/v1/responses".into(),
            reasoning_effort: Some(crate::types::ReasoningEffort::Max),
            ..Default::default()
        };
        let mut body = payload(
            &config,
            vec![json!({"role":"user","content":"JSON please"})],
            true,
        )
        .unwrap();
        body["tools"] = json!([{"type":"function","function":{"name":"lookup","parameters":{"type":"object","properties":{}}}}]);
        body["tool_choice"] = json!("auto");
        body["parallel_tool_calls"] = json!(false);
        body["response_format"] = json!({"type":"json_object"});
        body["max_tokens"] = json!(2048);
        let request = request_payload(&body);
        assert_eq!(request["reasoning"]["effort"], "max");
        assert_eq!(request["tools"][0]["name"], "lookup");
        assert_eq!(request["tools"][0]["strict"], false);
        assert_eq!(request["text"]["format"]["type"], "json_object");
        assert_eq!(request["parallel_tool_calls"], false);
        assert_eq!(request["tool_choice"], "auto");
        assert_eq!(request["store"], false);
        assert_eq!(request["max_output_tokens"], 2048);
        for field in [
            "messages",
            "stream_options",
            "reasoning_effort",
            "response_format",
            "enable_thinking",
            "temperature",
        ] {
            assert!(request.get(field).is_none(), "unexpected {field}");
        }
    }

    #[tokio::test]
    async fn agent_replays_encrypted_reasoning_and_tool_results() {
        let reasoning =
            json!({"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"opaque-state"});
        let call = json!({"type":"function_call","id":"fc_1","call_id":"call_1","name":"sag2_search","arguments":"{\"query\":\"查找资料 find evidence\"}","status":"completed"});
        let first =
            event(json!({"type":"response.reasoning_summary_text.delta","delta":"先查资料"}))
                + &completed(json!([reasoning, call]));
        let second = event(json!({"type":"response.output_text.delta","delta":"找到了证据 [1]。"}))
            + &completed(
                json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"找到了证据 [1]。","annotations":[]}]}]),
            );
        let (config, handle) = server(vec![first, second]);
        let mut message:crate::types::Message=serde_json::from_value(json!({"question":"查询本地文件","answer":"","reasoning":"","evidence":[],"model":"gpt-6-astra","scope":["/allowed"],"references":[],"status":"running","error":null,"retrieval_question":""})).unwrap();
        let candidate = Candidate {
            previous_score: None,
            id: 1,
            document_id: 1,
            path: "/allowed/1.md".into(),
            title: "1.md".into(),
            heading: String::new(),
            content: "找到了证据".into(),
            start_line: 1,
            end_line: 1,
            source_ranges: vec![(1, 1)],
            source_spans: vec![],
            bm25: 0.,
            score: 1.,
            order: 0,
            hash: "test".into(),
        };
        crate::agent::answer(
            &Client::new(),
            &config,
            "test",
            &mut message,
            &[],
            16384,
            512,
            5,
            &CancellationToken::new(),
            crate::agent::AgentOptions::default(),
            |_| {
                let candidate = candidate.clone();
                async move {
                    Ok(crate::retrieval::SearchOutput {
                        results: vec![candidate],
                        trace: vec![],
                    })
                }
            },
            |_, _| async { Ok(true) },
            |_, _, _, _| {},
        )
        .await
        .unwrap();
        let requests = handle.join().unwrap();
        assert_eq!(message.tool_calls.len(), 1);
        assert_eq!(message.answer, "找到了证据 [1]。");
        assert_eq!(message.reasoning, "先查资料");
        let input = requests[1]["input"].as_array().unwrap();
        let pos = input
            .iter()
            .position(|item| item["type"] == "reasoning")
            .unwrap();
        assert_eq!(input[pos], reasoning);
        assert_eq!(input[pos + 1], call);
        assert_eq!(input[pos + 2]["type"], "function_call_output");
        assert_eq!(input[pos + 2]["call_id"], "call_1");
        assert!(input[pos + 2]["output"]
            .as_str()
            .unwrap()
            .contains("找到了证据"));
        let context = message.context.unwrap();
        assert_eq!(context.input_tokens, Some(84));
        assert_eq!(context.output_tokens, Some(18));
    }

    #[tokio::test]
    async fn responses_connection_test_uses_streaming_endpoint() {
        let (config, handle) = server(vec![
            event(json!({"type":"response.output_text.delta","delta":"OK"}))
                + &completed(json!([])),
        ]);
        assert_eq!(
            test_connection(&Client::new(), "answer", &config, "test", 5)
                .await
                .unwrap(),
            "连接成功"
        );
        let requests = handle.join().unwrap();
        assert_eq!(requests[0]["stream"], true);
        assert_eq!(
            requests[0]["include"],
            json!(["reasoning.encrypted_content"])
        );
    }

    #[tokio::test]
    async fn buffered_summaries_preserve_retry_and_length_classification() {
        for (body, limited) in [
            (
                event(json!({"type":"response.output_text.delta","delta":"partial"})),
                false,
            ),
            (
                event(
                    json!({"type":"response.incomplete","response":{"incomplete_details":{"reason":"max_output_tokens"}}}),
                ),
                true,
            ),
        ] {
            let (config, handle) = server(vec![body]);
            let payload = summary_payload(
                &config,
                vec![json!({"role":"user","content":"Summarize"})],
                1024,
            )
            .unwrap();
            let error = summary_turn(
                &Client::new(),
                &config,
                "test",
                &payload,
                5,
                &RuntimeSettings::default(),
                &CancellationToken::new(),
            )
            .await
            .err()
            .unwrap();
            assert_eq!(summary_output_limited(&error), limited);
            assert_eq!(retryable_summary_error(&error), !limited);
            handle.join().unwrap();
        }
    }

    #[tokio::test]
    async fn failed_incomplete_and_truncated_streams_are_errors() {
        for (body, expected) in [
            (
                event(
                    json!({"type":"response.failed","response":{"error":{"message":"test-secret rejected"}}}),
                ),
                "[已隐藏] rejected",
            ),
            (
                event(json!({"type":"error","message":"stream failed"})),
                "stream failed",
            ),
            (
                event(
                    json!({"type":"response.incomplete","response":{"incomplete_details":{"reason":"max_output_tokens"}}}),
                ),
                "max_output_tokens",
            ),
            (
                event(json!({"type":"response.output_text.delta","delta":"半截内容"})),
                "完成事件前断开",
            ),
        ] {
            let (config, handle) = server(vec![body]);
            let payload = payload(&config, vec![], true).unwrap();
            let error = chat_turn(
                &Client::new(),
                &config,
                "test-secret",
                &payload,
                5,
                &CancellationToken::new(),
                |_| {},
            )
            .await
            .err()
            .unwrap();
            assert!(format!("{error:#}").contains(expected), "{error:#}");
            assert!(!format!("{error:#}").contains("test-secret"));
            handle.join().unwrap();
        }
    }
}
