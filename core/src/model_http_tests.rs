use super::*;
use std::time::Duration;
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
