use super::*;

#[cfg(target_os = "macos")]
#[test]
fn sandboxed_supervision_does_not_require_the_setuid_ps_executable() {
    const CHILD: &str = "SAG_TEST_SUPERVISOR_SANDBOX_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let ps = Command::new("/bin/ps")
            .args(["-p", &std::process::id().to_string()])
            .output();
        assert!(
            ps.is_err() || ps.is_ok_and(|output| !output.status.success()),
            "fixture must prohibit setuid ps"
        );
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "echo supervised; sleep .2"]);
        let mut observed = false;
        let result = run(
            &mut command,
            Limits {
                timeout: Duration::from_secs(2),
                memory_bytes: Some(256 * 1024 * 1024),
            },
            &|| false,
            |line| observed |= line == b"supervised",
        );
        assert!(result.is_ok(), "sandboxed supervision failed: {result:?}");
        assert!(observed);
        return;
    }
    let mut sandbox = Command::new("/usr/bin/sandbox-exec");
    if let Some(profile) = std::env::var_os("SAG_TEST_SUPERVISOR_SANDBOX_PROFILE") {
        sandbox.arg("-f").arg(profile);
    } else {
        sandbox.args(["-p", "(version 1) (allow default)"]);
    }
    let output = sandbox
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "process_supervisor::tests::sandboxed_supervision_does_not_require_the_setuid_ps_executable",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "sandboxed regression failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
fn tree_command(directory: &std::path::Path) -> Command {
    let mut command = Command::new("sh");
    command.current_dir(directory).args([
        "-c",
        "echo $$ > parent; sh -c 'echo $$ > child; sleep 20 & echo $! > grandchild; wait' & wait",
    ]);
    command
}
#[cfg(unix)]
fn assert_tree_dead(directory: &std::path::Path) {
    for name in ["parent", "child", "grandchild"] {
        let pid: u32 = std::fs::read_to_string(directory.join(name))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let output = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "stat="])
            .output()
            .unwrap();
        // Linux may retain an orphan zombie briefly until init reaps it.
        let state = String::from_utf8_lossy(&output.stdout);
        assert!(
            state.trim().is_empty() || state.trim().starts_with('Z'),
            "{name} {pid} survived: {state}"
        );
    }
}
#[cfg(unix)]
#[test]
fn timeout_stops_actual_child_and_grandchild() {
    let temp = tempfile::tempdir().unwrap();
    let result = run(
        &mut tree_command(temp.path()),
        Limits {
            timeout: Duration::from_millis(350),
            memory_bytes: Some(256 * 1024 * 1024),
        },
        &|| false,
        |_| {},
    )
    .unwrap_err();
    assert_eq!(result.kind, FailureKind::Timeout);
    assert_tree_dead(temp.path());
}
#[cfg(unix)]
#[test]
fn cancellation_stops_actual_child_and_grandchild() {
    let temp = tempfile::tempdir().unwrap();
    let result = run(
        &mut tree_command(temp.path()),
        Limits {
            timeout: Duration::from_secs(3),
            memory_bytes: Some(256 * 1024 * 1024),
        },
        &|| temp.path().join("grandchild").exists(),
        |_| {},
    )
    .unwrap_err();
    assert_eq!(result.kind, FailureKind::Cancelled);
    assert_tree_dead(temp.path());
}
#[cfg(unix)]
#[test]
fn memory_limit_stops_actual_child_and_grandchild() {
    let temp = tempfile::tempdir().unwrap();
    let mut command = Command::new("sh");
    // Give the complete tree time to start before the grandchild exceeds the
    // budget. A one-byte limit could kill the parent before children existed.
    command.current_dir(temp.path()).args([
        "-c",
        "echo $$ > parent; sh -c 'echo $$ > child; python3 -c \"import time; time.sleep(.1); data=bytearray(96*1024*1024); time.sleep(20)\" & echo $! > grandchild; wait' & wait",
    ]);
    let result = run(
        &mut command,
        Limits {
            timeout: Duration::from_secs(3),
            memory_bytes: Some(64 * 1024 * 1024),
        },
        &|| false,
        |_| {},
    )
    .unwrap_err();
    assert_eq!(result.kind, FailureKind::Memory);
    assert_tree_dead(temp.path());
}
#[cfg(unix)]
#[test]
fn output_is_bounded_and_a_background_writer_does_not_keep_run_alive() {
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 20 & i=0; while [ $i -lt 5000 ]; do printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n'; i=$((i+1)); done; exit 2"]);
    let start = Instant::now();
    let error = run(
        &mut command,
        Limits {
            timeout: Duration::from_secs(3),
            memory_bytes: Some(256 * 1024 * 1024),
        },
        &|| false,
        |_| {},
    )
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::Exit);
    assert!(error.message.len() < 70 * 1024);
    assert!(start.elapsed() < Duration::from_secs(3));
}
#[test]
fn output_queue_has_a_fixed_capacity_even_without_a_consumer() {
    let mut output = Output::default();
    for _ in 0..10000 {
        output.push(vec![b'x'; 8192]);
    }
    assert_eq!(output.pending.len(), MAX_PENDING);
    assert_eq!(output.tail.len(), 8);
}

#[cfg(unix)]
#[test]
fn an_escaped_inherited_writer_cannot_block_reader_shutdown() {
    // Intentional setsid escapes are outside process-group containment. Verify
    // bounded reader shutdown, and explicitly reap this fixture's escaped PID.
    let temp = tempfile::tempdir().unwrap();
    let mut command = Command::new("python3");
    command.current_dir(temp.path()).args([
        "-c",
        r#"import os,time
pid=os.fork()
if pid==0:
 os.setsid()
 open('escaped','w').write(str(os.getpid()))
 time.sleep(15)
else:
 while not os.path.exists('escaped'):time.sleep(.001)
"#,
    ]);
    let start = Instant::now();
    let result = run(
        &mut command,
        Limits {
            timeout: Duration::from_secs(2),
            memory_bytes: Some(256 * 1024 * 1024),
        },
        &|| false,
        |_| {},
    );
    let pid: i32 = std::fs::read_to_string(temp.path().join("escaped"))
        .unwrap()
        .parse()
        .unwrap();
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
    assert!(result.is_ok(), "{result:?}");
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[cfg(unix)]
#[test]
fn a_late_same_group_child_is_killed_before_reaping_the_parent() {
    for code in [0, 2] {
        let temp = tempfile::tempdir().unwrap();
        let mut command = Command::new("sh");
        command.current_dir(temp.path()).args(["-c",&format!("echo ready; while [ ! -f release ]; do sleep .01; done; sleep 20 & echo $! > late; exit {code}")]);
        let result = run(
            &mut command,
            Limits {
                timeout: Duration::from_secs(2),
                memory_bytes: Some(256 * 1024 * 1024),
            },
            &|| false,
            |line| {
                if line == b"ready" {
                    std::fs::write(temp.path().join("release"), "").unwrap();
                    std::thread::sleep(Duration::from_millis(100));
                }
            },
        );
        let pid: i32 = std::fs::read_to_string(temp.path().join("late"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let output = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "stat="])
            .output()
            .unwrap();
        let state = String::from_utf8_lossy(&output.stdout);
        let dead = state.trim().is_empty() || state.trim().starts_with('Z');
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
        assert!(dead, "late child {pid} survived parent exit {code}");
        assert_eq!(result.is_ok(), code == 0);
    }
}
