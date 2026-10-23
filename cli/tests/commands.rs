mod support;
use std::process::Command;
#[test]
fn config_saves_jev_without_desktop_and_never_prints_the_key() {
    use std::{io::Write, process::Stdio};
    let data = tempfile::tempdir().unwrap();
    let _cleanup=support::ServiceCleanup(data.path().to_path_buf());
    let mut child = Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", data.path())
        .args(["config", "--url", "https://example.com/decisions", "--model", "test-model", "--key-stdin"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(b"test-private-key\n").unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("test-private-key"));
    let shown = Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", data.path()).arg("config").output().unwrap();
    assert!(shown.status.success());
    let settings: serde_json::Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(settings["model"], "test-model");
    assert_eq!(settings["has_key"], true);
    assert!(!String::from_utf8_lossy(&shown.stdout).contains("test-private-key"));
}

#[test]
fn invalid_arguments_fail_but_status_works_without_desktop() {
    let home = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sag"))
            .env("SAG_DATA_DIR", home.path())
            .args(args)
            .output()
            .unwrap()
    };
    assert_eq!(
        run(&["question", "src", "--path", "other"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        run(&["question", "-m", "0"]).status.code(),
        Some(2)
    );
    for args in [&[][..], &["-c"][..], &[""][..]] {
        assert_eq!(run(args).status.code(), Some(2));
    }
    let missing = run(&["status"]);
    assert!(missing.status.success(), "{}", String::from_utf8_lossy(&missing.stderr));
    let status = String::from_utf8(missing.stdout).unwrap();
    assert!(status.contains("后台服务未运行"));
    assert!(!status.contains("索引"));
    assert!(!status.contains("目录："));
}

#[test]
fn cli_formats_json_and_text_without_changing_locations_or_query_arguments() {
    use std::{fs, path::Path};
    let home = tempfile::tempdir().unwrap();
    let cwd = home.path().join("项目 space");
    fs::create_dir_all(cwd.join("src")).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    #[cfg(windows)]
    assert!(matches!(cwd.components().next(), Some(std::path::Component::Prefix(prefix)) if prefix.kind().is_verbatim()));
    let path = cwd.join("src").join("main.rs");
    let server_path = path.clone();
    let query = "Where is 'quoted' permission\nvalidated?";
    let relative_path = format!("./{}", Path::new("src").join("main.rs").display());
    let mut step=0;
    let server=support::serve(home.path(),4,move|request|{
        let (expected_query,content)=[(query,true),(query,true),(query,true),("status",true)][step];step+=1;
        assert_eq!(request["command"]["query"],expected_query);
        assert_eq!(request["command"]["content"],content);
        assert!(Path::new(request["path"].as_str().unwrap()).ends_with("项目 space/src"));
        let mut item=serde_json::json!({"path":"./main.rs","absolute_path":server_path,"start_line":2,"end_line":4,"heading":"部署指南 / 生产环境"});
        if content{item["content"]=serde_json::json!("原文 'quoted'\nsecond line");}
        serde_json::json!({"type":"search","data":{"protocol_version":2,"scope":server_path.parent(),"results":[item],"truncated":false}})
    });
    let run = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sag"))
            .env("SAG_DATA_DIR", home.path())
            .current_dir(&cwd)
            .args([query, "src"])
            .args(extra)
            .output()
            .unwrap()
    };
    let plain = run(&[]);
    assert!(plain.status.success(), "{}", String::from_utf8_lossy(&plain.stderr));
    assert_eq!(
        String::from_utf8(plain.stdout).unwrap(),
        format!("[1/1]\nfilepath: {relative_path}\nheading: 部署指南 / 生产环境\nlines: 2-4\ncontent:\n原文 'quoted'\nsecond line\n\n")
    );
    let content = Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", home.path())
        .current_dir(&cwd)
        .args(["-c", query, "src"])
        .output().unwrap();
    assert!(content.status.success());
    assert!(String::from_utf8(content.stdout)
        .unwrap()
        .contains("原文 'quoted'\nsecond line"));
    let json = run(&["-c", "--json"]);
    assert!(json.status.success());
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value["results"][0]["path"], relative_path);
    assert_eq!(value["results"][0]["absolute_path"], path.to_str().unwrap());
    assert_eq!(value["results"][0]["heading"], "部署指南 / 生产环境");
    let reserved = Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", home.path())
        .current_dir(&cwd)
        .args(["--", "status", "src"])
        .output().unwrap();
    assert!(reserved.status.success(), "{}", String::from_utf8_lossy(&reserved.stderr));
    server.join().unwrap();
}

