#![cfg(unix)]
// These process tests isolate user directories through HOME, which is Unix-specific.
use std::{fs, process::Command};
#[test]
fn install_is_repeatable_and_preserves_foreign_and_edited_files() {
    let home = tempfile::tempdir().unwrap();
    let run = |verb: &str| {
        Command::new(env!("CARGO_BIN_EXE_sgrep"))
            .args([verb, "claude-code"])
            .env("HOME", home.path())
            .env_remove("ZDOTDIR")
            .env_remove("XDG_CONFIG_HOME")
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
        Command::new(env!("CARGO_BIN_EXE_sgrep"))
            .args([action, "cli"])
            .env("HOME", home.path())
            .env_remove("ZDOTDIR")
            .env_remove("XDG_CONFIG_HOME")
            .env("SGREP_DATA_DIR", home.path().join("data"))
            .env("PATH", path)
            .output()
            .unwrap()
    };
    assert!(run("install", &first).status.success());
    // Preserve installations made by the old PATH-based installer.
    let installed = home.path().join(".local/bin/sgrep");
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
    let output = Command::new(env!("CARGO_BIN_EXE_sgrep"))
        .args(["install", "cli"])
        .env("HOME", home.path())
        .env_remove("ZDOTDIR")
        .env_remove("XDG_CONFIG_HOME")
        .env("SGREP_DATA_DIR", home.path().join("data"))
        .env("PATH", &transient)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(home.path().join(".local/bin/sgrep").exists());
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
    let output = Command::new(env!("CARGO_BIN_EXE_sgrep"))
        .args(["install", "cli"])
        .env("HOME", home.path())
        .env_remove("ZDOTDIR")
        .env_remove("XDG_CONFIG_HOME")
        .env("SGREP_DATA_DIR", &data)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(home.path().join(".local/bin/sgrep").exists());
    assert!(!old.exists());
}

#[test]
fn fresh_terminals_find_cli_and_uninstall_preserves_shell_settings() {
    for (shell, files) in [
        ("/bin/zsh", vec![".zprofile", ".zshrc"]),
        ("/bin/bash", vec![".profile", ".bashrc"]),
    ] {
        if !std::path::Path::new(shell).exists() {
            continue;
        }
        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path().join("user space 'quoted");
        fs::create_dir(&home).unwrap();
        let original = "# user settings\nexport USER_MARKER=kept";
        for file in &files {
            fs::write(home.join(file), original).unwrap();
        }
        let run = |action: &str| {
            Command::new(env!("CARGO_BIN_EXE_sgrep"))
                .args([action, "cli"])
                .env_clear()
                .env("HOME", &home)
                .env("SHELL", shell)
                .env("PATH", "/usr/bin:/bin")
                .env("SGREP_DATA_DIR", home.join("data"))
                .output()
                .unwrap()
        };
        for _ in 0..2 {
            let output = run("install");
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        for mode in ["-ic", "-lc"] {
            let output = Command::new(shell)
                .args([mode, "sgrep --version && test \"$USER_MARKER\" = kept"])
                .env_clear()
                .env("HOME", &home)
                .env("SHELL", shell)
                .env("PATH", "/usr/bin:/bin")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{shell} {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("sgrep 0.1.0"));
        }
        assert!(run("uninstall").status.success());
        for file in &files {
            assert_eq!(fs::read_to_string(home.join(file)).unwrap(), original);
        }
    }
}

#[test]
fn custom_zdotdir_is_registered_and_cleanup_does_not_depend_on_the_current_shell() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("custom zsh");
    fs::create_dir(&config).unwrap();
    fs::write(config.join(".zshrc"), "# keep custom config").unwrap();
    let run = |action: &str, shell: &str| {
        Command::new(env!("CARGO_BIN_EXE_sgrep"))
            .args([action, "cli"])
            .env_clear()
            .env("HOME", home.path())
            .env("SHELL", shell)
            .env("ZDOTDIR", &config)
            .env("PATH", "/usr/bin:/bin")
            .env("SGREP_DATA_DIR", home.path().join("data"))
            .output()
            .unwrap()
    };
    assert!(run("install", "/bin/zsh").status.success());
    assert!(fs::read_to_string(config.join(".zshrc"))
        .unwrap()
        .contains("SAG CLI PATH"));
    assert!(!home.path().join(".zshrc").exists());
    assert!(run("uninstall", "/bin/bash").status.success());
    assert_eq!(
        fs::read_to_string(config.join(".zshrc")).unwrap(),
        "# keep custom config"
    );
}

