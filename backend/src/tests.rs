use super::*;
use axum::{body::{to_bytes, Body}, http::Request};
use std::{collections::BTreeMap, sync::{Mutex as StdMutex, Once}};
use std::os::unix::fs::PermissionsExt;
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

#[test]
fn http_rpc_survives_gateway_rewriting_without_fetch_metadata() {
    let headers = HeaderMap::from_iter([
        ("origin".parse().unwrap(), "http://nas.example:5666".parse().unwrap()),
        ("host".parse().unwrap(), "localhost".parse().unwrap()),
        ("x-sag-request".parse().unwrap(), "1".parse().unwrap()),
    ]);
    assert!(same_origin(&headers).is_ok());
    let mut cross_site = headers.clone();
    cross_site.insert("sec-fetch-site", "cross-site".parse().unwrap());
    assert!(same_origin(&cross_site).is_err());
    let mut opaque = headers.clone();
    opaque.insert("origin", "null".parse().unwrap());
    assert!(same_origin(&opaque).is_err());
    let mut simple = headers;
    simple.remove("x-sag-request");
    assert!(same_origin(&simple).is_err());
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
        let data = directory.path().join("data");
        let remote = Arc::new(remote::Access::open(&data).unwrap());
        let app = App { fnos: Fnos::new(socket).unwrap(), data, remote, worker_socket: directory.path().join("worker.sock"), users: Arc::new(Mutex::new(HashMap::new())) };
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

    async fn remote(&self, token: Option<&str>, value: Value, extra: &[(&str, &str)]) -> (StatusCode, Value) {
        let mut request = Request::builder().method("POST").uri("/mcp")
            .header("host", "nas.test:15679")
            .header("content-type", "application/json").header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-06-18");
        if let Some(token) = token { request = request.header("authorization", format!("Bearer {token}")); }
        for (key, value) in extra { request = request.header(*key, *value); }
        let response = remote::router(self.app.clone(), CancellationToken::new()).oneshot(request.body(Body::from(value.to_string())).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1024*1024).await.unwrap();
        let value = if bytes.first() == Some(&b'{') { serde_json::from_slice(&bytes).unwrap() } else { Value::String(String::from_utf8(bytes.to_vec()).unwrap()) };
        (status, value)
    }
}

