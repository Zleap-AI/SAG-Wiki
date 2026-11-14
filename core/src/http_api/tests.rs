use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Fixture {
    dir: tempfile::TempDir,
    service: Arc<Service>,
    url: String,
    a: PathBuf,
    b: PathBuf,
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
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let tx = tx.clone();
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
    f.service.api.reset().await.unwrap();
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
