#![cfg(unix)]
// These process tests isolate user directories through HOME, which is Unix-specific.
use std::{fs, process::Command};
#[test]
fn install_is_repeatable_and_preserves_foreign_and_edited_files() {
    let home = tempfile::tempdir().unwrap();
    let run = |verb: &str| {
        Command::new(env!("CARGO_BIN_EXE_sag"))
            .args([verb, "claude-code"])
            .env("HOME", home.path())
            .output()
            .unwrap()
    };
    let skill = home.path().join(".claude/skills/sag/SKILL.md");
    assert!(run("install").status.success());
    assert!(run("install").status.success());
    fs::write(skill.parent().unwrap().join("notes.txt"), "user data").unwrap();
    assert!(run("uninstall").status.success());
    assert_eq!(
        fs::read_to_string(skill.parent().unwrap().join("notes.txt")).unwrap(),
        "user data"
    );
    fs::write(&skill, "foreign skill").unwrap();
    assert!(!run("install").status.success());
    assert_eq!(fs::read_to_string(&skill).unwrap(), "foreign skill");
    fs::remove_file(&skill).unwrap();
    assert!(run("install").status.success());
    fs::write(&skill, "user edit").unwrap();
    assert!(!run("uninstall").status.success());
    assert_eq!(fs::read_to_string(&skill).unwrap(), "user edit");
}

#[test]
fn cli_uninstall_uses_recorded_location_when_path_changes() {
    let home = tempfile::tempdir().unwrap();
    let first = home.path().join("first");
    let second = home.path().join("second");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    let run = |action: &str, path: &std::path::Path| {
        Command::new(env!("CARGO_BIN_EXE_sag"))
            .args([action, "cli"])
            .env("HOME", home.path())
            .env("SAG_DESKTOP_DATA_DIR", home.path().join("data"))
            .env("PATH", path)
            .output()
            .unwrap()
    };
    assert!(run("install", &first).status.success());
    // Preserve installations made by the old PATH-based installer.
    let installed = home.path().join(".local/bin/sag");
    let binary = first.join("sag");
    fs::rename(&installed, &binary).unwrap();
    fs::rename(
        installed.with_extension("sag-install.json"),
        binary.with_extension("sag-install.json"),
    )
    .unwrap();
    fs::write(
        home.path().join("data/cli-install-path.json"),
        serde_json::to_vec(&binary).unwrap(),
    )
    .unwrap();
    assert!(run("uninstall", &second).status.success());
    assert!(!binary.exists());
}

#[test]
fn cli_installs_in_stable_user_bin_instead_of_a_temporary_path_entry() {
    let home = tempfile::tempdir().unwrap();
    let transient = home.path().join(".codex/tmp/arg0/session");
    fs::create_dir_all(&transient).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_sag"))
        .args(["install", "cli"])
        .env("HOME", home.path())
        .env("SAG_DESKTOP_DATA_DIR", home.path().join("data"))
        .env("PATH", &transient)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(home.path().join(".local/bin/sag").exists());
    assert!(!transient.join("sag").exists());
}

#[test]
fn missing_recorded_binary_does_not_recreate_the_old_temporary_directory() {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("data");
    let old = home.path().join(".codex/tmp/arg0/expired/sag");
    fs::create_dir(&data).unwrap();
    fs::write(
        data.join("cli-install-path.json"),
        serde_json::to_vec(&old).unwrap(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_sag"))
        .args(["install", "cli"])
        .env("HOME", home.path())
        .env("SAG_DESKTOP_DATA_DIR", &data)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(home.path().join(".local/bin/sag").exists());
    assert!(!old.exists());
}