#[tokio::test(flavor="multi_thread")]
async fn remote_mcp_authenticates_tokens_and_preserves_native_user_permissions() {
    let f = Fixture::new().await;
    for (uid, path) in [(1000, &f.a), (1001, &f.b)] {
        assert_eq!(f.rpc(Some(uid), "add_root", json!({"path":path})).await.0, StatusCode::OK);
        f.app.service(uid).await.unwrap().cli.configure(sag_protocol::Access {enabled:true}).unwrap();
    }
    let (status, generated) = f.rpc(Some(1000), "fnos_remote_rotate", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let token = generated["token"].as_str().unwrap();
    let stored = std::fs::read_to_string(f.app.data.join("remote-access.json")).unwrap();
    assert!(!stored.contains(token));
    assert_eq!(std::fs::metadata(f.app.data.join("remote-access.json")).unwrap().permissions().mode() & 0o777, 0o600);
    let status = f.rpc(Some(1000), "fnos_remote_status", json!({})).await.1;
    assert_eq!(status["enabled"], true);
    assert!(status.get("token").is_none());
    assert_eq!(f.rpc(Some(1001), "fnos_remote_status", json!({})).await.1["enabled"], false);
    assert!(!f.rpc(None, "fnos_remote_rotate", json!({})).await.0.is_success());

    let list = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"});
    assert_eq!(f.remote(None, list.clone(), &[]).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(f.remote(Some("wrong"), list.clone(), &[]).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(f.remote(Some(token), list.clone(), &[("origin","https://untrusted.example")]).await.0, StatusCode::FORBIDDEN);
    let (status, output) = f.remote(Some(token), list.clone(), &[]).await;
    assert_eq!(status, StatusCode::OK, "{output}");
    assert_eq!(output["result"]["tools"].as_array().unwrap().len(), 6);
    assert!(output["result"]["tools"].as_array().unwrap().iter().all(|tool| tool["annotations"]["readOnlyHint"] == true));
    let read = |path: &Path| json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"read","arguments":{"path":path}}});
    let (status, output) = f.remote(Some(token), read(&f.a.join("hello.md")), &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(output.to_string().contains("nativefnosmarker"));
    let (_, denied) = f.remote(Some(token), read(&f.b.join("secret.md")), &[("x-trim-userid","1001")]).await;
    assert_eq!(denied["result"]["isError"], true);
    assert!(!denied.to_string().contains("bob-private-marker"));
    let (_, denied) = f.remote(Some(token), json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"save_document","arguments":{}}}), &[]).await;
    assert_eq!(denied["result"]["isError"], true);
    f.denied.lock().unwrap().insert(f.a.join("hello.md").display().to_string());
    let (_, denied) = f.remote(Some(token), read(&f.a.join("hello.md")), &[]).await;
    assert_eq!(denied["result"]["isError"], true);
    assert!(!denied.to_string().contains("nativefnosmarker"));
    f.denied.lock().unwrap().clear();
    assert_eq!(f.rpc(Some(1000), "fnos_remote_revoke", json!({})).await.0, StatusCode::OK);
    assert_eq!(f.remote(Some(token), list.clone(), &[]).await.0, StatusCode::UNAUTHORIZED);
    let (_, generated) = f.rpc(Some(1000), "fnos_remote_rotate", json!({})).await;
    let next = generated["token"].as_str().unwrap();
    assert_ne!(token, next);
    assert_eq!(f.remote(Some(token), list.clone(), &[]).await.0, StatusCode::UNAUTHORIZED);
    f.app.service(1000).await.unwrap().cli.configure(sag_protocol::Access {enabled:false}).unwrap();
    assert_eq!(f.remote(Some(next), list, &[]).await.0, StatusCode::FORBIDDEN);
    f.finish().await;
}

#[tokio::test(flavor="multi_thread")]
async fn remote_mcp_streamable_http_client_initializes_and_reads_without_browser_session() {
    use rmcp::{ServiceExt, model::CallToolRequestParams, transport::{StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig}};
    let f = Fixture::new().await;
    f.rpc(Some(1000), "add_root", json!({"path":f.a})).await;
    f.app.service(1000).await.unwrap().cli.configure(sag_protocol::Access {enabled:true}).unwrap();
    let token = f.app.remote.rotate(1000).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let app = f.app.clone();
    let stop = CancellationToken::new();
    let shutdown = stop.clone();
    let server = tokio::spawn(async move { axum::serve(listener, remote::router(app, shutdown.clone())).with_graceful_shutdown(shutdown.cancelled_owned()).await.unwrap(); });
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    let http = reqwest::Client::builder().default_headers(headers).build().unwrap();
    let client = ().serve(StreamableHttpClientTransport::with_client(http, StreamableHttpClientTransportConfig::with_uri(url))).await.unwrap();
    assert_eq!(client.peer_info().unwrap().server_info.as_ref().unwrap().name, "SAG Wiki fnOS");
    assert_eq!(client.list_all_tools().await.unwrap().len(), 6);
    let result = client.call_tool(CallToolRequestParams::new("read").with_arguments(json!({"path":f.a.join("hello.md")}).as_object().unwrap().clone())).await.unwrap();
    assert!(serde_json::to_string(&result).unwrap().contains("nativefnosmarker"));
    client.cancel().await.unwrap();
    stop.cancel();server.await.unwrap();
    f.finish().await;
}

