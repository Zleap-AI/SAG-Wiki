use super::*;
use axum::{body::{to_bytes, Body}, http::Request};
use std::{collections::BTreeMap, sync::{Mutex as StdMutex, Once}};
use tower::ServiceExt;

#[test]
fn browser_same_origin_requests_survive_gateway_host_rewriting() {
    let headers = HeaderMap::from_iter([
        ("origin".parse().unwrap(), "https://nas.example".parse().unwrap()),
        ("host".parse().unwrap(), "localhost".parse().unwrap()),
        ("sec-fetch-site".parse().unwrap(), "same-origin".parse().unwrap()),
    ]);
    assert!(same_origin(&headers).is_ok());
}

#[test]
fn origin_comparison_normalizes_default_ports() {
    let headers = HeaderMap::from_iter([
        ("origin".parse().unwrap(), "https://nas.example".parse().unwrap()),
        ("host".parse().unwrap(), "nas.example:443".parse().unwrap()),
    ]);
    assert!(same_origin(&headers).is_ok());
}

#[test]
fn cross_site_requests_are_rejected_with_or_without_origin() {
    for origin in [None, Some("https://nas.example")] {
        let mut headers = HeaderMap::from_iter([
            ("host".parse().unwrap(), "nas.example".parse().unwrap()),
            ("sec-fetch-site".parse().unwrap(), "cross-site".parse().unwrap()),
        ]);
        if let Some(origin) = origin { headers.insert("origin", origin.parse().unwrap()); }
        assert!(same_origin(&headers).is_err());
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    app: App,
    a: PathBuf,
    b: PathBuf,
    readonly: PathBuf,
    requests: Arc<StdMutex<Vec<Value>>>,
    denied: Arc<StdMutex<HashSet<String>>>,
    server: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn new() -> Self {
        static TOKEN: Once = Once::new();
        TOKEN.call_once(|| std::env::set_var("TRIM_API_TOKEN", "test-only-token"));
        let directory = tempfile::Builder::new().prefix("fnos-").tempdir_in("/tmp").unwrap();
        let base = directory.path().canonicalize().unwrap();
        let a = base.join("alice");
        let b = base.join("bob");
        std::fs::create_dir(&a).unwrap(); std::fs::create_dir(&b).unwrap();
        let readonly = a.join("readonly.md");
        std::fs::write(a.join("hello.md"), "# Alice\nThe nativefnosmarker document.").unwrap();
        std::fs::write(&readonly, "Read only document.").unwrap();
        std::fs::write(b.join("secret.md"), "bob-private-marker").unwrap();
        let grants = BTreeMap::from([(1000, vec![a.clone()]), (1001, vec![b.clone()])]);
        let requests = Arc::new(StdMutex::new(Vec::new()));
        let denied = Arc::new(StdMutex::new(HashSet::new()));
        let calls = requests.clone(); let deny = denied.clone(); let read_only = readonly.clone();
        let gateway = Router::new().route("/api/v1/trimapp", post(move |headers: HeaderMap, Json(value): Json<Value>| {
            let grants = grants.clone(); let calls = calls.clone(); let deny = deny.clone(); let read_only = read_only.clone();
            async move {
                assert_eq!(headers["authorization"], "Bearer test-only-token");
                assert_eq!(value["appName"], fnos::APP_NAME);
                calls.lock().unwrap().push(value.clone());
                let uid = value["data"]["uid"].as_u64().unwrap_or(1000) as u32;
                let roots = grants.get(&uid).cloned().unwrap_or_default();
                let data = match value["req"].as_str().unwrap() {
                    "trim.file.getUserAccessibleFolders" => json!({"paths": roots}),
                    "trim.file.checkUserACL" => Value::Array(value["data"]["path"].as_array().unwrap().iter().map(|path| {
                        let path = path.as_str().unwrap();
                        let readable = roots.iter().any(|root| Path::new(path).starts_with(root)) && !deny.lock().unwrap().contains(path);
                        json!({"path":path,"readable":readable,"writable":readable && Path::new(path)!=read_only,"deletable":false})
                    }).collect()),
                    "trim.file.convertPath" => {
                        let paths = json!(value["data"]["path"].as_array().unwrap().iter().map(|path| json!({"path":path,"semanticPath":format!("我的文件/{}",Path::new(path.as_str().unwrap()).file_name().unwrap().to_string_lossy())})).collect::<Vec<_>>());
                        // Exercise both the documented envelope and the real NAS response.
                        if value["data"]["language"] == "en-US" { json!({"status":0,"result":paths}) } else { paths }
                    },
                    _ => panic!("unexpected API"),
                };
                Json(json!({"code":0,"msg":"","data":data}))
            }
        }));
        let socket = directory.path().join("api.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, gateway).await.unwrap(); });
        let app = App { fnos: Fnos::new(socket).unwrap(), data: directory.path().join("data"), worker_socket: directory.path().join("worker.sock"), users: Arc::new(Mutex::new(HashMap::new())) };
        Self { directory, app, a, b, readonly, requests, denied, server }
    }

    async fn rpc(&self, uid: Option<u32>, command: &str, args: Value) -> (StatusCode, Value) {
        let mut request = Request::builder().method("POST").uri(format!("{PREFIX}/api/rpc"))
            .header("content-type", "application/json");
        if let Some(uid) = uid { request = request.header("x-trim-userid", uid); }
        let response = router(self.app.clone(), self.directory.path().into()).oneshot(request.body(Body::from(json!({"command":command,"args":args,"session":uuid::Uuid::new_v4()}).to_string())).unwrap()).await.unwrap();
        let status = response.status();
        let value = serde_json::from_slice(&to_bytes(response.into_body(), 1024*1024).await.unwrap()).unwrap();
        (status, value)
    }

    async fn finish(self) {
        for service in self.app.users.lock().await.values() { service.finish().await.unwrap(); }
        self.server.abort();
    }
}

#[tokio::test(flavor="multi_thread")]
async fn path_conversion_accepts_real_and_documented_fnos_responses() {
    let f = Fixture::new().await;
    for language in ["zh-CN", "en-US"] {
        let (status, value) = f.rpc(Some(1000), "fnos_convert_paths", json!({"paths":[f.a],"language":language})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value, json!({"status":0,"result":[{"path":f.a,"semanticPath":"我的文件/alice"}]}));
    }
    f.finish().await;
}

#[tokio::test(flavor="multi_thread")]
async fn gateway_serves_application_and_static_assets_under_prefix() {
    let f = Fixture::new().await;
    std::fs::write(f.directory.path().join("index.html"), "<h1>SAG</h1>").unwrap();
    std::fs::write(f.directory.path().join("app.js"), "window.app=true;").unwrap();
    for path in [format!("{PREFIX}/"), format!("{PREFIX}/index.html"), format!("{PREFIX}/app.js")] {
        let response = router(f.app.clone(), f.directory.path().into()).oneshot(Request::builder().uri(&path).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
    }
    f.finish().await;
}

#[tokio::test(flavor="multi_thread")]
async fn native_permissions_reject_traversal_symlinks_and_readonly_writes() {
    let f = Fixture::new().await;
    assert!(f.app.fnos.allowed(1000, &f.a.join("hello.md"), false).await.unwrap());
    assert!(!f.app.fnos.allowed(1000, &f.b.join("secret.md"), false).await.unwrap());
    assert!(!f.app.fnos.allowed(1000, &f.readonly, true).await.unwrap());
    assert!(f.app.fnos.allowed(1000, &f.a.join("new.md"), true).await.unwrap());
    assert!(f.app.fnos.allowed(1000, &f.a.join("../bob/secret.md"), false).await.is_err());
    std::os::unix::fs::symlink(f.b.join("secret.md"), f.a.join("link.md")).unwrap();
    assert!(!f.app.fnos.allowed(1000, &f.a.join("link.md"), false).await.unwrap());
    assert!(f.requests.lock().unwrap().iter().all(|v| !v.to_string().contains("test-only-token")));
    f.finish().await;
}

#[tokio::test(flavor="multi_thread")]
async fn gateway_identity_and_native_command_boundary_are_required() {
    let f = Fixture::new().await;
    assert!(!f.rpc(None, "get_settings", json!({})).await.0.is_success());
    assert!(!f.rpc(Some(1000), "shutdown", json!({})).await.0.is_success());
    assert!(!f.rpc(Some(1000), "cli", json!({})).await.0.is_success());
    let (status, paths) = f.rpc(Some(1000), "fnos_accessible_folders", json!({})).await;
    assert_eq!(status, StatusCode::OK); assert_eq!(paths, json!([f.a]));
    assert!(!same_origin(&HeaderMap::from_iter([
        ("origin".parse().unwrap(), "https://other.example".parse().unwrap()),
        ("host".parse().unwrap(), "nas.local".parse().unwrap()),
    ])).is_ok());
    f.finish().await;
}

#[tokio::test(flavor="multi_thread")]
async fn indexing_search_save_and_user_isolation_use_native_acl() {
    let f = Fixture::new().await;
    assert_eq!(f.rpc(Some(1000), "add_root", json!({"path":f.a})).await.0, StatusCode::OK);
    let service = f.app.service(1000).await.unwrap();
    for _ in 0..100 {
        if service.store.files().unwrap().len() == 2 { break; }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    assert_eq!(service.store.files().unwrap().len(), 2);
    let (status, value) = f.rpc(Some(1000), "run_query", json!({"query":{"id":"search-1","kind":"search","question":"nativefnosmarker","scope":[],"references":[],"model":"","conversation_id":null,"replace_from":null,"page":1}})).await;
    assert_eq!(status, StatusCode::OK, "{value}"); assert_eq!(value["results"].as_array().unwrap().len(), 1);
    let (status, doc) = f.rpc(Some(1000), "read_document", json!({"path":f.a.join("hello.md")})).await;
    assert_eq!(status, StatusCode::OK); assert!(doc["body"].as_str().unwrap().contains("nativefnosmarker"));
    let (status, saved) = f.rpc(Some(1000), "save_document", json!({"path":f.a.join("hello.md"),"body":"Updated nativefnosmarker","expectedHash":doc["hash"]})).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert!(!f.rpc(Some(1000), "save_document", json!({"path":f.readonly,"body":"overwrite","expectedHash":null})).await.0.is_success());
    assert!(std::fs::read_to_string(&f.readonly).unwrap().contains("Read only"));
    assert!(!f.rpc(Some(1001), "read_document", json!({"path":f.a.join("hello.md")})).await.0.is_success());
    assert_eq!(f.rpc(Some(1001), "list_roots", json!({})).await.1, json!([]));
    assert!(!f.rpc(Some(1000), "add_root", json!({"path":f.b})).await.0.is_success());
    f.denied.lock().unwrap().insert(f.a.join("hello.md").to_string_lossy().into_owned());
    assert!(!f.rpc(Some(1000), "read_document", json!({"path":f.a.join("hello.md")})).await.0.is_success());
    f.finish().await;
}

#[tokio::test(flavor="multi_thread")]
async fn derived_files_require_source_access_even_through_aliases() {
    use sag_core::store::FileAccessMode;
    let f = Fixture::new().await;
    let source = f.a.join("report.pdf");
    std::fs::write(&source, "original").unwrap();
    let output = f.a.join(".sag-converted/nested/report.pdf.md");
    let nested_source = f.a.join("nested/report.pdf");
    std::fs::create_dir_all(nested_source.parent().unwrap()).unwrap();
    std::fs::write(&nested_source, "original").unwrap();
    // New nested directories inherit the nearest existing parent's creation ACL.
    assert!(f.app.fnos.allowed(1000, &output, true).await.unwrap());
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    std::fs::write(&output, "derived").unwrap();
    let alias = f.a.join("alias.md");
    std::os::unix::fs::symlink(&output, &alias).unwrap();
    assert!(f.app.fnos.allowed(1000, &alias, false).await.unwrap());
    f.denied.lock().unwrap().insert(nested_source.to_string_lossy().into_owned());
    assert!(!f.app.fnos.allowed(1000, &output, false).await.unwrap());
    assert!(!f.app.fnos.allowed(1000, &alias, false).await.unwrap());
    assert!(!f.app.fnos.allowed(1000, &output, true).await.unwrap());
    assert!(!f.app.fnos.allowed_action(1000, &output, FileAccessMode::Delete).await.unwrap());
    assert!(output.exists());
    f.finish().await;
}
