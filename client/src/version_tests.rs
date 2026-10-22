use super::*;
use std::{fs, os::unix::fs::PermissionsExt};
use tokio::{net::TcpListener, task::JoinHandle};

fn release(version: &str, build: &str) -> BuildInfo {
    BuildInfo {
        version: Some(version.into()),
        build_id: build.into(),
        channel: "release".into(),
        protocol_version: VERSION,
    }
}

fn executable(data: &Path, build: &BuildInfo) -> PathBuf {
    let path = data.join("sag");
    let json = serde_json::to_string(build).unwrap();
    fs::write(&path, format!("#!/bin/sh\nif [ \"$1\" = --build-info ]; then\n  printf '%s\\n' '{json}'\nelse\n  touch \"$SAG_DATA_DIR/started\"\nfi\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

async fn service(data: &Path, build: &BuildInfo) -> JoinHandle<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = data.join("sag-service.json");
    fs::write(
        &endpoint,
        serde_json::to_vec(&Endpoint {
            protocol_version: build.protocol_version,
            port: listener.local_addr().unwrap().port(),
            pid: 1,
            token: "test-token".into(),
        })
        .unwrap(),
    )
    .unwrap();
    let info = ServiceInfo {
        version: build.version.clone(),
        protocol_version: build.protocol_version,
        build_id: build.build_id.clone(),
        channel: build.channel.clone(),
        pid: 1,
        executable: "test-service".into(),
        data_directory: data.display().to_string(),
    };
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut read = BufReader::new(read);
        assert!(matches!(
            wire::read(&mut read).await.unwrap(),
            Some(Frame::Hello { .. })
        ));
        wire::write(&mut write, &Frame::Welcome { info: info.clone() })
            .await
            .unwrap();
        while let Some(Frame::Request { id, operation }) = wire::read(&mut read).await.unwrap() {
            let result = match operation {
                Operation::Info => Output::Info(info.clone()),
                Operation::Shutdown => {
                    fs::remove_file(&endpoint).unwrap();
                    wire::write(
                        &mut write,
                        &Frame::Response {
                            id,
                            result: Ok(Output::Unit),
                        },
                    )
                    .await
                    .unwrap();
                    return;
                }
                _ => panic!("unexpected operation"),
            };
            wire::write(
                &mut write,
                &Frame::Response {
                    id,
                    result: Ok(result),
                },
            )
            .await
            .unwrap();
        }
    })
}

#[tokio::test]
async fn old_desktop_reuses_same_or_newer_release_without_restarting_it() {
    for version in ["0.1.0", "0.2.0"] {
        let dir = tempfile::tempdir().unwrap();
        let current = release("0.1.0", "old-desktop");
        let binary = executable(dir.path(), &current);
        let server = service(dir.path(), &release(version, "installed-cli")).await;
        let client = ensure_build(dir.path(), &binary, &current).await.unwrap();
        assert_eq!(client.info().build_id, "installed-cli");
        assert!(matches!(
            client
                .call(Operation::Info, Duration::from_secs(1))
                .await
                .unwrap(),
            Output::Info(_)
        ));
        assert!(
            !dir.path().join("started").exists(),
            "must reuse the running service"
        );
        server.abort();
    }
}

#[tokio::test]
async fn old_desktop_starts_the_newer_installed_cli_and_upgrades_an_older_service() {
    for running in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let current = release("0.1.0", "old-desktop");
        let installed = release("0.2.0", "new-cli");
        let binary = executable(dir.path(), &installed);
        let old = if running {
            Some(service(dir.path(), &current).await)
        } else {
            None
        };
        let data = dir.path().to_owned();
        let new = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(2), async {
                while !data.join("started").exists() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            service(&data, &installed).await
        });
        let client = ensure_build(dir.path(), &binary, &current).await.unwrap();
        assert_eq!(client.info().build_id, "new-cli");
        assert_eq!(client.info().version.as_deref(), Some("0.2.0"));
        assert!(matches!(
            client
                .call(Operation::Info, Duration::from_secs(1))
                .await
                .unwrap(),
            Output::Info(_)
        ));
        new.await.unwrap().abort();
        if let Some(old) = old {
            old.await.unwrap();
        }
    }
}

#[tokio::test]
async fn incompatible_or_unversioned_service_is_not_stopped() {
    for unversioned in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let current = release("0.1.0", "desktop");
        let binary = executable(dir.path(), &current);
        let mut installed = release("0.2.0", "other-cli");
        if unversioned {
            installed.version = None;
        } else {
            installed.protocol_version += 1;
        }
        let server = service(dir.path(), &installed).await;
        let before = fs::read(dir.path().join("sag-service.json")).unwrap();
        let error = ensure_build(dir.path(), &binary, &current)
            .await
            .err()
            .unwrap();
        assert!(
            error.to_string().contains(if unversioned {
                "未提供版本号"
            } else {
                "协议"
            }),
            "{error}"
        );
        assert_eq!(
            fs::read(dir.path().join("sag-service.json")).unwrap(),
            before
        );
        assert!(!dir.path().join("started").exists());
        server.abort();
    }
}

#[tokio::test]
async fn incompatible_executable_never_replaces_a_running_service() {
    for development in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut current = release("0.1.0", "desktop");
        let mut installed = release("0.2.0", "other-cli");
        let mut running = release("0.2.0", "running-cli");
        if development {
            current.channel = "development".into();
            installed.channel = "development".into();
            running.channel = "development".into();
        } else {
            installed.protocol_version += 1;
        }
        let binary = executable(dir.path(), &installed);
        let server = service(dir.path(), &running).await;
        let before = fs::read(dir.path().join("sag-service.json")).unwrap();
        let error = ensure_build(dir.path(), &binary, &current)
            .await
            .err()
            .unwrap();
        assert!(
            error.to_string().contains(if development {
                "构建不同"
            } else {
                "协议不兼容"
            }),
            "{error}"
        );
        assert_eq!(
            fs::read(dir.path().join("sag-service.json")).unwrap(),
            before
        );
        assert!(!dir.path().join("started").exists());
        server.abort();
    }
}