#[test]
fn semantic_output_groups_passages_by_source_and_labels_each_range() {
    let data = tempfile::tempdir().unwrap();
    let cwd = data.path().canonicalize().unwrap();
    let source = cwd.join("auth.ts");
    let server = support::serve(data.path(), 1, move |_| {
        serde_json::json!({"type":"search","data":{
            "protocol_version":2,"scope":source.parent(),"truncated":false,
            "results":[
                {"path":"auth.ts","absolute_path":source,"start_line":10,"end_line":12,"heading":"Session validation","content":"function validate() {\n  return true;\n}"},
                {"path":"auth.ts","absolute_path":source,"start_line":20,"end_line":20,"heading":"Session validation","content":"validate();"},
                {"path":"tokens.ts","absolute_path":source.parent().unwrap().join("tokens.ts"),"start_line":3,"end_line":3,"content":"validateToken();"}
            ]
        }})
    });
    let result = Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", data.path())
        .current_dir(&cwd)
        .args(["How is a session validated?", "-m", "2"])
        .output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(String::from_utf8(result.stdout).unwrap(), concat!(
        "[1/2]\nfilepath: ./auth.ts\nheading: Session validation\nlines: 10-12\ncontent:\n",
        "function validate() {\n  return true;\n}\n\n",
        "lines: 20-20\ncontent:\nvalidate();\n\n",
        "[2/2]\nfilepath: ./tokens.ts\nlines: 3-3\ncontent:\nvalidateToken();\n\n"
    ));
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
    use std::fs;
    for (command, response_type, message) in [("add", "added", "已加入知识库"), ("remove", "removed", "已从知识库移除")] {
    let data = tempfile::tempdir().unwrap();
    let _cleanup=support::ServiceCleanup(data.path().to_path_buf());
    let cwd = data.path().join("项目 space");
    fs::create_dir_all(cwd.join("child")).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    let server_cwd=cwd.clone();let mut step=0;
    let server=support::serve(data.path(),2,move|request|{
        let suffix=["","child"][step];step+=1;
        assert_eq!(request["command"]["command"],command);
        assert_eq!(std::path::Path::new(request["path"].as_str().unwrap()),server_cwd.join(suffix));
        let mut data=serde_json::json!({"path":server_cwd.join(suffix)});
        if command=="add"{data["index_status"]="pending".into();data["cli_authorized"]=true.into();}
        serde_json::json!({"type":response_type,"data":data})
    });
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sag"))
            .current_dir(&cwd)
            .env("SAG_DATA_DIR", data.path())
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

#[test]
fn standalone_can_explicitly_enable_previously_disabled_access() {
    let data = tempfile::tempdir().unwrap();
    let _cleanup=support::ServiceCleanup(data.path().to_path_buf());
    std::fs::write(data.path().join("cli-access.json"), r#"{"enabled":false}"#).unwrap();
    let run = |args: &[&str]| Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", data.path()).args(args).output().unwrap();
    assert!(String::from_utf8(run(&["status"]).stdout).unwrap().contains("CLI 接入：false"));
    let enabled = run(&["config", "--enable"]);
    assert!(enabled.status.success(), "{}", String::from_utf8_lossy(&enabled.stderr));
    assert!(String::from_utf8(run(&["status"]).stdout).unwrap().contains("CLI 接入：true"));
}
