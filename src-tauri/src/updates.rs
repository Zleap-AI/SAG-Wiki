use anyhow::{bail, ensure, Context, Result};
use reqwest::Url;
use sag_protocol::types::{UpdateSettings, UpdateSource};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{ipc::Channel, State};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Default)]
pub struct UpdateSession(Mutex<Session>);

#[derive(Default)]
struct Session {
    busy: bool,
    package: Option<(Update, Arc<Vec<u8>>)>,
}

#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    stage: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct UpdateInfo {
    current_version: String,
    latest_version: String,
    available: bool,
    notes: String,
    download_url: Option<String>,
}

#[derive(Deserialize)]
struct UpdateManifest {
    version: String,
    #[serde(default)]
    notes: String,
    platforms: BTreeMap<String, ManifestPackage>,
}
#[derive(Deserialize)]
struct ManifestPackage {
    url: String,
}

fn https_url(value: &str) -> Result<Url> {
    let url = Url::parse(value.trim()).context("更新地址无效")?;
    ensure!(
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none(),
        "更新地址必须使用 HTTPS，且不能包含账号或密码"
    );
    Ok(url)
}

fn github_endpoint(value: &str) -> Result<Url> {
    let url = https_url(value)?;
    ensure!(
        url.host_str() == Some("github.com") && url.port().is_none(),
        "请填写 GitHub 项目地址"
    );
    let segments: Vec<_> = url.path().trim_matches('/').split('/').collect();
    ensure!(
        segments.len() == 2 && segments.iter().all(|part| !part.is_empty()),
        "请填写 GitHub 项目地址"
    );
    let repo = segments[1].trim_end_matches(".git");
    ensure!(!repo.is_empty(), "请填写 GitHub 项目地址");
    Ok(Url::parse(&format!(
        "https://github.com/{}/{repo}/releases/latest/download/latest-v2.json",
        segments[0]
    ))?)
}

fn update_info(current: &str, latest: &str, notes: String) -> Result<UpdateInfo> {
    let current_version = Version::parse(current).context("当前版本号无效")?;
    let latest_version =
        Version::parse(latest.trim().trim_start_matches('v')).context("发布版本号无效")?;
    ensure!(latest_version.pre.is_empty(), "更新源返回了预发布版本");
    Ok(UpdateInfo {
        current_version: current.into(),
        available: latest_version.cmp_precedence(&current_version).is_gt(),
        latest_version: latest_version.to_string(),
        notes,
        download_url: None,
    })
}

fn manifest_update(
    release: UpdateManifest,
    current: &str,
    os: &str,
    arch: &str,
) -> Result<UpdateInfo> {
    let mut info = update_info(current, &release.version, release.notes)?;
    if info.available {
        let os = if os == "macos" { "darwin" } else { os };
        let installer = release
            .platforms
            .get(&format!("{os}-{arch}"))
            .context("新版本尚未提供当前系统的安装包")?;
        info.download_url = Some(https_url(&installer.url)?.to_string());
    }
    Ok(info)
}

async fn check(config: UpdateSettings, current: &str, os: &str, arch: &str) -> Result<UpdateInfo> {
    let endpoint = manifest_endpoint(&config)?;
    let client = reqwest::Client::builder()
        .user_agent(format!("SAG-Wiki/{current}"))
        .https_only(true)
        .timeout(Duration::from_secs(15))
        .build()?;
    let response = client
        .get(endpoint)
        .header("Accept", "application/json")
        .send()
        .await?;
    if !response.status().is_success() {
        bail!("更新检查失败（HTTP {}）", response.status().as_u16());
    }
    manifest_update(response.json().await?, current, os, arch)
}

