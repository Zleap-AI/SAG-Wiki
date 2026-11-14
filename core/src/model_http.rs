use anyhow::{Context, Result};
use ipnet::IpNet;
use reqwest::{Client, Proxy, Url};
use std::{net::IpAddr, sync::Mutex};

#[derive(Default)]
pub(crate) struct ModelHttp(Mutex<Option<(ProxySettings, Client)>>);

impl ModelHttp {
    pub(crate) fn client(&self) -> Result<Client> {
        self.refresh(ProxySettings::read)
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
    http: Option<String>,
    https: Option<String>,
    system_bypass: Vec<String>,
    env_bypass: String,
}

impl ProxySettings {
    fn read() -> Result<Self> {
        let settings = system_settings()?;
        // Preserve reqwest's behavior when a CGI host turns HTTP headers into environment variables.
        if std::env::var_os("REQUEST_METHOD").is_some() {
            return Ok(settings);
        }
        Ok(settings.with_env(|name| std::env::var(name).ok()))
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
        let http = proxy_url(self.http.as_deref())?;
        let https = proxy_url(self.https.as_deref())?;
        let mut builder = Client::builder().no_proxy();
        if http.is_some() || https.is_some() {
            let bypass = Bypass::new(&self.system_bypass, &self.env_bypass)?;
            let proxy = Proxy::custom(move |url| {
                if bypass.matches(url) {
                    return None;
                }
                match url.scheme() {
                    "http" => http.clone(),
                    "https" => https.clone(),
                    _ => None,
                }
            });
            builder = builder.proxy(proxy);
        }
        Ok(builder.build()?)
    }
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
                matches!(url.scheme(), "http" | "https") && url.host().is_some(),
                "模型请求仅支持 HTTP 和 HTTPS 代理"
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
#[path = "model_http_tests.rs"]
mod tests;
