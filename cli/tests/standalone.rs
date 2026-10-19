use std::{fs, io::{BufRead, BufReader, Read, Write}, net::TcpListener, process::{Command, Stdio}, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};

struct ServiceCleanup(std::path::PathBuf);
impl Drop for ServiceCleanup {
    fn drop(&mut self) {
        if self.0.join("sag-service.json").exists() {
            let output=Command::new(env!("CARGO_BIN_EXE_sag")).env("SAG_DATA_DIR",&self.0).args(["service","stop"]).output().unwrap();
            if !output.status.success(){eprintln!("service cleanup: {}",String::from_utf8_lossy(&output.stderr));}
        }
    }
}

#[test]
fn twenty_standalone_processes_search_the_same_index_concurrently() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let _cleanup=ServiceCleanup(data.clone());
    let root = temp.path().join("project");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("release.md"), "Release requires a signed checklist.").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    fs::create_dir(&data).unwrap();
    let store = sag_core::store::Store::open(&data.join("sag.sqlite")).unwrap();
    let registered = store.add_root(&root).unwrap();
    store.scan(&registered, |_| {}).unwrap();
    let mut settings = store.settings().unwrap();
    settings.jev.url = format!("http://{}/decisions", listener.local_addr().unwrap());
    settings.retrieval.max_hops = 0;
    store.save_settings(&settings).unwrap();
    sag_core::models::save_secret(&data, "jev", "test-only").unwrap();
    drop(store);
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let mut requests = vec![];
        while requests.len() < 20 && std::time::Instant::now() < deadline {
            let (stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => { std::thread::sleep(Duration::from_millis(5)); continue; }
                Err(e) => panic!("{e}"),
            };
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" { break; }
                if let Some(value) = line.to_lowercase().strip_prefix("content-length:") { length = value.trim().parse().unwrap(); }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            requests.push((stream, payload));
        }
        let count = requests.len();
        for (mut stream, payload) in requests {
            let answers: serde_json::Map<_, _> = payload["questions"].as_object().unwrap().keys()
                .map(|key| (key.clone(), serde_json::json!({"type":"noul","noul":0.9}))).collect();
            let body = serde_json::json!({"answers":answers}).to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
        count
    });
    let children: Vec<_> = (0..20).map(|_| Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", &data).current_dir(&root)
        .args(["What is required before a release?", "-c", "--json", "--timeout", "15"])
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap()).collect();
    let concurrent = server.join().unwrap();
    let outputs: Vec<_> = children.into_iter().map(|child| child.wait_with_output().unwrap()).collect();
    assert_eq!(concurrent, 20, "twenty searches must reach the model before any receives a response");
    for output in outputs {
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["results"][0]["content"], "Release requires a signed checklist.");
    }
}

#[test]
fn standalone_cli_indexes_refreshes_and_removes_without_a_desktop() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let _cleanup=ServiceCleanup(data.clone());
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
    assert!(run(&["status"]).contains("已连接 SAG 后台服务"));
    let search = || -> serde_json::Value { serde_json::from_str(&run(&["release checks", "-c", "--json"])).unwrap() };
    let initial = search();
    assert!(initial["results"].as_array().unwrap().iter().any(|r| r["path"] == "./normal.md"));
    assert!(initial["results"].as_array().unwrap().iter().any(|r| r["path"] == "./keep.private.md"));
    assert!(!initial.to_string().contains("secret.md"));
    fs::write(root.join(".sagignore"), "normal.md\n").unwrap();
    let deadline=std::time::Instant::now()+Duration::from_secs(5);
    loop {
        let output=command().args(["release checks","-c","--json"]).output().unwrap();
        if output.status.success()&&!String::from_utf8_lossy(&output.stdout).contains("normal.md"){break;}
        assert!(std::time::Instant::now()<deadline,"ignore change must reach the shared index");
        std::thread::sleep(Duration::from_millis(30));
    }
    fs::write(root.join("keep.private.md"), "# Release\nRelease checks now require a second checksum.\n").unwrap();
    let deadline=std::time::Instant::now()+Duration::from_secs(5);
    loop {
        let output=command().args(["release checks","-c","--json"]).output().unwrap();
        if output.status.success()&&String::from_utf8_lossy(&output.stdout).contains("second checksum"){break;}
        assert!(std::time::Instant::now()<deadline,"file change must reach the shared index");
        std::thread::sleep(Duration::from_millis(30));
    }
    run(&["remove", "--json"]);
    assert!(root.join("normal.md").exists());
    let check = command().args(["check", "--json"]).output().unwrap();
    assert_eq!(check.status.code(), Some(1));
    let check: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(check["coverage"], "none");
    stopped.store(true, Ordering::SeqCst); server.join().unwrap();
}

#[test]
fn status_does_not_start_a_service_and_later_commands_reuse_one_process(){
    let temp=tempfile::tempdir().unwrap();let data=temp.path().join("data");let _cleanup=ServiceCleanup(data.clone());
    let status=Command::new(env!("CARGO_BIN_EXE_sag")).env("SAG_DATA_DIR",&data).arg("status").output().unwrap();
    assert!(status.status.success());assert!(!data.join("sag-service.json").exists());
    let root=temp.path().join("notes");fs::create_dir(&root).unwrap();fs::write(root.join("note.md"),"needle").unwrap();
    let added=Command::new(env!("CARGO_BIN_EXE_sag")).env("SAG_DATA_DIR",&data).args(["add",root.to_str().unwrap()]).output().unwrap();
    assert!(added.status.success(),"{}",String::from_utf8_lossy(&added.stderr));
    let endpoint=fs::read(data.join("sag-service.json")).unwrap();
    for _ in 0..3 {
        let result=Command::new(env!("CARGO_BIN_EXE_sag")).env("SAG_DATA_DIR",&data).current_dir(&root).args(["-F","needle"]).output().unwrap();
        assert!(result.status.success());assert_eq!(fs::read(data.join("sag-service.json")).unwrap(),endpoint);
    }
    let stopped=Command::new(env!("CARGO_BIN_EXE_sag")).env("SAG_DATA_DIR",&data).args(["service","stop"]).output().unwrap();
    assert!(stopped.status.success(),"{}",String::from_utf8_lossy(&stopped.stderr));assert!(!data.join("sag-service.json").exists());
}

#[test]
fn standalone_add_reports_unreadable_documents_without_requiring_desktop() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("broken.xml"), b"<?xml version=\"1.0\" encoding=\"UNKNOWN-ENCODING\"?><root>broken</root>").unwrap();
    let _cleanup=ServiceCleanup(temp.path().join("data"));
    let store = sag_core::store::Store::open(&temp.path().join("data/sag.sqlite")).unwrap();
    let mut settings = store.settings().unwrap();
    settings.file_types.config = true;
    store.save_settings(&settings).unwrap();
    drop(store);
    let output = Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", temp.path().join("data")).current_dir(&root)
        .arg("add").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("broken.xml"));
    assert!(root.join("broken.xml").exists());
}
