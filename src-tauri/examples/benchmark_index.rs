//! Compare indexing worker counts without changing source files or the application index.
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

use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Instant};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(
        args.next()
            .context("Usage: benchmark_index DIRECTORY [WORKERS ...]")?,
    )
    .canonicalize()?;
    let workers = args
        .map(|s| s.parse::<usize>())
        .collect::<Result<Vec<_>, _>>()?;
    let workers = if workers.is_empty() {
        vec![1, 8, 14]
    } else {
        workers
    };
    let temporary = tempfile::tempdir()?;
    let mut reference = None;
    for workers in workers {
        let db = store::Store::open(&temporary.path().join(format!("{workers}.sqlite")))?;
        let settings = types::Settings {
            index_workers: workers,
            ..Default::default()
        };
        db.save_settings(&settings)?;
        let root = db.add_root(&root)?;
        let started = Instant::now();
        let documents = db.scan(&root, |_| {})?;
        let seconds = started.elapsed().as_secs_f64();
        let mut digest = Sha256::new();
        let connection = db.connection();
        let mut statement =
            connection.prepare("SELECT rowid,title,heading,body FROM chunk_fts ORDER BY rowid")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            digest.update(row.get::<_, i64>(0)?.to_le_bytes());
            for column in 1..=3 {
                let value: String = row.get(column)?;
                digest.update((value.len() as u64).to_le_bytes());
                digest.update(value.as_bytes());
            }
        }
        let checksum = format!("{:x}", digest.finalize());
        if let Some(expected) = &reference {
            ensure!(
                expected == &checksum,
                "Index changed with {workers} workers"
            );
        } else {
            reference = Some(checksum.clone());
        }
        println!(
            "{}",
            serde_json::json!({
                "workers": workers, "documents": documents,
                "seconds": seconds, "index_sha256": checksum,
            })
        );
    }
    Ok(())
}