#[test]
fn user_edits_to_managed_shell_blocks_are_not_overwritten_or_removed() {
    let home = tempfile::tempdir().unwrap();
    let run = |action: &str| {
        Command::new(env!("CARGO_BIN_EXE_sgrep"))
            .args([action, "cli"])
            .env_clear()
            .env("HOME", home.path())
            .env("SHELL", "/bin/zsh")
            .env("PATH", "/usr/bin:/bin")
            .env("SGREP_DATA_DIR", home.path().join("data"))
            .output()
            .unwrap()
    };
    assert!(run("install").status.success());
    let login_before = fs::read_to_string(home.path().join(".zprofile")).unwrap();
    let profile = home.path().join(".zshrc");
    let edited = fs::read_to_string(&profile)
        .unwrap()
        .replace("export PATH=", "export USER_CHANGED_PATH=");
    fs::write(&profile, &edited).unwrap();
    assert!(!run("uninstall").status.success());
    assert_eq!(fs::read_to_string(&profile).unwrap(), edited);
    assert_eq!(
        fs::read_to_string(home.path().join(".zprofile")).unwrap(),
        login_before
    );
    assert!(home.path().join(".local/bin/sgrep").exists());
}

#[test]
fn reinstall_moves_a_legacy_binary_without_registering_the_temporary_path() {
    let home = tempfile::tempdir().unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_sgrep"))
            .args(["install", "cli"])
            .env_clear()
            .env("HOME", home.path())
            .env("SHELL", "/bin/zsh")
            .env("PATH", "/usr/bin:/bin")
            .env("SGREP_DATA_DIR", home.path().join("data"))
            .output()
            .unwrap()
    };
    assert!(run().status.success());
    let stable = home.path().join(".local/bin/sgrep");
    let old = home.path().join(".codex/tmp/session/sag");
    fs::create_dir_all(old.parent().unwrap()).unwrap();
    fs::rename(&stable, &old).unwrap();
    fs::rename(
        stable.with_extension("sag-install.json"),
        old.with_extension("sag-install.json"),
    )
    .unwrap();
    fs::write(
        home.path().join("data/cli-install-path.json"),
        serde_json::to_vec(&old).unwrap(),
    )
    .unwrap();
    assert!(run().status.success());
    assert!(stable.exists());
    assert!(!old.exists());
    assert!(!fs::read_to_string(home.path().join(".zshrc"))
        .unwrap()
        .contains(".codex/tmp"));
}

