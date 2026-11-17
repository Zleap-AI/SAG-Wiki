#[allow(dead_code)]
mod support;
use serde_json::{json, Value};
use std::{fs, path::Path, process::{Command, Output}};

fn run(data: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sag")).env("SAG_DATA_DIR",data).args(args).output().unwrap()
}
fn value(output: Output) -> Value {
    assert!(output.status.success(), "{}",String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn legacy_and_scoped_key_commands_preserve_mcp_config_and_select_explicit_keys() {
    let temp=tempfile::tempdir().unwrap();
    let data=temp.path().join("data");
    let _cleanup=support::ServiceCleanup(data.clone());
    let a=temp.path().join("project A");let b=temp.path().join("project B");
    fs::create_dir_all(a.join("sub")).unwrap();fs::create_dir(&b).unwrap();
    let a=a.canonicalize().unwrap();let b=b.canonicalize().unwrap();let sub=a.join("sub");
    let store=sag_core::store::Store::open(&data.join("sag.sqlite")).unwrap();
    store.add_root(&a).unwrap();store.add_root(&b).unwrap();drop(store);
    let old=value(run(&data,&["api","key","create","--name","old","--json"]));
    assert_eq!(old["scope"],json!({"type":"all"}));
    let project=value(run(&data,&["api","key","create","--name","project","--path",sub.to_str().unwrap(),"--path",b.to_str().unwrap(),"--json"]));
    assert_eq!(project["scope"]["type"],"folders");
    assert_eq!(project["scope"]["paths"].as_array().unwrap().len(),2);
    let id=project["id"].as_str().unwrap();
    let exported=value(run(&data,&["api","mcp","config","--key-id",id]));
    assert_eq!(exported,json!({"mcpServers":{"sag-wiki":{"type":"http","url":"http://127.0.0.1:24681/mcp",
        "headers":{"Authorization":format!("Bearer {}",project["key"].as_str().unwrap())}}}}));
    let edited=value(run(&data,&["api","key","set-scope",id,"--path",b.to_str().unwrap(),"--json"]));
    assert_eq!(edited["scope"],json!({"type":"folders","paths":[b]}));
    let listed=value(run(&data,&["api","key","list","--json"]));
    assert_eq!(listed[0]["scope"],json!({"type":"all"}));
    assert!(!listed.to_string().contains(project["key"].as_str().unwrap()));
    assert_eq!(value(run(&data,&["api","mcp","config","--key-id",old["id"].as_str().unwrap()]))["mcpServers"]["sag-wiki"]["headers"]["Authorization"],format!("Bearer {}",old["key"].as_str().unwrap()));
    assert_eq!(value(run(&data,&["api","key","set-scope",id,"--all","--json"]))["scope"],json!({"type":"all"}));
    assert_eq!(run(&data,&["api","key","set-scope",id]).status.code(),Some(2));
    assert_eq!(run(&data,&["api","key","set-scope",id,"--all","--path",b.to_str().unwrap()]).status.code(),Some(2));
}
