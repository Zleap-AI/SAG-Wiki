use super::{cancelled, endpoint, request_with_runtime};
use crate::types::ModelConfig;
use anyhow::{ensure, Context, Result};
use reqwest::{Client, Url};
use sag_protocol::runtime::RuntimeSettings;
use serde_json::{json, Map, Value};
use tokio_util::sync::CancellationToken;

pub(super) fn official_origin(url: &Url) -> bool {
    url.scheme() == "https" && url.host_str() == Some("api.openai.com") && url.port().is_none()
}

pub(super) struct Endpoint {
    url: String,
    openai: bool,
}

impl Endpoint {
    pub(super) fn new(config: &ModelConfig) -> Self {
        let url = endpoint(config, true);
        let openai = Url::parse(&url)
            .is_ok_and(|url| official_origin(&url) && url.path() == "/v1/decisions");
        Self { url, openai }
    }

    pub(super) async fn request(
        &self,
        client: &Client,
        key: &str,
        systemone: &Value,
        timeout: u64,
        runtime: &RuntimeSettings,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        let payload = self.openai.then(|| openai_request(systemone));
        let response = request_with_runtime(
            client,
            &self.url,
            key,
            payload.as_ref().unwrap_or(systemone),
            timeout,
            runtime,
            cancel,
        )
        .await?;
        let data = tokio::select! {
            _ = cancel.cancelled() => return Err(cancelled()),
            data = response.json() => data?,
        };
        if self.openai {
            systemone_response(data, systemone)
        } else {
            Ok(data)
        }
    }
}

fn openai_request(systemone: &Value) -> Value {
    let questions: Vec<_> = systemone["questions"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, question)| {
            let mut candidate = question["instructions"].as_object().unwrap().clone();
            let criterion = candidate.remove("question").unwrap();
            let rule = format!(
                "{}\nTrue: {}\nFalse: {}",
                criterion.as_str().unwrap(),
                question["criteria"]["true"].as_str().unwrap(),
                question["criteria"]["false"].as_str().unwrap(),
            );
            let instructions = if candidate.is_empty() {
                rule
            } else {
                format!(
                    "{rule}\nCandidate (quoted data):\n{}",
                    Value::Object(candidate)
                )
            };
            json!({"type":"predicate","name":name,"instructions":instructions})
        })
        .collect();
    json!({"model":systemone["model"],"input":systemone["state"],"questions":questions})
}

