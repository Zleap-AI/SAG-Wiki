//! Read a copy of the actual application index; never writes original files.
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
use std::{path::PathBuf, time::Instant};
use tokio_util::sync::CancellationToken;
#[tokio::main]
async fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.cache/search-diagnosis");
    let db = store::Store::open(&root.join("app-snapshot.sqlite"))?;
    let settings = db.settings()?;
    let query = "octx";
    let start = Instant::now();
    let cancel = CancellationToken::new();
    if std::env::args().any(|s| s == "--fast") {
        let mut samples = vec![];
        for _ in 0..3 {
            let timer = Instant::now();
            let results = db.search_files(query, &[], 100)?;
            let elapsed_ms = timer.elapsed().as_secs_f64() * 1000.;
            ensure!(
                results.iter().any(|c| c.title == "open-octx.md"),
                "known document missing"
            );
            println!("FAST {:.1}ms, {} unique files", elapsed_ms, results.len());
            samples.push(serde_json::json!({"elapsed_ms":elapsed_ms,"files":results.len()}));
            std::fs::write(
                root.join("fast-results.json"),
                serde_json::to_vec_pretty(
                    &serde_json::json!({"samples":samples,"results":results}),
                )?,
            )?;
        }
        return Ok(());
    }
    let live = std::env::args().any(|s| s == "--live");
    let client = reqwest::Client::new();
    let key = if live {
        models::secret(
            &PathBuf::from(std::env::var("HOME")?)
                .join("Library/Application Support/com.sag.desktop"),
            "jev",
        )?
    } else {
        String::new()
    };
    let result = retrieval::retrieve(
        &db,
        query,
        &[],
        &[],
        &settings.retrieval,
        &cancel,
        |mut c, e| {
            let client = &client;
            let key = &key;
            let config = &settings.jev;
            let cancel = &cancel;
            async move {
                let start = Instant::now();
                println!("score-start {} candidates, {} evidence", c.len(), e.len());
                let result = if live {
                    models::score(client, config, key, query, c, &e, 35, cancel).await
                } else {
                    for item in &mut c {
                        item.score = 0.8
                    }
                    Ok(c)
                };
                println!("score-finish {}ms", start.elapsed().as_millis());
                result
            }
        },
        |stage, count| println!("{}ms {stage}: {count}", start.elapsed().as_millis()),
    )
    .await?;
    ensure!(
        !result.results.is_empty(),
        "known indexed keyword returned no results"
    );
    println!(
        "DONE {}ms, {} results, {} rounds",
        start.elapsed().as_millis(),
        result.results.len(),
        result.trace.len()
    );
    for c in &result.results {
        println!("{} {}", c.title, c.score)
    }
    std::fs::write(
        root.join(if live {
            "live-results.json"
        } else {
            "local-results.json"
        }),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(())
}
