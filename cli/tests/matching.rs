mod support;
use std::{fs, path::Path, process::{Command, Output}};

#[test]
fn fixed_and_regex_modes_match_lines_without_model_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let _cleanup = support::ServiceCleanup(dir.path().join("data"));
    let root = dir.path().join("项目 space");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/sample.txt"), "a.b\r\naxb\r\na.b a.b\r\nERROR 42\r\nwarn 7\r\nerror 2\r\n").unwrap();
    let run = |args: &[&str]| Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", dir.path().join("data")).current_dir(&root).args(args).output().unwrap();
    let json = |output: Output| -> serde_json::Value {
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        serde_json::from_slice(&output.stdout).unwrap()
    };
    assert!(run(&["add"]).status.success());
    let fixed = json(run(&["-F", "a.b", "-c", "--json"]));
    let results = fixed["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(Path::new(results[0]["path"].as_str().unwrap()), Path::new("./src/sample.txt"));
    assert_eq!(results[0]["start_line"], 1);
    assert_eq!(results[0]["end_line"], 1);
    assert_eq!(results[0]["content"], "a.b");
    assert_eq!(results[1]["start_line"], 3);
    let regex = json(run(&["-E", r"^(ERROR|warn) \d+$", "src", "--json"]));
    assert_eq!(regex["results"].as_array().unwrap().len(), 2);
    assert_eq!(regex["results"][0]["start_line"], 4);
    assert_eq!(regex["results"][1]["start_line"], 5);
    assert!(regex["results"][0].get("content").is_none());
    let limited = json(run(&["--fixed-strings", "a.b", "--path", "src", "-m", "1", "--json"]));
    assert_eq!(limited["results"].as_array().unwrap().len(), 1);
    let missing = json(run(&["--extended-regexp", "^missing$", "--json"]));
    assert!(missing["results"].as_array().unwrap().is_empty());
    assert_eq!(run(&["-E", "-F", "a.b"]).status.code(), Some(2));
    let invalid = run(&["-E", "["]);
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("正则"));
    let semantic = run(&["a.b"]);
    assert!(!semantic.status.success());
    assert!(String::from_utf8_lossy(&semantic.stderr).contains("JEV 未配置"));
}

#[test]
fn local_matches_follow_ignore_rules_and_removed_subdirectories() {
    let dir = tempfile::tempdir().unwrap();
    let _cleanup = support::ServiceCleanup(dir.path().join("data"));
    let root = dir.path().join("project");
    fs::create_dir_all(root.join("hidden/deep")).unwrap();
    fs::create_dir(root.join("removed")).unwrap();
    for name in ["visible.txt", "hidden/deep/keep.txt", "hidden/deep/secret.txt", "removed/file.txt"] {
        fs::write(root.join(name), "needle\n").unwrap();
    }
    fs::write(root.join(".gitignore"), "hidden/\n").unwrap();
    fs::write(root.join(".sagignore"), "!hidden/\nhidden/*\n!hidden/deep/\nhidden/deep/*\n!hidden/deep/keep.txt\n").unwrap();
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_sag"))
            .env("SAG_DATA_DIR", dir.path().join("data")).current_dir(&root).args(args).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        output
    };
    run(&["add"]);
    run(&["remove", "removed"]);
    for flag in ["-E", "-F"] {
        let output = run(&[flag, "needle", "--json"]);
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let paths: Vec<_> = json["results"].as_array().unwrap().iter().map(|r| Path::new(r["path"].as_str().unwrap())).collect();
        assert_eq!(paths, [Path::new("./hidden/deep/keep.txt"), Path::new("./visible.txt")]);
    }
    run(&["add", "removed"]);
    let restored = run(&["-F", "needle", "removed", "-c"]);
    assert_eq!(String::from_utf8(restored.stdout).unwrap(), format!("./{}:1-1\nneedle\n\n", Path::new("removed").join("file.txt").display()));
    fs::write(root.join(".sagignore"), "visible.txt\n").unwrap();
    let refreshed = run(&["-F", "needle", "--json"]);
    assert!(!String::from_utf8(refreshed.stdout).unwrap().contains("visible.txt"));
}

#[test]
fn line_anchors_and_whitespace_keep_real_line_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let _cleanup = support::ServiceCleanup(dir.path().join("data"));
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("lines.txt"), "\r\n \r\nvalue\r\n").unwrap();
    fs::write(root.join("empty.txt"), "").unwrap();
    let run = |args: &[&str]| {
        let result = Command::new(env!("CARGO_BIN_EXE_sag"))
            .env("SAG_DATA_DIR", dir.path().join("data")).current_dir(&root).args(args).output().unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap()
    };
    let output = Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", dir.path().join("data")).current_dir(&root).arg("add").output().unwrap();
    assert!(output.status.success());
    let blank = run(&["-E", "^$", "--json", "-c"]);
    assert_eq!(blank["results"].as_array().unwrap().len(), 1);
    assert_eq!(blank["results"][0]["path"], "./lines.txt");
    assert_eq!(blank["results"][0]["start_line"], 1);
    assert_eq!(blank["results"][0]["content"], "");
    let space = run(&["-F", " ", "--json"]);
    assert_eq!(space["results"].as_array().unwrap().len(), 1);
    assert_eq!(space["results"][0]["start_line"], 2);
}

#[test]
fn local_matching_marks_oversized_content_without_breaking_json() {
    let dir = tempfile::tempdir().unwrap();
    let _cleanup = support::ServiceCleanup(dir.path().join("data"));
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("large.txt"), format!("needle {}\n", "x".repeat(600_000))).unwrap();
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_sag"))
            .env("SAG_DATA_DIR", dir.path().join("data")).current_dir(&root).args(args).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        output
    };
    run(&["add"]);
    let content = run(&["-F", "needle", "-c", "--json"]);
    let output: serde_json::Value = serde_json::from_slice(&content.stdout).unwrap();
    assert_eq!(output["truncated"], true);
    assert!(output["results"].as_array().unwrap().is_empty());
    assert!(String::from_utf8_lossy(&content.stderr).contains("长度上限"));
    let locations = run(&["-F", "needle", "--json"]);
    let output: serde_json::Value = serde_json::from_slice(&locations.stdout).unwrap();
    assert_eq!(output["truncated"], false);
    assert_eq!(output["results"].as_array().unwrap().len(), 1);
}
