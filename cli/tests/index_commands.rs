#[allow(dead_code)]
mod support;

use serde_json::Value;
use std::{fs, process::Command, time::{Duration, Instant}};

#[test]
fn list_and_check_distinguish_parent_coverage_exclusions_and_empty_directories() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let _cleanup = support::ServiceCleanup(data.clone());
    let store = sag_core::store::Store::open(&data.join("sag.sqlite")).unwrap();
    let mut settings = store.settings().unwrap();
    settings.file_types.code = true;
    store.save_settings(&settings).unwrap();
    drop(store);
    let project = temp.path().join("项目 space");
    for directory in ["src/nested", "ignored", "private", "removed", "empty", ".hidden"] {
        fs::create_dir_all(project.join(directory)).unwrap();
    }
    for file in ["src/main.rs", "src/nested/helper.rs", "ignored/note.md", "private/note.md", "removed/note.md", ".hidden/note.md"] {
        fs::write(project.join(file), "Searchable fixture content.\n").unwrap();
    }
    fs::write(project.join(".gitignore"), "ignored/\n").unwrap();
    fs::write(project.join(".sagignore"), "private/\n").unwrap();
    let project = project.canonicalize().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sag"))
            .env("SAG_DATA_DIR", &data).current_dir(&project).args(args).output().unwrap()
    };
    let empty = run(&["list", "--json"]);
    assert!(empty.status.success(), "{}", String::from_utf8_lossy(&empty.stderr));
    assert_eq!(serde_json::from_slice::<Value>(&empty.stdout).unwrap(), serde_json::json!([]));
    let status_before = run(&["status"]);
    assert!(status_before.status.success());
    let status_text = String::from_utf8(status_before.stdout.clone()).unwrap();
    assert!(status_text.contains("服务版本："));
    assert!(status_text.contains("CLI 接入：true"));
    assert!(status_text.contains("JEV 已配置：false"));
    assert!(!status_text.contains("索引"));
    assert!(!status_text.contains("目录："));
    let missing = run(&["check", "--json"]);
    assert_eq!(missing.status.code(), Some(1));
    assert_eq!(serde_json::from_slice::<Value>(&missing.stdout).unwrap()["coverage"], "none");

    assert!(run(&["add", "src"]).status.success());
    let status_after = Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", &data).current_dir(project.join("src"))
        .arg("status").output().unwrap();
    assert!(status_after.status.success());
    assert_eq!(status_before.stdout, status_after.stdout);
    let partial = run(&["check", "--json"]);
    assert_eq!(partial.status.code(), Some(1));
    let partial: Value = serde_json::from_slice(&partial.stdout).unwrap();
    assert_eq!(partial["coverage"], "partial");
    assert_eq!(partial["indexed_files"], 2);
    let partial_text = run(&["check"]);
    assert!(String::from_utf8(partial_text.stdout).unwrap().contains("仅部分子目录已加入索引"));
    for directory in ["src", "src/nested"] {
        let checked = run(&["check", directory, "--json"]);
        assert!(checked.status.success(), "{}", String::from_utf8_lossy(&checked.stderr));
        let checked: Value = serde_json::from_slice(&checked.stdout).unwrap();
        assert_eq!(checked["coverage"], "included");
        assert_eq!(checked["index_ready"], true);
        assert_eq!(checked["enabled"], true);
    }
    let listed = run(&["list", "--json"]);
    assert!(listed.status.success());
    let listed: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["path"], project.join("src").to_str().unwrap());
    assert_eq!(listed[0]["count"], 2);
    assert_eq!(listed[0]["status"], "ready");
    assert!(String::from_utf8(run(&["list"]).stdout).unwrap().contains("已就绪\t2 个文件"));

    assert!(run(&["add"]).status.success());
    assert!(run(&["remove", "removed"]).status.success());
    for directory in ["ignored", "private", "removed", ".hidden"] {
        let checked = run(&["check", directory, "--json"]);
        assert_eq!(checked.status.code(), Some(1), "{directory}");
        let checked: Value = serde_json::from_slice(&checked.stdout).unwrap();
        assert_eq!(checked["coverage"], "excluded", "{directory}");
        assert_eq!(checked["indexed_files"], 0);
    }
    let empty_directory = run(&["check", "empty", "--json"]);
    assert_eq!(empty_directory.status.code(), Some(1));
    let checked: Value = serde_json::from_slice(&empty_directory.stdout).unwrap();
    assert_eq!(checked["coverage"], "included");
    assert_eq!(checked["indexed_files"], 0);
    let sibling = project.with_file_name("项目 space-other");
    fs::create_dir(&sibling).unwrap();
    let outside = run(&["check", sibling.to_str().unwrap(), "--json"]);
    assert_eq!(outside.status.code(), Some(1));
    assert_eq!(serde_json::from_slice::<Value>(&outside.stdout).unwrap()["coverage"], "none");
    let invalid = run(&["check", "src/main.rs"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8(invalid.stderr).unwrap().contains("必须是目录"));
}

