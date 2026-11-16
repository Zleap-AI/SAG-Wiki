use std::{fs, process::{Command, Output}};
use serde_json::{json, Value};

fn run(data: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sag")).env("SAG_DATA_DIR", data).args(args).output().unwrap()
}

#[test]
fn history_lists_local_shares_without_secrets_and_deletes_expired_and_cancelled_records() {
    let data = tempfile::tempdir().unwrap();
    let record = |code: &str, expires: &str, cancelled: bool| json!({
        "code":code,"name":"Notes","shared_at":1700000000u64,"expires_at":expires,
        "cancelled":cancelled,"request_id":"original-request-id","request_key":"private-proof"
    });
    let path = data.path().join("share-history.json");
    fs::write(&path, serde_json::to_vec(&vec![record("ABCDEFGH","2099-01-01T00:00:00Z",false),
        record("BCDEFGHJ","2099-01-01T00:00:00Z",true),record("CDEFGHJK","2020-01-01T00:00:00Z",false)]).unwrap()).unwrap();
    let output = run(data.path(), &["share-history","--json"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let records: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(records.as_array().unwrap().len(),1);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("private-proof"));assert!(!text.contains("request_id"));
    let stored = fs::read_to_string(path).unwrap();
    assert!(!stored.contains("CDEFGHJK"));
    assert!(!stored.contains("BCDEFGHJ"));
    let output = run(data.path(), &["share-history"]);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("SAG-ABCDEFGH"));assert!(!text.contains("已取消"));
    assert!(!data.path().join("sag-service.json").exists());
}

#[test]
fn missing_or_invalid_share_cannot_be_cancelled() {
    let data = tempfile::tempdir().unwrap();
    for code in ["SAG-ABCDEFGH", "abcdefgh"] {
        let output = run(data.path(), &["cancel-share",code]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("找不到分享记录"));
        assert!(output.stdout.is_empty());
    }
    let output = run(data.path(), &["cancel-share","invalid"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("请输入有效的 8 位分享码"));
    let output = run(data.path(), &["share-history","--json"]);
    assert!(output.status.success());assert_eq!(serde_json::from_slice::<Value>(&output.stdout).unwrap(),json!([]));
    assert!(!data.path().join("sag-service.json").exists());
}
