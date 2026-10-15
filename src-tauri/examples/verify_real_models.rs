//! Manual integration check against real services. Run only with explicit user authorization.
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
use anyhow::Result;
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    let out = PathBuf::from(std::env::var("SAG_TEST_OUTPUT")?);
    let root = PathBuf::from(std::env::var("SAG_TEST_ROOT")?);
    let key = std::fs::read_to_string(std::env::var("SAG_TEST_KEY_FILE")?)?;
    let client = reqwest::Client::builder().build()?;
    let answer = types::ModelConfig {
        url: "https://api.302.ai/v1".into(),
        ..Default::default()
    };
    let jev = types::ModelConfig {
        url: "https://api.302.ai/typesafeai/v1".into(),
        model: "jev-latest".into(),
        ..Default::default()
    };
    let s = store::Store::open(&out.join("validation.sqlite"))?;
    let mut settings = types::Settings::default();
    settings
        .excluded
        .extend([".venv", ".pytest_cache", ".ruff_cache"].map(String::from));
    s.save_settings(&settings)?;
    let root_text = s.add_root(&root)?;
    let start = Instant::now();
    let count = s.scan(&root_text, |_| {})?;
    let index_ms = start.elapsed().as_millis();
    println!(
        "indexed {count} documents, {} chunks, {index_ms} ms",
        s.chunk_count()?
    );
    let cases=vec![
        ("search-id", "UUIDv4 转换成 UUIDv7 后，应该用什么函数恢复原来的标识？", Vec::<String>::new(), false, vec!["api/sag-align.md"]),
        ("ask-multifile", "一个外部 OCTX 包已经安装，但声明的结构层无效，需要在本地重新构建。把重建后的内容发布给别人时，能否继续使用原发布者的资产身份？应如何记录它的来源？",vec![],true,vec!["importing-octx.md","exporting-octx.md"]),
        ("search-scope", "CreateResult 包含哪些字段，输出文件路径从哪里取得？", vec![root.join("api").to_string_lossy().into_owned()],false,vec!["api/create-result.md"]),
        ("ask-unanswerable", "星澜计划 ZL-94817 的项目负责人、批准预算和正式上线日期分别是什么？只能依据文件回答。",vec![],true,vec![]),
    ];
    let mut reports = vec![];
    for (name, question, scope, do_answer, expected) in cases {
        println!("START {name}");
        let start = Instant::now();
        let rounds = Arc::new(Mutex::new(Vec::new()));
        let captured = rounds.clone();
        let cancel = CancellationToken::new();
        let result=retrieval::retrieve(&s,question,&scope,&[],&settings.retrieval,&cancel,
            |candidates,evidence| {
                let captured=captured.clone();let client=&client;let key=&key;let config=&jev;let cancel=&cancel;
                async move {
                    let pool=candidates.iter().map(|c|json!({"id":c.id,"path":c.path,"heading":c.heading})).collect::<Vec<_>>();
                    let evidence_ids=evidence.iter().map(|c|c.id).collect::<Vec<_>>();
                    let started=Instant::now();
                    let scored=models::score(client,config,key,question,candidates,&evidence,120,cancel).await?;
                    captured.lock().unwrap().push(json!({"pool":pool,"evidence_ids":evidence_ids,"scores":scored.iter().map(|c|json!({"id":c.id,"score":c.score})).collect::<Vec<_>>(),"elapsed_ms":started.elapsed().as_millis()}));
                    Ok(scored)
                }
            }, |stage,count|println!("  {stage} {count}")).await;
        let record = match result {
            Ok(result) => {
                let text = if do_answer {
                    Some(
                        models::answer(
                            &client,
                            &answer,
                            &key,
                            &models::prepare_answer(
                                &answer,
                                question,
                                &result.results,
                                &[],
                                16000,
                                2048,
                            )?,
                            120,
                            &cancel,
                            |_| {},
                        )
                        .await?
                        .text,
                    )
                } else {
                    None
                };
                let expected_found=expected.iter().map(|suffix|json!({"file":suffix,"found":result.results.iter().any(|c|c.path.ends_with(suffix))})).collect::<Vec<_>>();
                let scope_ok = scope.is_empty()
                    || result
                        .results
                        .iter()
                        .all(|c| scope.iter().any(|p| PathBuf::from(&c.path).starts_with(p)));
                println!(
                    "DONE {name}: {} rounds, {} results, {} ms",
                    result.trace.len(),
                    result.results.len(),
                    start.elapsed().as_millis()
                );
                json!({"case":name,"question":question,"scope":scope,"scope_ok":scope_ok,"expected_sources":expected_found,"elapsed_ms":start.elapsed().as_millis(),"answer":text,"trace":result.trace,"results":result.results,"jev_rounds":*rounds.lock().unwrap()})
            }
            Err(error) => {
                println!("FAILED {name}: {error}");
                json!({"case":name,"error":error.to_string(),"jev_rounds":*rounds.lock().unwrap()})
            }
        };
        reports.push(record);
        std::fs::write(
            out.join("results.json"),
            serde_json::to_vec_pretty(
                &json!({"root":root,"document_count":count,"chunk_count":s.chunk_count()?,"index_ms":index_ms,"models":{"answer":answer.model,"jev":jev.model},"cases":reports}),
            )?,
        )?;
    }
    anyhow::ensure!(
        reports.iter().all(|r| r.get("error").is_none()
            && r["scope_ok"].as_bool() == Some(true)
            && r["expected_sources"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e["found"].as_bool() == Some(true))),
        "At least one source or scope assertion failed; see results.json"
    );
    Ok(())
}