#[tokio::test(flavor="multi_thread")]
async fn remote_mcp_search_uses_shared_jev_and_revocation_blocks_in_flight_results() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let f = Fixture::new().await;
    f.rpc(Some(1000), "add_root", json!({"path":f.a})).await;
    let service = f.app.service(1000).await.unwrap();
    service.cli.configure(sag_protocol::Access {enabled:true}).unwrap();
    for _ in 0..100 {
        if service.store.roots().unwrap().iter().all(|root| root.status == "ready") { break; }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let hold = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    let paused = hold.clone(); let started = entered.clone(); let release = resume.clone();
    let model = Router::new().fallback(post(move |Json(request): Json<Value>| {
        let hold=paused.clone();let entered=started.clone();let resume=release.clone();
        async move {
            if hold.load(Ordering::SeqCst) { entered.notify_one(); resume.notified().await; }
            let answers=request["questions"].as_object().unwrap().keys().map(|key|(key.clone(),json!({"type":"noul","noul":0.8}))).collect::<serde_json::Map<_,_>>();
            Json(json!({"answers":answers}))
        }
    }));
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut settings=service.store.settings().unwrap();
    settings.jev.url=format!("http://{}",listener.local_addr().unwrap());settings.retrieval.max_hops=0;
    service.store.save_settings(&settings).unwrap();
    sag_core::models::save_secret(&service.data_dir,"jev","local-test-only").unwrap();
    let model_server=tokio::spawn(async move{axum::serve(listener,model).await.unwrap()});
    let token=f.app.remote.rotate(1000).unwrap();
    let query=|question:&str|json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search","arguments":{"query":question,"top_k":1}}});
    let (status, output)=f.remote(Some(&token),query("What does Alice's document say?"),&[]).await;
    assert_eq!(status,StatusCode::OK,"{output}");
    assert_ne!(output["result"]["isError"],true,"{output}");
    assert!(output.to_string().contains("nativefnosmarker"),"{output}");
    hold.store(true,Ordering::SeqCst);
    let app=f.app.clone();let secret=token.clone();let body=query("Find the contents of Alice's native document.");
    let pending=tokio::spawn(async move{
        remote::router(app,CancellationToken::new()).oneshot(Request::builder().method("POST").uri("/mcp")
            .header("host","nas.test:15679").header("authorization",format!("Bearer {secret}"))
            .header("content-type","application/json").header("accept","application/json, text/event-stream")
            .header("mcp-protocol-version","2025-06-18").body(Body::from(body.to_string())).unwrap()).await.unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(10),entered.notified()).await.unwrap();
    f.app.remote.revoke(1000).unwrap();resume.notify_waiters();
    let response=pending.await.unwrap();let status=response.status();
    let bytes=to_bytes(response.into_body(),1024*1024).await.unwrap();
    let text=String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!text.contains("nativefnosmarker"),"{text}");
    assert!(status==StatusCode::UNAUTHORIZED||text.contains("远程接入已撤销"),"{status}: {text}");
    model_server.abort();f.finish().await;
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
    // This fixture simulates NAS ACLs for synthetic UIDs, not OS accounts or a root worker.
    // Exercise the gateway/store contract here; fnos-document-publish.py covers the real worker.
    service.store.set_document_publisher(Arc::new(|temporary, target, _expected| {
        std::fs::rename(temporary, target)?;
        Ok(())
    }));
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

#[tokio::test(flavor="multi_thread")]
async fn cli_uses_web_index_permissions_access_switch_and_user_lifecycle() {
    use sag_protocol::{Command, MatchMode, MatchOptions, MatchRequest, Output, Response};
    let f = Fixture::new().await;
    assert_eq!(f.rpc(Some(1000), "add_root", json!({"path":f.a})).await.0, StatusCode::OK);
    let service = f.app.service(1000).await.unwrap();
    for _ in 0..200 {
        if service.store.roots().unwrap().iter().all(|root| root.status == "ready") { break; }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let call = |uid, path: PathBuf, command| cli::execute(&f.app, uid, Operation::Cli {path,command}, CancellationToken::new());
    let result = call(1000, f.a.clone(), Command::List).await.unwrap();
    assert!(matches!(result, Output::Cli(Response::Roots(roots)) if roots.len()==1 && roots[0].path==f.a.to_str().unwrap()));
    let matching = || Command::Match(MatchRequest { patterns: vec!["nativefnosmarker".into()], mode: MatchMode::Fixed,
        max_results: Some(5), content: true, timeout: 10, options: MatchOptions::default() });
    assert!(matches!(call(1000, f.a.clone(), matching()).await.unwrap(), Output::Cli(Response::Matches(output)) if output.matched));
    assert!(call(1001, f.a.clone(), matching()).await.is_err());
    f.denied.lock().unwrap().insert(f.a.join("hello.md").display().to_string());
    assert!(call(1000, f.a.clone(), matching()).await.is_err());
    f.denied.lock().unwrap().clear();
    service.cli.configure(sag_protocol::Access {enabled:false}).unwrap();
    assert!(call(1000, f.a.clone(), matching()).await.is_err());
    call(1000, f.a.clone(), Command::Configure {enable:true,url:None,model:None,key:None}).await.unwrap();
    assert!(service.cli.access().enabled);
    std::fs::write(f.a.join(".sagignore"), "# retained rules\n").unwrap();
    f.denied.lock().unwrap().insert(f.a.join(".sagignore").display().to_string());
    assert!(call(1000, f.a.clone(), Command::AddWithTypes {extensions:vec!["md".into()]}).await.is_err());
    assert_eq!(std::fs::read_to_string(f.a.join(".sagignore")).unwrap(), "# retained rules\n");
    f.app.service(1001).await.unwrap();
    assert!(cli::execute(&f.app,1000,Operation::Snapshot,CancellationToken::new()).await.is_err());
    cli::execute(&f.app,1000,Operation::Shutdown,CancellationToken::new()).await.unwrap();
    assert!(!f.app.users.lock().await.contains_key(&1000));
    assert!(f.app.users.lock().await.contains_key(&1001));
    f.finish().await;
}

#[tokio::test(flavor="multi_thread")]
async fn browser_cli_only_accepts_sag_and_authorized_working_directories() {
    let f = Fixture::new().await;
    for (directory, command) in [(f.b.clone(),"sag status"),(f.a.clone(),"sh -c id"),(f.a.clone(),"sag \"unterminated")] {
        let args = json!({"directory":directory,"command":command}).as_object().unwrap().clone();
        assert!(cli::run_command(&f.app,1000,&args).await.is_err());
    }
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revoked_draft_does_not_hide_other_drafts_or_leak_its_body() {
    let f = Fixture::new().await;
    f.rpc(Some(1000), "add_root", json!({"path":f.a})).await;
    let blocked = f.a.join("hello.md");
    let allowed = f.a.join("allowed.md");
    std::fs::write(&allowed, "allowed").unwrap();
    for (path, body) in [(&blocked, "revoked secret draft"), (&allowed, "allowed draft")] {
        let (status, _) = f.rpc(Some(1000), "put_draft", json!({"draft":{
            "path":path,"name":path.file_name().unwrap().to_str().unwrap(),"body":body,"hash":"old","is_new":false
        }})).await;
        assert_eq!(status, StatusCode::OK);
    }
    f.denied.lock().unwrap().insert(blocked.display().to_string());
    let (status, drafts) = f.rpc(Some(1000), "list_drafts", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(drafts.as_array().unwrap().len(), 1);
    assert_eq!(drafts[0]["body"], "allowed draft");
    f.denied.lock().unwrap().clear();
    let (_, drafts) = f.rpc(Some(1000), "list_drafts", json!({})).await;
    assert_eq!(drafts.as_array().unwrap().len(), 2, "revoked drafts remain recoverable after access is restored");
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreign_scripts_cannot_preflight_the_http_rpc_marker() {
    let f = Fixture::new().await;
    let response = router(f.app.clone(), f.directory.path().into()).oneshot(Request::builder()
        .method("OPTIONS").uri(format!("{PREFIX}/api/rpc"))
        .header("origin", "http://untrusted.example")
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "content-type,x-sag-request")
        .body(Body::empty()).unwrap()).await.unwrap();
    assert!(!response.headers().contains_key("access-control-allow-origin"));
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    f.finish().await;
}