#[tauri::command]
pub async fn check_app_update(
    app: tauri::AppHandle,
    config: UpdateSettings,
) -> Result<UpdateInfo, String> {
    check(
        config,
        &app.package_info().version.to_string(),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
    .await
    .map_err(|error| format!("{error:#}"))
}

fn manifest_endpoint(config: &UpdateSettings) -> Result<Url> {
    match config.source {
        UpdateSource::Github => github_endpoint(&config.github_url),
        UpdateSource::Website => https_url(&config.website_url),
    }
}

async fn download(
    app: &tauri::AppHandle,
    config: &UpdateSettings,
    version: &str,
    on_progress: Channel<DownloadProgress>,
) -> Result<(Update, Arc<Vec<u8>>)> {
    let update = app
        .updater_builder()
        .endpoints(vec![manifest_endpoint(config)?])?
        .timeout(Duration::from_secs(15))
        .build()?
        .check()
        .await
        .context("无法获取已签名的更新，请确认更新清单和签名已发布")?
        .context("当前已是最新版本，请重新检查更新")?;
    https_url(update.download_url.as_str())?;
    download_package(update, version, on_progress).await
}

async fn download_package(
    mut update: Update,
    version: &str,
    on_progress: Channel<DownloadProgress>,
) -> Result<(Update, Arc<Vec<u8>>)> {
    ensure!(update.version == version, "发布版本已变化，请重新检查更新");
    update.timeout = Some(Duration::from_secs(1800));
    let mut downloaded = 0;
    let bytes = update
        .download(
            |length, total| {
                downloaded += length as u64;
                if let Err(error) = on_progress.send(DownloadProgress {
                    stage: "downloading",
                    downloaded,
                    total,
                }) {
                    eprintln!("update progress: {error}");
                }
            },
            || {
                if let Err(error) = on_progress.send(DownloadProgress {
                    stage: "verifying",
                    downloaded: 0,
                    total: None,
                }) {
                    eprintln!("update progress: {error}");
                }
            },
        )
        .await
        .context("更新下载或签名校验失败")?;
    Ok((update, Arc::new(bytes)))
}

#[tauri::command]
pub async fn download_app_update(
    app: tauri::AppHandle,
    session: State<'_, UpdateSession>,
    config: UpdateSettings,
    version: String,
    on_progress: Channel<DownloadProgress>,
) -> Result<(), String> {
    {
        let mut state = session.0.lock().unwrap();
        if state.busy {
            return Err("更新操作正在进行".into());
        }
        state.busy = true;
        state.package = None;
    }
    let result = download(&app, &config, &version, on_progress).await;
    let mut state = session.0.lock().unwrap();
    state.busy = false;
    match result {
        Ok(package) => {
            state.package = Some(package);
            Ok(())
        }
        Err(error) => Err(format!("{error:#}")),
    }
}

#[tauri::command]
pub async fn install_app_update(
    app: tauri::AppHandle,
    session: State<'_, UpdateSession>,
    version: String,
) -> Result<(), String> {
    // Installing from cargo run would replace a development build, not the installed application.
    if cfg!(debug_assertions) {
        return Err("请在安装版中执行更新".into());
    }
    let (update, bytes) = {
        let mut state = session.0.lock().unwrap();
        if state.busy {
            return Err("更新操作正在进行".into());
        }
        let (update, bytes) = state.package.as_ref().ok_or("请先下载更新")?;
        if update.version != version {
            return Err("下载的更新版本不一致，请重新下载".into());
        }
        let package = (update.clone(), bytes.clone());
        state.busy = true;
        package
    };
    let result = tauri::async_runtime::spawn_blocking(move || update.install(bytes.as_ref())).await;
    session.0.lock().unwrap().busy = false;
    result
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    // Windows' updater launches the installer, exits and relaunches the app itself.
    #[cfg(not(windows))]
    app.restart();
    #[cfg(windows)]
    let _ = app;
    #[cfg(windows)]
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn checks_release_manifest_without_rest_api() {
        let config = UpdateSettings::default();
        assert_eq!(
            github_endpoint(&config.github_url).unwrap(),
            Url::parse("https://github.com/Zleap-AI/SAG-Wiki/releases/latest/download/latest-v2.json")
                .unwrap(),
            "checking and downloading must use the same release manifest, without the REST API"
        );
        assert_eq!(
            github_endpoint(&config.github_url).unwrap(),
            manifest_endpoint(&config).unwrap()
        );
    }

    #[test]
    fn resolves_signed_manifests_without_opening_a_browser() {
        let config = UpdateSettings {
            github_url: "https://github.com/Zleap-AI/SAG-Wiki.git/".into(),
            ..Default::default()
        };
        assert_eq!(
            manifest_endpoint(&config).unwrap().as_str(),
            "https://github.com/Zleap-AI/SAG-Wiki/releases/latest/download/latest-v2.json"
        );
        let config = UpdateSettings {
            source: UpdateSource::Website,
            website_url: "https://example.com/latest.json".into(),
            ..Default::default()
        };
        assert_eq!(
            manifest_endpoint(&config).unwrap().as_str(),
            "https://example.com/latest.json"
        );
    }

    // Real HTTP transfer and updater signature/installation code, scoped to temporary files.
    #[cfg(target_os = "macos")]
    #[test]
    fn downloads_verifies_and_replaces_only_the_temporary_application() {
        use std::io::{Read, Write};
        use tauri::ipc::InvokeResponseBody;
        use tauri::test::{mock_builder, mock_context, noop_assets};
        let temp = tempfile::tempdir().unwrap();
        let application = temp.path().join("SAG-Test.app/Contents/MacOS/SAG-Test");
        std::fs::create_dir_all(application.parent().unwrap()).unwrap();
        std::fs::write(&application, b"old application").unwrap();
        let data = temp.path().join("user-data");
        std::fs::create_dir(&data).unwrap();
        std::fs::write(data.join("conversation.json"), b"preserved conversation").unwrap();
        std::fs::write(data.join("original.md"), b"preserved document").unwrap();
        let mut context = mock_context(noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            json!({
                "pubkey":include_str!("update-fixtures/public.key").trim(),
                "dangerousInsecureTransportProtocol":true,
                "requireSignedVersion":true
            }),
        );
        let app = mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap();
        let payload = include_bytes!("update-fixtures/SAG-Test.app.tar.gz");
        let signature = include_str!("update-fixtures/SAG-Test.app.tar.gz.sig").trim();
        for corrupt in [true, false] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                for request in 0..2 {
                    let (mut stream, _) = listener.accept().unwrap();
                    let mut buffer = [0; 4096];
                    stream.read(&mut buffer).unwrap();
                    let manifest = serde_json::to_vec(
                        &json!({"version":"9.9.9","platforms":{"darwin-aarch64":{
                            "url":format!("http://{address}/update"),"signature":signature
                        }}}),
                    )
                    .unwrap();
                    let body: &[u8] = if request == 0 {
                        manifest.as_slice()
                    } else if corrupt {
                        b"corrupt download"
                    } else {
                        payload
                    };
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .unwrap();
                    stream.write_all(body).unwrap();
                }
            });
            let updater = app
                .updater_builder()
                .executable_path(&application)
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .endpoints(vec![format!("http://{address}/latest.json")
                    .parse()
                    .unwrap()])
                .unwrap()
                .build()
                .unwrap();
            let update = tauri::async_runtime::block_on(updater.check())
                .unwrap()
                .unwrap();
            let progress = Arc::new(Mutex::new(Vec::new()));
            let recorded = progress.clone();
            let channel = Channel::new(move |body| {
                if let InvokeResponseBody::Json(value) = body {
                    recorded
                        .lock()
                        .unwrap()
                        .push(serde_json::from_str::<serde_json::Value>(&value).unwrap());
                }
                Ok(())
            });
            let result = tauri::async_runtime::block_on(download_package(update, "9.9.9", channel));
            server.join().unwrap();
            if corrupt {
                assert!(result.is_err());
                assert_eq!(std::fs::read(&application).unwrap(), b"old application");
            } else {
                let (update, bytes) = result.unwrap();
                assert_eq!(bytes.as_slice(), payload);
                assert_eq!(
                    std::fs::read(&application).unwrap(),
                    b"old application",
                    "download must not install"
                );
                update.install(bytes.as_ref()).unwrap();
                assert_eq!(
                    std::fs::read(&application).unwrap(),
                    b"updated fixture application\n"
                );
                let progress = progress.lock().unwrap();
                assert!(progress.iter().any(|event| event["stage"] == "downloading"
                    && event["downloaded"].as_u64().unwrap() > 0));
                assert_eq!(progress.last().unwrap()["stage"], "verifying");
            }
            assert_eq!(
                std::fs::read(data.join("conversation.json")).unwrap(),
                b"preserved conversation"
            );
            assert_eq!(
                std::fs::read(data.join("original.md")).unwrap(),
                b"preserved document"
            );
        }
    }

    fn release() -> UpdateManifest {
        serde_json::from_value(json!({
            "version":"0.1.10", "notes":"Release notes", "platforms":{
                "darwin-aarch64":{"url":"https://example.com/mac-arm.app.tar.gz"},
                "darwin-x86_64":{"url":"https://example.com/mac-intel.app.tar.gz"},
                "windows-x86_64":{"url":"https://example.com/windows.exe"}
            }
        }))
        .unwrap()
    }

    #[test]
    fn selects_the_matching_installer() {
        for (os, arch, suffix) in [
            ("macos", "aarch64", "mac-arm.app.tar.gz"),
            ("macos", "x86_64", "mac-intel.app.tar.gz"),
            ("windows", "x86_64", "windows.exe"),
        ] {
            let info = manifest_update(release(), "0.1.9", os, arch).unwrap();
            assert!(info.available);
            assert_eq!(info.notes, "Release notes");
            assert!(info.download_url.unwrap().ends_with(suffix));
        }
        assert!(manifest_update(release(), "0.1.9", "windows", "aarch64").is_err());
    }

    #[test]
    fn compares_semantic_versions_and_does_not_offer_downgrades() {
        assert!(
            manifest_update(release(), "0.1.9", "macos", "aarch64")
                .unwrap()
                .available
        );
        for current in ["0.1.10", "0.1.10+customer1", "0.2.0"] {
            let info = manifest_update(release(), current, "macos", "aarch64").unwrap();
            assert!(!info.available);
            assert!(info.download_url.is_none());
        }
        assert!(update_info("0.1.5", "v0.2.0-beta.1", String::new()).is_err());
        assert!(update_info("0.1.5", "invalid", String::new()).is_err());
    }

    #[test]
    fn validates_external_addresses() {
        assert_eq!(
            github_endpoint("https://github.com/customer/wiki.git/")
                .unwrap()
                .as_str(),
            "https://github.com/customer/wiki/releases/latest/download/latest-v2.json"
        );
        for address in [
            "http://github.com/a/b",
            "https://github.com.evil.test/a/b",
            "https://github.com/a",
            "https://github.com/a/b/releases",
            "https://user:secret@github.com/a/b",
        ] {
            assert!(github_endpoint(address).is_err(), "{address}");
        }
        assert!(https_url("file:///tmp/update.exe").is_err());
        assert!(https_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn supports_a_future_website_source() {
        let release = serde_json::from_value(
            json!({"version":"0.2.0", "notes":"Website notes", "platforms":{
                "darwin-aarch64":{"url":"https://example.com/new.dmg"},
                "windows-x86_64":{"url":"https://example.com/new.exe"}
            }}),
        )
        .unwrap();
        let info = manifest_update(release, "0.1.5", "windows", "x86_64").unwrap();
        assert_eq!(
            info.download_url.as_deref(),
            Some("https://example.com/new.exe")
        );
        assert_eq!(info.notes, "Website notes");
        let settings: sag_protocol::types::Settings = serde_json::from_str("{}").unwrap();
        assert!(matches!(settings.updates.source, UpdateSource::Github));
        assert!(settings.updates.website_url.is_empty());
    }

    #[test]
    #[ignore = "read-only live GitHub integration check"]
    fn checks_real_github_release() {
        for (os, arch, suffix) in [
            ("macos", "aarch64", "macOS-arm64.app.tar.gz"),
            ("windows", "x86_64", "Windows-x64-setup.exe"),
        ] {
            let info =
                tauri::async_runtime::block_on(check(UpdateSettings::default(), "0.1.6", os, arch))
                    .unwrap();
            assert!(info.available);
            assert!(info.download_url.unwrap().ends_with(suffix));
            assert!(!info.notes.is_empty());
            println!(
                "GitHub manifest version for {os}/{arch}: {}",
                info.latest_version
            );
        }
    }
}
