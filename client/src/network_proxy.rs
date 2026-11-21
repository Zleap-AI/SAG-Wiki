use anyhow::{Context, Result};
use ipnet::IpNet;
use reqwest::{Client, Proxy, Url};
use sag_protocol::types::{ModelProxy, ProxyMode};
use std::{net::IpAddr, path::Path, process::Command, sync::Mutex, time::Duration};

#[derive(Default)]
pub struct ModelHttp(Mutex<Option<(ProxySettings, Client)>>);

impl ModelHttp {
    pub fn client(&self, proxy: &ModelProxy) -> Result<Client> {
        self.refresh(|| ProxySettings::read(proxy))
    }

    fn refresh(&self, read: impl FnOnce() -> Result<ProxySettings>) -> Result<Client> {
        let mut cached = self.0.lock().unwrap();
        let settings = read()?;
        if let Some((current, client)) = cached.as_ref() {
            if current == &settings {
                return Ok(client.clone());
            }
        }
        let client = settings.build()?;
        *cached = Some((settings, client.clone()));
        Ok(client)
    }
}

#[derive(Default, PartialEq, Eq)]
struct ProxySettings {
    mode: ProxyMode,
    http: Option<String>,
    https: Option<String>,
    system_bypass: Vec<String>,
    env_bypass: String,
}

impl ProxySettings {
    fn read(proxy: &ModelProxy) -> Result<Self> {
        let env = |name: &str| std::env::var(name).ok();
        let mut settings = match proxy.mode {
            ProxyMode::None => return Ok(Self { mode: ProxyMode::None, ..Self::default() }),
            ProxyMode::Custom => {
                let mut address = custom_proxy_url(&proxy.url)?.context("请填写代理地址，例如 http://127.0.0.1:7890")?;
                let username = proxy.username.trim();
                if !username.is_empty() {
                    // A password typed in settings wins over the stored one.
                    let password = proxy.password.as_deref().filter(|value| !value.is_empty()).or(proxy.stored_password.as_deref());
                    address.set_username(username).map_err(|_| anyhow::anyhow!("代理用户名无效"))?;
                    address.set_password(password).map_err(|_| anyhow::anyhow!("代理密码无效"))?;
                }
                let address = address.to_string();
                Self {
                    mode: ProxyMode::Custom,
                    http: Some(address.clone()),
                    https: Some(address),
                    env_bypass: env("NO_PROXY").or_else(|| env("no_proxy")).unwrap_or_default(),
                    ..Self::default()
                }
            }
            ProxyMode::System => {
                let settings = system_settings()?;
                // CGI hosts can turn HTTP headers into environment variables.
                if std::env::var_os("REQUEST_METHOD").is_some() { settings } else { settings.with_env(env) }
            }
        };
        // The UI accepts commas, semicolons and line breaks; the matcher and child
        // processes only understand commas, so the list is normalized here.
        let bypass = bypass_rules(&proxy.bypass);
        if !bypass.is_empty() {
            settings.env_bypass.push(',');
            settings.env_bypass.push_str(&bypass.join(","));
        }
        Ok(settings)
    }

    fn with_env(mut self, get: impl Fn(&str) -> Option<String>) -> Self {
        let env = |upper, lower| {
            get(upper)
                .or_else(|| get(lower))
                .filter(|value| !value.trim().is_empty())
        };
        let all = env("ALL_PROXY", "all_proxy");
        self.http = env("HTTP_PROXY", "http_proxy")
            .or_else(|| all.clone())
            .or(self.http);
        self.https = env("HTTPS_PROXY", "https_proxy").or(all).or(self.https);
        self.env_bypass = env("NO_PROXY", "no_proxy").unwrap_or_default();
        self
    }

    fn build(&self) -> Result<Client> {
        Ok(self.transport()?.apply(Client::builder()).build()?)
    }

    fn transport(&self) -> Result<NetworkProxy> {
        let http = proxy_url(self.http.as_deref())?;
        let https = proxy_url(self.https.as_deref())?;
        if http.is_none() && https.is_none() {
            return Ok(NetworkProxy(None));
        }
        let bypass = Bypass::new(&self.system_bypass, &self.env_bypass)?;
        Ok(NetworkProxy(Some(Proxy::custom(move |url| {
            if bypass.matches(url) {
                return None;
            }
            match url.scheme() {
                "http" => http.clone(),
                "https" => https.clone(),
                _ => None,
            }
        }))))
    }

