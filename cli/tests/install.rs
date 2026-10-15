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
