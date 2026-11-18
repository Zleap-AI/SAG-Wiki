use super::*;

#[test]
#[ignore = "requires SAG_REPLAY_PDF, SAG_REPLAY_ZIPS and the normal QPDF splitter"]
fn full_book_mineru_c1_and_forced_out_of_order_c2_are_equivalent() {
    use sag_protocol::runtime::RuntimeSettings;
    use std::{
        collections::BTreeMap,
        sync::{Condvar, Mutex},
        time::Duration,
    };
    use walkdir::WalkDir;

    const SOURCE_SHA: &str = "164ba82cc5029e15e7634b4663ccaa9aa0314851a887a3d5f44fe257f80df7a4";
    // The baseline auditor independently checked all 3,643 fields, 116 HTML
    // tables and 6,493 cells in this immutable Markdown, including image paths.
    const FULL_MARKDOWN_SHA: &str =
        "8836e1e66789827e674297ef44e3d0b508df754ee0337eea4d4e4b07158f91b9";
    const LIMIT: u64 = 16 * 1024 * 1024;
    const WAIT: Duration = Duration::from_secs(90);
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

    let source = PathBuf::from(std::env::var_os("SAG_REPLAY_PDF").expect("set SAG_REPLAY_PDF"));
    let zip_dir = PathBuf::from(std::env::var_os("SAG_REPLAY_ZIPS").expect("set SAG_REPLAY_ZIPS"));
    assert_eq!(crate::docling::fingerprint(&source).unwrap(), SOURCE_SHA);
    let parts = mineru::prepare_pdf_segments(
        &source,
        200 * 1024 * 1024,
        &RuntimeSettings::default(),
        &|| false,
    )
    .unwrap();
    let ranges: Vec<(u32, u32)> = (1_u32..=378)
        .step_by(50)
        .map(|start| (start, (start + 49).min(378)))
        .collect();
    assert_eq!(
        parts.iter().map(|p| (p.start, p.end)).collect::<Vec<_>>(),
        ranges
    );

    let image_extension = |path: &Path| {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg"
                )
            })
    };
    let mut expected_assets = BTreeMap::new();
    let archives: Vec<PathBuf> = ranges
        .iter()
        .zip(ZIP_HASHES)
        .map(|(&(start, end), hash)| {
            let path = zip_dir.join(format!("pages-{start:03}-{end:03}.zip"));
            assert_eq!(crate::docling::fingerprint(&path).unwrap(), hash);
            let mut zip = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i).unwrap();
                let Some(relative) = entry.enclosed_name() else {
                    continue;
                };
                if entry.is_dir() || !image_extension(&relative) {
                    continue;
                }
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).unwrap();
                assert!(
                    expected_assets
                        .insert(
                            Path::new(hash).join(relative),
                            format!("{:x}", Sha256::digest(&bytes))
                        )
                        .is_none()
                );
            }
            path
        })
        .collect();
    assert_eq!(expected_assets.len(), 399);
    let sandbox = tempfile::tempdir().unwrap();

    let run_width = |width| {
        let directory = sandbox.path().join(format!("width-{width}"));
        fs::create_dir(&directory).unwrap();
        let output = directory.join("book.md");
        let doc = Document::new(&parts, &output, "frozen-cloud-replay", LIMIT).unwrap();
        // Two-way, bounded synchronization. The second worker only proceeds
        // after the first reports active. The first only proceeds after the
        // coordinator publishes the second segment and reports it complete.
        let rendezvous = (Mutex::new((false, false)), Condvar::new());
        let all_started = (Mutex::new(0usize), Condvar::new());
        let mut order = Vec::new();
        let mut active_peak = 0;
        schedule(
            &doc,
            width,
            &|| false,
            &mut |attempt| {
                let Some((progress, _)) = attempt.segment_status else {
                    return;
                };
                active_peak = active_peak.max(progress.active.len());
                if progress.completed <= order.len() {
                    return;
                }
                let segment = attempt.segment.expect("completion has a segment ID");
                assert_eq!(progress.completed, order.len() + 1);
                order.push(segment.current);
                if width >= 2 && segment.current == 2 {
                    rendezvous.0.lock().unwrap().1 = true;
                    rendezvous.1.notify_all();
                }
            },
            |i, job, stop, events| {
                events(Attempt {
                    engine: "MinerU".into(),
                    phase: Some("downloading".into()),
                    ..progress_only()
                });
                if width == 4 && i < 4 {
                    let mut started = all_started.0.lock().unwrap();
                    *started += 1;
                    all_started.1.notify_all();
                    let (started, _) = all_started.1.wait_timeout_while(started, WAIT, |n| *n < 4).unwrap();
                    ensure!(*started == 4, "four cloud workers failed to overlap");
                }
                if width >= 2 && i < 2 {
                    let mut state = rendezvous.0.lock().unwrap();
                    if i == 0 {
                        state.0 = true;
                        rendezvous.1.notify_all();
                    }
                    let (state, _) =
                        rendezvous
                            .1
                            .wait_timeout_while(state, WAIT, |state| {
                                if i == 0 { !state.1 } else { !state.0 }
                            })
                            .unwrap();
                    ensure!(
                        if i == 0 { state.1 } else { state.0 },
                        "first two cloud workers failed to rendezvous"
                    );
                }
                ensure!(!stop(), "replay stopped before archive decode");
                let part = &parts[i];
                let text = mineru::replay_archive_with_assets(
                    fs::read(&archives[i])?,
                    part.start,
                    part.end,
                    LIMIT,
                    job.parsed.parent().unwrap(),
                )?;
                fs::write(&job.parsed, text)?;
                Ok(())
            },
            || {
                |_, _: &Job, _: &Stop<'_>, _: &mut dyn FnMut(Attempt)| {
                    anyhow::bail!("valid archive unexpectedly required local fallback")
                }
            },
        )
        .unwrap();
        assert_eq!(active_peak, width);
        assert!(
            !output.exists(),
            "no final Markdown before the atomic merge"
        );
        if width == 1 {
            assert_eq!(order, (1..=8).collect::<Vec<_>>());
        } else if width == 2 {
            assert_eq!(
                order.first(),
                Some(&2),
                "second segment must complete before first"
            );
            assert!(order.contains(&1));
        } else {
            assert!(order.iter().position(|n| *n == 2) < order.iter().position(|n| *n == 1));
        }
        doc.merge(&output, &|| false).unwrap();

        let package = directory.join(".sag-pdf-parts/.sag-mineru-assets");
        let assets: BTreeMap<_, _> = WalkDir::new(&package)
            .into_iter()
            .map(|entry| entry.unwrap())
            .filter(|entry| entry.file_type().is_file() && image_extension(entry.path()))
            .map(|entry| {
                let path = entry.path();
                (
                    path.strip_prefix(&package).unwrap().to_owned(),
                    crate::docling::fingerprint(path).unwrap(),
                )
            })
            .collect();
        assert_eq!(
            assets, expected_assets,
            "every image must match the frozen ZIP"
        );
        let body = fs::read_to_string(output).unwrap();
        let digest = format!("{:x}", Sha256::digest(body.as_bytes()));
        assert_eq!(
            digest, FULL_MARKDOWN_SHA,
            "check all audited text and table bytes"
        );
        let ranges_re = regex::Regex::new(r"<!-- source-pages:(\d+)-(\d+) -->").unwrap();
        assert_eq!(
            ranges_re
                .captures_iter(&body)
                .map(|m| (m[1].parse::<u32>().unwrap(), m[2].parse::<u32>().unwrap()))
                .collect::<Vec<_>>(),
            ranges
        );
        for expression in [r"<!-- source-page:(\d+) -->", r#"<a id="page-(\d+)"></a>"#] {
            let re = regex::Regex::new(expression).unwrap();
            assert_eq!(
                re.captures_iter(&body)
                    .map(|m| m[1].parse::<u32>().unwrap())
                    .collect::<Vec<_>>(),
                (1..=378).collect::<Vec<_>>()
            );
        }
        println!(
            "frozen PDF replay: width={width} active_peak={active_peak} completion_order={order:?} markdown_sha256={digest} image_count={}",
            assets.len()
        );
        (body, assets)
    };
    let serial = run_width(1);
    assert_eq!(serial, run_width(2), "scheduling must preserve final output");
    assert_eq!(serial, run_width(4), "four cloud workers must preserve final output");
    assert_eq!(crate::docling::fingerprint(&source).unwrap(), SOURCE_SHA);
}
