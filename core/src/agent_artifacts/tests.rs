use super::*;

fn apply(service: &Service, options: &Options, auto_sync: bool, exceptions: Vec<String>) -> Report {
    let result = handle(
        service,
        Action::Scan {
            options: options.clone(),
        },
    )
    .unwrap();
    let token = result["scan"].as_str().unwrap().to_owned();
    serde_json::from_value(
        handle(
            service,
            Action::Start {
                scan: token,
                exclude_all: false,
                exceptions,
                auto_sync,
            },
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn mac_detection_uses_only_known_delivery_locations() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    let documents = home.join("Documents");
    for path in [
        "Documents/Claude Code/outputs",
        "Documents/Codex/2026-11-22/task/outputs",
        "Documents/Codex/other/task/outputs",
        "Documents/Codex/2026-11-22/task/work/outputs",
        "Workbuddy",
        "Library/Application Support/Claude-3p/local-agent-mode-sessions/a/b/c/outputs",
        "Library/Application Support/Claude-3p/local-agent-mode-sessions/a/b/c/uploads",
        "ordinary-project/outputs",
    ] {
        fs::create_dir_all(home.join(path)).unwrap();
    }
    let detected = Agent::ALL.map(|agent| detect(agent, home, &documents, true));
    assert_eq!(
        detected
            .iter()
            .map(|item| item.directories.len())
            .collect::<Vec<_>>(),
        [0, 1, 1, 1]
    );
    assert!(Agent::ALL
        .map(|agent| detect(agent, home, &documents, false))
        .iter()
        .all(|item| item.directories.is_empty()));
    assert!(detected[1].directories[0].ends_with("2026-11-22/task/outputs"));
}

#[test]
fn workbuddy_uses_only_registered_local_deliverables_inside_its_workspace() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().canonicalize().unwrap();
    let root = home.join("WorkBuddy");
    let index = home.join(".workbuddy/artifact-index");
    fs::create_dir(&root).unwrap();
    fs::create_dir_all(&index).unwrap();
    let delivered = root.join("报告 1.pdf");
    let scratch = root.join("draft.pdf");
    let unrelated = home.join("outside.pdf");
    for file in [&delivered, &scratch, &unrelated] {
        fs::write(file, "example").unwrap();
    }
    let url = |path: &Path| reqwest::Url::from_file_path(path).unwrap().to_string();
    fs::write(
        index.join("account.json"),
        serde_json::to_vec(&serde_json::json!({
            "artifacts": [
                { "uri": url(&delivered) },
                { "uri": url(&unrelated) },
                { "uri": "https://example.com/report.pdf" },
                { "uri": url(&root.join("deleted.pdf")) }
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(workbuddy_files(&root).unwrap(), vec![delivered]);
}

#[tokio::test]
async fn codex_default_collects_top_level_deliveries_and_html_packages() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path().canonicalize().unwrap();
    let output = base.join("outputs");
    fs::create_dir_all(output.join("app/assets")).unwrap();
    fs::create_dir_all(output.join("project/work")).unwrap();
    fs::write(output.join("app/index.html"), "html").unwrap();
    fs::write(output.join("app/assets/asset.png"), "asset").unwrap();
    fs::write(output.join("project/work/screenshot.png"), "scratch").unwrap();
    fs::write(output.join("deliverable.pdf"), "pdf").unwrap();
    fs::write(output.join("notes.md"), "# Markdown output").unwrap();
    let service = Service::open(base.join("service")).unwrap();
    let options = Options {
        sources: vec![Source {
            agent: Agent::Codex,
            enabled: true,
            directory: None,
        }],
        formats: DEFAULT_FORMATS.to_vec(),
        destination: base.join("archive"),
    };
    let detected = Detection {
        agent: Agent::Codex,
        directories: vec![output],
    };
    let preview = scan(&service, options.clone(), &detected, false).unwrap();
    assert_eq!(preview.candidates.len(), 3);
    assert!(preview
        .candidates
        .iter()
        .any(|item| item.name == "notes.md"));
    assert!(preview
        .candidates
        .iter()
        .any(|item| item.name == "app" && item.bundle));
    assert!(preview
        .candidates
        .iter()
        .any(|item| item.name == "deliverable.pdf"));
    let preview = scan(
        &service,
        Options {
            formats: vec![Format::Image],
            ..options
        },
        &detected,
        false,
    )
    .unwrap();
    assert!(preview.candidates.is_empty());
    service.finish().await.unwrap();
}

#[tokio::test]
async fn manual_import_leaves_sync_off_and_sync_is_scoped_by_app() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path().canonicalize().unwrap();
    let delivery = base.join("outputs");
    fs::create_dir(&delivery).unwrap();
    fs::write(delivery.join("first.pdf"), "first").unwrap();
    let archive = base.join("archive");
    let service = Service::open(base.join("service")).unwrap();
    let agent = Agent::Codex;
    let options = Options {
        sources: vec![Source {
            agent,
            enabled: true,
            directory: Some(delivery.clone()),
        }],
        formats: vec![Format::Pdf],
        destination: archive.clone(),
    };
    let status = |agent| {
        handle(&service, Action::Status { agent }).unwrap()["running"]
            .as_bool()
            .unwrap()
    };
    assert!(!status(agent));
    assert_eq!(apply(&service, &options, false, vec![]).changed, 1);
    assert!(archive.join("codex/first.pdf").is_file());
    fs::write(delivery.join("next.pdf"), "next").unwrap();
    run_scheduled(&service).unwrap();
    assert!(!status(agent));
    assert!(!archive.join("codex/next.pdf").exists());
    assert_eq!(apply(&service, &options, true, vec![]).changed, 1);
    assert!(status(agent));
    assert!(!status(Agent::WorkBuddy));
    fs::write(delivery.join("after.pdf"), "after").unwrap();
    run_scheduled(&service).unwrap();
    assert!(archive.join("codex/after.pdf").is_file());
    service.finish().await.unwrap();
}

#[tokio::test]
async fn web_artifacts_use_connected_folders_and_recheck_saved_access() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path().canonicalize().unwrap();
    let source = base.join("outputs");
    let outside = base.join("outside");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(source.join("report.md"), "report").unwrap();
    fs::write(outside.join("private.md"), "private").unwrap();
    let service = Service::open(base.join("service")).unwrap();
    let agent = Agent::Codex;
    let status = handle_web(&service, Action::Status { agent }).unwrap();
    let mut options: Options = serde_json::from_value(status["options"].clone()).unwrap();
    let archive = options.destination.clone();
    options.sources[0].directory = Some(outside.clone());
    assert!(handle_web(&service, Action::Scan { options: options.clone() }).is_err());
    let desktop = handle(&service, Action::Scan { options: options.clone() }).unwrap();
    let token = desktop["scan"].as_str().unwrap().to_owned();
    assert!(handle_web(&service, Action::Page { scan: token, page: 0 }).is_err());

    let root = service.store.add_root(&source).unwrap();
    options.sources[0].directory = Some(source.clone());
    options.destination = outside;
    assert!(handle_web(&service, Action::Scan { options: options.clone() }).is_err());
    options.destination = archive.clone();
    let preview = handle_web(&service, Action::Scan { options }).unwrap();
    assert_eq!(preview["total"], 1);
    let token = preview["scan"].as_str().unwrap().to_owned();
    assert_eq!(
        handle_web(&service, Action::Start {
            scan: token, exclude_all: false, exceptions: vec![], auto_sync: true,
        }).unwrap()["changed"],
        1
    );
    assert!(archive.join("codex/report.md").is_file());
    assert!(service.store.roots().unwrap().iter().any(|root| Path::new(&root.path) == archive));
    assert_eq!(handle_web(&service, Action::Status { agent }).unwrap()["running"], true);

    service.store.remove_root(&root).unwrap();
    fs::write(source.join("revoked.md"), "revoked").unwrap();
    run_scheduled(&service).unwrap();
    assert!(!archive.join("codex/revoked.md").exists());
    handle_web(&service, Action::Stop { agent }).unwrap();
    assert_eq!(handle_web(&service, Action::Status { agent }).unwrap()["running"], false);
    service.finish().await.unwrap();
}

#[tokio::test]
async fn copies_local_artifacts_and_keeps_history_selection_after_reopening() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path().canonicalize().unwrap();
    let delivery = base.join("delivered");
    let archive = base.join("archived");
    fs::create_dir_all(delivery.join("demo/assets")).unwrap();
    fs::write(
        delivery.join("demo/index.html"),
        "<img src='assets/photo.png'>",
    )
    .unwrap();
    fs::write(delivery.join("demo/assets/photo.png"), "image").unwrap();
    fs::write(delivery.join("demo/assets/readme.txt"), "bundle content").unwrap();
    fs::write(delivery.join("report.pdf"), "pdf-v1").unwrap();
    fs::write(delivery.join("skip.pdf"), "skip-v1").unwrap();
    fs::write(delivery.join("old.md"), "disabled").unwrap();
    fs::write(delivery.join("~$office.docx"), "office lock").unwrap();
    let options = Options {
        destination: archive.clone(),
        formats: DEFAULT_FORMATS
            .into_iter()
            .filter(|format| *format != Format::Markdown)
            .collect(),
        sources: vec![Source {
            agent: Agent::Codex,
            enabled: true,
            directory: Some(delivery.clone()),
        }],
    };
    let service_path = base.join("service");
    let service = Service::open(service_path.clone()).unwrap();
    let preview = handle(
        &service,
        Action::Scan {
            options: options.clone(),
        },
    )
    .unwrap();
    assert_eq!(preview["total"], 3);
    let excluded = preview["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "skip.pdf")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let result = apply(&service, &options, true, vec![excluded]);
    assert_eq!((result.changed, result.failed), (2, 0));
    let out = archive.join("codex");
    assert_eq!(
        fs::read_to_string(out.join("demo/assets/photo.png")).unwrap(),
        "image"
    );
    assert!(service
        .store
        .authorize(&out.join("demo/index.html"))
        .is_ok());
    assert!(!out.join("skip.pdf").exists());
    fs::remove_file(out.join("report.pdf")).unwrap();
    run_scheduled(&service).unwrap();
    assert!(!out.join("report.pdf").exists());
    service.finish().await.unwrap();
    drop(service);
    fs::write(delivery.join("skip.pdf"), "skip-v2 changed").unwrap();
    fs::write(delivery.join("report.pdf"), "pdf-v2 changed").unwrap();
    fs::write(delivery.join("while-off.pdf"), "new").unwrap();
    let service = Service::open(service_path).unwrap();
    run_scheduled(&service).unwrap();
    assert_eq!(
        fs::read_to_string(out.join("skip.pdf")).unwrap(),
        "skip-v2 changed"
    );
    assert_eq!(
        fs::read_to_string(out.join("report.pdf")).unwrap(),
        "pdf-v2 changed"
    );
    assert!(out.join("while-off.pdf").exists());
    fs::write(out.join("report.pdf"), "user edit").unwrap();
    fs::write(delivery.join("report.pdf"), "pdf-v3 changed!").unwrap();
    run_scheduled(&service).unwrap();
    assert_eq!(
        fs::read_to_string(out.join("report.pdf")).unwrap(),
        "user edit"
    );
    assert_eq!(
        fs::read_to_string(out.join("report (2).pdf")).unwrap(),
        "pdf-v3 changed!"
    );
    handle(
        &service,
        Action::Stop {
            agent: Agent::Codex,
        },
    )
    .unwrap();
    fs::write(delivery.join("stopped.pdf"), "stopped").unwrap();
    run_scheduled(&service).unwrap();
    assert!(!out.join("stopped.pdf").exists());
    fs::remove_dir_all(delivery).unwrap();
    assert_eq!(
        fs::read_to_string(out.join("demo/index.html")).unwrap(),
        "<img src='assets/photo.png'>"
    );
    service.finish().await.unwrap();
}
