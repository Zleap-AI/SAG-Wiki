#![cfg(unix)]
mod support;
use std::{fs::File, io::{Read, Write}, os::fd::{AsRawFd, FromRawFd}, os::unix::process::CommandExt, process::{Child, Command, Stdio}, time::{Duration, Instant}};

struct Terminal { child: Child, master: Option<File>, output: String }
impl Terminal {
    fn start(mut command: Command) -> Self {
        let (mut master, mut slave) = (-1, -1);
        assert_eq!(unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) }, 0);
        let master = unsafe { File::from_raw_fd(master) };
        assert_eq!(unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) }, 0);
        let slave = unsafe { File::from_raw_fd(slave) };
        for fd in [master.as_raw_fd(), slave.as_raw_fd()] {
            assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) }, 0);
        }
        command.stdin(Stdio::from(slave.try_clone().unwrap())).stdout(Stdio::from(slave.try_clone().unwrap())).stderr(Stdio::from(slave));
        unsafe { command.pre_exec(|| {
            if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 { return Err(std::io::Error::last_os_error()); }
            Ok(())
        }); }
        Self { child: command.spawn().unwrap(), master: Some(master), output: String::new() }
    }
    fn read_pending(&mut self) {
        loop {
            let mut bytes = [0; 4096];
            match self.master.as_mut().unwrap().read(&mut bytes) {
                Ok(0) => break,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock || error.raw_os_error() == Some(libc::EIO) => break,
                Err(error) => panic!("reading terminal: {error}"),
                Ok(size) => self.output.push_str(&String::from_utf8_lossy(&bytes[..size])),
            }
        }
    }
    fn wait_for(&mut self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while !self.output.contains(text) && Instant::now() < deadline {
            self.read_pending();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(self.output.contains(text), "missing {text}: {}", self.output);
    }
    fn send(&mut self, text: &str) { self.master.as_mut().unwrap().write_all(text.as_bytes()).unwrap(); }
    fn send_secret(&mut self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let mut mode = std::mem::MaybeUninit::<libc::termios>::uninit();
            assert_eq!(unsafe { libc::tcgetattr(self.master.as_ref().unwrap().as_raw_fd(), mode.as_mut_ptr()) }, 0);
            if unsafe { mode.assume_init() }.c_lflag & libc::ECHO == 0 { break; }
            assert!(Instant::now() < deadline, "secret input still echoes");
            std::thread::sleep(Duration::from_millis(5));
        }
        self.send(text);
    }
    fn finish(&mut self) { self.expect_exit(0); }
    fn expect_exit(&mut self, code: i32) {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            self.read_pending();
            if let Some(status) = self.child.try_wait().unwrap() { assert_eq!(status.code(), Some(code), "{}", self.output); return; }
            assert!(Instant::now() < deadline, "CLI did not finish: {}", self.output);
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        self.master.take();
        let _ = self.child.kill();
        // Reap a finished child without blocking the test harness during cleanup.
        let _ = self.child.try_wait();
    }
}

#[test]
fn first_run_guides_configuration_and_does_not_echo_the_key() {
    let data = tempfile::tempdir().unwrap();
    let _cleanup=support::ServiceCleanup(data.path().to_path_buf());
    let mut command = Command::new(env!("CARGO_BIN_EXE_sag"));
    command.env("SAG_DATA_DIR", data.path());
    let mut terminal = Terminal::start(command);
    terminal.wait_for("JEV 接口地址 [https://api.302ai.cn/typesafeai/v1]：");
    assert!(terminal.output.contains("https://api.302ai.cn/typesafeai/v1"));
    terminal.send("\n");
    terminal.wait_for("模型名称 [jev-latest]：");
    assert!(terminal.output.contains("jev-latest"));
    terminal.send("\n");
    terminal.wait_for("API 密钥（输入隐藏）：");
    terminal.send_secret("first-run-private-key\n");
    terminal.wait_for("配置已保存");
    terminal.finish();
    assert!(!terminal.output.contains("first-run-private-key"));
    let shown = Command::new(env!("CARGO_BIN_EXE_sag")).arg("config")
        .env("SAG_DATA_DIR", data.path()).output().unwrap();
    let config: serde_json::Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(config["has_key"], true);
    let mut again = Command::new(env!("CARGO_BIN_EXE_sag"));
    again.env("SAG_DATA_DIR", data.path());
    let mut terminal = Terminal::start(again);
    terminal.wait_for("Usage:");
    terminal.finish();
    assert!(!terminal.output.contains("API 密钥"));
}

#[test]
fn interactive_local_matching_skips_first_use_model_setup() {
    let dir = tempfile::tempdir().unwrap();
    let _cleanup = support::ServiceCleanup(dir.path().join("data"));
    let root = dir.path().join("project");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("note.txt"), "needle\n").unwrap();
    let added = Command::new(env!("CARGO_BIN_EXE_sag"))
        .env("SAG_DATA_DIR", dir.path().join("data")).current_dir(&root).arg("add").output().unwrap();
    assert!(added.status.success());
    for flag in ["-E", "-F"] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sag"));
        command.args([flag, "needle", "-c"]).current_dir(&root).env("SAG_DATA_DIR", dir.path().join("data"));
        let mut terminal = Terminal::start(command);
        terminal.wait_for("needle");
        terminal.finish();
        assert!(terminal.output.contains("./note.txt:1-1"));
        assert!(!terminal.output.contains("API 密钥"));
    }
}