#[test]
fn add_selects_extensions_persists_them_and_replaces_only_the_requested_scope() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let _cleanup = support::ServiceCleanup(data.clone());
    let project = temp.path().join("项目 formats");
    fs::create_dir_all(project.join("child")).unwrap();
    let sources = [
        ("readme.MD", "format fixture markdown"),
        ("main.py", "format fixture python"),
        ("main.js", "format fixture javascript"),
        ("settings.json", "{\"fixture\":\"format fixture json\"}"),
        ("page.html", "<p>format fixture html</p>"),
        ("child/main.ts", "format fixture typescript"),
        ("child/main.js", "format fixture child javascript"),
        ("ignored.ts", "format fixture ignored"),
    ];
    for (name, content) in sources { fs::write(project.join(name), content).unwrap(); }
    fs::write(project.join(".sagignore"), "# User rule\nignored.ts\n").unwrap();
    let run = |args: &[&str]| Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", &data).current_dir(&project).args(args).output().unwrap();
    let check = |directory: &str| {
        // Adding a root schedules background indexing; success does not imply readiness.
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let output = run(&["check", directory, "--json"]);
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            if output.status.success() { break value; }
            let indexing = output.status.code() == Some(1) && value["coverage"] == "included"
                && value["roots"].as_array().is_some_and(|roots| {
                    roots.iter().all(|root| root["error"].is_null())
                        && roots.iter().any(|root| matches!(root["status"].as_str(), Some("pending" | "indexing")))
                });
            assert!(indexing && Instant::now() < deadline,
                "index did not become ready: {value}; {}", String::from_utf8_lossy(&output.stderr));
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    let matches = || {
        let output = run(&["-F", "format fixture", ".", "-l", "--json"]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["files"].as_array().unwrap()
            .iter().map(|file| file["path"].as_str().unwrap().to_owned()).collect::<Vec<_>>()
    };
    for types in ["pdf", "md,unknown", "", "md,,py", "../md"] {
        let output = run(&["add", "--types", types]);
        assert!(!output.status.success(), "accepted {types}");
        assert_eq!(fs::read_to_string(project.join(".sagignore")).unwrap(), "# User rule\nignored.ts\n");
    }
    let missing = run(&["check", "--json"]);
    assert_eq!(serde_json::from_slice::<Value>(&missing.stdout).unwrap()["coverage"], "none");
    let added = run(&["add", "--types", ".MD,py,TS,md", "--json"]);
    assert!(added.status.success(), "{}", String::from_utf8_lossy(&added.stderr));
    assert_eq!(check(".")["extensions"], serde_json::json!(["md", "py", "ts"]));
    assert_eq!(check(".")["indexed_files"], 3);
    let files = matches();
    assert_eq!(files.len(), 3);
    assert!(files.iter().any(|path| path.ends_with("readme.MD")));
    assert!(files.iter().any(|path| path.ends_with("main.py")));
    assert!(files.iter().any(|path| path.ends_with("child/main.ts")));
    assert!(run(&["add"]).status.success());
    assert_eq!(check(".")["extensions"], serde_json::json!(["md", "py", "ts"]));
    assert!(String::from_utf8(run(&["check"]).stdout).unwrap().contains("原生索引格式：md,py,ts"));
    assert!(run(&["service", "stop"]).status.success());
    assert_eq!(check(".")["indexed_files"], 3);
    assert_eq!(check(".")["extensions"], serde_json::json!(["md", "py", "ts"]));

    // A child selection updates that subtree while retaining the parent's selection.
    let child = run(&["add", "child", "--types", "js"]);
    assert!(child.status.success(), "{}", String::from_utf8_lossy(&child.stderr));
    assert_eq!(check("child")["extensions"], serde_json::json!(["js"]));
    assert_eq!(check(".")["extensions"], serde_json::json!(["md", "py", "ts"]));
    let files = matches();
    assert_eq!(files.len(), 3);
    assert!(files.iter().any(|path| path.ends_with("child/main.js")));
    assert!(!files.iter().any(|path| path.ends_with("main.ts")));
    assert!(files.iter().any(|path| path.ends_with("main.py")));

    let replaced = run(&["add", "--types", "html,json"]);
    assert!(replaced.status.success(), "{}", String::from_utf8_lossy(&replaced.stderr));
    // The explicit child selection remains in effect.
    assert_eq!(check(".")["extensions"], serde_json::json!(["html", "json"]));
    assert_eq!(check(".")["indexed_files"], 3);
    let files = matches();
    assert!(files.iter().any(|path| path.ends_with("page.html")));
    assert!(files.iter().any(|path| path.ends_with("settings.json")));
    assert!(files.iter().any(|path| path.ends_with("child/main.js")));
    for (name, content) in sources { assert_eq!(fs::read_to_string(project.join(name)).unwrap(), content); }
    let rules = fs::read_to_string(project.join(".sagignore")).unwrap();
    assert!(rules.starts_with("# User rule\nignored.ts\n"));
    assert_eq!(rules.matches("# SAG Wiki index options: begin").count(), 1);
    assert!(run(&["service", "stop"]).status.success());
    let store = sag_core::store::Store::open(&data.join("sag.sqlite")).unwrap();
    let files = store.files().unwrap();
    assert_eq!(files.len(), 3);
    for name in ["page.html", "settings.json", "child/main.js"] {
        assert!(files.iter().any(|file| file.path.ends_with(name)), "missing index entry: {name}");
    }
}
