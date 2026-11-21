use super::{now, remote, Jobs};
use crate::{models, service::Service};
use anyhow::{bail, ensure, Context, Result};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use http_body_util::Full;
use hyper::{
    body::{Bytes, Incoming},
    service::service_fn,
    Method, Request, Response, StatusCode,
};
use hyper_util::rt::{TokioIo, TokioTimer};
use reqwest::{Client, Url};
use sag_protocol::online_import::{Connection, Platform};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{convert::Infallible, fmt, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::oneshot};
use tokio_util::sync::CancellationToken;

const SCOPES: &str =
    "drive:drive:readonly docx:document:readonly drive:export:readonly offline_access";
const LOGIN_TTL: Duration = Duration::from_secs(300);

#[derive(Clone, Copy)]
struct Provider<'a> {
    platform: Platform,
    authorize_url: &'static str,
    token_url: &'a str,
    callback: &'static str,
    callback_path: &'static str,
    wiki_scopes: &'static str,
}

const FEISHU: Provider<'static> = Provider {
    platform: Platform::Feishu,
    authorize_url: "https://accounts.feishu.cn/open-apis/authen/v1/authorize",
    token_url: "https://open.feishu.cn/open-apis/authen/v2/oauth/token",
    callback: "http://127.0.0.1:24682/feishu/oauth/callback",
    callback_path: "/feishu/oauth/callback",
    wiki_scopes: " wiki:space:retrieve wiki:node:retrieve wiki:node:read",
};

const LARK: Provider<'static> = Provider {
    platform: Platform::Lark,
    authorize_url: "https://accounts.larksuite.com/open-apis/authen/v1/authorize",
    token_url: "https://open.larksuite.com/open-apis/authen/v2/oauth/token",
    callback: "http://127.0.0.1:24682/lark/oauth/callback",
    callback_path: "/lark/oauth/callback",
    wiki_scopes: " wiki:wiki:readonly",
};

impl Provider<'static> {
    fn for_platform(platform: Platform) -> Result<Self> {
        match platform {
            Platform::Feishu => Ok(FEISHU),
            Platform::Lark => Ok(LARK),
            _ => bail!("此数据源不支持账号授权"),
        }
    }
}

pub(super) fn account_name(platform: Platform) -> &'static str {
    if platform == Platform::Lark { "Lark" } else { "飞书" }
}

#[derive(Debug)]
pub(super) struct Reconnect(pub(super) Platform);

impl fmt::Display for Reconnect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.0 {
            Platform::Lark => "请重新授权 Lark 账号",
            _ => "请重新授权飞书账号",
        })
    }
}

impl std::error::Error for Reconnect {}

#[derive(Serialize, Deserialize)]
pub(super) struct Grant {
    app_id: String,
    app_secret: String,
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}

pub(super) struct Login {
    id: String,
    platform: Platform,
    drive: bool,
    expires_at: i64,
    cancel: CancellationToken,
    result: Option<Result<Grant>>,
}

fn nonce() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

fn authorization_url(
    provider: Provider<'_>,
    app_id: &str,
    drive: bool,
    state: &str,
    verifier: &str,
) -> Result<Url> {
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url = Url::parse(provider.authorize_url)?;
    let mut scopes = SCOPES.to_owned();
    if !drive {
        scopes.push_str(provider.wiki_scopes);
    }
    url.query_pairs_mut().extend_pairs([
        ("client_id", app_id),
        ("response_type", "code"),
        ("redirect_uri", provider.callback),
        ("scope", scopes.as_str()),
        ("state", state),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("prompt", "consent"),
    ]);
    Ok(url)
}

