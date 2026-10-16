use std::{fs, io::{BufRead, BufReader, Read, Write}, net::TcpListener, process::{Command, Stdio}, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};

#[test]
fn standalone_cli_indexes_refreshes_and_removes_without_a_desktop() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let root = temp.path().join("project");
    fs::create_dir_all(root.join("ignored")).unwrap();
    fs::write(root.join("normal.md"), "# Release\nRelease checks require a signed checklist.\n").unwrap();
    fs::write(root.join("keep.private.md"), "# Release\nRelease checks require an archive checksum.\n").unwrap();
    fs::write(root.join("ignored/secret.md"), "# Release\nHidden release instructions.\n").unwrap();
    fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
    fs::write(root.join(".sagignore"), "*.private.md\n!keep.private.md\n").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/decisions", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let server = std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            let (mut stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => { std::thread::sleep(Duration::from_millis(5)); continue; },
                Err(e) => panic!("{e}"),
            };
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" { break; }
                if let Some(value) = line.to_lowercase().strip_prefix("content-length:") { length = value.trim().parse().unwrap(); }
            }
            let mut body = vec![0; length]; reader.read_exact(&mut body).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let answers: serde_json::Map<_, _> = request["questions"].as_object().unwrap().keys()
                .map(|key| (key.clone(), serde_json::json!({"type":"noul","noul":0.9}))).collect();
            let body = serde_json::json!({"answers":answers}).to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    let command = || {
        let mut c = Command::new(env!("CARGO_BIN_EXE_sag"));
        c.env("SAG_DATA_DIR", &data).current_dir(&root);
        c
    };
    let run = |args: &[&str]| {
        let output = command().args(args).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    };
    let mut config = command().args(["config", "--url", &url, "--model", "test", "--key-stdin"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    config.stdin.take().unwrap().write_all(b"test-key\n").unwrap();
    assert!(config.wait_with_output().unwrap().status.success());
    let add: serde_json::Value = serde_json::from_str(&run(&["add", "--json"])).unwrap();
    assert_eq!(add["index_status"], "ready");
    assert!(run(&["status"]).contains("独立模式"));
    let search = || -> serde_json::Value { serde_json::from_str(&run(&["release checks", "-c", "--json"])).unwrap() };
    let initial = search();
    assert!(initial["results"].as_array().unwrap().iter().any(|r| r["path"] == "./normal.md"));
    assert!(initial["results"].as_array().unwrap().iter().any(|r| r["path"] == "./keep.private.md"));
    assert!(!initial.to_string().contains("secret.md"));
    fs::write(root.join(".sagignore"), "normal.md\n").unwrap();
    let changed = search();
    assert!(!changed.to_string().contains("normal.md"));
    fs::write(root.join("keep.private.md"), "# Release\nRelease checks now require a second checksum.\n").unwrap();
    assert!(search().to_string().contains("second checksum"));
    run(&["remove", "--json"]);
    assert!(root.join("normal.md").exists());
    assert!(run(&["status"]).contains("当前范围有索引：false"));
    stopped.store(true, Ordering::SeqCst); server.join().unwrap();
}

#[cfg(unix)]
#[test]
fn standalone_timeout_includes_blocked_index_refresh_and_releases_the_core() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let root = temp.path().join("project");
    fs::create_dir_all(&data).unwrap();
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("file.md"), "test content").unwrap();
    let store = sag_core::store::Store::open(&data.join("sag.sqlite")).unwrap();
    store.add_root(&root).unwrap();
    drop(store);
    // A FIFO makes reading the ignore rules block until the process deadline.
    let rule = root.join(".sagignore");
    assert!(Command::new("mkfifo").arg(&rule).status().unwrap().success());
    for (args, code) in [(vec!["test", "--timeout", "1"], 1), (vec!["-F", "test", "--timeout", "1"], 1), (vec!["-F", "test", "-q", "--timeout", "1"], 2)] {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", &data).current_dir(&root)
        .args(args).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(4);
    while child.try_wait().unwrap().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    if child.try_wait().unwrap().is_none() { child.kill().unwrap(); }
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(code));
    assert!(String::from_utf8_lossy(&output.stderr).contains("请求超时"));
    }
    fs::remove_file(rule).unwrap();
    assert!(Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", &data).arg("status").output().unwrap().status.success());
}

#[test]
fn standalone_add_reports_unreadable_documents_without_requiring_desktop() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("broken.xml"), b"<?xml version=\"1.0\" encoding=\"UNKNOWN-ENCODING\"?><root>broken</root>").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", temp.path().join("data")).current_dir(&root)
        .arg("add").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("broken.xml"));
    assert!(root.join("broken.xml").exists());
}
