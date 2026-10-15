//! Isolate answer synthesis using the exact passages saved by the original run.
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
use futures_util::{stream, StreamExt};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::Arc, time::Instant};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    let baseline = PathBuf::from(std::env::var("SAG_EVAL_BASELINE")?);
    let folder = PathBuf::from(std::env::var("SAG_EVAL_DIR")?);
    fs::create_dir_all(folder.join("results"))?;
    let credentials = PathBuf::from(std::env::var("SAG_TEST_CREDENTIALS")?);
    let key = Arc::new(models::secret(&credentials, "answer")?);
    let config: Value = serde_json::from_slice(&fs::read(baseline.join("config.json"))?)?;
    let model: types::ModelConfig = serde_json::from_value(config["answer"].clone())?;
    let config = Arc::new(config);
    let details: Vec<Value> = serde_json::from_slice(&fs::read(baseline.join("details.json"))?)?;
    let selected: Option<Vec<String>> = std::env::var_os("SAG_EVAL_CASES")
        .map(|path| -> Result<_> { Ok(serde_json::from_slice(&fs::read(path)?)?) })
        .transpose()?;
    let cases: Vec<_> = details
        .into_iter()
        .filter(|d| {
            if let Some(selected) = &selected {
                selected.iter().any(|id| d["id"] == id.as_str())
            } else {
                d["all_gold_retrieved"] == true
                    && d["parse_error"].is_null()
                    && d["prediction"] == ""
            }
        })
        .collect();
    fs::write(
        folder.join("cases.json"),
        serde_json::to_vec_pretty(&cases)?,
    )?;
    let client = reqwest::Client::new();
    let mut jobs=stream::iter(cases.into_iter().map(|case| {
        let (folder,baseline,key,client,config,model)=(folder.clone(),baseline.clone(),key.clone(),client.clone(),config.clone(),model.clone());
        async move {
            let id=case["id"].as_str().unwrap();
            let old:Value=serde_json::from_slice(&fs::read(baseline.join("results").join(format!("{id}.json")))?)?;
            let mut message:types::Message=serde_json::from_value(old["message"].clone())?;
            let original_calls=message.tool_calls.len();
            let fixed_evidence=serde_json::to_value(&message.evidence)?;
            message.answer.clear();message.status="running".into();message.error=None;
            let started=Instant::now();println!("START {id}");
            let result=agent::answer(&client,&model,&key,&mut message,&[],
                config["answer_context_tokens"].as_u64().unwrap() as usize,
                config["answer_output_tokens"].as_u64().unwrap() as usize,
                config["timeout_secs"].as_u64().unwrap(),&CancellationToken::new(),
                agent::AgentOptions{max_tool_calls:original_calls,sag2_enabled:true,system_prompt:config["system_prompt"].as_str().unwrap(),mcp:None},
                |_|async{anyhow::bail!("Fixed-evidence replay must not retrieve additional passages")},
                |_,_|async{Ok(true)},|_,_,_,_|{}).await;
            ensure!(serde_json::to_value(&message.evidence)?==fixed_evidence,"Replay changed the fixed evidence");
            let error=result.err().map(|e|format!("{e:#}"));
            message.error=error.clone();message.status=if error.is_none(){"complete"}else{"failed"}.into();
            fs::write(folder.join("results").join(format!("{id}.json")),serde_json::to_vec_pretty(&json!({"id":id,"status":message.status,"message":message,"error":error,"elapsed_ms":started.elapsed().as_millis(),"new_tool_calls":message.tool_calls.len()-original_calls,"fixed_evidence":true}))?)?;
            println!("DONE {id} {:.1}s {}",started.elapsed().as_secs_f64(),if error.is_none(){"complete"}else{"error"});
            Ok::<_,anyhow::Error>(())
        }
    })).buffer_unordered(2);
    while let Some(result) = jobs.next().await {
        result?;
    }
    Ok(())
}
