use super::*;

#[cfg(unix)]
#[test]
#[ignore = "requires SAG_REPLAY_PDF and SAG_REPLAY_ZIPS, plus the normal QPDF splitter"]
fn full_book_official_mineru_zip_failure_reuses_parts_and_task() {
    use crate::{
        docling::{Docling, fingerprint},
        store::Store,
    };
    use sag_protocol::types::Settings;
    use std::{collections::BTreeMap, fs};
    use walkdir::WalkDir;

    const SOURCE_SHA: &str = "164ba82cc5029e15e7634b4663ccaa9aa0314851a887a3d5f44fe257f80df7a4";
    const FAKE_KEY: &str = "offline-replay";
    const RANGES: [(u32, u32); 8] = [
        (1, 50),
        (51, 100),
        (101, 150),
        (151, 200),
        (201, 250),
        (251, 300),
        (301, 350),
        (351, 378),
    ];
    const ZIP_HASHES: [&str; 8] = [
        "196676da77badf280a9935741e68fe545aa67d85572b5e8f63a883a2ae667684",
        "7fb6460c31301683cdf1b6f5ab4c69c270d0cf2080d9dfa2e26202358a510f16",
        "8d3573cded300ebd321d0f641cdf95d8812a135e5e942c23a7810acfb3590c06",
        "439958081fd265833823882e0a50a530832fc974975a25f35193b0f449ade15f",
        "89bbbacf271d40b507aee6e18ebb97a302cd856440879deb632a56040e6d981f",
        "de2ac33f61d35571be7dce1882f2e2b27ab52211e8eb0b8fbbbd7f2751043ae7",
        "84faa6f3cdd33292aaf15dd05db1eb588375ef01a908f675734344863f1b2253",
        "5b77fa6d22f2f81a07a331e05a6317531119f628cc925ec1d6a2fdadbcfa887c",
    ];
    const UPLOADS: [&str; 8] = [
        "/upload/0",
        "/upload/1",
        "/upload/2",
        "/upload/3",
        "/upload/4",
        "/upload/5",
        "/upload/6",
        "/upload/7",
    ];
    const POLLS: [&str; 8] = [
        "/extract-results/batch/replay-0",
        "/extract-results/batch/replay-1",
        "/extract-results/batch/replay-2",
        "/extract-results/batch/replay-3",
        "/extract-results/batch/replay-4",
        "/extract-results/batch/replay-5",
        "/extract-results/batch/replay-6",
        "/extract-results/batch/replay-7",
    ];
    const DOWNLOADS: [&str; 8] = [
        "/zip/0", "/zip/1", "/zip/2", "/zip/3", "/zip/4", "/zip/5", "/zip/6", "/zip/7",
    ];

    let source = PathBuf::from(std::env::var_os("SAG_REPLAY_PDF").expect("set SAG_REPLAY_PDF"));
    assert_eq!(
        fingerprint(&source).unwrap(),
        SOURCE_SHA,
        "use the frozen benchmark PDF"
    );
    let archive_dir =
        PathBuf::from(std::env::var_os("SAG_REPLAY_ZIPS").expect("set SAG_REPLAY_ZIPS"));
    let mut archives = Vec::with_capacity(RANGES.len());
    for (index, (start, end)) in RANGES.into_iter().enumerate() {
        let archive = archive_dir.join(format!("pages-{start:03}-{end:03}.zip"));
        assert_eq!(
            fingerprint(&archive).unwrap(),
            ZIP_HASHES[index],
            "wrong archived ZIP"
        );
        archives.push(fs::read(archive).unwrap());
    }
    let server = Fixture::new(|base| {
        let mut steps = Vec::new();
        for (index, (start, end)) in RANGES.into_iter().enumerate() {
            let poll = || {
                Step::json(
                    "GET",
                    POLLS[index],
                    json!({"code":0,"data":{"extract_result":[{
                        "file_name": format!("pages-{start}-{end}.pdf"),
                        "state":"done",
                        "full_zip_url":format!("{base}{}", DOWNLOADS[index])
                    }]}}),
                )
            };
            steps.push(Step::json(
                "POST",
                "/file-urls/batch",
                json!({"code":0,"data":{"batch_id":format!("replay-{index}"),
                    "file_urls":[format!("{base}{}", UPLOADS[index])]}}),
            ));
            steps.push(Step {
                method: "PUT",
                path: UPLOADS[index],
                status: 200,
                body: vec![],
            });
            steps.push(poll());
            if index == RANGES.len() - 1 {
                steps.push(Step {
                    method: "GET",
                    path: DOWNLOADS[index],
                    status: 200,
                    body: b"intentionally invalid ZIP for the initial attempt".to_vec(),
                });
                steps.push(poll());
            }
            steps.push(Step {
                method: "GET",
                path: DOWNLOADS[index],
                status: 200,
                body: std::mem::take(&mut archives[index]),
            });
        }
        steps
    });

    let sandbox = tempfile::tempdir().unwrap();
    let root = sandbox.path().join("library");
    let data = sandbox.path().join("data");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&data).unwrap();
    let pdf = root.join("benchmark.pdf");
    fs::copy(&source, &pdf).unwrap();
    let pdf = pdf.canonicalize().unwrap();
    let healthy = root.join("healthy.md");
    fs::write(
        &healthy,
        "Unchanged index evidence for selective replay retry.",
    )
    .unwrap();
    let healthy = healthy.canonicalize().unwrap();
    let store = Store::open(&data.join("sag.sqlite")).unwrap();
    assert!(!Docling::new(&data).has_profile(sag_protocol::DoclingProfile::Documents));
    let mut settings = Settings::default();
    settings.convert_other_formats = true;
    settings.conversion_formats = Some(vec!["pdf".into()]);
    settings.mineru = MineruConfig {
        provider: MineruProvider::Official,
        enabled: true,
        base_url: server.base.clone(),
        // Deterministic seven durable parts, then one failed ZIP.
        concurrency: 1,
        request_timeout_secs: 60,
        task_timeout_secs: 180,
        ..Default::default()
    };
    save_key(&data, &settings.mineru, FAKE_KEY).unwrap();
    store.save_settings(&settings).unwrap();
    let root = store.add_root(&root).unwrap();

    let published = || -> Option<(String, String, String)> {
        use rusqlite::OptionalExtension;
        store.connection().query_row(
            "SELECT output, source_hash, output_hash FROM conversions WHERE valid=1 AND source=?",
            [pdf.to_str().unwrap()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional().unwrap()
    };
    let snapshots = |parent_name: &str, extension: &str| -> BTreeMap<PathBuf, String> {
        WalkDir::new(data.join(".sag-converted"))
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| {
                entry.file_type().is_file()
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|name| name == extension)
                    && entry
                        .path()
                        .parent()
                        .unwrap()
                        .file_name()
                        .is_some_and(|name| name == parent_name)
            })
            .map(|entry| {
                let path = entry.into_path();
                let hash = fingerprint(&path).unwrap();
                (path, hash)
            })
            .collect()
    };
    let healthy_hash = || -> String {
        store
            .connection()
            .query_row(
                "SELECT hash FROM documents WHERE path=?",
                [healthy.to_str().unwrap()],
                |row| row.get(0),
            )
            .unwrap()
    };

    // Mirror the service's foreground scan plus independent enhancement worker.
    store.scan_basic(&root, |_| {}).unwrap();
    let original_publication = published();
    let original_healthy_hash = healthy_hash();
    assert_eq!(server.requests.lock().unwrap().len(), 0);
    store.enhance(&root, |_| {}).unwrap();

    assert_eq!(store.failed_index_count().unwrap(), 1);
    assert_eq!(
        published(),
        original_publication,
        "a partial merge must never publish"
    );
    if let Some((path, _, hash)) = &original_publication {
        assert_eq!(fingerprint(Path::new(path)).unwrap(), *hash);
    }
    let cache = snapshots(".sag-pdf-parts", "md");
    assert_eq!(
        cache.len(),
        7,
        "successful segment Markdown must survive failure"
    );
    let journal = snapshots(".sag-mineru-tasks", "json");
    assert_eq!(journal.len(), 8, "all submitted tasks remain durable");
    assert!(
        journal.keys().any(|path| {
            let task: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            task["id"] == "replay-7" && task["uploaded"] == true && task["upload"].is_null()
        }),
        "the failed segment already has a resumable uploaded task"
    );
    assert_eq!(server.requests.lock().unwrap().len(), 32);

    fs::write(
        &healthy,
        "Changed disk content to catch a retry that rescans healthy files.",
    )
    .unwrap();
    let work = store.prepare_failed_reindex().unwrap();
    assert_eq!(work, vec![(root.clone(), vec![pdf.clone()])]);
    store
        .retry_failed_indexes_basic(&root, &work[0].1, |_| {})
        .unwrap();
    store.enhance(&root, |_| {}).unwrap();

    assert_eq!(store.failed_index_count().unwrap(), 0);
    assert_eq!(store.roots().unwrap()[0].status, "ready");
    assert_eq!(healthy_hash(), original_healthy_hash, "retry is selective");
    let (output, source_hash, output_hash) = published().expect("complete conversion after retry");
    assert!(source_hash.ends_with(SOURCE_SHA));
    assert!(!source_hash.starts_with("basic-"));
    assert_eq!(fingerprint(Path::new(&output)).unwrap(), output_hash);
    let body = fs::read_to_string(&output).unwrap();
    let ranges = regex::Regex::new(r"<!-- source-pages:(\d+)-(\d+) -->").unwrap();
    let observed: Vec<(u32, u32)> = ranges
        .captures_iter(&body)
        .map(|m| (m[1].parse().unwrap(), m[2].parse().unwrap()))
        .collect();
    assert_eq!(observed, RANGES);
    let pages = regex::Regex::new(r"<!-- source-page:(\d+) -->").unwrap();
    let observed: Vec<u32> = pages
        .captures_iter(&body)
        .map(|m| m[1].parse().unwrap())
        .collect();
    assert_eq!(observed, (1..=378).collect::<Vec<_>>());
    assert_eq!(snapshots(".sag-mineru-tasks", "json"), journal);
    let recovered_cache = snapshots(".sag-pdf-parts", "md");
    assert_eq!(recovered_cache.len(), 8);
    assert!(
        cache
            .iter()
            .all(|(path, hash)| recovered_cache.get(path) == Some(hash))
    );
    assert_eq!(fingerprint(&source).unwrap(), SOURCE_SHA);
    assert_eq!(fingerprint(&pdf).unwrap(), SOURCE_SHA);

    {
        let requests = server.requests.lock().unwrap();
        assert_eq!(
            requests.len(),
            34,
            "only the last poll and ZIP download repeat"
        );
        for (index, request) in requests.iter().enumerate() {
            let header = request.split_once("\r\n\r\n").unwrap().0;
            let authorized = header
                .lines()
                .any(|line| line.eq_ignore_ascii_case("authorization: Bearer offline-replay"));
            let storage = index % 4 == 1 || index % 4 == 3;
            // Requests 32 and 33 are the failed task's poll and download.
            if index < 32 {
                assert_eq!(authorized, !storage);
            } else {
                assert_eq!(authorized, index == 32);
            }
        }
        for (index, (start, end)) in RANGES.into_iter().enumerate() {
            assert!(requests[index * 4].contains(&format!("\"name\":\"pages-{start}-{end}.pdf\"")));
            assert!(
                requests[index * 4 + 1]
                    .split_once("\r\n\r\n")
                    .unwrap()
                    .1
                    .starts_with("%PDF")
            );
        }
    }
    server.finish(34);
    println!(
        "full PDF replay: 8 segments and 378 pages, 7 durable cache hits, 1 resumed task, 8 uploads total"
    );
}
