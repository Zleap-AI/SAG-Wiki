use std::process::Command;
#[test]
fn invalid_arguments_are_distinct_from_unavailable_desktop() {
    let home = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sag"))
            .env("SAG_DESKTOP_DATA_DIR", home.path())
            .args(args)
            .output()
            .unwrap()
    };
    assert_eq!(
        run(&["search", "question", "src", "--path", "other"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        run(&["search", "question", "-m", "0"]).status.code(),
        Some(2)
    );
    let missing = run(&["status"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(missing.stdout.is_empty());
    assert!(String::from_utf8(missing.stderr)
        .unwrap()
        .contains("SAG-Desktop"));
}

#[test]
fn cli_formats_json_and_text_without_changing_locations_or_query_arguments() {
    use std::{
        fs,
        io::{BufRead, BufReader, Write},
        net::TcpListener,
    };
    let home = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    fs::write(
        home.path().join("cli-endpoint.json"),
        format!(
            r#"{{"protocol_version":1,"port":{},"pid":1,"token":"test-token"}}"#,
            listener.local_addr().unwrap().port()
        ),
    )
    .unwrap();
    let cwd = home.path().join("项目 space");
    fs::create_dir_all(cwd.join("src")).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    let path = cwd.join("src/main.rs");
    let server_path = path.clone();
    let query = "Where is 'quoted' permission\nvalidated?";
    let server = std::thread::spawn(move || {
        for content in [false, true, true] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            let request: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["command"]["query"], query);
            assert_eq!(request["command"]["content"], content);
            assert!(request["path"]
                .as_str()
                .unwrap()
                .ends_with("项目 space/src"));
            let mut item = serde_json::json!({"path":"./main.rs","absolute_path":server_path,"start_line":2,"end_line":4});
            if content {
                item["content"] = serde_json::json!("原文 'quoted'\nsecond line");
            }
            let reply = serde_json::json!({"type":"search","data":{"protocol_version":1,"scope":server_path.parent(),"results":[item],"truncated":false}});
            writeln!(stream, "{reply}").unwrap();
        }
    });
    let run = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sag"))
            .env("SAG_DESKTOP_DATA_DIR", home.path())
            .current_dir(&cwd)
            .args(["search", query, "src"])
            .args(extra)
            .output()
            .unwrap()
    };
    let plain = run(&[]);
    assert!(plain.status.success());
    assert_eq!(
        String::from_utf8(plain.stdout).unwrap(),
        "./src/main.rs:2-4\n"
    );
    let content = run(&["-c"]);
    assert!(content.status.success());
    assert!(String::from_utf8(content.stdout)
        .unwrap()
        .contains("原文 'quoted'\nsecond line"));
    let json = run(&["-c", "--json"]);
    assert!(json.status.success());
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value["results"][0]["path"], "./src/main.rs");
    assert_eq!(value["results"][0]["absolute_path"], path.to_str().unwrap());
    server.join().unwrap();
}

#[test]
fn factory_droid_is_not_a_supported_installation_target() {
    for command in ["install", "uninstall"] {
        let result = Command::new(env!("CARGO_BIN_EXE_sag"))
            .args([command, "factory-droid"])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        let help = Command::new(env!("CARGO_BIN_EXE_sag"))
            .args([command, "--help"])
            .output()
            .unwrap();
        assert!(!String::from_utf8(help.stdout)
            .unwrap()
            .contains("factory-droid"));
    }
}

#[test]
fn folder_commands_default_to_current_directory_and_accept_an_explicit_path() {
    use std::{
        fs,
        io::{BufRead, BufReader, Write},
        net::TcpListener,
    };
    for (command, response_type, message) in [("add", "added", "已加入知识库"), ("remove", "removed", "已从知识库移除")] {
    let data = tempfile::tempdir().unwrap();
    let cwd = data.path().join("项目 space");
    fs::create_dir_all(cwd.join("child")).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    fs::write(data.path().join("cli-endpoint.json"), serde_json::json!({"protocol_version":1,"port":listener.local_addr().unwrap().port(),"pid":1,"token":"test-token"}).to_string()).unwrap();
    listener.set_nonblocking(true).unwrap();
    let server_cwd = cwd.clone();
    let server = std::thread::spawn(move || {
        for suffix in ["", "child"] {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(std::time::Duration::from_millis(10))
                    }
                    Err(e) => panic!("CLI did not send add request: {e}"),
                }
            };
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            let request: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["command"]["command"], command);
            assert_eq!(
                std::path::Path::new(request["path"].as_str().unwrap()),
                server_cwd.join(suffix)
            );
            let mut data = serde_json::json!({"path":server_cwd.join(suffix)});
            if command == "add" { data["index_status"] = "pending".into(); data["cli_authorized"] = true.into(); }
            writeln!(stream,"{}",serde_json::json!({"type":response_type,"data":data})).unwrap();
        }
    });
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sag"))
            .current_dir(&cwd)
            .env("SAG_DESKTOP_DATA_DIR", data.path())
            .args(args)
            .output()
            .unwrap()
    };
    let result = run(&[command]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains(message));
    let result = run(&[command, "child", "--json"]);
    assert!(result.status.success());
    let response: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(response["path"], cwd.join("child").to_str().unwrap());
    server.join().unwrap();
    }
}
