#![cfg(unix)]
use std::{fs, path::PathBuf, process::Command};
use sha2::{Digest, Sha256};
use sag_core::protocol as cli_protocol;
#[path = "../src/install.rs"]
mod install;

#[test]
fn desktop_bootstrap_updates_a_renamed_command_and_respects_uninstall() {
    if std::env::var_os("SAG_BOOTSTRAP_CHILD").is_some() {
        let source = std::env::var_os("SAG_BOOTSTRAP_SOURCE").map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_sag")));
        install::bootstrap(&PathBuf::from(std::env::var_os("HOME").unwrap()), &source).unwrap();
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("data");
    let command = |program: &std::path::Path, args: &[&str], child: bool| {
        let mut command = Command::new(program);
        command.args(args).env_clear().env("HOME", home.path()).env("SHELL", "/bin/zsh")
            .env("PATH", "/usr/bin:/bin").env("SAG_DATA_DIR", &data);
        if child { command.env("SAG_BOOTSTRAP_CHILD", "1"); }
        let result = command.output().unwrap();
        assert!(result.status.success(), "{} {}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
    };
    let binary = std::path::Path::new(env!("CARGO_BIN_EXE_sag"));
    command(binary, &["install", "cli"], false);
    let target = home.path().join(".local/bin/sag");
    let previous = home.path().join(".local/bin/previous-command");
    fs::rename(&target, &previous).unwrap();
    fs::rename(target.with_extension("sag-install.json"), previous.with_extension("sag-install.json")).unwrap();
    fs::write(data.join("cli-install-path.json"), serde_json::to_vec(&previous).unwrap()).unwrap();
    command(binary, &["install", "codex"], false);
    let skill = home.path().join(".agents/skills/sag/SKILL.md");
    fs::write(&skill, "previous managed instructions").unwrap();
    let receipt_path = skill.with_extension("sag-install.json");
    let mut receipt: serde_json::Value = serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
    receipt["hash"] = format!("{:x}", Sha256::digest(fs::read(&skill).unwrap())).into();
    fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    let runner = std::env::current_exe().unwrap();
    let test_args = ["--exact", "desktop_bootstrap_updates_a_renamed_command_and_respects_uninstall"];
    // A managed Skill that cannot be replaced must not remove the old command.
    let original_skill = skill.with_extension("original");
    fs::rename(&skill, &original_skill).unwrap();
    std::os::unix::fs::symlink(&original_skill, &skill).unwrap();
    let failed = Command::new(&runner).args(test_args).env_clear()
        .env("HOME", home.path()).env("SHELL", "/bin/zsh").env("PATH", "/usr/bin:/bin")
        .env("SAG_DATA_DIR", &data).env("SAG_BOOTSTRAP_CHILD", "1").output().unwrap();
    assert!(!failed.status.success());
    assert!(previous.exists(), "failed upgrade must preserve the old command");
    assert!(!target.exists());
    fs::remove_file(&skill).unwrap();
    fs::rename(&original_skill, &skill).unwrap();
    command(&runner, &test_args, true);
    assert!(target.exists());
    assert!(!previous.exists());
    assert_eq!(fs::read_to_string(&skill).unwrap(), install::SKILL);
    fs::write(&skill, "custom user instructions").unwrap();
    command(&runner, &test_args, true);
    assert_eq!(fs::read_to_string(&skill).unwrap(), "custom user instructions");
    command(binary, &["uninstall", "cli"], false);
    command(&runner, &test_args, true);
    assert!(!target.exists(), "explicit uninstall must not be reversed");
}

#[test]
fn desktop_bootstrap_updates_an_owned_binary_when_commands_are_added() {
    let home = tempfile::tempdir().unwrap();
    let source = home.path().join("bundled-cli");
    let target = home.path().join(".local/bin/sag");
    let bootstrap = || {
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "desktop_bootstrap_updates_a_renamed_command_and_respects_uninstall"])
            .env_clear().env("HOME", home.path()).env("SHELL", "/bin/zsh")
            .env("PATH", "/usr/bin:/bin").env("SAG_DATA_DIR", home.path().join("data"))
            .env("SAG_BOOTSTRAP_CHILD", "1").env("SAG_BOOTSTRAP_SOURCE", &source).output().unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stdout));
    };
    fs::write(&source, "first release").unwrap();
    bootstrap();
    fs::write(&source, "release with new commands").unwrap();
    bootstrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "release with new commands");
    fs::write(&target, "user edited command").unwrap();
    bootstrap();
    assert_eq!(fs::read_to_string(target).unwrap(), "user edited command");
}
