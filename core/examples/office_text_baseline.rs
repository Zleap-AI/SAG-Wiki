//! Local synthetic-document comparison. Fresh DB per invocation.
use sag_core::store::Store;
use std::{path::PathBuf, time::Instant};
fn main() -> anyhow::Result<()> {
    let a: Vec<_> = std::env::args().collect();
    let source = PathBuf::from(&a[1]).canonicalize()?;
    let data = PathBuf::from(&a[2]);
    std::fs::create_dir_all(&data)?;
    let root = data.join("files");
    std::fs::create_dir_all(&root)?;
    std::fs::copy(&source, root.join(source.file_name().unwrap()))?;
    let store = Store::open(&data.join("index.sqlite"))?;
    let mut s = store.settings()?;
    s.runtime.office_text_only = a[3] == "on";
    s.watch = false;
    s.mineru.enabled = false;
    store.save_settings(&s)?;
    let key = store.add_root(&root)?;
    let started = Instant::now();
    let result = store.scan(&key, |_| {});
    let elapsed = started.elapsed().as_secs_f64();
    let hits = store.search_files("officetexttoken", &[], 10)?.len();
    let roots = store.roots()?;
    println!(
        "{}",
        serde_json::json!({"mode":a[3],"seconds":elapsed,"hits":hits,"files":store.files()?.len(),"root_status":roots[0].status,"error":roots[0].error,"scan_error":result.err().map(|e|e.to_string())})
    );
    Ok(())
}
