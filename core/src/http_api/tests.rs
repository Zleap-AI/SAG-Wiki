use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Fixture {
    dir: tempfile::TempDir,
    service: Arc<Service>,
    url: String,
    a: PathBuf,
    b: PathBuf,
    model_requests: Arc<Mutex<Vec<Value>>>,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir(&data).unwrap();
        let a = dir.path().join("项目 A");
        let b = dir.path().join("项目 B");
        std::fs::create_dir_all(a.join("sub")).unwrap();
        std::fs::create_dir(&b).unwrap();
        std::fs::write(a.join("one.md"), "# First\n\ndeploymarker release steps\n").unwrap();
        std::fs::write(
            a.join("sub/two.md"),
            "# Second\n\ndeploymarker nested steps\n",
        )
        .unwrap();
        std::fs::write(b.join("three.md"), "# Third\n\ndeploymarker other steps\n").unwrap();
        let a = a.canonicalize().unwrap();
        let b = b.canonicalize().unwrap();
        let service = Service::open(data).unwrap();
        for root in [&a, &b] {
            service.store.add_root(root).unwrap();
            service.store.scan(root.to_str().unwrap(), |_| {}).unwrap();
        }
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        drop(socket);
        service
            .api
            .manage(
                ApiAction::Start {
                    host: Some("127.0.0.1".into()),
                    port: Some(port),
                },
                &service,
            )
            .await
            .unwrap();
        Self {
            dir,
            service,
            url: format!("http://127.0.0.1:{port}"),
            a: a.canonicalize().unwrap(),
            b: b.canonicalize().unwrap(),
            model_requests: Arc::new(Mutex::new(vec![])),
        }
    }
    async fn key(&self, name: &str) -> Value {
        self.service
            .api
            .manage(ApiAction::CreateKey { name: name.into() }, &self.service)
            .await
            .unwrap()
    }
    async fn get(&self, path: &str, key: &str) -> reqwest::Response {
        reqwest::Client::new()
            .get(format!("{}{path}", self.url))
            .bearer_auth(key)
            .send()
            .await
            .unwrap()
    }
    async fn search(&self, key: &str, body: Value) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{}/v1/search", self.url))
            .bearer_auth(key)
            .json(&body)
            .send()
            .await
            .unwrap()
    }
    async fn finish(self) {
        self.service.finish().await.unwrap()
    }
    async fn model(
        &self,
        hold: bool,
    ) -> (tokio::sync::mpsc::UnboundedReceiver<()>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut settings = self.service.store.settings().unwrap();
        settings.jev.url = format!("http://{}", listener.local_addr().unwrap());
        settings.jev.model = "api-test".into();
        settings.jev.auth_mode = crate::types::ModelAuthMode::None;
        settings.retrieval.max_hops = 0;
        self.service.store.save_settings(&settings).unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let model_requests = self.model_requests.clone();
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let tx = tx.clone();
                let model_requests = model_requests.clone();
                tokio::spawn(async move {
                    let mut bytes = vec![];
                    let mut buf = [0u8; 4096];
                    let value: Value = loop {
                        let n = socket.read(&mut buf).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        bytes.extend_from_slice(&buf[..n]);
                        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                            let headers = String::from_utf8_lossy(&bytes[..end]);
                            let length = headers
                                .lines()
                                .find_map(|l| {
                                    l.to_lowercase()
                                        .strip_prefix("content-length: ")
                                        .and_then(|v| v.parse::<usize>().ok())
                                })
                                .unwrap();
                            if bytes.len() >= end + 4 + length {
                                break serde_json::from_slice(&bytes[end + 4..end + 4 + length])
                                    .unwrap();
                            }
                        }
                    };
                    model_requests.lock().unwrap().push(value.clone());
                    tx.send(()).unwrap();
                    if hold {
                        let _ = socket.read(&mut buf).await;
                        return;
                    }
                    let answers = value["questions"]
                        .as_object()
                        .unwrap()
                        .keys()
                        .map(|id| (id.clone(), json!({"type":"noul","noul":0.7})))
                        .collect::<serde_json::Map<_, _>>();
                    let body = json!({"answers":answers}).to_string();
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                });
            }
        });
        (rx, task)
    }
}
#[tokio::test]
async fn authentication_keys_private_storage_and_stop_restore() {
    let f = Fixture::new().await;
    for path in ["/v1/status", "/v1/folders", "/v1/search"] {
        let r = f.get(path, "").await;
        assert_eq!(r.status(), 401);
        assert_eq!(
            r.json::<Value>().await.unwrap()["error"]["code"],
            "unauthorized"
        );
    }
    let key = f.key("Application A").await;
    let secret = key["key"].as_str().unwrap();
    let listed = f
        .service
        .api
        .manage(ApiAction::ListKeys, &f.service)
        .await
        .unwrap();
    assert!(listed[0].get("key").is_none());
    assert!(!listed.to_string().contains(secret));
    let stored = std::fs::read_to_string(f.service.data_dir.join("http-api.json")).unwrap();
    assert!(stored.contains(secret));
    assert_eq!(listed[0]["copyable"], true);
    let reopened = ApiService::open(&f.service.data_dir).unwrap();
    for id in [None, Some(key["id"].as_str().unwrap().into())] {
        assert_eq!(reopened.manage(ApiAction::CopyKey { id }, &f.service).await.unwrap()["key"], secret);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(f.service.data_dir.join("http-api.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let r = f.get("/v1/status", secret).await;
    assert_eq!(r.status(), 200);
    assert!(r.headers().get("access-control-allow-origin").is_none());
    assert_eq!(
        r.json::<Value>().await.unwrap()["decision_configured"],
        false
    );
    f.service
        .api
        .manage(
            ApiAction::RevokeKey {
                id: key["id"].as_str().unwrap().into(),
            },
            &f.service,
        )
        .await
        .unwrap();
    assert_eq!(f.get("/v1/status", secret).await.status(), 401);
    assert_eq!(f.service.api.manage(ApiAction::CopyKey { id: None }, &f.service).await.unwrap_err().to_string(), "请先创建 API Key");
    let status = f.service.api.status().unwrap();
    assert_eq!(status["enabled"], true);
    assert_eq!(status["running"], true);
    assert_eq!(status["key_count"], 0);
    let restored = ApiService::open(&f.service.data_dir).unwrap();
    assert_eq!(restored.status().unwrap()["enabled"], true);
    f.service
        .api
        .manage(ApiAction::Stop, &f.service)
        .await
        .unwrap();
    assert!(!f.service.stop.is_cancelled());
    f.service.api.restore(&f.service).await;
    assert_eq!(f.service.api.status().unwrap()["running"], false);
    assert_eq!(
        ApiService::open(&f.service.data_dir)
            .unwrap()
            .status()
            .unwrap()["enabled"],
        false
    );
    f.finish().await;
}

#[tokio::test]
async fn mcp_reuses_api_authentication_search_scope_and_lifecycle() {
    let f = Fixture::new().await;
    let key = f.key("MCP").await;
    let secret = key["key"].as_str().unwrap();
    let client = reqwest::Client::new();
    let post = || client.post(format!("{}/mcp", f.url)).bearer_auth(secret)
        .header("accept", "application/json, text/event-stream");
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}
    }});
    assert_eq!(f.get("/mcp", "invalid").await.status(), 401);
    assert_eq!(post().header("origin", "https://example.com").json(&init).send().await.unwrap().status(), 403);
    assert_eq!(post().header("host", "example.com").json(&init).send().await.unwrap().status(), 403);
    let response = post().json(&init).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let init = response.json::<Value>().await.unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "sag-wiki");
    let version = init["result"]["protocolVersion"].as_str().unwrap();
    let request = || post().header("MCP-Protocol-Version", version);
    let response = request().json(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).send().await.unwrap();
    assert_eq!(response.status(), 202);
    let listed = request().json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).send().await.unwrap().json::<Value>().await.unwrap();
    let names = listed["result"]["tools"].as_array().unwrap().iter().map(|tool| tool["name"].as_str().unwrap()).collect::<Vec<_>>();
    assert_eq!(names, ["search", "folders"]);
    let call = |name: &'static str, arguments: Value| async move {
        request().json(&json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":name,"arguments":arguments}})).send().await.unwrap().json::<Value>().await.unwrap()
    };
    let folders = call("folders", json!({"path":f.a})).await;
    assert_eq!(folders["result"]["structuredContent"]["folders"][0]["name"], "sub");
    let search = call("search", json!({"query":"deploymarker","mode":"fast","paths":[f.a]})).await;
    let results = search["result"]["structuredContent"]["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|item| Path::new(item["path"].as_str().unwrap()).starts_with(&f.a)));
    let excluded = call("search", json!({"query":"deploymarker","mode":"fast","paths":[f.a,f.b],"exclude":["sub/**","three.md"]})).await;
    let kept = excluded["result"]["structuredContent"]["results"].as_array().unwrap();
    assert_eq!(kept.len(), 1);
    assert!(kept[0]["path"].as_str().unwrap().ends_with("one.md"));
    let empty = call("search", json!({"query":"deploymarker","mode":"fast","paths":[f.a],"exclude":["**"]})).await;
    assert!(empty["result"]["structuredContent"]["results"].as_array().unwrap().is_empty());
    assert_eq!(listed["result"]["tools"][0]["inputSchema"]["properties"]["exclude"]["type"], "array");
    let outside = call("folders", json!({"path":f.dir.path()})).await;
    assert_eq!(outside["result"]["isError"], true);
    assert_eq!(outside["result"]["structuredContent"]["code"], "path_not_allowed");
    f.service.api.manage(ApiAction::RevokeKey { id: key["id"].as_str().unwrap().into() }, &f.service).await.unwrap();
    assert_eq!(f.get("/mcp", secret).await.status(), 401);
    f.service.api.manage(ApiAction::Stop, &f.service).await.unwrap();
    assert!(client.get(format!("{}/mcp", f.url)).send().await.is_err());
    f.finish().await;
}
#[tokio::test]
async fn fast_search_reads_external_conversion_within_the_original_scope() {
    let f = Fixture::new().await;
    let source = f.a.join("sub/scan.rtf");
    std::fs::write(&source, br"{\rtf1\ansi externalconversionneedle}").unwrap();
    f.service.store.scan(f.a.to_str().unwrap(), |_| {}).unwrap();
    let key = f.key("conversion search").await;
    let token = key["key"].as_str().unwrap();
    for exclude in [json!([]), json!(["not-a-match/**"])] {
        let response = f.search(token, json!({"query":"externalconversionneedle","mode":"fast","paths":[f.a.join("sub")],"exclude":exclude})).await;
        let status = response.status();
        let result: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{result}");
        let matches = result["results"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches[0]["content"].as_str().unwrap().contains("externalconversionneedle"));
    }
    f.finish().await;
}

