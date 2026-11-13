//! Local-only real-document regression. Run in release mode with PDF and output directory.
use anyhow::Result;
use sag_core::store::Store;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let basic_only = std::env::var_os("SAG_BENCH_BASIC_ONLY").is_some();
    let input = PathBuf::from(&args[1]).canonicalize()?;
    let output = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&output)?;
    let root = output.join("library");
    std::fs::create_dir_all(&root)?;
    std::fs::copy(&input, root.join("300_1.pdf"))?;
    let data = output.join("data");
    std::fs::create_dir_all(&data)?;
    let installed = PathBuf::from(
        std::env::var("SAG_BENCH_DOCLING")
            .expect("set SAG_BENCH_DOCLING to installed local component directory"),
    );
    let runtime = data.join("docling");
    std::fs::create_dir_all(&runtime)?;
    std::fs::copy(installed.join("active.json"), runtime.join("active.json"))?;
    // APFS clones isolate writable runtime files without duplicating model blocks.
    let copied = std::process::Command::new("/bin/cp")
        .arg(if cfg!(target_os = "macos") {
            "-cR"
        } else {
            "-R"
        })
        .arg(installed.join("components"))
        .arg(runtime.join("components"))
        .status()?;
    anyhow::ensure!(copied.success(), "component clone failed");
    let store = Arc::new(Store::open(&data.join("index.sqlite"))?);
    let mut settings = store.settings()?;
    settings.convert_other_formats = true;
    settings.conversion_formats = Some(vec!["pdf".into()]);
    settings.mineru.enabled = false;
    store.save_settings(&settings)?;
    let library = store.add_root(&root)?;
    let stopped = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    let poll_store = store.clone();
    let done = stopped.clone();
    let snapshot = output.join("first-search");
    let observer = std::thread::spawn(move || {
        let mut first = None;
        loop {
            if first.is_none()
                && poll_store
                    .search_files("defects", &[], 10)
                    .is_ok_and(|f| !f.is_empty())
            {
                first = Some(started.elapsed().as_millis());
                eprintln!("FIRST_SEARCHABLE_MS={}", first.unwrap());
                std::fs::create_dir_all(&snapshot).unwrap();
                if let Ok(files) = poll_store.files() {
                    for file in &files {
                        if let Some(name) = std::path::Path::new(&file.path).file_name() {
                            let _ = std::fs::copy(&file.path, snapshot.join(name));
                        }
                    }
                    let _ = std::fs::write(
                        snapshot.join("files.json"),
                        serde_json::to_vec_pretty(&files).unwrap(),
                    );
                }
            }
            if done.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        first
    });
    let progress = |p: sag_core::types::IndexProgress| {
        eprintln!(
            "PROGRESS {} {} {}",
            started.elapsed().as_millis(),
            p.count,
            p.total
        )
    };
    let result = if basic_only {
        store.scan_basic(&library, progress)
    } else {
        store.scan(&library, progress)
    };
    let total = started.elapsed().as_millis();
    stopped.store(true, Ordering::SeqCst);
    let first = observer.join().unwrap();
    let files = store.files()?;
    let final_hits = store.search_files("defects", &[], 10)?.len();
    let reader = rusqlite::Connection::open(data.join("index.sqlite"))?;
    let chunks: i64 = reader.query_row("SELECT count(*) FROM chunks", [], |row| row.get(0))?;
    let report = serde_json::json!({"input":input,"input_bytes":std::fs::metadata(&input)?.len(),"basic_only":basic_only,"elapsed_ms":total,"first_searchable_ms":first,"query":"defects","final_query_hits":final_hits,"chunks":chunks,"files":files,"roots":store.roots()?,"error":result.err().map(|e|format!("{e:#}")),"settings":settings});
    std::fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
