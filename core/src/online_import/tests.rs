use super::*;

#[test]
fn github_reopens_with_the_narrowest_selected_scopes() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("index.sqlite")).unwrap();
    let mut saved = example(temp.path());
    saved.connection.platform = Platform::Github;
    saved.resources = [
        "github:tree:main:",
        "github:tree:main:docs",
        "github:blob:main:docs/README.md",
        "github:blob:main:overview.md",
        "github:tree:archive:docs",
    ].into_iter().map(str::to_owned).collect();
    store.connection().execute(
        "INSERT INTO online_sources VALUES(?1,?2,NULL)",
        params![saved.id, serde_json::to_string(&saved).unwrap()],
    ).unwrap();
    assert_eq!(
        source(&store, &saved.id).unwrap().0.resources,
        ["github:blob:main:docs/README.md", "github:blob:main:overview.md", "github:tree:archive:docs"]
    );
}

fn example(directory: &Path) -> Source {
    Source {
        id: "fixture".into(),
        name: "Demo".into(),
        connection: Connection {
            platform: Platform::Rss,
            drive: false,
            user_access: false,
            app_id: String::new(),
            address: "https://example.org/feed".into(),
        },
        resources: vec!["feed".into()],
        directory: directory.to_str().unwrap().into(),
        schedule: Schedule::Manual,
        daily_time: "09:00".into(),
        paused: false,
        api: None,
    }
}

#[test]
fn preserves_path_and_user_edits_across_updates() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("index.sqlite")).unwrap();
    let directory = temp.path().join("library");
    let source = example(&directory);
    store
        .connection()
        .execute(
            "INSERT INTO online_sources VALUES(?1,?2,NULL)",
            params![source.id, serde_json::to_string(&source).unwrap()],
        )
        .unwrap();
    let mut item = remote::Item {
        id: "entry".into(),
        title: "Title".into(),
        version: "1".into(),
        link: String::new(),
        folders: vec!["Chapter".into()],
        kind: "rss".into(),
        inline: None,
    };
    let content = |text: &str| remote::Content {
        bytes: text.as_bytes().to_owned(),
        extension: "md".into(),
        assets: vec![],
    };
    let records = || -> Vec<FileRecord> {
        let serialized = store
            .connection()
            .query_row("SELECT files FROM online_items", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap();
        serde_json::from_str(&serialized).unwrap()
    };
    let Outcome::Written {
        path,
        updated: false,
    } = install_content(
        &store,
        &source,
        &directory,
        &item,
        None,
        content("Original"),
    )
    .unwrap()
    else {
        panic!("first import must create a file")
    };
    assert!(path.starts_with(directory.join("Chapter")));
    item.title = "Renamed remotely".into();
    item.version = "2".into();
    let Outcome::Written {
        path: updated_path,
        updated: true,
    } = install_content(
        &store,
        &source,
        &directory,
        &item,
        Some(records()),
        content("Remote update"),
    )
    .unwrap()
    else {
        panic!("remote update must replace the original file")
    };
    assert_eq!(path, updated_path);
    assert!(matches!(
        install_content(
            &store,
            &source,
            &directory,
            &item,
            Some(records()),
            content("Remote update")
        )
        .unwrap(),
        Outcome::Skipped
    ));
    fs::write(&path, "Local edits").unwrap();
    item.version = "3".into();
    assert!(matches!(
        install_content(
            &store,
            &source,
            &directory,
            &item,
            Some(records()),
            content("Next remote update")
        )
        .unwrap(),
        Outcome::Conflict(_)
    ));
    assert_eq!(fs::read_to_string(path).unwrap(), "Local edits");
}

#[test]
fn next_run_respects_mode_and_pause() {
    let mut source = example(Path::new("/example"));
    let instant = 1_800_000_000;
    assert_eq!(next_run(&source, instant).unwrap(), None);
    for (schedule, seconds) in [
        (Schedule::HalfHourly, 1800),
        (Schedule::Hourly, 3600),
        (Schedule::EverySixHours, 6 * 3600),
        (Schedule::EveryTwelveHours, 12 * 3600),
    ] {
        source.schedule = schedule;
        assert_eq!(next_run(&source, instant).unwrap(), Some(instant + seconds));
    }
    source.schedule = Schedule::Daily;
    source.daily_time = "16:45".into();
    let next = next_run(&source, instant).unwrap().unwrap();
    assert!(next > instant && next - instant <= 25 * 3600);
    assert_eq!(
        Local
            .timestamp_opt(next, 0)
            .unwrap()
            .format("%H:%M")
            .to_string(),
        "16:45"
    );
    source.paused = true;
    assert_eq!(next_run(&source, instant).unwrap(), None);
}

#[tokio::test]
async fn saves_sync_frequency_per_connection() {
    let temp = tempfile::tempdir().unwrap();
    let service = Service::open(temp.path().join("data")).unwrap();
    let mut first = example(&temp.path().join("first"));
    let mut second = example(&temp.path().join("second"));
    second.id = "second".into();
    second.schedule = Schedule::EveryTwelveHours;
    let saved_second_run = now() + 12 * 3600;
    {
        let db = service.store.connection();
        for (source, next) in [(&first, None), (&second, Some(saved_second_run))] {
            db.execute(
                "INSERT INTO online_sources VALUES(?1,?2,?3)",
                params![source.id, serde_json::to_string(source).unwrap(), next],
            )
            .unwrap();
        }
    }

    let before = now();
    first.schedule = Schedule::HalfHourly;
    handle(
        &service,
        Action::Save {
            source: first,
            secret: None,
            auth_session: None,
        },
    )
    .await
    .unwrap();
    let listed = handle(
        &service,
        Action::List {
            platform: Platform::Rss,
        },
    )
    .await
    .unwrap();
    let sources = listed["sources"].as_array().unwrap();
    assert_eq!(sources[0]["source"]["schedule"], "half_hourly");
    let next = sources[0]["next_at"].as_i64().unwrap();
    assert!((before + 1800..=now() + 1800).contains(&next));
    assert_eq!(sources[1]["source"]["schedule"], "every_twelve_hours");
    assert_eq!(sources[1]["next_at"], saved_second_run);
    service.finish().await.unwrap();
}

#[tokio::test]
async fn scheduler_handles_an_overdue_source_once_and_records_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let service = Service::open(temp.path().join("data")).unwrap();
    let mut source = example(&temp.path().join("files"));
    source.connection.address = "http://127.0.0.1/private-feed".into();
    source.schedule = Schedule::Hourly;
    {
        let db = service.store.connection();
        db.execute(
            "INSERT INTO online_sources VALUES(?1,?2,?3)",
            params![
                source.id,
                serde_json::to_string(&source).unwrap(),
                now() - 7200
            ],
        )
        .unwrap();
        db.execute(
            "INSERT INTO online_runs(source,started,result) VALUES(?1,?2,?3)",
            params![
                source.id,
                now() - 7200,
                serde_json::to_string(&Report {
                    status: "running".into(),
                    ..Report::default()
                })
                .unwrap()
            ],
        )
        .unwrap();
    }
    let scheduler = tokio::spawn(schedule(service.clone()));
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if history(&service.store, &source.id)
                .unwrap()
                .first()
                .is_some_and(|run| run["result"]["status"] == "failed")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let rows = history(&service.store, &source.id).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["result"]["status"], "interrupted");
    assert!(self::source(&service.store, &source.id).unwrap().1.unwrap() > now());
    service.stop.cancel();
    scheduler.await.unwrap();
    service.finish().await.unwrap();
}