#[tokio::test]
async fn fast_search_uses_multiple_scopes_and_browses_one_folder_level() {
    let f = Fixture::new().await;
    let key = f.key("caller").await;
    let secret = key["key"].as_str().unwrap();
    let roots = f
        .get("/v1/folders", secret)
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(roots["folders"].as_array().unwrap().len(), 2);
    let mut url = reqwest::Url::parse(&format!("{}/v1/folders", f.url)).unwrap();
    url.query_pairs_mut()
        .append_pair("path", f.a.to_str().unwrap());
    let r = reqwest::Client::new()
        .get(url)
        .bearer_auth(secret)
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.json::<Value>().await.unwrap()["folders"][0]["name"],
        "sub"
    );
    for paths in [json!([]), json!([f.a, f.a.join("sub"), f.b])] {
        let r = f
            .search(
                secret,
                json!({"query":"deploymarker","mode":"fast","paths":paths}),
            )
            .await;
        assert_eq!(r.status(), 200);
        let value = r.json::<Value>().await.unwrap();
        let results = value["results"].as_array().unwrap();
        assert_eq!(results.len(), 3);
        let unique = results
            .iter()
            .map(|r| r["path"].as_str().unwrap())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique.len(), 3);
        for result in results {
            assert!(Path::new(result["path"].as_str().unwrap()).is_absolute());
            assert!(result["content"].as_str().unwrap().contains("deploymarker"));
            assert!(result["start_line"].as_u64().unwrap() > 0);
        }
        assert_eq!(value["truncated"], false);
    }
    let r = f
        .search(
            secret,
            json!({"query":"deploymarker","mode":"fast","paths":[f.a.join("sub")],"limit":1}),
        )
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(r["results"].as_array().unwrap().len(), 1);
    assert!(r["results"][0]["path"]
        .as_str()
        .unwrap()
        .ends_with("two.md"));
    for query in ["nothingmatcheshere", "!!!"] {
        let r = f.search(secret, json!({"query":query,"mode":"fast"})).await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.json::<Value>().await.unwrap()["results"], json!([]));
    }
    f.finish().await;
}
#[tokio::test]
#[cfg(unix)]
async fn searches_and_folder_browsing_support_historical_symlink_roots() {
    let f = Fixture::new().await;
    let actual = f.dir.path().join("actual-root");
    std::fs::rename(&f.a, &actual).unwrap();
    std::os::unix::fs::symlink(&actual, &f.a).unwrap();
    let actual = actual.canonicalize().unwrap();
    let key = f.key("symlink caller").await;
    let secret = key["key"].as_str().unwrap();
    let (_arrived, model) = f.model(false).await;
    for mode in ["fast", "deep"] {
        for (paths, expected) in [
            (json!([]), 3),
            (json!([f.a]), 2),
            (json!([actual]), 2),
            (json!([actual.join("sub")]), 1),
            (json!([f.a, actual, f.b]), 3),
        ] {
            let response = f.search(secret, json!({"query":"deploymarker","mode":mode,"paths":paths})).await;
            let status = response.status();
            let body = response.json::<Value>().await.unwrap();
            assert_eq!(status, 200, "{mode}, {paths}: {body}");
            let results = body["results"].as_array().unwrap();
            assert_eq!(results.len(), expected, "{mode}, {paths}: {body}");
            assert!(results.iter().all(|r| r["content"].as_str().unwrap().contains("deploymarker")));
        }
    }
    let mut url = reqwest::Url::parse(&format!("{}/v1/folders", f.url)).unwrap();
    url.query_pairs_mut().append_pair("path", actual.to_str().unwrap());
    let response = reqwest::Client::new().get(url.clone()).bearer_auth(secret).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.json::<Value>().await.unwrap()["folders"][0]["name"], "sub");
    f.service.store.remove_path(&f.a.join("sub")).unwrap();
    let response = reqwest::Client::new().get(url).bearer_auth(secret).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.json::<Value>().await.unwrap()["folders"], json!([]));
    std::os::unix::fs::symlink(f.dir.path(), actual.join("outside")).unwrap();
    for path in [actual.join("sub"), actual.join("outside")] {
        assert_eq!(f.search(secret, json!({"query":"deploymarker","mode":"fast","paths":[path]})).await.status(), 403);
    }
    for mode in ["fast", "deep"] {
        let response = f.search(secret, json!({"query":"deploymarker","mode":mode})).await;
        assert_eq!(response.status(), 200);
        assert_eq!(response.json::<Value>().await.unwrap()["results"].as_array().unwrap().len(), 2);
    }
    model.abort();
    f.finish().await;
}
#[tokio::test]
async fn request_validation_and_deep_search_without_answer_model() {
    let f = Fixture::new().await;
    let key = f.key("caller").await;
    let secret = key["key"].as_str().unwrap();
    let r = f.search(secret, json!({"query":"deploymarker"})).await;
    assert_eq!(r.status(), 409);
    assert_eq!(
        r.json::<Value>().await.unwrap()["error"]["code"],
        "decision_not_configured"
    );
    for body in [
        json!({"query":""}),
        json!({"query":"x","limit":0}),
        json!({"query":"x","mode":"other"}),
        json!({"query":"x","paths":null}),
        json!({"query":"x","extra":true}),
    ] {
        assert_eq!(f.search(secret, body).await.status(), 400);
    }
    for (path, code) in [
        ("relative".into(), 400),
        (
            f.dir.path().join("missing").to_str().unwrap().to_owned(),
            404,
        ),
        (f.dir.path().to_str().unwrap().into(), 403),
    ] {
        assert_eq!(
            f.search(
                secret,
                json!({"query":"deploymarker","mode":"fast","paths":[f.a,path]})
            )
            .await
            .status(),
            code
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(f.dir.path(), f.a.join("outside")).unwrap();
        assert_eq!(
            f.search(
                secret,
                json!({"query":"x","mode":"fast","paths":[f.a.join("outside")]})
            )
            .await
            .status(),
            403
        );
    }
    assert_eq!(f.get("/v1/search", secret).await.status(), 405);
    assert_eq!(f.get("/v1/unknown", secret).await.status(), 404);
    assert_eq!(
        f.search(secret, json!({"query":"x".repeat(40000)}))
            .await
            .status(),
        413
    );
    let (_arrived, model) = f.model(false).await;
    let r = f
        .search(secret, json!({"query":"deploymarker","limit":2}))
        .await;
    assert_eq!(r.status(), 200);
    let value = r.json::<Value>().await.unwrap();
    assert_eq!(value["mode"], "deep");
    assert!(!value["results"].as_array().unwrap().is_empty());
    assert!(value["results"].as_array().unwrap().len() <= 2);
    model.abort();
    f.finish().await;
}
#[tokio::test]
async fn queued_revocation_and_stop_preserve_other_clients() {
    let f = Fixture::new().await;
    let a = f.key("A").await;
    let b = f.key("B").await;
    let permits = f
        .service
        .requests
        .clone()
        .acquire_many_owned(20)
        .await
        .unwrap();
    let request = reqwest::Client::new()
        .post(format!("{}/v1/search", f.url))
        .bearer_auth(a["key"].as_str().unwrap())
        .json(&json!({"query":"deploymarker","mode":"fast"}));
    let pending = tokio::spawn(async move { request.send().await.unwrap() });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!pending.is_finished());
    f.service
        .api
        .manage(
            ApiAction::RevokeKey {
                id: a["id"].as_str().unwrap().into(),
            },
            &f.service,
        )
        .await
        .unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), pending)
            .await
            .unwrap()
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        f.get("/v1/status", b["key"].as_str().unwrap())
            .await
            .status(),
        200
    );
    let request = reqwest::Client::new()
        .post(format!("{}/v1/search", f.url))
        .bearer_auth(b["key"].as_str().unwrap())
        .json(&json!({"query":"deploymarker","mode":"fast"}));
    let pending = tokio::spawn(async move { request.send().await.unwrap() });
    tokio::time::sleep(Duration::from_millis(100)).await;
    f.service
        .api
        .manage(ApiAction::Stop, &f.service)
        .await
        .unwrap();
    assert_eq!(pending.await.unwrap().status(), 503);
    assert!(!f.service.stop.is_cancelled());
    drop(permits);
    assert_eq!(f.service.requests.available_permits(), 20);
    f.finish().await;
}
#[tokio::test]
async fn client_disconnect_cancels_running_model_and_releases_shared_permit() {
    let f = Fixture::new().await;
    let key = f.key("caller").await;
    let (mut arrived, model) = f.model(true).await;
    let mut stream = tokio::net::TcpStream::connect(f.url.trim_start_matches("http://"))
        .await
        .unwrap();
    let body = json!({"query":"deploymarker"}).to_string();
    stream.write_all(format!("POST /v1/search HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",key["key"].as_str().unwrap(),body.len()).as_bytes()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), arrived.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(f.service.requests.available_permits(), 19);
    drop(stream);
    tokio::time::timeout(Duration::from_secs(3), async {
        while f.service.requests.available_permits() != 20 {
            tokio::time::sleep(Duration::from_millis(10)).await
        }
    })
    .await
    .unwrap();
    model.abort();
    f.finish().await;
}
#[tokio::test]
async fn failed_bind_never_claims_running_or_persists_manual_enable() {
    let dir = tempfile::tempdir().unwrap();
    let service = Service::open(dir.path().into()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    assert!(service
        .api
        .manage(
            ApiAction::Start {
                host: Some("127.0.0.1".into()),
                port: Some(listener.local_addr().unwrap().port())
            },
            &service
        )
        .await
        .is_err());
    assert_eq!(service.api.status().unwrap()["enabled"], false);
    assert_eq!(service.api.status().unwrap()["running"], false);
    service.finish().await.unwrap();
}

#[tokio::test]
async fn whole_request_deadlines_include_waiting_for_shared_capacity() {
    let f = Fixture::new().await;
    let key = f.key("caller").await;
    let permits = f
        .service
        .requests
        .clone()
        .acquire_many_owned(20)
        .await
        .unwrap();
    let client = reqwest::Client::new();
    let started = Instant::now();
    let mut tasks = vec![];
    for mode in ["fast", "deep"] {
        let request = client
            .post(format!("{}/v1/search", f.url))
            .bearer_auth(key["key"].as_str().unwrap())
            .json(&json!({"query":"deploymarker","mode":mode}));
        tasks.push(tokio::spawn(async move {
            let response = request.send().await.unwrap();
            (response.status(), response.json::<Value>().await.unwrap())
        }));
    }
    let fast = tasks.remove(0).await.unwrap();
    assert_eq!(fast.0, 504);
    assert_eq!(fast.1["error"]["code"], "search_timeout");
    assert!(started.elapsed() >= Duration::from_secs(30));
    assert!(!tasks[0].is_finished());
    let deep = tasks.remove(0).await.unwrap();
    assert_eq!(deep.0, 504);
    assert!(started.elapsed() >= Duration::from_secs(60));
    drop(permits);
    assert_eq!(f.service.requests.available_permits(), 20);
    f.finish().await;
}

#[tokio::test]
async fn api_access_is_independent_of_cli_access_and_reset_revokes_all_keys() {
    let f = Fixture::new().await;
    let key = f.key("caller").await;
    f.service
        .cli
        .configure(sag_protocol::Access { enabled: false })
        .unwrap();
    assert_eq!(
        f.search(
            key["key"].as_str().unwrap(),
            json!({"query":"deploymarker","mode":"fast"})
        )
        .await
        .status(),
        200
    );
    f.service.api.reset(false).await.unwrap();
    assert_eq!(f.service.api.status().unwrap()["key_count"], 0);
    assert_eq!(f.service.api.status().unwrap()["running"], false);
    assert_eq!(
        ApiService::open(&f.service.data_dir)
            .unwrap()
            .status()
            .unwrap()["enabled"],
        false
    );
    f.finish().await;
}

#[tokio::test]
async fn oversized_source_passages_are_omitted_whole_and_marked_truncated() {
    let f=Fixture::new().await;let key=f.key("caller").await;
    std::fs::write(f.a.join("large.md"),format!("# Large\n\npayloadmarker {}\n","word ".repeat(140_000))).unwrap();
    f.service.store.scan(f.a.to_str().unwrap(),|_|{}).unwrap();
    // Seed a complete large source span so this transport-boundary test is
    // independent of the indexer's default chunk splitting.
    let path=f.a.join("large.md");let document=f.service.store.document(&path).unwrap();
    let span=crate::text::Lines::new(&document.body).span(&document.body,0,document.body.len());
    f.service.store.connection().execute("UPDATE chunks SET spans=?1 WHERE document_id=(SELECT id FROM documents WHERE path=?2)",rusqlite::params![serde_json::to_string(&vec![span]).unwrap(),store::path_text(&path).unwrap()]).unwrap();
    let response=f.search(key["key"].as_str().unwrap(),json!({"query":"payloadmarker","mode":"fast"})).await;
    assert_eq!(response.status(),200);let bytes=response.bytes().await.unwrap();
    assert!(bytes.len()<sag_protocol::MAX_RESPONSE as usize);let value:Value=serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["truncated"],true);assert_eq!(value["results"],json!([]));f.finish().await;
}