#[test]
fn failed_terminal_setup_preserves_legacy_install_for_retry() {
    let home = tempfile::tempdir().unwrap();
    let run = |action: &str| {
        Command::new(env!("CARGO_BIN_EXE_sgrep"))
            .args([action, "cli"])
            .env_clear()
            .env("HOME", home.path())
            .env("SHELL", "/bin/zsh")
            .env("PATH", "/usr/bin:/bin")
            .env("SGREP_DATA_DIR", home.path().join("data"))
            .output()
            .unwrap()
    };
    assert!(run("install").status.success());
    assert!(run("uninstall").status.success());
    fs::write(
        home.path().join(".zshrc"),
        "# >>> SAG CLI PATH >>>\nuser config\n",
    )
    .unwrap();
    assert!(!run("install").status.success());
    let stable = home.path().join(".local/bin/sgrep");
    assert!(!stable.exists(), "failed setup must not appear installed");
    fs::write(home.path().join(".zshrc"), "# user config\n").unwrap();
    assert!(run("install").status.success());
    let old = home.path().join(".codex/tmp/session/sag");
    fs::create_dir_all(old.parent().unwrap()).unwrap();
    fs::rename(&stable, &old).unwrap();
    fs::rename(
        stable.with_extension("sag-install.json"),
        old.with_extension("sag-install.json"),
    )
    .unwrap();
    let record = home.path().join("data/cli-install-path.json");
    fs::write(&record, serde_json::to_vec(&old).unwrap()).unwrap();
    let profile = home.path().join(".zshrc");
    let good = fs::read_to_string(&profile).unwrap();
    fs::write(
        &profile,
        good.replace("export PATH=", "export USER_CHANGED_PATH="),
    )
    .unwrap();
    assert!(!run("install").status.success());
    assert!(old.exists());
    assert!(!stable.exists());
    assert_eq!(
        serde_json::from_slice::<std::path::PathBuf>(&fs::read(&record).unwrap()).unwrap(),
        old
    );
    fs::write(&profile, good).unwrap();
    assert!(run("install").status.success());
    assert!(stable.exists());
    assert!(!old.exists());
    assert!(run("uninstall").status.success());
    assert!(!stable.exists());
}

#[test]
fn explicit_install_and_uninstall_record_the_initialization_choice() {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("data");
    let run = |action: &str| Command::new(env!("CARGO_BIN_EXE_sgrep"))
        .args([action,"cli"]).env_clear().env("HOME",home.path()).env("SHELL","/bin/zsh")
        .env("PATH","/usr/bin:/bin").env("SGREP_DATA_DIR",&data).output().unwrap();
    assert!(run("install").status.success());
    let marker = data.join("cli-initialized");
    assert!(marker.exists());
    fs::remove_file(&marker).unwrap();
    assert!(run("uninstall").status.success());
    assert!(marker.exists());
    assert!(!home.path().join(".local/bin/sgrep").exists());
}

#[test]
fn release_installs_with_one_command_without_rust_or_desktop() {
    let home = tempfile::tempdir().unwrap();
    let release = home.path().join("download");
    fs::create_dir(&release).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_sgrep"), release.join("sgrep")).unwrap();
    fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/install.sh"), release.join("install.sh")).unwrap();
    let result = Command::new("/bin/sh").arg(release.join("install.sh"))
        .env_clear().env("HOME", home.path()).env("SHELL", "/bin/zsh")
        .env("PATH", "/usr/bin:/bin").env("SGREP_DATA_DIR", home.path().join("data"))
        .output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let installed = home.path().join(".local/bin/sgrep");
    let status = Command::new(&installed).arg("status").current_dir(home.path())
        .env_clear().env("HOME", home.path()).env("SGREP_DATA_DIR", home.path().join("data"))
        .output().unwrap();
    assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
    assert!(String::from_utf8_lossy(&status.stdout).contains("独立模式"));
}

#[test]
fn installer_accepts_key_and_model_without_echoing_the_key() {
    let home = tempfile::tempdir().unwrap();
    let result = Command::new("/bin/sh")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/install.sh"))
        .args(["--key", "test-install-private-key", "--url", "https://example.com/decisions", "--model", "install-model"])
        .env_clear().env("HOME", home.path()).env("SHELL", "/bin/zsh")
        .env("PATH", "/usr/bin:/bin").env("SGREP_DATA_DIR", home.path().join("data"))
        .env("SGREP_BINARY", env!("CARGO_BIN_EXE_sgrep"))
        .output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("test-install-private-key"));
    assert!(!String::from_utf8_lossy(&result.stderr).contains("test-install-private-key"));
    let shown = Command::new(home.path().join(".local/bin/sgrep"))
        .arg("config").env("SGREP_DATA_DIR", home.path().join("data")).output().unwrap();
    let config: serde_json::Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(config["has_key"], true);
    assert_eq!(config["model"], "install-model");
}