pub(super) async fn start(
    service: &Arc<Service>,
    platform: Platform,
    app_id: String,
    app_secret: String,
    drive: bool,
) -> Result<Value> {
    let provider = Provider::for_platform(platform)?;
    let app_id = app_id.trim().to_owned();
    let app_secret = app_secret.trim().to_owned();
    ensure!(
        !app_id.is_empty()
            && app_id.len() <= 128
            && !app_secret.is_empty()
            && app_secret.len() <= 4096,
        "请填写有效的 App ID 和 App Secret"
    );
    service.online_import.account_limits.check(platform, &app_id)?;
    let access = service
        .online_import
        .access
        .clone()
        .try_read_owned()
        .context("正在清除本地数据")?;
    ensure!(!service.stop.is_cancelled(), "后台正在退出");
    let id = nonce();
    let state = nonce();
    let verifier = nonce();
    let url = authorization_url(provider, &app_id, drive, &state, &verifier)?;
    let listener = std::net::TcpListener::bind(("127.0.0.1", 24682))
        .context("账号授权端口 24682 无法监听，请关闭正在等待授权的窗口后重试")?;
    listener.set_nonblocking(true)?;
    let listener = TcpListener::from_std(listener)?;
    let cancel = service.stop.child_token();
    *service.online_import.account_login.lock().unwrap() = Some(Login {
        id: id.clone(),
        platform,
        drive,
        expires_at: now() + LOGIN_TTL.as_secs() as i64,
        cancel: cancel.clone(),
        result: None,
    });
    let task_id = id.clone();
    let service = service.clone();
    tokio::spawn(async move {
        let _access = access;
        let result = tokio::select! {
            _ = cancel.cancelled() => return,
            result = tokio::time::timeout(LOGIN_TTL, async {
                let code = receive(listener, provider, state, cancel.clone()).await?;
                exchange(&Client::builder().timeout(Duration::from_secs(30)).redirect(reqwest::redirect::Policy::none()).build()?,
                    &service.online_import.account_limits, provider, &app_id, &app_secret, &[("grant_type", "authorization_code"), ("code", &code), ("redirect_uri", provider.callback), ("code_verifier", &verifier)]).await
            }) => result.context("账号授权等待超时").and_then(|result| result),
        };
        {
            let mut login = service.online_import.account_login.lock().unwrap();
            if let Some(login) = login.as_mut().filter(|login| login.id == task_id) {
                login.result = Some(result);
                login.expires_at = now() + 900;
            }
        }
        drop(_access);
        tokio::select! {
            _ = cancel.cancelled() => {},
            _ = tokio::time::sleep(Duration::from_secs(900)) => cancel_login(&service.online_import, &task_id),
        }
    });
    Ok(json!({"session":id,"url":url.as_str()}))
}

async fn receive(
    listener: TcpListener,
    provider: Provider<'static>,
    state: String,
    cancel: CancellationToken,
) -> Result<String> {
    let (sender, received) = oneshot::channel();
    let sender = Arc::new(std::sync::Mutex::new(Some(sender)));
    tokio::pin!(received);
    loop {
        tokio::select! {
            result = &mut received => return result?,
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let sender = sender.clone();
                let state = state.clone();
                let cancel = cancel.clone();
                tokio::spawn(async move {
                    let handler = service_fn(move |request: Request<Incoming>| {
                        let reply = callback(&request, provider, &state);
                        let (status, message) = match reply {
                            Ok(result) => {
                                if let Some(sender) = sender.lock().unwrap().take() {
                                    let accepted = result.is_ok();
                                    let _ = sender.send(result);
                                    let language = request.headers().get("accept-language")
                                        .and_then(|value| value.to_str().ok()).unwrap_or_default();
                                    return std::future::ready(Ok::<_, Infallible>(
                                        callback_page(provider, accepted, language),
                                    ));
                                } else {
                                    (StatusCode::GONE, "Authorization request has already been handled.")
                                }
                            }
                            Err(status) => (status, "Invalid authorization callback."),
                        };
                        let response = Response::builder().status(status)
                            .header("content-type", "text/plain; charset=utf-8")
                            .header("cache-control", "no-store")
                            .header("referrer-policy", "no-referrer")
                            .body(Full::new(Bytes::from_static(message.as_bytes()))).unwrap();
                        std::future::ready(Ok::<_, Infallible>(response))
                    });
                    let mut http = hyper::server::conn::http1::Builder::new();
                    http.timer(TokioTimer::new()).header_read_timeout(Duration::from_secs(5)).keep_alive(false);
                    tokio::select! {
                        _ = cancel.cancelled() => {},
                        result = http.serve_connection(TokioIo::new(stream), handler) => if let Err(error) = result {
                            eprintln!("账号授权回调连接失败：{}", if error.is_timeout() {"超时"} else {"HTTP 连接异常"});
                        },
                    }
                });
            }
        }
    }
}