#[test]
fn piped_installer_guides_setup_through_the_terminal() {
    let home = tempfile::tempdir().unwrap();
    let _cleanup = support::ServiceCleanup(home.path().join("data"));
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "cat \"$1\" | sh", "installer", concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/install.sh")])
        .env_clear().env("HOME", home.path()).env("SHELL", "/bin/zsh")
        .env("PATH", "/usr/bin:/bin").env("SAG_DATA_DIR", home.path().join("data"))
        .env("SAG_BINARY", env!("CARGO_BIN_EXE_sag"));
    let mut terminal = Terminal::start(command);
    terminal.wait_for("JEV 接口地址 [https://api.302ai.cn/typesafeai/v1]："); terminal.send("\n");
    terminal.wait_for("模型名称 [jev-latest]："); terminal.send("\n");
    terminal.wait_for("API 密钥（输入隐藏）："); terminal.send_secret("installer-hidden-key\n");
    terminal.wait_for("配置已保存"); terminal.finish();
    assert!(!terminal.output.contains("installer-hidden-key"));
    assert!(home.path().join(".local/bin/sag").exists());
}

#[test]
fn setup_continues_the_original_search_after_saving() {
    let data = tempfile::tempdir().unwrap();
    let _cleanup=support::ServiceCleanup(data.path().to_path_buf());
    let root=data.path().canonicalize().unwrap();let server_root=root.clone();let mut step=0;
    let server=support::serve(data.path(),3,move|request|{
        let current=step;step+=1;
        if current<2{
            assert_eq!(request["command"]["command"],"configure");
            if current==1{assert_eq!(request["command"]["key"],"search-private-key");}
            serde_json::json!({"type":"configured","data":{"url":"https://api.302ai.cn/typesafeai/v1","model":"jev-latest","models":[],"has_key":current==1}})
        }else{
            assert_eq!(request["command"]["query"],"where is session validation?");
            serde_json::json!({"type":"search","data":{"protocol_version":2,"scope":server_root,"results":[{"path":"./evidence.md","absolute_path":server_root.join("evidence.md"),"start_line":1,"end_line":1,"content":"session validation evidence"}],"truncated":false}})
        }
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_sag"));
    command.args(["where is session validation?", "-c"]).current_dir(root).env("SAG_DATA_DIR", data.path());
    let mut terminal = Terminal::start(command);
    terminal.wait_for("JEV 接口地址 [https://api.302ai.cn/typesafeai/v1]："); terminal.send("\n");
    terminal.wait_for("模型名称 [jev-latest]："); terminal.send("\n");
    terminal.wait_for("API 密钥（输入隐藏）："); terminal.send_secret("search-private-key\n");
    terminal.wait_for("session validation evidence"); terminal.finish();
    assert!(!terminal.output.contains("search-private-key"));
    server.join().unwrap();
}

#[test]
fn terminal_search_animates_until_results_or_errors_are_ready() {
    for fail in [false, true] {
        let data = tempfile::tempdir().unwrap();
        let root = data.path().canonicalize().unwrap();
        let source = root.join("evidence.md");
        let mut step = 0;
        let server = support::serve(data.path(), 2, move |_| {
            step += 1;
            if step == 1 {
                return serde_json::json!({"type":"configured","data":{"url":"https://example.com","model":"test","models":[],"has_key":true}});
            }
            std::thread::sleep(Duration::from_millis(450));
            if fail {
                serde_json::json!({"type":"error","data":"test search failed"})
            } else {
                serde_json::json!({"type":"search","data":{"protocol_version":2,"scope":source.parent(),"results":[{"path":"evidence.md","absolute_path":source,"start_line":1,"end_line":1,"content":"search evidence"}],"truncated":false}})
            }
        });
        let mut command = Command::new(env!("CARGO_BIN_EXE_sag"));
        command.args(["-c", "How is the session validated?"])
            .current_dir(root).env("SAG_DATA_DIR", data.path());
        let mut terminal = Terminal::start(command);
        terminal.wait_for("⠋");
        terminal.expect_exit(if fail { 1 } else { 0 });
        let result = if fail { "test search failed" } else { "[1/1]" };
        assert!(terminal.output.contains('⠙'), "{}", terminal.output);
        assert!(!terminal.output.contains("正在搜索"));
        assert!(terminal.output.rfind('⠙').unwrap() < terminal.output.find(result).unwrap());
        assert!(terminal.output.contains(&format!("\r\u{1b}[2K{result}")), "{}", terminal.output);
        server.join().unwrap();
    }
}

#[test]
fn piped_search_and_terminal_json_do_not_contain_animation() {
    let data = tempfile::tempdir().unwrap();
    let root = data.path().canonicalize().unwrap();
    let source = root.join("evidence.md");
    let server = support::serve(data.path(), 2, move |_| {
        serde_json::json!({"type":"search","data":{"protocol_version":2,"scope":source.parent(),"results":[{"path":"evidence.md","absolute_path":source,"start_line":1,"end_line":1,"content":"search evidence"}],"truncated":false}})
    });
    let result = Command::new(env!("CARGO_BIN_EXE_sag"))
        .args(["-c", "How is the session validated?"])
        .current_dir(&root).env("SAG_DATA_DIR", data.path()).output().unwrap();
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    assert!(String::from_utf8(result.stdout).unwrap().starts_with("[1/1]"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_sag"));
    command.args(["--json", "-c", "How is the session validated?"])
        .current_dir(root).env("SAG_DATA_DIR", data.path());
    let mut terminal = Terminal::start(command);
    terminal.finish();
    let result: serde_json::Value = serde_json::from_str(terminal.output.trim()).unwrap();
    assert_eq!(result["results"][0]["content"], "search evidence");
    server.join().unwrap();
}

#[test]
fn cancelling_search_clears_the_animation_before_returning() {
    use sag_protocol::{Endpoint, Frame, ServiceInfo, BUILD_CHANNEL, BUILD_ID, BUILD_VERSION, VERSION};
    let data = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    std::fs::write(data.path().join("sag-service.json"), serde_json::to_vec(&Endpoint {
        protocol_version: VERSION, port: listener.local_addr().unwrap().port(), pid: 1, token: "test-token".into(),
    }).unwrap()).unwrap();
    let (requested, request_received) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || tokio::runtime::Runtime::new().unwrap().block_on(async {
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut read = tokio::io::BufReader::new(read);
        assert!(matches!(sag_client::wire::read(&mut read).await.unwrap(), Some(Frame::Hello { .. })));
        sag_client::wire::write(&mut write, &Frame::Welcome { info: ServiceInfo {
            version: Some(BUILD_VERSION.into()), protocol_version: VERSION, build_id: BUILD_ID.into(),
            channel: BUILD_CHANNEL.into(), pid: 1, executable: "test-sag".into(), data_directory: "test-data".into(),
        }}).await.unwrap();
        assert!(matches!(sag_client::wire::read(&mut read).await.unwrap(), Some(Frame::Request { .. })));
        requested.send(()).unwrap();
        let next = tokio::time::timeout(Duration::from_secs(10), sag_client::wire::read(&mut read)).await.unwrap().unwrap();
        assert!(matches!(next, None | Some(Frame::Cancel { .. })));
    }));
    let mut command = Command::new(env!("CARGO_BIN_EXE_sag"));
    command.args(["-F", "needle"]).current_dir(data.path()).env("SAG_DATA_DIR", data.path());
    let mut terminal = Terminal::start(command);
    terminal.wait_for("⠙");
    // The spinner can appear before the network handshake/request has completed.
    request_received.recv_timeout(Duration::from_secs(8)).unwrap();
    terminal.send("\u{3}");
    terminal.expect_exit(130);
    assert!(terminal.output.contains("\r\u{1b}[2K操作已取消"), "{}", terminal.output);
    server.join().unwrap();
}

#[test]
fn cancelling_setup_does_not_save_a_key() {
    let data = tempfile::tempdir().unwrap();
    let _cleanup=support::ServiceCleanup(data.path().to_path_buf());
    let mut command = Command::new(env!("CARGO_BIN_EXE_sag"));
    command.env("SAG_DATA_DIR", data.path());
    let mut terminal = Terminal::start(command);
    terminal.wait_for("JEV 接口地址 [https://api.302ai.cn/typesafeai/v1]：");
    terminal.send("\u{3}");
    terminal.expect_exit(130);
    let shown = Command::new(env!("CARGO_BIN_EXE_sag")).arg("config")
        .env("SAG_DATA_DIR", data.path()).output().unwrap();
    let config: serde_json::Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(config["has_key"], false);
}

#[test]
fn cancelling_password_input_restores_terminal_echo() {
    let data = tempfile::tempdir().unwrap();
    let _cleanup=support::ServiceCleanup(data.path().to_path_buf());
    let mut command = Command::new(env!("CARGO_BIN_EXE_sag"));
    command.env("SAG_DATA_DIR", data.path());
    let mut terminal = Terminal::start(command);
    terminal.wait_for("JEV 接口地址 [https://api.302ai.cn/typesafeai/v1]："); terminal.send("\n");
    terminal.wait_for("模型名称 [jev-latest]："); terminal.send("\n");
    terminal.wait_for("API 密钥（输入隐藏）："); terminal.send_secret("\u{3}");
    terminal.expect_exit(130);
    let mut mode = std::mem::MaybeUninit::<libc::termios>::uninit();
    assert_eq!(unsafe { libc::tcgetattr(terminal.master.as_ref().unwrap().as_raw_fd(), mode.as_mut_ptr()) }, 0);
    let flags = unsafe { mode.assume_init() }.c_lflag;
    assert_ne!(flags & libc::ECHO, 0, "terminal echo was not restored");
    assert_ne!(flags & libc::ICANON, 0, "terminal line input was not restored");
}
