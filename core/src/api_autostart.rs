//! Startup registration is independent of the saved HTTP enabled state.
#[cfg(not(target_os = "macos"))]
use anyhow::bail;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[cfg(not(target_os = "macos"))]
use std::process::Command;

fn name(data: &Path) -> String {
    format!(
        "com.sag.api.{}",
        &crate::store::hash(data.as_os_str().as_encoded_bytes())[..12]
    )
}
fn registration(data: &Path) -> Result<PathBuf> {
    let name = name(data);
    #[cfg(target_os = "macos")]
    return Ok(dirs::home_dir()
        .context("无法获取用户目录")?
        .join("Library/LaunchAgents")
        .join(format!("{name}.plist")));
    #[cfg(target_os = "linux")]
    return Ok(PathBuf::from("/etc/systemd/system").join(format!("{name}.service")));
    #[cfg(target_os = "windows")]
    return Ok(data.join(format!("{name}.startup")));
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    bail!("此平台不支持自动启动")
}
pub fn enabled(data: &Path) -> Result<bool> {
    if !data.try_exists()? {
        return Ok(false);
    }
    let canonical = data.canonicalize()?;
    let data = canonical.as_path();
    #[cfg(target_os = "windows")]
    {
        let output = sag_client::run_captured(Command::new("reg.exe").args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            &name(data),
        ]))?;
        return match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => bail!(
                "无法读取自动启动设置：{}",
                String::from_utf8_lossy(&output.stderr)
            ),
        };
    }
    #[cfg(not(target_os = "windows"))]
    {
        if !registration(data)?.try_exists()? {
            return Ok(false);
        }
        #[cfg(target_os = "linux")]
        {
            let output = Command::new("systemctl")
                .args(["is-enabled", &format!("{}.service", name(data))])
                .output()?;
            return match output.status.code() {
                Some(0) => Ok(true),
                Some(1) => Ok(false),
                _ => bail!(
                    "无法读取自动启动设置：{}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            };
        }
        #[cfg(not(target_os = "linux"))]
        Ok(true)
    }
}
#[cfg(target_os = "macos")]
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn configure(data: &Path, executable: &Path, enable: bool) -> Result<bool> {
    let data = data.canonicalize()?;
    let path = registration(&data)?;
    #[cfg(target_os = "macos")]
    {
        if enable {
            let executable = executable.canonicalize()?;
            let plist=format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>Label</key><string>{}</string><key>ProgramArguments</key><array><string>{}</string><string>service</string><string>serve</string><string>--data-dir</string><string>{}</string></array><key>RunAtLoad</key><true/></dict></plist>\n",name(&data),xml(executable.to_str().context("程序路径不是 UTF-8")?),xml(data.to_str().context("数据目录不是 UTF-8")?));
            std::fs::create_dir_all(path.parent().unwrap())?;
            crate::http_api::write_private(&path, plist.as_bytes())?;
        } else if path.try_exists()? {
            std::fs::remove_file(path)?;
        }
    }
    #[cfg(target_os = "windows")]
    {
        let _ = path;
        let mut command = Command::new("reg.exe");
        if enable {
            let executable = executable.canonicalize()?;
            let value = format!(
                "\"{}\" service serve --data-dir \"{}\"",
                executable.display(),
                data.display()
            );
            command.args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                &name(&data),
                "/t",
                "REG_SZ",
                "/d",
                &value,
                "/f",
            ]);
        } else {
            if !enabled(&data)? {
                return Ok(false);
            }
            command.args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                &name(&data),
                "/f",
            ]);
        }
        let result = sag_client::run_captured(&mut command)?;
        if !result.status.success() {
            bail!(
                "无法修改自动启动：{}",
                String::from_utf8_lossy(&result.stderr)
            )
        }
    }
    #[cfg(target_os = "linux")]
    {
        // The CLI runs this operation in its foreground process so sudo can
        // prompt. The service always runs as that CLI user with the same data.
        fn quote(value: &str) -> String {
            format!(
                "\"{}\"",
                value
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('%', "%%")
                    .replace('$', "$$")
            )
        }
        fn admin(args: &[&std::ffi::OsStr]) -> Result<()> {
            let status = if unsafe { libc::geteuid() } == 0 {
                Command::new(args[0]).args(&args[1..]).status()?
            } else {
                Command::new("sudo").args(args).status()?
            };
            if !status.success() {
                bail!("自动启动注册失败，请在终端运行 sag api autostart enable 或 disable，并完成管理员授权")
            }
            Ok(())
        }
        use std::ffi::OsStr;
        let unit = format!("{}.service", name(&data));
        if enable {
            let executable = executable.canonicalize()?;
            let uid = unsafe { libc::getuid() };
            let gid = unsafe { libc::getgid() };
            let content=format!("[Unit]\nDescription=SAG API backend\nAfter=network.target\n\n[Service]\nType=simple\nUser={uid}\nGroup={gid}\nExecStart={} service serve --data-dir {}\n\n[Install]\nWantedBy=multi-user.target\n",quote(executable.to_str().context("程序路径不是 UTF-8")?),quote(data.to_str().context("数据目录不是 UTF-8")?));
            use std::io::Write;
            let mut temp = tempfile::NamedTempFile::new()?;
            temp.write_all(content.as_bytes())?;
            admin(&[
                OsStr::new("install"),
                OsStr::new("-m"),
                OsStr::new("644"),
                temp.path().as_os_str(),
                path.as_os_str(),
            ])?;
            admin(&[OsStr::new("systemctl"), OsStr::new("daemon-reload")])?;
            admin(&[
                OsStr::new("systemctl"),
                OsStr::new("enable"),
                OsStr::new(&unit),
            ])?;
        } else if path.try_exists()? {
            admin(&[
                OsStr::new("systemctl"),
                OsStr::new("disable"),
                OsStr::new(&unit),
            ])?;
            admin(&[OsStr::new("rm"), path.as_os_str()])?;
            admin(&[OsStr::new("systemctl"), OsStr::new("daemon-reload")])?;
        }
    }
    enabled(&data)
}
