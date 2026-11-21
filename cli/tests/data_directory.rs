#[cfg(target_os = "macos")]
#[test]
fn migration_switches_the_real_launch_agent_and_recovery_switches_it_back() {
    use std::{fs, process::Command};

    let home = tempfile::tempdir().unwrap();
    let source = home.path().join("old");
    let destination = home.path().join("new");
    let run = |data: Option<&std::path::Path>, args: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sag"));
        command.env_clear().env("HOME", home.path()).args(args);
        if let Some(data) = data {
            command.env("SAG_DATA_DIR", data);
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        output.stdout
    };
    drop(sag_core::store::Store::open(&source.join("sag.sqlite")).unwrap());
    fs::create_dir(&destination).unwrap();
    let agents = home.path().join("Library/LaunchAgents");
    let registration = || {
        let files = fs::read_dir(&agents).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(files.len(), 1);
        fs::read_to_string(files[0].path()).unwrap()
    };
    run(Some(&source), &["api", "autostart", "enable"]);
    assert!(registration().contains(&*source.canonicalize().unwrap().to_string_lossy()));
    run(None, &["service", "migrate", "--source", source.to_str().unwrap(), "--destination", destination.to_str().unwrap()]);
    assert!(registration().contains(&*destination.canonicalize().unwrap().to_string_lossy()));
    assert!(String::from_utf8(run(None, &["api", "autostart", "status"])).unwrap().contains("已开启"));

    fs::rename(&destination, home.path().join("unavailable")).unwrap();
    run(None, &["service", "recover", "--directory", source.to_str().unwrap()]);
    assert!(registration().contains(&*source.canonicalize().unwrap().to_string_lossy()));
    assert!(String::from_utf8(run(None, &["api", "autostart", "status"])).unwrap().contains("已开启"));
}