async fn scoped_key(f: &Fixture, paths: Value) -> Value {
    // JSON exercises the same versioned local-management boundary as desktop/CLI.
    let action = serde_json::from_value(json!({"action":"create_scoped_key","name":"project","paths":paths})).unwrap();
    f.service.api.manage(action, &f.service).await.unwrap()
}

async fn mcp_call(f: &Fixture, token: &str, name: &str, arguments: Value) -> Value {
    reqwest::Client::new().post(format!("{}/mcp", f.url)).bearer_auth(token)
        .header("accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2025-11-25")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":name,"arguments":arguments}}))
        .send().await.unwrap().json().await.unwrap()
}

#[tokio::test]
async fn folder_keys_and_legacy_keys_coexist_across_http_and_mcp() {
    let f = Fixture::new().await;
    let old = f.key("old global key").await;
    let a = scoped_key(&f, json!([f.a.join("sub")])).await;
    let b = scoped_key(&f, json!([f.b])).await;
    let (_arrived, model) = f.model(false).await;
    for (key, expected, visible) in [(&old, 3, None), (&a, 1, Some(f.a.join("sub"))), (&b, 1, Some(f.b.clone()))] {
        let token = key["key"].as_str().unwrap();
        for mode in ["fast", "deep"] {
            for extra in [json!({}), json!({"paths":[]}), json!({"exclude":["unmatched/**"]})] {
                let mut input = json!({"query":"deploymarker","mode":mode});
                input.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
                let response = f.search(token, input.clone()).await;
                assert_eq!(response.status(), 200);
                let body: Value = response.json().await.unwrap();
                assert_eq!(body["results"].as_array().unwrap().len(), expected, "{body}");
                let body = mcp_call(&f, token, "search", input).await;
                assert_eq!(body["result"]["structuredContent"]["results"].as_array().unwrap().len(), expected, "{body}");
            }
        }
        let listed = f.get("/v1/folders", token).await.json::<Value>().await.unwrap();
        let mcp = mcp_call(&f, token, "folders", json!({})).await;
        assert_eq!(mcp["result"]["structuredContent"], listed);
        if let Some(path) = visible {
            assert_eq!(listed["folders"].as_array().unwrap().len(), 1);
            assert_eq!(listed["folders"][0]["path"], path.to_str().unwrap());
            let status = f.get("/v1/status", token).await.json::<Value>().await.unwrap();
            assert_eq!(status["roots"].as_array().unwrap().len(), 1);
            assert_eq!(status["roots"][0]["path"], path.to_str().unwrap());
            assert_eq!(status["roots"][0]["indexed_files"], 1);
        }
    }
    let token = a["key"].as_str().unwrap();
    for paths in [json!([f.a]), json!([f.b]), json!([f.a.join("sub"),f.b])] {
        let input = json!({"query":"deploymarker","mode":"fast","paths":paths});
        assert_eq!(f.search(token, input.clone()).await.status(), 403);
        let body = mcp_call(&f, token, "search", input).await;
        assert_eq!(body["result"]["structuredContent"]["code"], "path_not_allowed");
    }
    let mut url = reqwest::Url::parse(&format!("{}/v1/folders", f.url)).unwrap();
    url.query_pairs_mut().append_pair("path", f.a.to_str().unwrap());
    assert_eq!(reqwest::Client::new().get(url).bearer_auth(token).send().await.unwrap().status(), 403);
    model.abort();
    f.finish().await;
}

