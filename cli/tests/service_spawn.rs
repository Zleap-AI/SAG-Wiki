use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

struct Foreground(Child);

impl Foreground {
    fn wait(&mut self, timeout: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.0.try_wait().expect("inspect foreground CLI") {
                return Some(status);
            }
            if Instant::now() >= deadline {
                return None;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Foreground {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.wait(Duration::from_secs(2));
        }
    }
}

struct ServiceCleanup(PathBuf);

impl ServiceCleanup {
    fn stop(&self) -> Result<(), String> {
        if !self.0.join("sag-service.json").exists() {
            return Ok(());
        }
        runtime().block_on(async {
            tokio::time::timeout(Duration::from_secs(5), async {
                if let Some(client) = sag_client::Client::connect(&self.0).await? {
                    client
                        .call(sag_protocol::Operation::Shutdown, Duration::from_secs(3))
                        .await?;
                    sag_client::wait_stopped(&self.0).await?;
                }
                Ok::<_, anyhow::Error>(())
            })
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())
        })
    }
}

impl Drop for ServiceCleanup {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("isolated service cleanup failed: {error}");
        }
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
}

fn running_service(data: &Path) -> Result<u32, String> {
    runtime().block_on(async {
        tokio::time::timeout(Duration::from_secs(5), async {
            let client = sag_client::Client::connect(data)
                .await?
                .ok_or_else(|| anyhow::anyhow!("background service is not running"))?;
            let response = client
                .call(sag_protocol::Operation::Info, Duration::from_secs(3))
                .await?;
            assert!(matches!(response, sag_protocol::Output::Info(_)));
            Ok::<_, anyhow::Error>(client.info().pid)
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
    })
}

#[test]
fn background_service_releases_both_foreground_capture_pipes_while_still_running() {
    let temporary = tempfile::tempdir().unwrap();
    let data = temporary.path().join("data");
    let cleanup = ServiceCleanup(data.clone());
    let mut foreground = Foreground(
        Command::new(env!("CARGO_BIN_EXE_sag"))
            .env("SAG_DATA_DIR", &data)
            .args([
                "config",
                "--url",
                "http://127.0.0.1:1/decisions",
                "--model",
                "service-spawn-fixture",
                "--auth",
                "none",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let foreground_pid = foreground.0.id();
    let readers: [(&str, Box<dyn Read + Send>); 2] = [
        ("stdout", Box::new(foreground.0.stdout.take().unwrap())),
        ("stderr", Box::new(foreground.0.stderr.take().unwrap())),
    ];
    let (sender, receiver) = mpsc::channel();
    for (name, mut reader) in readers {
        let sender = sender.clone();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = reader.read_to_end(&mut bytes);
            let _ = sender.send((name, result, bytes));
        });
    }
    drop(sender);

    let status = foreground.wait(Duration::from_secs(15));
    let first = receiver.recv_timeout(Duration::from_secs(5));
    let second = receiver.recv_timeout(Duration::from_secs(5));
    // Probe before shutdown: EOF is required while the daemon is still alive,
    // rather than being produced as a side effect of test cleanup.
    let service_pid = running_service(&data);
    drop(foreground);
    let stopped = cleanup.stop();

    assert!(
        status.is_some_and(|status| status.success()),
        "foreground CLI did not exit successfully: {status:?}"
    );
    let first = first.expect("stdout or stderr was retained by the running service");
    let second = second.expect("stdout or stderr was retained by the running service");
    assert_ne!(first.0, second.0, "both capture pipes must reach EOF");
    for (name, result, bytes) in [first, second] {
        assert!(result.is_ok(), "could not drain {name}: {result:?}");
        if name == "stdout" {
            let config: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(config["model"], "service-spawn-fixture");
        } else {
            assert!(String::from_utf8(bytes).unwrap().contains("配置已保存"));
        }
    }
    assert_ne!(
        service_pid.expect("background service must still answer after both EOFs"),
        foreground_pid
    );
    assert!(
        stopped.is_ok(),
        "could not stop the isolated background service: {stopped:?}"
    );
}
