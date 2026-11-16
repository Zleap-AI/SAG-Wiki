use super::run_captured;
use std::process::Command;

#[test]
fn captured_child() {
    if std::env::var_os("SAG_CAPTURED_TEST_CHILD").is_none() {
        return;
    }
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetConsoleWindow() -> *mut std::ffi::c_void;
        }
        assert!(
            unsafe { GetConsoleWindow() }.is_null(),
            "captured commands must not have a console window"
        );
    }
    // The caller must close stdin without opening the Windows NUL device.
    let mut input = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut input).unwrap();
    assert!(input.is_empty());
    println!("captured stdout");
    eprintln!("captured stderr");
}

#[test]
fn captured_commands_have_no_console_and_preserve_piped_io() {
    let output = run_captured(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "captured_tests::captured_child", "--nocapture"])
            .env("SAG_CAPTURED_TEST_CHILD", "1"),
    )
    .unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("captured stdout"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("captured stderr"));
}