#[tokio::test]
async fn upgrading_scope_less_config_preserves_credentials_and_dynamic_all_access() {
    let f = Fixture::new().await;
    let key = f.key("pre-upgrade").await;
    let secret = key["key"].as_str().unwrap();
    let path = f.service.data_dir.join("http-api.json");
    let mut config: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for record in config["keys"].as_array_mut().unwrap() { record.as_object_mut().unwrap().remove("scope"); }
    // One legacy key has no recoverable secret, as in versions before copy support.
    let mut digest_only = config["keys"][0].clone();
    digest_only["id"] = json!("legacy-digest-only");
    digest_only["digest"] = json!(Sha256::digest(b"legacy-digest-token").to_vec());
    digest_only.as_object_mut().unwrap().remove("secret");
    config["keys"].as_array_mut().unwrap().push(digest_only);
    f.service.api.shutdown().await.unwrap();
    write_private(&path, &serde_json::to_vec(&config).unwrap()).unwrap();
    let mut f = f;
    let data = f.service.data_dir.clone();
    f.service.finish().await.unwrap();
    f.service = Service::open(data).unwrap();
    f.service.api.restore(&f.service).await;
    // A legacy management client still omits scope on CreateKey.
    let created: ApiAction = serde_json::from_value(json!({"action":"create_key","name":"legacy client"})).unwrap();
    let created = f.service.api.manage(created, &f.service).await.unwrap();
    assert_eq!(created["scope"], json!({"type":"all"}));
    let c = f.dir.path().join("new-project");
    std::fs::create_dir(&c).unwrap();
    std::fs::write(c.join("new.md"), "deploymarker new material").unwrap();
    let c = c.canonicalize().unwrap();
    f.service.store.add_root(&c).unwrap();
    f.service.store.scan(c.to_str().unwrap(), |_| {}).unwrap();
    let restricted = scoped_key(&f, json!([f.b])).await;
    assert_eq!(f.service.api.manage(ApiAction::CopyKey { id: Some(key["id"].as_str().unwrap().into()) }, &f.service).await.unwrap()["key"], secret);
    for token in [secret, "legacy-digest-token", created["key"].as_str().unwrap()] {
        for extra in [json!({}),json!({"paths":[]})] {
            let mut input = json!({"query":"deploymarker","mode":"fast"});
            input.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            let body = mcp_call(&f, token, "search", input).await;
            assert_eq!(body["result"]["structuredContent"]["results"].as_array().unwrap().len(), 4, "{body}");
        }
        assert_eq!(mcp_call(&f, token, "folders", json!({})).await["result"]["structuredContent"]["folders"].as_array().unwrap().len(), 3);
    }
    // Creating a scoped key persists the migrated records without rotating them.
    let persisted: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(persisted["keys"][0]["id"], key["id"]);
    assert_eq!(persisted["keys"][0]["secret"], secret);
    assert_eq!(persisted["keys"][0]["scope"], json!({"type":"all"}));
    let token = restricted["key"].as_str().unwrap();
    assert_eq!(f.search(token, json!({"query":"deploymarker","mode":"fast"})).await.json::<Value>().await.unwrap()["results"].as_array().unwrap().len(), 1);
    f.finish().await;
}