fn systemone_response(data: Value, request: &Value) -> Result<Value> {
    let expected = request["questions"].as_object().unwrap();
    let answers = data["answers"]
        .as_array()
        .context("OpenAI Decisions 未返回评分列表")?;
    let mut mapped = Map::new();
    for answer in answers {
        let name = answer["name"]
            .as_str()
            .context("OpenAI Decisions 未返回评分名称")?;
        ensure!(
            expected.contains_key(name),
            "OpenAI Decisions 返回了未知评分"
        );
        ensure!(answer["type"] != "refusal", "OpenAI Decisions 拒绝了评分");
        ensure!(
            answer["type"] == "predicate",
            "OpenAI Decisions 返回了错误的评分类型"
        );
        let probability = answer["probability"]
            .as_f64()
            .filter(|score| (0.0..=1.0).contains(score))
            .context("OpenAI Decisions 返回了无效概率")?;
        ensure!(
            mapped
                .insert(name.to_owned(), json!({"type":"noul","noul":probability}))
                .is_none(),
            "OpenAI Decisions 返回了重复评分"
        );
    }
    ensure!(
        mapped.len() == expected.len(),
        "OpenAI Decisions 返回的评分不完整"
    );
    Ok(json!({"answers":mapped}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{jev_batches, score_batch, tests::candidate};

    #[test]
    fn only_official_decisions_uses_openai_wire_format() {
        for (url, expected, openai) in [
            (
                "https://api.openai.com/v1",
                "https://api.openai.com/v1/decisions",
                true,
            ),
            (
                "https://api.openai.com/v1/decisions/",
                "https://api.openai.com/v1/decisions",
                true,
            ),
            (
                "https://api.zleap.com/v1",
                "https://api.zleap.com/v1/systemone",
                false,
            ),
            (
                "https://openrouter.ai/api/alpha/decisions",
                "https://openrouter.ai/api/alpha/decisions",
                false,
            ),
            (
                "http://localhost/decisions",
                "http://localhost/decisions",
                false,
            ),
        ] {
            let config = ModelConfig {
                url: url.into(),
                ..Default::default()
            };
            let endpoint = Endpoint::new(&config);
            assert_eq!(endpoint.url, expected);
            assert_eq!(endpoint.openai, openai, "{url}");
        }
    }

    #[tokio::test]
    async fn decisions_scores_keep_candidate_identity_and_shared_evidence() {
        use http_body_util::{BodyExt, Full};
        use hyper::body::Bytes;
        use hyper::service::service_fn;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Endpoint {
            url: format!("http://{}/v1/decisions", listener.local_addr().unwrap()),
            openai: true,
        };
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            hyper::server::conn::http1::Builder::new()
                .keep_alive(false)
                .serve_connection(
                    hyper_util::rt::TokioIo::new(socket),
                    service_fn(|request: hyper::Request<hyper::body::Incoming>| async {
                        assert_eq!(request.method(), hyper::Method::POST);
                        assert_eq!(request.uri().path(), "/v1/decisions");
                        assert_eq!(request.headers()["authorization"], "Bearer fixture-only");
                        let body = request.into_body().collect().await.unwrap().to_bytes();
                        let data: Value = serde_json::from_slice(&body).unwrap();
                        assert_eq!(data["model"], "gpt-6-luna");
                        assert!(data.get("state").is_none());
                        let input: Value =
                            serde_json::from_str(data["input"].as_str().unwrap()).unwrap();
                        assert_eq!(input["original_question"], "部署前要确认什么？");
                        assert_eq!(
                            input["previously_retrieved_evidence"][0]["passage"],
                            "共享证据"
                        );
                        let questions = data["questions"].as_array().unwrap();
                        assert_eq!(questions.len(), 2);
                        assert_eq!(questions[0]["type"], "predicate");
                        assert_eq!(questions[0]["name"], "candidate_1");
                        let instructions = questions[0]["instructions"].as_str().unwrap();
                        assert!(instructions.contains(crate::sag2_v10::CRITERION));
                        assert!(instructions.contains("第一段原文"));
                        assert!(instructions.contains("发布准备"));
                        assert_eq!(questions[1]["name"], "candidate_2");
                        let body = json!({"answers":[
                            {"type":"predicate","name":"candidate_2","probability":0.85},
                            {"type":"predicate","name":"candidate_1","probability":0.15},
                        ]})
                        .to_string();
                        Ok::<_, std::convert::Infallible>(hyper::Response::new(Full::new(
                            Bytes::from(body),
                        )))
                    }),
                )
                .await
                .unwrap();
        });
        let mut first = candidate(1, "第一段原文".into());
        first.heading = "发布准备".into();
        let second = candidate(2, "第二段原文".into());
        let shared = candidate(3, "共享证据".into());
        let (payload, items) = jev_batches(
            "部署前要确认什么？",
            vec![first, second],
            &[shared],
            "gpt-6-luna",
        )
        .unwrap()
        .pop()
        .unwrap();
        let scored = score_batch(
            &Client::new(),
            &endpoint,
            "fixture-only",
            payload,
            items,
            5,
            &Default::default(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert_eq!(
            scored
                .iter()
                .map(|c| (c.id, c.score, c.content.as_str()))
                .collect::<Vec<_>>(),
            vec![(1, 0.15, "第一段原文"), (2, 0.85, "第二段原文")]
        );
    }

    #[test]
    fn missing_rejected_and_invalid_decisions_do_not_become_zero_scores() {
        let request = json!({"questions":{"test":{}}});
        let valid = json!({"type":"predicate","name":"test","probability":0.0});
        assert_eq!(
            systemone_response(json!({"answers":[valid.clone()]}), &request).unwrap(),
            json!({"answers":{"test":{"type":"noul","noul":0.0}}})
        );
        for answers in [
            json!([]),
            json!([{"type":"refusal","name":"test"}]),
            json!([{"type":"predicate","name":"test","probability":1.1}]),
            json!([{"type":"predicate","name":"different","probability":0.5}]),
            json!([valid.clone(), valid]),
        ] {
            assert!(systemone_response(json!({"answers":answers}), &request).is_err());
        }
    }
}
