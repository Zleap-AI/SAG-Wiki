mod support;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

struct Project {
    _cleanup: support::ServiceCleanup,
    _dir: tempfile::TempDir,
    root: PathBuf,
    data: PathBuf,
}
impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("项目 space");
        fs::create_dir(&root).unwrap();
        for (name, contents) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let data = dir.path().join("data");
        let project = Self {
            _cleanup: support::ServiceCleanup(data.clone()),
            _dir: dir,
            root,
            data,
        };
        assert!(project.run(&["add"]).status.success());
        project
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_sag"))
            .current_dir(&self.root)
            .env("SAG_DATA_DIR", &self.data)
            .args(args)
            .output()
            .unwrap()
    }
    fn json(&self, args: &[&str]) -> serde_json::Value {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

#[test]
fn matching_controls_preserve_case_words_lines_and_inversion() {
    let p = Project::new(&[(
        "sample.txt",
        "cat\nCat\nconcatenate\ncat cat\n@\n@@\nab\n\n",
    )]);
    let lines = |args: &[&str]| {
        p.json(args)["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["start_line"].as_u64().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(lines(&["-F", "-i", "cat", "--json"]), [1, 2, 3, 4]);
    assert_eq!(lines(&["-F", "-w", "cat", "--json"]), [1, 4]);
    assert_eq!(lines(&["-F", "-x", "cat", "--json"]), [1]);
    assert_eq!(
        lines(&["-F", "-v", "-x", "cat", "--json"]),
        [2, 3, 4, 5, 6, 7, 8]
    );
    assert_eq!(lines(&["-F", "-w", "@", "--json"]), [5, 6]);
    assert_eq!(lines(&["-E", "-w", "a|ab", "--json"]), [7]);
    assert_eq!(p.run(&["-i", "cat"]).status.code(), Some(2));
}

#[test]
fn context_merges_adjacent_groups_and_reports_matching_lines() {
    let p = Project::new(&[(
        "sample.txt",
        "zero\r\nneedle one\r\ntwo\r\nneedle three\r\nfour\r\nfive\r\n",
    )]);
    let output = p.json(&["-F", "needle", "-C", "1", "--json"]);
    let rows = output["results"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["start_line"], 1);
    assert_eq!(rows[0]["end_line"], 5);
    assert_eq!(rows[0]["matched_lines"], serde_json::json!([2, 4]));
    assert_eq!(
        rows[0]["content"],
        "zero\r\nneedle one\r\ntwo\r\nneedle three\r\nfour"
    );
    let output = p.json(&["-F", "needle", "-A", "2", "-B", "0", "-m", "1", "--json"]);
    assert_eq!(output["results"][0]["start_line"], 2);
    assert_eq!(output["results"][0]["end_line"], 4);
    assert_eq!(
        output["results"][0]["matched_lines"],
        serde_json::json!([2])
    );
}

#[test]
fn output_modes_list_count_extract_and_test_existence() {
    let p = Project::new(&[("a.txt", "cat cat\nCat\n"), ("b.txt", "dog\n")]);
    let files = p.run(&["-F", "cat", "-l"]);
    assert!(files.status.success());
    assert_eq!(String::from_utf8(files.stdout).unwrap(), "./a.txt\n");
    let count = p.json(&["-F", "cat", "--count", "--json"]);
    assert_eq!(count["files"][0]["count"], 1);
    assert_eq!(count["files"][1]["count"], 0);
    let parts = p.json(&["-F", "cat", "-o", "--json"]);
    assert_eq!(parts["results"].as_array().unwrap().len(), 2);
    assert_eq!(parts["results"][0]["content"], "cat");
    assert_eq!(parts["results"][1]["content"], "cat");
    let found = p.run(&["-F", "cat", "-q"]);
    assert_eq!(found.status.code(), Some(0));
    assert!(found.stdout.is_empty() && found.stderr.is_empty());
    let missing = p.run(&["-F", "absent", "-q"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(missing.stdout.is_empty() && missing.stderr.is_empty());
    assert_eq!(p.run(&["-E", "[", "-q"]).status.code(), Some(2));
    assert_eq!(p.run(&["-F", "cat", "-o", "-v"]).status.code(), Some(2));
    assert_eq!(
        p.run(&["-F", "cat", "-l", "--count"]).status.code(),
        Some(2)
    );
    assert_eq!(p.run(&["-F", "cat", "-q", "--json"]).status.code(), Some(2));
    fs::write(p.root.join("a.txt"), "cat\n".repeat(25)).unwrap();
    let all = p.json(&["-F", "cat", "--count", "--json"]);
    assert_eq!(all["files"][0]["count"], 25);
}

#[test]
fn multiple_patterns_and_ordered_file_filters_share_one_search() {
    let p = Project::new(&[
        ("a.txt", "cat\ndog\n"),
        ("b.txt", "dog\n"),
        ("src/code.rs", "cat\n"),
        ("patterns", "cat\ndog\n"),
    ]);
    assert!(p.run(&["add", "--types", "txt,rs"]).status.success());
    let multiple = p.json(&[
        "-F",
        "-e",
        "cat",
        "-e",
        "dog",
        "--include",
        "*.txt",
        "--json",
    ]);
    assert_eq!(multiple["results"].as_array().unwrap().len(), 3);
    let file = p.json(&["-F", "-f", "patterns", "src", "--json"]);
    assert_eq!(file["results"].as_array().unwrap().len(), 1);
    assert_eq!(file["results"][0]["path"], "./src/code.rs");
    let excluded = p.json(&["-F", "dog", "--include=*.txt", "--exclude=a*", "--json"]);
    assert_eq!(excluded["results"].as_array().unwrap().len(), 1);
    assert_eq!(excluded["results"][0]["path"], "./b.txt");
    let restored = p.json(&["-F", "cat", "--exclude=*.txt", "--include=a.txt", "--json"]);
    assert_eq!(restored["results"].as_array().unwrap().len(), 2);
    assert_eq!(restored["results"][0]["path"], "./a.txt");
    fs::write(p.root.join("empty-patterns"), "").unwrap();
    let empty = p.json(&["-F", "-f", "empty-patterns", "--json"]);
    assert!(empty["results"].as_array().unwrap().is_empty());
    let empty_word = p.json(&["-F", "-w", "-e", "", "--json"]);
    assert!(empty_word["results"].as_array().unwrap().is_empty());
    assert_eq!(p.run(&["-F", "-f", "missing"]).status.code(), Some(1));
    assert_eq!(
        p.run(&["-F", "cat", "--include", "["]).status.code(),
        Some(1)
    );
}

#[test]
fn stdin_patterns_empty_matches_and_word_fragments_are_unambiguous() {
    let p = Project::new(&[("a.txt", "@@\n猫 猫\nCAT\n")]);
    let mut child = Command::new(env!("CARGO_BIN_EXE_sag"))
        .current_dir(&p.root)
        .env("SAG_DATA_DIR", &p.data)
        .args(["-F", "-f", "-", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all("猫\nCAT\n".as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output["results"].as_array().unwrap().len(), 2);
    let punctuation = p.json(&["-F", "-w", "-o", "@", "--json"]);
    assert_eq!(punctuation["results"].as_array().unwrap().len(), 2);
    let unicode = p.json(&["-F", "-w", "-o", "猫", "--json"]);
    assert_eq!(unicode["results"].as_array().unwrap().len(), 2);
    let empty = p.json(&["-E", "-o", "-e", "", "--json"]);
    assert!(empty["matched"].as_bool().unwrap());
    assert!(empty["results"].as_array().unwrap().is_empty());
    let multi = p.json(&["-F", "-e", "@", "-e", "@@", "-o", "--json"]);
    assert_eq!(multi["results"].as_array().unwrap().len(), 1);
    assert_eq!(multi["results"][0]["content"], "@@");
}

#[test]
fn regex_comments_and_inline_case_options_survive_match_controls() {
    let p = Project::new(&[("sample.txt", "cat\nconcatenate\nCat\n#\n")]);
    for (flags, count) in [(vec![], 2), (vec!["-w"], 1), (vec!["-x"], 1)] {
        let mut args = vec!["-E", "(?x) cat # trailing comment", "--json"];
        args.extend(flags);
        let output = p.json(&args);
        assert_eq!(output["results"].as_array().unwrap().len(), count);
    }
    let inline_case = p.json(&["-E", "-i", "(?-i:cat)", "--json"]);
    assert_eq!(inline_case["results"].as_array().unwrap().len(), 2);
    let hash = p.json(&["-E", r"(?x) [\#] # literal hash", "--json"]);
    assert_eq!(hash["results"].as_array().unwrap().len(), 1);
}