#[tokio::test]
async fn folder_grants_reject_empty_invalid_and_removed_scopes_without_global_fallback() {
    let f = Fixture::new().await;
    for paths in [json!([]), json!(["relative"]), json!([f.dir.path()]), json!([f.a.join("one.md")])] {
        let action = serde_json::from_value(json!({"action":"create_scoped_key","name":"invalid","paths":paths})).unwrap();
        assert!(f.service.api.manage(action, &f.service).await.is_err());
    }
    let a = scoped_key(&f, json!([f.a.join("sub")])).await;
    f.service.store.remove_path(&f.a.join("sub")).unwrap();
    let token = a["key"].as_str().unwrap();
    for route in ["/v1/folders", "/v1/status"] { assert_eq!(f.get(route, token).await.status(), 403); }
    let body = f.search(token, json!({"query":"deploymarker","mode":"fast"})).await;
    assert_eq!(body.status(), 403);
    assert_eq!(body.json::<Value>().await.unwrap()["error"]["code"], "scope_unavailable");
    assert_eq!(mcp_call(&f, token, "search", json!({"query":"deploymarker","mode":"fast"})).await["result"]["isError"], true);
    f.finish().await;
}

#[tokio::test]
async fn scoped_search_inherits_external_conversion_access_from_original_directory() {
    let f = Fixture::new().await;
    for root in [f.a.join("sub"), f.b.clone()] {
        std::fs::write(root.join("scan.rtf"), br"{\rtf1\ansi convertedgrantneedle}").unwrap();
    }
    for root in [&f.a, &f.b] { f.service.store.scan(root.to_str().unwrap(), |_| {}).unwrap(); }
    let key = scoped_key(&f, json!([f.a.join("sub")])).await;
    let (_arrived, model) = f.model(false).await;
    for mode in ["fast", "deep"] {
        for exclude in [json!([]),json!(["unmatched/**"])] {
            let body = mcp_call(&f, key["key"].as_str().unwrap(), "search", json!({"query":"convertedgrantneedle","mode":mode,"exclude":exclude})).await;
            let results = body["result"]["structuredContent"]["results"].as_array().unwrap();
            assert_eq!(results.len(), 1, "{body}");
            assert!(results[0]["content"].as_str().unwrap().contains("convertedgrantneedle"));
        }
    }
    model.abort();
    f.finish().await;
}