    /// Where a request for `target` would go, mirroring `transport()`.
    fn route(&self, target: &Url) -> Result<Route> {
        let proxy = match target.scheme() {
            "http" => proxy_url(self.http.as_deref())?,
            _ => proxy_url(self.https.as_deref())?,
        };
        let Some(proxy) = proxy else { return Ok(Route::Direct) };
        let bypass = Bypass::new(&self.system_bypass, &self.env_bypass)?;
        Ok(if bypass.matches(target) { Route::Direct } else { Route::Proxy(display_address(&proxy)) })
    }
}

enum Route {
    Proxy(String),
    Direct,
}

/// Host and port only, so an address can be shown without its credentials.
fn display_address(url: &Url) -> String {
    match (url.host_str(), url.port_or_known_default()) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.to_owned(),
        _ => String::new(),
    }
}

pub struct NetworkProxy(Option<Proxy>);

impl NetworkProxy {
    pub fn new(proxy: &ModelProxy) -> Result<Self> {
        ProxySettings::read(proxy)?.transport()
    }

    pub fn apply(&self, builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        let builder = builder.no_proxy();
        match &self.0 {
            Some(proxy) => builder.proxy(proxy.clone()),
            None => builder,
        }
    }
}

pub fn apply_proxy(builder: reqwest::ClientBuilder, proxy: &ModelProxy) -> Result<reqwest::ClientBuilder> {
    Ok(NetworkProxy::new(proxy)?.apply(builder))
}

/// Child processes such as uv, Python and stdio MCP servers only learn the proxy
/// from their environment, so the resolved settings are written there.
pub fn configure_child_proxy_env(command: &mut Command, proxy: &ModelProxy) -> Result<()> {
    let settings = ProxySettings::read(proxy)?;
    if settings.mode == ProxyMode::None {
        // Python's urllib also reads the macOS system proxy, so a wildcard bypass keeps it direct.
        for key in ["HTTP_PROXY", "http_proxy", "HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"] {
            command.env_remove(key);
        }
        command.env("NO_PROXY", "*").env("no_proxy", "*");
        return Ok(());
    }
    let bypass = settings.system_bypass.iter().map(String::as_str)
        .chain(settings.env_bypass.split(','))
        .chain(["localhost", ".localhost", "127.0.0.1", "::1"])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>().join(",");
    let http = proxy_url(settings.http.as_deref())?;
    let https = proxy_url(settings.https.as_deref())?;
    command.env_remove("ALL_PROXY").env_remove("all_proxy");
    for (keys, url) in [
        (["HTTP_PROXY", "http_proxy"], http.as_ref()),
        (["HTTPS_PROXY", "https_proxy"], https.as_ref()),
    ] {
        for key in keys {
            match url {
                Some(url) => { command.env(key, url.as_str()); }
                None => { command.env_remove(key); }
            }
        }
    }
    command.env("NO_PROXY", &bypass).env("no_proxy", &bypass);
    Ok(())
}

/// Share commands also work before the service has been started.
pub fn saved_proxy(data_dir: &Path) -> Result<ModelProxy> {
    use rusqlite::{Connection, OpenFlags, OptionalExtension};

    #[derive(serde::Deserialize, Default)]
    #[serde(default)]
    struct Preferences {
        model_proxy: ModelProxy,
    }

    let database = data_dir.join("sag.sqlite");
    if !database.try_exists()? {
        return Ok(ModelProxy::default());
    }
    let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .context("无法打开代理设置数据库")?;
    let preferences: Option<String> = connection
        .query_row("SELECT value FROM preferences WHERE id=1", [], |row| row.get(0))
        .optional().context("无法读取代理设置")?;
    let mut proxy = match preferences {
        Some(value) => serde_json::from_str::<Preferences>(&value)?.model_proxy,
        None => ModelProxy::default(),
    };
    Ok(proxy.with_stored_password(stored_proxy_password(data_dir)?))
}

