//! Explicit real-model tool calling verification; source files are read-only.
#![allow(dead_code)]
#[path = "../src/agent.rs"]
mod agent;
#[path = "../src/mcp.rs"]
mod mcp;
#[path = "../src/sag_mcp.rs"]
mod sag_mcp;
#[path = "../src/models.rs"]
mod models;
#[path = "../src/retrieval.rs"]
mod retrieval;
#[path = "../src/sag2_v10.rs"]
mod sag2_v10;
#[path = "../src/store.rs"]
mod store;
#[path = "../src/text.rs"]
mod text;
#[path = "../src/types.rs"]
mod types;
use anyhow::{ensure, Result};
use serde_json::json;
use std::{path::PathBuf, time::Instant};
use tokio_util::sync::CancellationToken;
#[tokio::main]
async fn main() -> Result<()> {
    let output = PathBuf::from(std::env::var("SAG_TEST_OUTPUT")?);
    std::fs::create_dir_all(&output)?;
    let credentials = PathBuf::from(std::env::var("SAG_TEST_CREDENTIALS")?);
    let answer_key = models::secret(&credentials, "answer")?;
    let jev_key = models::secret(&credentials, "jev")?;
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../.cache/long-validation/files")
        .canonicalize()?;
    let original = PathBuf::from("/Users/jomymac/Desktop/open-context/src/octx");
    let store = store::Store::open(&output.join("index.sqlite"))?;
    for path in [&fixtures, &original] {
        let root = store.add_root(path)?;
        store.scan(&root, |_| {})?;
    }
    let config = types::ModelConfig {
        url: "https://api.302.ai/v1".into(),
        ..Default::default()
    };
    let jev = types::ModelConfig {
        url: "https://api.302.ai/typesafeai/v1".into(),
        model: "jev-latest".into(),
        ..Default::default()
    };
    let client = reqwest::Client::new();
    let options = types::Retrieval::default();
    let mut reports = vec![];
    let mut history = vec![];
    for (name, question, scope, expected, should_search) in [
        (
            "greeting",
            "你好，用一句话打招呼即可。",
            fixtures.clone(),
            "",
            false,
        ),
        (
            "long-paragraph",
            "星河验收方案的释放验证码是什么？请查文件回答。",
            fixtures.clone(),
            "LYRA-73492",
            true,
        ),
        (
            "long-code",
            "验收代码里的 launch_quota 返回什么 batch_limit 和 region？",
            fixtures.clone(),
            "north-2",
            true,
        ),
        (
            "follow-up",
            "刚才那个函数的 batch_limit 是多少？请核对文件。",
            fixtures.clone(),
            "37",
            true,
        ),
        (
            "original-python",
            "logical_path 会拒绝反斜杠和 .. 路径段吗？抛出什么异常？",
            original.clone(),
            "OctxSecurityError",
            true,
        ),
    ] {
        if std::env::var("SAG_TEST_CASE").is_ok_and(|filter| filter != name) {
            continue;
        }
        let started = Instant::now();
        let cancel = CancellationToken::new();
        println!("START {name}");
        let mut message = types::Message {
            question: question.into(),
            answer: String::new(),
            reasoning: String::new(),
            evidence: vec![],
            model: config.model.clone(),
            scope: vec![scope.to_string_lossy().into()],
            references: vec![],
            status: "running".into(),
            error: None,
            retrieval_question: question.into(),
            tool_calls: vec![],
            context: None,
        };
        let active_history = if name == "follow-up" {
            history.as_slice()
        } else {
            &[]
        };
        let outcome = agent::answer(
            &client,
            &config,
            &answer_key,
            &mut message,
            active_history,
            4096,
            512,
            120,
            &cancel,
            agent::AgentOptions::default(),
            |query| {
                println!(" TOOL query: {query}");
                let scope = &scope;
                let client = &client;
                let options = &options;
                let jev = &jev;
                let key = &jev_key;
                let store = &store;
                let cancel = &cancel;
                async move {
                    retrieval::retrieve(
                        store,
                        &query,
                        &[scope.to_string_lossy().into()],
                        &[],
                        options,
                        cancel,
                        |c, e| {
                            let query = &query;
                            async move {
                                {
                                    for (payload, items) in
                                        models::jev_batches(query, c.clone(), &e, &jev.model)?
                                    {
                                        println!(
                                            " JEV batch: {} candidates, {} estimated tokens",
                                            items.len(),
                                            tiktoken_rs::o200k_base_singleton()
                                                .encode_ordinary(&payload.to_string())
                                                .len()
                                        );
                                    }
                                    models::score(client, jev, key, query, c, &e, 120, cancel).await
                                }
                            }
                        },
                        |_, _| {},
                    )
                    .await
                }
            },
            |_, _| async { Ok(true) },
            |stage, delta, count, _tool| {
                if delta.is_none() {
                    println!(" {stage}: {count}")
                }
            },
        )
        .await;
        if let Err(error) = &outcome {
            std::fs::write(
                output.join("failure.json"),
                serde_json::to_vec_pretty(
                    &json!({"case":name,"error":error.to_string(),"message":message}),
                )?,
            )?;
        }
        let trace = outcome?;
        ensure!(
            !message.tool_calls.is_empty() == should_search,
            "unexpected tool decision {name}"
        );
        ensure!(
            message.answer.contains(expected),
            "incorrect answer: {}",
            message.answer
        );
        ensure!(
            message
                .evidence
                .iter()
                .all(|c| std::path::Path::new(&c.path).starts_with(&scope)),
            "scope changed"
        );
        for c in &message.evidence {
            let raw = std::fs::read_to_string(&c.path)?;
            ensure!(
                c.content
                    == c.source_spans
                        .iter()
                        .map(|s| &raw[s.start_byte..s.end_byte])
                        .collect::<Vec<_>>()
                        .join("\n\n"),
                "source mismatch"
            )
        }
        let context = message.context.as_ref().unwrap();
        ensure!(
            context
                .input_tokens
                .zip(context.output_tokens)
                .is_some_and(|(i, o)| i + o <= 4096),
            "provider usage exceeded limit or missing"
        );
        if should_search {
            ensure!(!message.evidence.is_empty(), "no sources");
            ensure!(message.answer.contains("[1]"), "missing citation")
        }
        message.status = "complete".into();
        println!(
            "PASS {name}: {} tools, {} sources, {:?}+{:?} tokens",
            message.tool_calls.len(),
            message.evidence.len(),
            context.input_tokens,
            context.output_tokens
        );
        reports.push(json!({"case":name,"elapsed_ms":started.elapsed().as_millis(),"message":message,"retrieval_rounds":trace.len()}));
        if name == "long-code" {
            history = vec![message.clone()]
        }
        std::fs::write(
            output.join("results.json"),
            serde_json::to_vec_pretty(&reports)?,
        )?;
    }
    Ok(())
}
