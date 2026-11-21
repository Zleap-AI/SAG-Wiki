use super::*;
use std::time::Duration;
use sag_protocol::types::ProxyMode;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    task::JoinHandle,
    time::timeout,
};

#[test]
fn windows_manual_settings_support_shared_and_separate_proxies() {
    let shared = windows_settings("127.0.0.1:7890", "<local>; *.example.test 192.168.*");
    assert_eq!(shared.http.as_deref(), Some("127.0.0.1:7890"));
    assert_eq!(shared.https, shared.http);
    assert_eq!(
        shared.system_bypass,
        ["<local>", "*.example.test", "192.168.*"]
    );

    let split = windows_settings("fallback:8080;HTTP=web:8081 HTTPS=secure:8082", "");
    assert_eq!(split.http.as_deref(), Some("web:8081"));
    assert_eq!(split.https.as_deref(), Some("secure:8082"));
    let fallback = windows_settings("http=web:8081 fallback:8080", "");
    assert_eq!(fallback.https.as_deref(), Some("fallback:8080"));
}

#[test]
fn system_and_environment_bypass_rules_apply_together() {
    let settings = windows_settings(
        "http=system:8080;https=system:8081",
        "<local>;*.lan.test;10.0.0.0/8;fd00::/8;192.168.*;169.254/16",
    )
    .with_env(|name| match name {
        "HTTPS_PROXY" => Some("override:3128".into()),
        "no_proxy" => Some(".office.test,*.vpn.test,172.16.0.0/12".into()),
        _ => None,
    });
    assert_eq!(settings.http.as_deref(), Some("system:8080"));
    assert_eq!(settings.https.as_deref(), Some("override:3128"));
    let bypass = Bypass::new(&settings.system_bypass, &settings.env_bypass).unwrap();
    for host in [
        "localhost.",
        "api.localhost",
        "127.5.6.7",
        "[::1]",
        "[::ffff:127.0.0.1]",
        "nas",
        "model.LAN.test",
        "10.2.3.4",
        "192.168.1.9",
        "169.254.3.2",
        "[fd00::1]",
        "office.test",
        "models.office.test",
        "api.vpn.test",
        "172.22.4.5",
    ] {
        assert!(
            bypass.matches(&Url::parse(&format!("https://{host}:8000/v1")).unwrap()),
            "{host}"
        );
    }
    for host in [
        "api.public.test",
        "not-office.test",
        "office.test.invalid",
        "192.169.1.9",
        "172.32.4.5",
    ] {
        assert!(
            !bypass.matches(&Url::parse(&format!("https://{host}/v1")).unwrap()),
            "{host}"
        );
    }
    assert!(Bypass::new(&[], "*")
        .unwrap()
        .matches(&Url::parse("https://203.0.113.3").unwrap()));
}

async fn record_request(status: &'static str) -> (String, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(stream);
        let mut first = String::new();
        stream.read_line(&mut first).await.unwrap();
        let mut header = String::new();
        while stream.read_line(&mut header).await.unwrap() > 0 && header != "\r\n" {
            header.clear();
        }
        stream
            .get_mut()
            .write_all(
                format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        first
    });
    (address, task)
}

