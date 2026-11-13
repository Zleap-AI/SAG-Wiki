//! Read-only search/content smoke checks for a completed real-document benchmark.
use anyhow::Result;
use sag_core::store::Store;
fn main() -> Result<()> {
    let db = std::env::args().nth(1).expect("database path");
    let store = Store::open(std::path::Path::new(&db))?;
    let queries = ["defects", "Cranford", "Gaskell"];
    let mut results = Vec::new();
    for query in queries {
        results.push((query, store.search_files(query, &[], 10)?.len()));
    }
    let connection = rusqlite::Connection::open(&db)?;
    let chunks: i64 = connection.query_row("SELECT count(*) FROM chunks", [], |row| row.get(0))?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"queries":results,"chunks":chunks,"files":store.files()?,"roots":store.roots()?})
        )?
    );
    Ok(())
}