#[tokio::test]
async fn editing_scope_cancels_old_requests_and_preserves_other_keys() {
    let f = Fixture::new().await;
    let a = f.key("editable").await;
    let b = f.key("unchanged").await;
    let (mut arrived, model) = f.model(true).await;
    let client = reqwest::Client::new();
    let request = client.post(format!("{}/v1/search", f.url)).bearer_auth(a["key"].as_str().unwrap())
        .json(&json!({"query":"deploymarker"})).send();
    let running = tokio::spawn(request);
    arrived.recv().await.unwrap();
    let permits = f.service.requests.clone().acquire_many_owned(19).await.unwrap();
    let queued_request = client.post(format!("{}/v1/search", f.url)).bearer_auth(a["key"].as_str().unwrap())
        .json(&json!({"query":"deploymarker","mode":"fast"}));
    let queued = tokio::spawn(async move { queued_request.send().await.unwrap() });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!queued.is_finished());
    let action = serde_json::from_value(json!({"action":"update_key_scope","id":a["id"],"scope":{"type":"folders","paths":[f.b]}})).unwrap();
    f.service.api.manage(action, &f.service).await.unwrap();
    assert_eq!(running.await.unwrap().unwrap().status(), 401);
    assert_eq!(tokio::time::timeout(Duration::from_secs(2), queued).await.unwrap().unwrap().status(), 401);
    drop(permits);
    for (key, count) in [(&a,1),(&b,3)] {
        let response = f.search(key["key"].as_str().unwrap(), json!({"query":"deploymarker","mode":"fast"})).await;
        assert_eq!(response.status(),200);
        assert_eq!(response.json::<Value>().await.unwrap()["results"].as_array().unwrap().len(),count);
    }
    model.abort();
    f.finish().await;
}

