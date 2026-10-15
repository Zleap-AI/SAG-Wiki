//! Explicit, manual real-model verification. Original files are read-only.
#![allow(dead_code)]
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
    let root = PathBuf::from(std::env::var("SAG_TEST_OUTPUT")?);
    let fixture = root.join("files").canonicalize()?;
    let real = PathBuf::from("/Users/jomymac/Desktop/open-context/src/octx");
    let key = std::fs::read_to_string(root.join("api-key"))?;
    let db = store::Store::open(&root.join("index.sqlite"))?;
    db.invalidate_index()?;
    let mut settings = types::Settings::default();
    settings.answer_context_tokens = 4096;
    settings.answer_output_tokens = 512;
    db.save_settings(&settings)?;
    for folder in [&fixture, &real] {
        let name = db.add_root(folder)?;
        db.scan(&name, |_| {})?;
    }
    let refs = db.files()?.into_iter().map(|f| f.path).collect::<Vec<_>>();
    let mut span_count = 0;
    let mut max_length = 0;
    for c in db.reference_candidates(&refs, 10000)? {
        let source = std::fs::read_to_string(&c.path)?;
        ensure!(
            c.content
                == c.source_spans
                    .iter()
                    .map(|sp| &source[sp.start_byte..sp.end_byte])
                    .collect::<Vec<_>>()
                    .join("\n\n"),
            "citation mismatch"
        );
        let length = c.content.encode_utf16().count();
        ensure!(length <= 2200, "chunk too long");
        span_count += 1;
        max_length = max_length.max(length);
    }
    println!("verified {span_count} source chunks; maximum {max_length} UTF-16 characters");
    let client = reqwest::Client::new();
    let config = types::ModelConfig {
        url: "https://api.302.ai/v1".into(),
        ..Default::default()
    };
    let jev = types::ModelConfig {
        url: "https://api.302.ai/typesafeai/v1".into(),
        model: "jev-latest".into(),
        ..Default::default()
    };
    let cases = [
        (
            "long-paragraph",
            "星河验收方案的释放验证码是什么？",
            fixture.clone(),
            "LYRA-73492",
        ),
        (
            "long-code",
            "验收代码里的 launch_quota 返回什么 batch_limit 和 region？",
            fixture.clone(),
            "north-2",
        ),
        (
            "original-python",
            "logical_path 会拒绝反斜杠和 .. 路径段吗？抛出什么异常？",
            real.clone(),
            "OctxSecurityError",
        ),
    ];
    let mut reports = vec![];
    for (name, question, scope, expected) in cases {
        let started = Instant::now();
        let cancel = CancellationToken::new();
        println!("START {name}");
        let search = retrieval::retrieve(
            &db,
            question,
            &[scope.to_string_lossy().into()],
            &[],
            &settings.retrieval,
            &cancel,
            |c, e| {
                let client = &client;
                let jev = &jev;
                let key = &key;
                let cancel = &cancel;
                async move { models::score(client, jev, key, question, c, &e, 120, cancel).await }
            },
            |stage, count| println!(" {stage}: {count}"),
        )
        .await?;
        let plan = models::prepare_answer(&config, question, &search.results, &[], 4096, 512)?;
        ensure!(
            plan.context.estimated_input + 512 <= 4096,
            "estimated context exceeded"
        );
        let answer = models::answer(&client, &config, &key, &plan, 120, &cancel, |_| {}).await?;
        let actual = answer
            .input_tokens
            .zip(answer.output_tokens)
            .map(|(a, b)| a + b);
        ensure!(
            actual.is_some_and(|n| n <= 4096),
            "provider usage missing or exceeds context: {actual:?}"
        );
        ensure!(
            answer.text.contains(expected),
            "answer does not contain expected fact: {}",
            answer.text
        );
        println!(
            "PASS {name}: {}ms, input {:?} + output {:?}, {} sources, {} omitted",
            started.elapsed().as_millis(),
            answer.input_tokens,
            answer.output_tokens,
            plan.evidence.len(),
            plan.context.omitted_sources
        );
        reports.push(json!({"case":name,"question":question,"expected":expected,"answer":answer.text,"elapsed_ms":started.elapsed().as_millis(),"input_tokens":answer.input_tokens,"output_tokens":answer.output_tokens,"plan":plan.context,"evidence":plan.evidence,"trace":search.trace}));
        std::fs::write(
            root.join("results.json"),
            serde_json::to_vec_pretty(
                &json!({"verified_chunks":span_count,"max_chunk_characters":max_length,"cases":reports}),
            )?,
        )?;
    }
    Ok(())
}