#[test]
fn reads_saved_proxy_without_starting_a_service_or_creating_a_database() {
    let data = tempfile::tempdir().unwrap();
    let database = data.path().join("sag.sqlite");
    assert!(saved_proxy(data.path()).unwrap().url.is_empty());
    assert!(!database.exists());
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection.execute_batch("CREATE TABLE preferences(id INTEGER PRIMARY KEY, value TEXT NOT NULL);").unwrap();
    connection.execute("INSERT INTO preferences VALUES(1, '{}')", []).unwrap();
    assert!(saved_proxy(data.path()).unwrap().url.is_empty());
    connection.execute("UPDATE preferences SET value=? WHERE id=1", [
        r#"{"model_proxy":{"url":"http://proxy.example:8080","bypass":".internal.example"}}"#,
    ]).unwrap();
    let saved = saved_proxy(data.path()).unwrap();
    // Settings written by 0.4.0 carry no mode; a non-empty address meant a custom proxy.
    assert_eq!(saved.mode, ProxyMode::Custom);
    assert_eq!(saved.url, "http://proxy.example:8080");
    assert_eq!(saved.bypass, ".internal.example");
    assert_eq!(saved.password, None);
    std::fs::write(data.path().join("credentials.json"), r#"{"answer":"key","proxy":"s3cret"}"#).unwrap();
    let saved = saved_proxy(data.path()).unwrap();
    assert_eq!(saved.stored_password.as_deref(), Some("s3cret"));
    assert!(saved.has_password && saved.password.is_none());
    connection.execute("UPDATE preferences SET value=? WHERE id=1", [
        r#"{"model_proxy":{"mode":"none","url":"http://proxy.example:8080"}}"#,
    ]).unwrap();
    assert_eq!(saved_proxy(data.path()).unwrap().mode, ProxyMode::None);
}

fn custom(url: &str, bypass: &str) -> ModelProxy {
    ModelProxy { mode: ProxyMode::Custom, url: url.into(), bypass: bypass.into(), ..Default::default() }
}

#[test]
fn validation_accepts_supported_schemes_and_rejects_the_rest() {
    for url in ["http://127.0.0.1:7890", "https://proxy.example:3128", "socks5://10.0.0.2:1080", "socks5h://proxy.lan", "proxy.lan:8080"] {
        validate_proxy(&custom(url, "")).unwrap_or_else(|error| panic!("{url}: {error}"));
    }
    for url in ["", "ftp://proxy.lan", "socks4://proxy.lan", "http://", "http://user:pw@proxy.lan:8080", "http://proxy.lan:8080/path", "not a url"] {
        assert!(validate_proxy(&custom(url, "")).is_err(), "{url}");
    }
    // Other modes keep whatever was typed, so switching back restores the draft.
    validate_proxy(&ModelProxy { mode: ProxyMode::None, ..Default::default() }).unwrap();
    validate_proxy(&ModelProxy { mode: ProxyMode::System, url: "ftp://x".into(), ..Default::default() }).unwrap();
    assert!(validate_proxy(&ModelProxy { mode: ProxyMode::System, url: "x".repeat(600), ..Default::default() }).is_err());
    validate_proxy(&custom("http://proxy.lan:8080", ".corp.example, 10.0.0.0/8, *.internal, 192.168.1.9, [fd00::1]")).unwrap();
    assert!(validate_proxy(&custom("http://proxy.lan:8080", "https://intranet.example")).is_err());
    assert!(validate_proxy(&custom("http://proxy.lan:8080", "intranet.example/path")).is_err());
    assert!(validate_proxy(&ModelProxy { mode: ProxyMode::System, bypass: "https://intranet.example".into(), ..Default::default() }).is_err());
}

#[test]
fn custom_mode_resolves_credentials_and_bypass_rules() {
    let proxy = ModelProxy { username: "ali ce".into(), password: Some("p@ss:word".into()), ..custom("proxy.lan:8080", ".corp.example, 10.0.0.0/8") };
    let resolved = ProxySettings::read(&proxy).unwrap();
    assert_eq!(resolved.mode, ProxyMode::Custom);
    // Line breaks and semicolons from the settings textarea become comma-separated rules.
    let multiline = ProxySettings::read(&custom("proxy.lan:8080", "10.0.0.0/8\n corp.example;*.internal")).unwrap();
    assert_eq!(multiline.env_bypass, ",10.0.0.0/8,corp.example,*.internal");
    assert!(Bypass::new(&[], &multiline.env_bypass).unwrap().matches(&Url::parse("https://wiki.corp.example/").unwrap()));
    assert_eq!(resolved.http.as_deref(), Some("http://ali%20ce:p%40ss%3Aword@proxy.lan:8080/"));
    assert_eq!(resolved.https, resolved.http);
    let bypass = Bypass::new(&resolved.system_bypass, &resolved.env_bypass).unwrap();
    for host in ["localhost", "127.0.0.1", "[::1]", "corp.example", "wiki.corp.example", "10.20.30.40"] {
        assert!(bypass.matches(&Url::parse(&format!("https://{host}/v1")).unwrap()), "{host}");
    }
    for host in ["api.openai.com", "notcorp.example", "11.0.0.1"] {
        assert!(!bypass.matches(&Url::parse(&format!("https://{host}/v1")).unwrap()), "{host}");
    }
    // Without a username the password is ignored and the address stays clean.
    let anonymous = ProxySettings::read(&ModelProxy { password: Some("secret".into()), ..custom("socks5://proxy.lan", "") }).unwrap();
    assert_eq!(anonymous.https.as_deref(), Some("socks5://proxy.lan"));
    // The stored password applies when none was typed, and a typed one replaces it.
    let stored = ModelProxy { username: "alice".into(), stored_password: Some("old".into()), ..custom("proxy.lan:8080", "") };
    assert_eq!(ProxySettings::read(&stored).unwrap().https.as_deref(), Some("http://alice:old@proxy.lan:8080/"));
    let retyped = ModelProxy { password: Some("new".into()), ..stored };
    assert_eq!(ProxySettings::read(&retyped).unwrap().https.as_deref(), Some("http://alice:new@proxy.lan:8080/"));
    let direct = ProxySettings::read(&ModelProxy { mode: ProxyMode::None, ..custom("proxy.lan:8080", "") }).unwrap();
    assert_eq!(direct.mode, ProxyMode::None);
    assert!(direct.http.is_none() && direct.https.is_none());
    assert!(direct.transport().unwrap().0.is_none());
}

#[test]
fn direct_mode_clears_proxy_variables_for_child_processes() {
    let mut command = Command::new("uv");
    command.env("HTTPS_PROXY", "http://stale.example:8080").env("all_proxy", "socks5://stale.example:1080");
    configure_child_proxy_env(&mut command, &ModelProxy { mode: ProxyMode::None, ..custom("http://proxy.example:8080", "") }).unwrap();
    // Windows keeps one case-insensitive entry per variable, so the lowercase
    // removals collapse into the uppercase ones there.
    for key in ["HTTPS_PROXY", "HTTP_PROXY", "https_proxy", "http_proxy", "ALL_PROXY", "all_proxy"] {
        let removed = command.get_envs().any(|(name, value)| value.is_none()
            && (name == key || cfg!(windows) && name.to_string_lossy().eq_ignore_ascii_case(key)));
        assert!(removed, "{key} must be removed");
    }
    let (_, value) = command.get_envs().find(|(name, _)| *name == "NO_PROXY").unwrap();
    assert_eq!(value.unwrap(), "*");
    let mut command = Command::new("uv");
    configure_child_proxy_env(&mut command, &ModelProxy { username: "alice".into(), password: Some("secret".into()), ..custom("socks5h://proxy.example:1080", "") }).unwrap();
    let (_, value) = command.get_envs().find(|(name, _)| *name == "HTTPS_PROXY").unwrap();
    assert_eq!(value.unwrap(), "socks5h://alice:secret@proxy.example:1080");
}

#[tokio::test]
async fn probe_reports_the_route_and_hides_credentials() {
    timeout(Duration::from_secs(5), async {
        let (proxy, request) = record_request("407 Proxy Authentication Required").await;
        let draft = ModelProxy { username: "alice".into(), password: Some("secret".into()), ..custom(&proxy, "") };
        let message = probe(&draft, "http://192.0.2.10/v1/models").await.unwrap();
        assert_eq!(message, format!("已通过代理 {proxy} 连接 192.0.2.10（HTTP 407）"));
        assert!(!message.contains("secret") && !message.contains("alice"));
        assert_eq!(request.await.unwrap().trim_end(), "GET http://192.0.2.10/v1/models HTTP/1.1");

        let (target, direct) = record_request("200 OK").await;
        let message = probe(&custom("http://192.0.2.1:9", &target), &format!("http://{target}/")).await.unwrap();
        assert!(message.contains("属于直连地址"), "{message}");
        direct.await.unwrap();

        let (target, direct) = record_request("404 Not Found").await;
        let message = probe(&ModelProxy { mode: ProxyMode::None, ..Default::default() }, &format!("http://{target}/")).await.unwrap();
        assert!(message.starts_with("已直接连接"), "{message}");
        direct.await.unwrap();

        let error = probe(&custom("http://127.0.0.1:9", ""), "http://192.0.2.10/").await.unwrap_err().to_string();
        assert_eq!(error, "无法连接代理 127.0.0.1:9");
        assert!(probe(&custom("http://127.0.0.1:9", ""), "ftp://x").await.is_err());
        assert!(probe(&custom("", ""), "http://192.0.2.10/").await.is_err());
    })
    .await
    .unwrap();
}

#[test]
fn docling_install_commands_get_the_custom_proxy_and_bypass_addresses() {
    let mut command = Command::new("uv");
    configure_child_proxy_env(&mut command, &ModelProxy {
        mode: ProxyMode::Custom,
        url: "http://proxy.example:8080".into(),
        bypass: ".internal.example, 192.0.2.5".into(),
        ..Default::default()
    }).unwrap();
    for key in ["HTTP_PROXY", "HTTPS_PROXY"] {
        assert!(command.get_envs().any(|(name, value)| name.eq_ignore_ascii_case(key) && value.unwrap() == "http://proxy.example:8080/"));
    }
    let (_, value) = command.get_envs().find(|(name, _)| name.eq_ignore_ascii_case("NO_PROXY")).unwrap();
    let bypass = value.unwrap().to_str().unwrap().split(',').collect::<Vec<_>>();
    for address in [".internal.example", "192.0.2.5", "localhost", "127.0.0.1"] {
        assert!(bypass.contains(&address));
    }
}

#[tokio::test]
async fn custom_proxy_can_bypass_intranet_networks() {
    timeout(Duration::from_secs(5), async {
        let (proxy_address, proxied) = record_request("502 Bad Gateway").await;
        let client = ModelHttp::default().client(&ModelProxy {
            mode: ProxyMode::Custom,
            url: format!("http://{proxy_address}"),
            bypass: "192.0.2.0/24".into(),
            ..Default::default()
        }).unwrap();
        assert!(client.get("https://198.51.100.10/v1/models").send().await.is_err());
        assert_eq!(proxied.await.unwrap().trim_end(), "CONNECT 198.51.100.10:443 HTTP/1.1");
        let settings = ProxySettings::read(&ModelProxy {
            mode: ProxyMode::Custom,
            url: format!("http://{proxy_address}"),
            bypass: "192.0.2.0/24".into(),
            ..Default::default()
        }).unwrap();
        assert!(Bypass::new(&[], &settings.env_bypass).unwrap()
            .matches(&Url::parse("https://192.0.2.10/v1").unwrap()));
    }).await.unwrap();
}

#[tokio::test]
async fn switches_proxy_on_next_operation_and_keeps_loopback_direct() {
    timeout(Duration::from_secs(5), async {
        let (first_proxy, first_request) = record_request("200 OK").await;
        let (second_proxy, second_request) = record_request("502 Bad Gateway").await;
        let (loopback, direct_request) = record_request("200 OK").await;
        let model_http = ModelHttp::default();
        let proxy_settings = |address: String| ProxySettings {
            http: Some(address.clone()),
            https: Some(address),
            ..ProxySettings::default()
        };
        let first = model_http
            .refresh(|| Ok(proxy_settings(first_proxy)))
            .unwrap();
        let second = model_http
            .refresh(|| Ok(proxy_settings(second_proxy)))
            .unwrap();

        first
            .get("http://192.0.2.10/v1/models")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        assert_eq!(
            first_request.await.unwrap().trim_end(),
            "GET http://192.0.2.10/v1/models HTTP/1.1"
        );

        assert!(second
            .get("https://192.0.2.10/v1/models")
            .send()
            .await
            .is_err());
        assert_eq!(
            second_request.await.unwrap().trim_end(),
            "CONNECT 192.0.2.10:443 HTTP/1.1"
        );

        second
            .get(format!("http://{loopback}/v1/models"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        assert_eq!(
            direct_request.await.unwrap().trim_end(),
            "GET /v1/models HTTP/1.1"
        );
    })
    .await
    .unwrap();
}

#[cfg(any(windows, target_os = "macos"))]
#[test]
fn reads_proxy_settings_from_os() {
    system_settings().unwrap();
}