#[cfg(unix)]
#[tokio::test]
async fn folder_grants_reject_symlink_escape_and_changed_grant_targets() {
    let f = Fixture::new().await;
    let key = scoped_key(&f, json!([f.a.join("sub")])).await;
    let token = key["key"].as_str().unwrap();
    std::os::unix::fs::symlink(&f.b, f.a.join("sub/escape")).unwrap();
    assert_eq!(f.search(token,json!({"query":"deploymarker","mode":"fast","paths":[f.a.join("sub/escape")]})).await.status(),403);
    std::fs::remove_file(f.a.join("sub/escape")).unwrap();
    std::fs::rename(f.a.join("sub"), f.a.join("original-sub")).unwrap();
    std::os::unix::fs::symlink(&f.b, f.a.join("sub")).unwrap();
    assert_eq!(f.search(token,json!({"query":"deploymarker","mode":"fast"})).await.status(),403);
    f.finish().await;
}

#[tokio::test]
async fn folder_grants_persist_and_empty_stored_grants_never_mean_all() {
    let mut f = Fixture::new().await;
    let old = f.key("all").await;
    let project = scoped_key(&f, json!([f.a, f.b])).await;
    let data = f.service.data_dir.clone();
    f.service.finish().await.unwrap();
    f.service = Service::open(data.clone()).unwrap();
    f.service.api.restore(&f.service).await;
    let token = project["key"].as_str().unwrap();
    assert_eq!(f.search(token,json!({"query":"deploymarker","mode":"fast"})).await.json::<Value>().await.unwrap()["results"].as_array().unwrap().len(),3);
    f.service.finish().await.unwrap();
    let path = data.join("http-api.json");
    let mut config: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    config["keys"][1]["scope"]["directories"] = json!([]);
    write_private(&path, &serde_json::to_vec(&config).unwrap()).unwrap();
    f.service = Service::open(data).unwrap();
    f.service.api.restore(&f.service).await;
    assert_eq!(f.search(token,json!({"query":"deploymarker","mode":"fast"})).await.status(),403);
    assert_eq!(f.search(old["key"].as_str().unwrap(),json!({"query":"deploymarker","mode":"fast"})).await.json::<Value>().await.unwrap()["results"].as_array().unwrap().len(),3);
    f.finish().await;
}