fn callback_page(provider: Provider<'_>, accepted: bool, language: &str) -> Response<Full<Bytes>> {
    let chinese = language.is_empty() || language.to_ascii_lowercase().starts_with("zh");
    let name = if provider.platform == Platform::Lark { "Lark" } else if chinese { "飞书" } else { "Feishu" };
    let (language, title, message, close_note) = match (chinese, accepted) {
        (true, true) => (
            "zh-CN",
            format!("已收到{name}授权"),
            "请返回 SAG Wiki 继续。",
            "页面会尝试自动关闭。若仍显示，可手动关闭。",
        ),
        (true, false) => (
            "zh-CN",
            format!("{name}授权未完成"),
            "请返回 SAG Wiki 重新发起授权。",
            "现在可以关闭此页。",
        ),
        (false, true) => (
            "en",
            format!("{name} authorization received"),
            "Return to SAG Wiki to continue.",
            "This page will try to close automatically. If it stays open, close the tab.",
        ),
        (false, false) => (
            "en",
            format!("{name} authorization incomplete"),
            "Return to SAG Wiki to try again.",
            "You can close this page now.",
        ),
    };
    let nonce = nonce();
    let html = include_str!("feishu_callback.html")
        .replace("{{language}}", language)
        .replace("{{title}}", &title)
        .replace("{{message}}", message)
        .replace("{{closeNote}}", close_note)
        .replace("{{autoClose}}", if accepted { "true" } else { "false" })
        .replace("{{nonce}}", &nonce)
        .replace(
            "{{logo}}",
            &STANDARD.encode(include_bytes!("../../../src/assets/brand-icon.png")),
        );
    Response::builder()
        .header("content-type", "text/html; charset=utf-8")
        .header("cache-control", "no-store")
        .header("referrer-policy", "no-referrer")
        .header("content-security-policy", format!(
            "default-src 'none'; img-src data:; style-src 'unsafe-inline'; script-src 'nonce-{nonce}'; base-uri 'none'; frame-ancestors 'none'"
        ))
        .body(Full::new(Bytes::from(html)))
        .unwrap()
}

fn callback<B>(
    request: &Request<B>,
    provider: Provider<'_>,
    state: &str,
) -> std::result::Result<Result<String>, StatusCode> {
    if request.method() != Method::GET
        || request.uri().path() != provider.callback_path
        || request
            .headers()
            .get("host")
            .and_then(|host| host.to_str().ok())
            != Some("127.0.0.1:24682")
    {
        return Err(StatusCode::NOT_FOUND);
    }
    let query = Url::parse(&format!(
        "{}?{}",
        provider.callback,
        request.uri().query().unwrap_or_default()
    ))
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    let get = |key: &str| {
        let mut values = query
            .query_pairs()
            .filter(|(name, _)| name == key)
            .map(|(_, value)| value.into_owned());
        let first = values.next();
        first.filter(|_| values.next().is_none())
    };
    if get("state").as_deref() != Some(state) {
        return Err(StatusCode::BAD_REQUEST);
    }
    if get("error").is_some() {
        return Ok(Err(anyhow::anyhow!("账号授权未完成，请重试")));
    }
    let code = get("code")
        .filter(|code| !code.is_empty() && code.len() <= 4096)
        .ok_or(StatusCode::BAD_REQUEST)?;
    Ok(Ok(code))
}

fn find<'a>(login: &'a Option<Login>, session: &str) -> Result<&'a Login> {
    login
        .as_ref()
        .filter(|login| login.id == session && login.expires_at >= now())
        .context("账号授权已过期，请重新授权")
}