/// The proxy password is kept with the model keys, outside the preferences row.
pub fn stored_proxy_password(data_dir: &Path) -> Result<Option<String>> {
    let bytes = match std::fs::read(data_dir.join("credentials.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let values: std::collections::BTreeMap<String, String> =
        serde_json::from_slice(&bytes).context("本地密钥文件格式无效")?;
    Ok(values.get("proxy").cloned())
}

pub fn validate_proxy(proxy: &ModelProxy) -> Result<()> {
    // Length limits apply in every mode so stored settings stay bounded; the address is only
    // checked when it is about to be used, so a half-typed draft never blocks switching back.
    anyhow::ensure!(proxy.url.len() <= 512, "代理地址过长");
    anyhow::ensure!(proxy.username.chars().count() <= 256, "代理用户名过长");
    anyhow::ensure!(proxy.password.as_ref().map_or(true, |value| value.chars().count() <= 256), "代理密码过长");
    anyhow::ensure!(proxy.bypass.len() <= 4096, "直连地址列表过长");
    for rule in bypass_rules(&proxy.bypass) {
        anyhow::ensure!(rule.len() <= 253, "直连地址过长：{rule}");
        anyhow::ensure!(
            !rule.contains("://") && (!rule.contains('/') || bypass_network(&rule).is_some()),
            "直连地址只需填写域名、IP 或网段，不要包含协议和路径：{rule}"
        );
    }
    Bypass::new(&[], &proxy.bypass).context("直连地址无效")?;
    if proxy.mode == ProxyMode::Custom {
        custom_proxy_url(&proxy.url)?.context("请填写代理地址，例如 http://127.0.0.1:7890")?;
    }
    Ok(())
}

fn bypass_rules(value: &str) -> Vec<String> {
    value
        .split(|ch: char| ch == ',' || ch == ';' || ch.is_whitespace())
        .map(str::trim)
        .filter(|rule| !rule.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Sends one request through draft settings and reports the route it took, without saving.
pub async fn probe(proxy: &ModelProxy, url: &str) -> Result<String> {
    validate_proxy(proxy)?;
    let target = Url::parse(url.trim()).map_err(|_| anyhow::anyhow!("测试地址无效"))?;
    anyhow::ensure!(
        matches!(target.scheme(), "http" | "https") && target.host_str().is_some(),
        "测试地址需要是 http 或 https 地址"
    );
    let host = target.host_str().unwrap_or_default().to_owned();
    let settings = ProxySettings::read(proxy)?;
    let route = settings.route(&target)?;
    let builder = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none());
    let client = settings.transport()?.apply(builder).build()?;
    let response = client.get(target).send().await.map_err(|error| {
        let (timed_out, connect) = (error.is_timeout(), error.is_connect());
        // Drop the URL so proxy credentials never reach the UI or logs.
        let detail = error.without_url().to_string();
        match (&route, timed_out, connect) {
            (Route::Proxy(address), true, _) => anyhow::anyhow!("经代理 {address} 连接 {host} 超时"),
            (Route::Proxy(address), _, true) => anyhow::anyhow!("无法连接代理 {address}"),
            (Route::Proxy(address), _, _) => anyhow::anyhow!("经代理 {address} 连接 {host} 失败：{detail}"),
            (Route::Direct, true, _) => anyhow::anyhow!("直接连接 {host} 超时"),
            (Route::Direct, _, true) => anyhow::anyhow!("无法连接 {host}"),
            (Route::Direct, _, _) => anyhow::anyhow!("直接连接 {host} 失败：{detail}"),
        }
    })?;
    let status = response.status().as_u16();
    Ok(match (route, settings.mode) {
        (Route::Proxy(address), ProxyMode::Custom) => format!("已通过代理 {address} 连接 {host}（HTTP {status}）"),
        (Route::Proxy(address), _) => format!("已通过系统代理 {address} 连接 {host}（HTTP {status}）"),
        (Route::Direct, ProxyMode::Custom) => format!("{host} 属于直连地址，已直接连接（HTTP {status}）"),
        (Route::Direct, ProxyMode::System) => format!("未检测到系统代理，已直接连接 {host}（HTTP {status}）"),
        (Route::Direct, ProxyMode::None) => format!("已直接连接 {host}（HTTP {status}）"),
    })
}

/// HTTP, HTTPS and SOCKS5 (with local or remote DNS); SOCKS4 is not offered.
fn supported_scheme(scheme: &str) -> bool {
    matches!(scheme, "http" | "https" | "socks5" | "socks5h")
}

fn custom_proxy_url(value: &str) -> Result<Option<Url>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let parsed = if value.contains("://") { Url::parse(value) } else { Url::parse(&format!("http://{value}")) };
    let url = parsed.ok().filter(|url| {
        supported_scheme(url.scheme()) && url.host().is_some()
            && matches!(url.path(), "" | "/") && url.query().is_none() && url.fragment().is_none()
    }).context("代理地址须为 http://、https:// 或 socks5:// 地址，例如 http://proxy.example.com:8080")?;
    anyhow::ensure!(url.username().is_empty() && url.password().is_none(), "代理地址不能包含用户名或密码");
    Ok(Some(url))
}

fn proxy_url(value: Option<&str>) -> Result<Option<Url>> {
    value
        .map(|value| {
            let value = value.trim();
            let url = if value.contains("://") {
                Url::parse(value)
            } else {
                Url::parse(&format!("http://{value}"))
            }
            .context("系统或环境变量中的代理地址无效")?;
            anyhow::ensure!(
                supported_scheme(url.scheme()) && url.host().is_some(),
                "仅支持 HTTP、HTTPS 和 SOCKS5 代理"
            );
            Ok(url)
        })
        .transpose()
}

#[derive(Default)]
struct Bypass {
    simple_hosts: bool,
    networks: Vec<IpNet>,
    hosts: Vec<glob::Pattern>,
    domain_suffixes: Vec<String>,
}

impl Bypass {
    fn new(settings: &[String], env: &str) -> Result<Self> {
        let mut rules = Self::default();
        let system = settings.iter().map(|rule| (rule.as_str(), false));
        let environment = env.split(',').map(|rule| (rule, true));
        for (rule, from_env) in system.chain(environment) {
            let rule = rule.trim();
            if rule.is_empty() {
                continue;
            }
            if rule.eq_ignore_ascii_case("<local>") {
                rules.simple_hosts = true;
            } else if let Some(network) = bypass_network(rule) {
                rules.networks.push(network);
            } else {
                let host = rule.trim_matches(['[', ']']).trim_end_matches('.');
                if let Ok(ip) = host.parse::<IpAddr>() {
                    rules.networks.push(ip.into());
                } else if !host.contains(['*', '?']) && (host.starts_with('.') || from_env) {
                    rules
                        .domain_suffixes
                        .push(host.trim_start_matches('.').to_ascii_lowercase());
                } else {
                    rules
                        .hosts
                        .push(glob::Pattern::new(host).context("系统代理绕过规则无效")?);
                }
            }
        }
        Ok(rules)
    }

    fn matches(&self, url: &Url) -> bool {
        let host = url
            .host_str()
            .expect("HTTP URLs have a host")
            .trim_matches(['[', ']'])
            .trim_end_matches('.')
            .to_ascii_lowercase();
        let ip = host.parse::<IpAddr>().ok();
        host == "localhost"
            || host.ends_with(".localhost")
            || ip.is_some_and(|ip| {
                ip.to_canonical().is_loopback()
                    || self.networks.iter().any(|network| network.contains(&ip))
            })
            || (self.simple_hosts && ip.is_none() && !host.contains('.'))
            || self.domain_suffixes.iter().any(|suffix| {
                host == *suffix
                    || host
                        .strip_suffix(suffix)
                        .is_some_and(|prefix| prefix.ends_with('.'))
            })
            || self.hosts.iter().any(|pattern| {
                pattern.matches_with(
                    &host,
                    glob::MatchOptions {
                        case_sensitive: false,
                        ..glob::MatchOptions::new()
                    },
                )
            })
    }
}

fn bypass_network(rule: &str) -> Option<IpNet> {
    if let Ok(network) = rule.parse() {
        return Some(network);
    }
    // macOS also accepts shortened IPv4 networks, such as 169.254/16.
    let (address, prefix) = rule.split_once('/')?;
    let mut octets = [0; 4];
    for (i, octet) in address.split('.').enumerate() {
        *octets.get_mut(i)? = octet.parse().ok()?;
    }
    ipnet::Ipv4Net::new(octets.into(), prefix.parse().ok()?)
        .ok()
        .map(IpNet::V4)
}

#[cfg(target_os = "macos")]
fn system_settings() -> Result<ProxySettings> {
    use system_configuration::{
        core_foundation::{
            array::CFArray,
            base::{CFType, TCFType},
            number::CFNumber,
            string::{CFString, CFStringRef},
        },
        dynamic_store::SCDynamicStoreBuilder,
        sys::schema_definitions::*,
    };
    let store = SCDynamicStoreBuilder::new("SAG model proxy")
        .build()
        .context("无法读取 macOS 系统代理设置")?;
    let Some(proxies) = store.get_proxies() else {
        return Ok(ProxySettings::default());
    };
    let enabled = |key: CFStringRef| {
        proxies
            .find(key)
            .and_then(|value| value.downcast::<CFNumber>())
            .and_then(|value| value.to_i32())
            == Some(1)
    };
    let address = |enable, host, port| -> Result<Option<String>> {
        if !enabled(enable) {
            return Ok(None);
        }
        let host = proxies
            .find(host)
            .and_then(|value| value.downcast::<CFString>())
            .context("macOS 系统代理缺少地址")?
            .to_string();
        let port = proxies
            .find(port)
            .and_then(|value| value.downcast::<CFNumber>())
            .and_then(|value| value.to_i32())
            .filter(|port| (1..=65535).contains(port))
            .context("macOS 系统代理端口无效")?;
        let host = if host.parse::<std::net::Ipv6Addr>().is_ok() {
            format!("[{host}]")
        } else {
            host
        };
        Ok(Some(format!("{host}:{port}")))
    };
    // These immutable CFString keys are provided by SystemConfiguration.
    let (http, https, exceptions, exclude_simple) = unsafe {
        (
            address(
                kSCPropNetProxiesHTTPEnable,
                kSCPropNetProxiesHTTPProxy,
                kSCPropNetProxiesHTTPPort,
            )?,
            address(
                kSCPropNetProxiesHTTPSEnable,
                kSCPropNetProxiesHTTPSProxy,
                kSCPropNetProxiesHTTPSPort,
            )?,
            proxies.find(kSCPropNetProxiesExceptionsList),
            enabled(kSCPropNetProxiesExcludeSimpleHostnames),
        )
    };
    let mut system_bypass = Vec::new();
    if let Some(exceptions) = exceptions {
        let list = exceptions
            .downcast::<CFArray>()
            .context("macOS 代理绕过列表无效")?;
        for value in list.iter() {
            // Each value belongs to the retained exceptions array during this loop.
            let value = unsafe { CFType::wrap_under_get_rule(*value) };
            let rules = value
                .downcast::<CFString>()
                .context("macOS 代理绕过地址无效")?
                .to_string();
            system_bypass.extend(rules.split(',').map(str::to_owned));
        }
    }
    if exclude_simple {
        system_bypass.push("<local>".into());
    }
    Ok(ProxySettings {
        http,
        https,
        system_bypass,
        ..ProxySettings::default()
    })
}

#[cfg(windows)]
fn system_settings() -> Result<ProxySettings> {
    use windows_sys::Win32::{
        Foundation::{GlobalFree, ERROR_FILE_NOT_FOUND},
        Networking::WinHttp::{
            WinHttpGetIEProxyConfigForCurrentUser, WINHTTP_CURRENT_USER_IE_PROXY_CONFIG,
        },
    };
    let mut raw = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default();
    // The background process runs as the user whose proxy settings are queried.
    let success = unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut raw) } != 0;
    let failure = (!success).then(std::io::Error::last_os_error);
    let take = |ptr: *mut u16| {
        if ptr.is_null() {
            return String::new();
        }
        // WinHTTP returns a NUL-terminated allocation released with GlobalFree.
        unsafe {
            let mut len = 0;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let value = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
            GlobalFree(ptr.cast());
            value
        }
    };
    let server = take(raw.lpszProxy);
    let bypass = take(raw.lpszProxyBypass);
    take(raw.lpszAutoConfigUrl);
    if let Some(error) = failure {
        if error.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32) {
            return Ok(ProxySettings::default());
        }
        return Err(error).context("无法读取 Windows 系统代理设置");
    }
    Ok(windows_settings(&server, &bypass))
}

#[cfg(any(windows, test))]
fn windows_settings(server: &str, bypass: &str) -> ProxySettings {
    let mut settings = ProxySettings::default();
    let mut default = None;
    for entry in server
        .split(|ch: char| ch == ';' || ch.is_whitespace())
        .filter(|s| !s.is_empty())
    {
        match entry.split_once('=') {
            Some((scheme, address)) if scheme.eq_ignore_ascii_case("http") => {
                settings.http = Some(address.into())
            }
            Some((scheme, address)) if scheme.eq_ignore_ascii_case("https") => {
                settings.https = Some(address.into())
            }
            Some(_) => {}
            None => default = Some(entry.to_owned()),
        }
    }
    settings.http = settings.http.or_else(|| default.clone());
    settings.https = settings.https.or(default);
    settings.system_bypass = bypass
        .split(|ch: char| ch == ';' || ch.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    settings
}

#[cfg(not(any(windows, target_os = "macos")))]
fn system_settings() -> Result<ProxySettings> {
    Ok(ProxySettings::default())
}

#[cfg(test)]
#[path = "network_proxy_tests.rs"]
mod tests;