#[cfg(unix)]
#[tokio::test]
async fn restricted_search_never_sends_backslash_sibling_text_to_the_model() {
    let f = Fixture::new().await;
    let sibling = f.a.join("sub\\private");
    std::fs::create_dir(&sibling).unwrap();
    std::fs::write(sibling.join("secret.md"), "deploymarker PRIVATE_SIBLING_EVIDENCE").unwrap();
    f.service.store.scan(f.a.to_str().unwrap(), |_| {}).unwrap();
    let key = scoped_key(&f,json!([f.a.join("sub")])).await;
    let (_arrived, model) = f.model(false).await;
    let token = key["key"].as_str().unwrap();
    let response = f.search(token,json!({"query":"deploymarker","mode":"deep"})).await;
    let sent = serde_json::to_string(&*f.model_requests.lock().unwrap()).unwrap();
    assert!(sent.contains("nested steps"), "expected the allowed passage to reach decision scoring");
    assert!(!sent.contains("PRIVATE_SIBLING_EVIDENCE"), "outside text reached decision scoring");
    assert_eq!(response.status(),200);
    let body = f.search(token,json!({"query":"deploymarker","mode":"fast"})).await.json::<Value>().await.unwrap();
    assert_eq!(body["results"].as_array().unwrap().len(),1);
    model.abort();
    f.finish().await;
}

#[tokio::test]
async fn restricted_keys_do_not_reveal_ungranted_path_existence_or_type() {
    let f = Fixture::new().await;
    let key = scoped_key(&f,json!([f.a.join("sub")])).await;
    let token = key["key"].as_str().unwrap();
    for path in [f.b.clone(), f.b.join("three.md"), f.b.join("missing"), f.dir.path().join("not-indexed")] {
        let mut url = reqwest::Url::parse(&format!("{}/v1/folders",f.url)).unwrap();
        url.query_pairs_mut().append_pair("path",path.to_str().unwrap());
        let response = reqwest::Client::new().get(url).bearer_auth(token).send().await.unwrap();
        assert_eq!(response.status(),403,"{path:?}");
        assert_eq!(response.json::<Value>().await.unwrap()["error"]["code"],"path_not_allowed");
        let body=mcp_call(&f,token,"folders",json!({"path":path})).await;
        assert_eq!(body["result"]["structuredContent"]["code"],"path_not_allowed");
        let response=f.search(token,json!({"query":"deploymarker","mode":"fast","paths":[path]})).await;
        assert_eq!(response.status(),403);
    }
    assert_eq!(f.search(token,json!({"query":"deploymarker","mode":"fast","paths":[f.a.join("sub/missing")]})).await.status(),404);
    f.finish().await;
}

#[tokio::test]
async fn scoped_creation_uses_an_operation_old_services_cannot_accept_as_global() {
    #[derive(Deserialize)]
    #[serde(tag="action",rename_all="snake_case")]
    enum LegacyAction { CreateKey { name: String } }
    // Same old serde shape: extra fields on create_key would be silently ignored.
    let legacy: LegacyAction = serde_json::from_value(json!({"action":"create_key","name":"old"})).unwrap();
    let LegacyAction::CreateKey { name }=legacy;assert_eq!(name,"old");
    let action: ApiAction=serde_json::from_value(json!({"action":"create_scoped_key","name":"project","paths":["/indexed"]})).unwrap();
    assert!(serde_json::from_value::<LegacyAction>(serde_json::to_value(action).unwrap()).is_err());
}