pub(super) fn status(jobs: &Jobs, session: &str) -> Result<Value> {
    let login = jobs.account_login.lock().unwrap();
    match find(&login, session)?.result.as_ref() {
        Some(Ok(_)) => Ok(json!({"ready":true})),
        Some(Err(error)) => bail!("{error}"),
        None => Ok(json!({"ready":false})),
    }
}

pub(super) fn cancel_login(jobs: &Jobs, session: &str) {
    let mut login = jobs.account_login.lock().unwrap();
    if login.as_ref().is_some_and(|login| login.id == session) {
        login.take().unwrap().cancel.cancel();
    }
}

pub(super) fn pending(jobs: &Jobs, session: &str, connection: &Connection) -> Result<String> {
    let login = jobs.account_login.lock().unwrap();
    let login = find(&login, session)?;
    let grant = login
        .result
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .context(Reconnect(connection.platform))?;
    ensure!(
        connection.user_access
            && connection.platform == login.platform
            && connection.drive == login.drive
            && grant.app_id == connection.app_id,
        Reconnect(connection.platform)
    );
    Ok(serde_json::to_string(grant)?)
}

fn token_request(
    client: &Client,
    provider: Provider<'_>,
    app_id: &str,
    app_secret: &str,
    parameters: &[(&str, &str)],
) -> Result<reqwest::RequestBuilder> {
    let fields = [("client_id", app_id), ("client_secret", app_secret)]
        .into_iter()
        .chain(parameters.iter().copied());
    // Feishu's v3 authorization-code exchange can reject valid PKCE verifiers.
    // The v3 refresh endpoint accepts refresh tokens issued by v2.
    if provider.platform == Platform::Lark || parameters.contains(&("grant_type", "authorization_code")) {
        let body: std::collections::BTreeMap<_, _> = fields.collect();
        return Ok(client.post(provider.token_url).json(&body));
    }
    let refresh_url = "https://accounts.feishu.cn/oauth/v3/token";
    let mut body = Url::parse(refresh_url)?;
    body.query_pairs_mut().extend_pairs(fields);
    Ok(client
        .post(refresh_url)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body.query().unwrap().to_owned()))
}

async fn exchange(
    client: &Client,
    limits: &remote::AccountLimits,
    provider: Provider<'_>,
    app_id: &str,
    app_secret: &str,
    parameters: &[(&str, &str)],
) -> Result<Grant> {
    limits.check(provider.platform, app_id)?;
    let response = token_request(client, provider, app_id, app_secret, parameters)?
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("无法连接账号授权服务"))?;
    let status = response.status();
    let retry_after = remote::account_retry_after(response.headers());
    let body = remote::bounded_body(response, 64 * 1024).await.and_then(|bytes| {
        serde_json::from_slice::<Value>(&bytes).context("账号授权服务返回了无效响应")
    });
    let code = body.as_ref().ok().and_then(|value| value["code"].as_i64());
    limits.check_response(provider.platform, app_id, status, retry_after, code)?;
    let body = body?;
    let code = code.context("账号授权响应缺少状态码")?;
    if parameters.contains(&("grant_type", "refresh_token"))
        && matches!(
            code,
            20002
                | 20008
                | 20009
                | 20010
                | 20024
                | 20026
                | 20037
                | 20048
                | 20064
                | 20066
                | 20069
                | 20073
                | 20074
        )
    {
        return Err(Reconnect(provider.platform).into());
    }
    ensure!(
        status.is_success() && code == 0,
        "{}账号授权失败（{code}）",
        account_name(provider.platform)
    );
    let access_token = body["access_token"]
        .as_str()
        .filter(|value| !value.is_empty())
        .context("授权服务未返回访问凭据")?;
    let refresh_token = body["refresh_token"]
        .as_str()
        .filter(|value| !value.is_empty())
        .context("请开通离线访问权限后重新授权")?;
    let expires_in = body["expires_in"]
        .as_i64()
        .filter(|seconds| *seconds > 0)
        .context("授权服务未返回凭据有效期")?;
    Ok(Grant {
        app_id: app_id.into(),
        app_secret: app_secret.into(),
        access_token: access_token.into(),
        refresh_token: refresh_token.into(),
        expires_at: now().saturating_add(expires_in),
    })
}

pub(super) async fn token(
    service: &Service,
    id: Option<&str>,
    connection: &Connection,
    secret: Option<String>,
) -> Result<Option<String>> {
    if !connection.user_access {
        return Ok(secret);
    }
    let provider = Provider::for_platform(connection.platform)?;
    let reconnect = || Reconnect(connection.platform);
    let _serial = service.online_import.account_refresh.lock().await;
    let secret = match id {
        Some(id) => models::online_secret(&service.data_dir, id)?,
        None => secret,
    }
    .with_context(reconnect)?;
    let mut grant: Grant = serde_json::from_str(&secret).with_context(reconnect)?;
    ensure!(grant.app_id == connection.app_id, reconnect());
    if grant.expires_at <= now() + 60 {
        let id = id.with_context(reconnect)?;
        grant = exchange(
            &Client::builder()
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            &service.online_import.account_limits,
            provider,
            &grant.app_id,
            &grant.app_secret,
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &grant.refresh_token),
            ],
        )
        .await?;
        models::save_online_secret(&service.data_dir, id, Some(&serde_json::to_string(&grant)?))?;
    }
    Ok(Some(grant.access_token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_authorization_uses_v2_pkce_and_renewal_stays_on_v3() {
        let client = Client::new();
        let request = token_request(
            &client,
            FEISHU,
            "test-app",
            "test-secret",
            &[
                ("grant_type", "authorization_code"),
                ("code", "test-code"),
                ("code_verifier", "test-verifier"),
                ("redirect_uri", FEISHU.callback),
            ],
        )
        .unwrap()
        .build()
        .unwrap();
        assert_eq!(
            request.url().as_str(),
            "https://open.feishu.cn/open-apis/authen/v2/oauth/token"
        );
        assert_eq!(request.headers()["content-type"], "application/json");
        let body: Value =
            serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(
            body,
            json!({
                "client_id": "test-app", "client_secret": "test-secret", "grant_type": "authorization_code",
                "code": "test-code", "code_verifier": "test-verifier", "redirect_uri": FEISHU.callback
            })
        );

        let request = token_request(
            &client,
            FEISHU,
            "test-app",
            "test-secret",
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", "test-refresh"),
            ],
        )
        .unwrap()
        .build()
        .unwrap();
        assert_eq!(
            request.url().as_str(),
            "https://accounts.feishu.cn/oauth/v3/token"
        );
        assert_eq!(
            request.headers()["content-type"],
            "application/x-www-form-urlencoded"
        );
        let body = request.body().unwrap().as_bytes().unwrap();
        let fields: std::collections::BTreeMap<_, _> = reqwest::Url::parse(&format!(
            "http://localhost/?{}",
            std::str::from_utf8(body).unwrap()
        ))
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect();
        assert_eq!(fields["grant_type"], "refresh_token");
        assert_eq!(fields["refresh_token"], "test-refresh");
    }

    #[test]
    fn lark_uses_its_own_login_callback_and_v2_token_exchange() {
        for drive in [false, true] {
            let url = authorization_url(LARK, "test-app", drive, "state", "verifier").unwrap();
            assert_eq!(url.host_str(), Some("accounts.larksuite.com"));
            let fields: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
            assert_eq!(fields["redirect_uri"], LARK.callback);
            assert_eq!(fields["code_challenge_method"], "S256");
            let scopes: Vec<_> = fields["scope"].split_whitespace().collect();
            assert!(scopes.contains(&"offline_access"));
            assert!(scopes.contains(&"drive:drive:readonly"));
            assert_eq!(scopes.contains(&"wiki:wiki:readonly"), !drive);
        }
        for (grant_type, parameter) in [
            ("authorization_code", "code"),
            ("refresh_token", "refresh_token"),
        ] {
            let request = token_request(
                &Client::new(),
                LARK,
                "test-app",
                "test-secret",
                &[("grant_type", grant_type), (parameter, "test-value")],
            )
            .unwrap()
            .build()
            .unwrap();
            assert_eq!(request.url().as_str(), "https://open.larksuite.com/open-apis/authen/v2/oauth/token");
            assert_eq!(request.headers()["content-type"], "application/json");
            let body: Value =
                serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
            assert_eq!(body["grant_type"], grant_type);
            assert_eq!(body[parameter], "test-value");
        }
    }

    #[tokio::test]
    async fn lark_refresh_rotates_both_tokens() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let url = format!("http://{}/token", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0; 4096];
            loop {
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
                if let Some((_, body)) = String::from_utf8_lossy(&request).split_once("\r\n\r\n") {
                    if let Ok(body) = serde_json::from_str::<Value>(body) {
                        assert_eq!(body["grant_type"], "refresh_token");
                        assert_eq!(body["refresh_token"], "previous-refresh");
                        break;
                    }
                }
            }
            let body = json!({"code":0,"access_token":"new-access","refresh_token":"new-refresh","expires_in":7200}).to_string();
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            socket.write_all(reply.as_bytes()).await.unwrap();
        });
        let provider = Provider { token_url: &url, ..LARK };
        let grant = exchange(
            &Client::builder().no_proxy().timeout(Duration::from_secs(3)).build().unwrap(),
            &remote::AccountLimits::default(),
            provider,
            "test-app",
            "test-secret",
            &[("grant_type", "refresh_token"), ("refresh_token", "previous-refresh")],
        ).await.unwrap();
        assert_eq!(grant.access_token, "new-access");
        assert_eq!(grant.refresh_token, "new-refresh");
        assert!(grant.expires_at > now() + 60);
        server.await.unwrap();
    }

    #[test]
    fn callback_requires_the_matching_state_and_a_single_code() {
        let request = |query: &str| {
            Request::builder()
                .uri(format!("/feishu/oauth/callback?{query}"))
                .header("host", "127.0.0.1:24682")
                .body(())
                .unwrap()
        };
        assert_eq!(
            callback(&request("code=example&state=expected"), FEISHU, "expected")
                .unwrap()
                .unwrap(),
            "example"
        );
        assert_eq!(
            callback(&request("code=example&state=wrong"), FEISHU, "expected").unwrap_err(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            callback(&request("code=one&code=two&state=expected"), FEISHU, "expected").unwrap_err(),
            StatusCode::BAD_REQUEST
        );
        assert!(
            callback(&request("error=access_denied&state=expected"), FEISHU, "expected")
                .unwrap()
                .is_err()
        );
    }

    #[tokio::test]
    async fn callback_page_embeds_the_logo_and_closes_only_after_receiving_authorization() {
        use http_body_util::BodyExt;

        for (accepted, language, expected_title) in [
            (true, "zh-CN,zh;q=0.9", "已收到飞书授权"),
            (false, "en-US,en;q=0.9", "Feishu authorization incomplete"),
        ] {
            let page = callback_page(FEISHU, accepted, language);
            assert_eq!(page.headers()["content-type"], "text/html; charset=utf-8");
            assert_eq!(page.headers()["cache-control"], "no-store");
            assert_eq!(page.headers()["referrer-policy"], "no-referrer");
            let policy = page.headers()["content-security-policy"]
                .to_str()
                .unwrap()
                .to_string();
            let body = page.into_body().collect().await.unwrap().to_bytes();
            let html = std::str::from_utf8(&body).unwrap();
            assert!(html.contains(expected_title));
            assert!(html.contains("data:image/png;base64,iVBOR"));
            assert!(html.contains(&format!("<main data-close=\"{accepted}\">")));
            let nonce = html
                .split_once("<script nonce=\"")
                .unwrap()
                .1
                .split_once('"')
                .unwrap()
                .0;
            assert!(policy.contains(&format!("script-src 'nonce-{nonce}'")));
        }
    }

    #[test]
    fn pending_authorization_is_bound_to_provider_and_import_type() {
        let jobs = Jobs::default();
        *jobs.account_login.lock().unwrap() = Some(Login {
            id: "wiki-login".into(),
            platform: Platform::Feishu,
            drive: false,
            expires_at: now() + 60,
            cancel: CancellationToken::new(),
            result: Some(Ok(Grant {
                app_id: "app".into(),
                app_secret: "secret".into(),
                access_token: "user-token".into(),
                refresh_token: "refresh-token".into(),
                expires_at: now() + 3600,
            })),
        });
        let mut connection = Connection {
            platform: Platform::Feishu,
            drive: false,
            user_access: true,
            app_id: "app".into(),
            address: String::new(),
        };
        assert!(pending(&jobs, "wiki-login", &connection).is_ok());
        connection.drive = true;
        assert!(pending(&jobs, "wiki-login", &connection).is_err());
        connection.drive = false;
        connection.platform = Platform::Lark;
        assert!(pending(&jobs, "wiki-login", &connection).is_err());
    }

    #[tokio::test]
    async fn saves_an_authorized_drive_without_folders_then_queues_the_first_import() {
        for platform in [Platform::Feishu, Platform::Lark] {
            saves_without_resources(platform, true, "drive:folder", "未选择文件夹").await;
        }
    }

    #[tokio::test]
    async fn saves_an_authorized_wiki_without_pages_then_queues_the_first_import() {
        for platform in [Platform::Feishu, Platform::Lark] {
            saves_without_resources(platform, false, "wiki:space:page", "未选择知识库或页面").await;
        }
    }

    async fn saves_without_resources(platform: Platform, drive: bool, resource: &str, not_selected: &str) {
        use super::super::{handle, history, source};
        use sag_protocol::online_import::{Action, Schedule, Source};

        let temp = tempfile::tempdir().unwrap();
        let service = Service::open(temp.path().join("data")).unwrap();
        *service.online_import.account_login.lock().unwrap() = Some(Login {
            id: "feishu-login".into(),
            platform,
            drive,
            expires_at: now() + 60,
            cancel: CancellationToken::new(),
            result: Some(Ok(Grant {
                app_id: "app".into(),
                app_secret: "secret".into(),
                access_token: "user-token".into(),
                refresh_token: "refresh-token".into(),
                expires_at: now() + 3600,
            })),
        });
        let draft = Source {
            id: String::new(),
            name: account_name(platform).into(),
            connection: Connection {
                platform,
                drive,
                user_access: true,
                app_id: "app".into(),
                address: String::new(),
            },
            resources: vec![],
            directory: temp.path().to_str().unwrap().into(),
            schedule: Schedule::Hourly,
            daily_time: "09:00".into(),
            paused: false,
            api: None,
        };
        let saved = handle(
            &service,
            Action::Save {
                source: draft,
                secret: None,
                auth_session: Some("feishu-login".into()),
            },
        )
        .await
        .unwrap();
        let mut saved: Source = serde_json::from_value(saved).unwrap();
        assert!(models::online_secret(&service.data_dir, &saved.id)
            .unwrap()
            .is_some());
        assert_eq!(source(&service.store, &saved.id).unwrap().1, None);
        assert!(history(&service.store, &saved.id).unwrap().is_empty());
        assert!(!std::path::Path::new(&saved.directory).exists());
        assert_eq!(
            handle(
                &service,
                Action::Run {
                    id: saved.id.clone(),
                    retry: false
                }
            )
            .await
            .unwrap_err()
            .to_string(),
            not_selected
        );

        // Occupy both sync slots so the first import is queued without a network request.
        {
            let mut active = service.online_import.active.lock().unwrap();
            active.insert("running-a".into(), CancellationToken::new());
            active.insert("running-b".into(), CancellationToken::new());
        }
        saved.resources = vec![resource.into()];
        let id = saved.id.clone();
        let before = now();
        handle(
            &service,
            Action::Save {
                source: saved,
                secret: None,
                auth_session: None,
            },
        )
        .await
        .unwrap();
        let (saved, next) = source(&service.store, &id).unwrap();
        assert_eq!(saved.resources, [resource]);
        assert!((before..=now()).contains(&next.unwrap()));
        service.finish().await.unwrap();
    }
}
