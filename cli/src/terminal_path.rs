use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Default, Deserialize, Serialize)]
struct Registration {
    #[serde(default)]
    blocks: Vec<ShellBlock>,
    #[serde(default)]
    windows_path: Option<PathBuf>,
}
#[derive(Deserialize, Serialize)]
struct ShellBlock {
    file: PathBuf,
    text: String,
}
#[cfg(unix)]
const START: &str = "# >>> SAG CLI PATH >>>";
#[cfg(unix)]
const END: &str = "# <<< SAG CLI PATH <<<";

fn read_optional(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e).with_context(|| format!("无法读取终端配置：{}", path.display())),
    }
}
fn save(record: &Path, registration: &Registration) -> Result<()> {
    fs::write(record, serde_json::to_vec(registration)?)?;
    Ok(())
}

pub fn configure(home: &Path, data: &Path, bin: &Path, uninstall: bool) -> Result<()> {
    let record = data.join("cli-terminal-path.json");
    let content = read_optional(&record)?;
    let mut registration: Registration = if content.is_empty() {
        Registration::default()
    } else {
        serde_json::from_str(&content).context("CLI 终端配置记录损坏")?
    };
    #[cfg(unix)]
    {
        use std::io::Write;
        // Validate every block before changing any shell configuration.
        let mut changes = Vec::new();
        for block in &registration.blocks {
            let current = read_optional(&block.file)?;
            if current.contains(&block.text) {
                changes.push((&block.file, current.replacen(&block.text, "", 1)));
            } else if current.contains(START) {
                bail!("SAG 终端配置已被修改，未覆盖：{}", block.file.display());
            }
        }
        for (file, content) in changes {
            fs::write(file, content)?;
        }
        registration.blocks.clear();
        if !uninstall {
            fs::create_dir_all(data)?;
            save(&record, &registration)?;
            let shell = std::env::var_os("SHELL")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(if cfg!(target_os = "macos") {
                        "/bin/zsh"
                    } else {
                        "/bin/bash"
                    })
                });
            let quoted = format!("'{}'", bin.to_string_lossy().replace('\'', "'\"'\"'"));
            let (files, command) = match shell.file_name().and_then(|name| name.to_str()) {
                Some("zsh") => {
                    let dir = std::env::var_os("ZDOTDIR").filter(|value| !value.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.to_path_buf());
                    (vec![dir.join(".zprofile"), dir.join(".zshrc")], format!("case \":$PATH:\" in *:{quoted}:*) ;; *) export PATH={quoted}:\"$PATH\" ;; esac"))
                }
                Some("bash") => {
                    let login = [".bash_profile", ".bash_login", ".profile"].into_iter().map(|name| home.join(name)).find(|file| file.exists()).unwrap_or_else(|| home.join(".profile"));
                    (vec![login, home.join(".bashrc")], format!("case \":$PATH:\" in *:{quoted}:*) ;; *) export PATH={quoted}:\"$PATH\" ;; esac"))
                }
                Some("fish") => {
                    let dir = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
                    (vec![dir.join("fish/config.fish")], format!("if not contains -- {quoted} $PATH\n    set -gx PATH {quoted} $PATH\nend"))
                }
                Some("sh" | "dash" | "ksh") => (vec![home.join(".profile")], format!("case \":$PATH:\" in *:{quoted}:*) ;; *) export PATH={quoted}:\"$PATH\" ;; esac")),
                _ => bail!("暂不支持自动配置此终端：{}", shell.display()),
            };
            let text = format!("\n{START}\n{command}\n{END}\n");
            for file in files {
                let current = read_optional(&file)?;
                if current.contains(START) {
                    bail!("终端中已存在未登记的 SAG 配置，未覆盖：{}", file.display());
                }
                fs::create_dir_all(file.parent().unwrap())?;
                registration.blocks.push(ShellBlock {
                    file: file.clone(),
                    text: text.clone(),
                });
                // Persist ownership before editing so an interrupted install can be removed.
                save(&record, &registration)?;
                fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&file)
                    .with_context(|| format!("无法配置终端：{}", file.display()))?
                    .write_all(text.as_bytes())?;
            }
        }
    }
    #[cfg(windows)]
    {
        let _ = home;
        if uninstall {
            if let Some(path) = &registration.windows_path {
                windows_path("remove", path)?;
            }
        } else if !windows_path("contains", bin)? {
            fs::create_dir_all(data)?;
            registration.windows_path = Some(bin.to_path_buf());
            save(&record, &registration)?;
            windows_path("add", bin)?;
        }
    }
    if uninstall && record.try_exists()? {
        fs::remove_file(record)?;
    }
    Ok(())
}

#[cfg(windows)]
fn windows_path(action: &str, bin: &Path) -> Result<bool> {
    // Keep existing registry value text/type and notify newly launched applications.
    let script = r#"
$ErrorActionPreference = 'Stop'
$bin = $env:SAG_CLI_PATH_DIRECTORY
$key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
$current = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
$kind = if ($key.GetValueNames() -contains 'Path') { $key.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
$parts = @($current -split ';')
function Set-UserPath([string]$value) {
 if ($value -eq '') { $key.DeleteValue('Path', $false) } else { $key.SetValue('Path', $value, $kind) }
 Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class SAGPathNotify {
 [DllImport("user32.dll", CharSet = CharSet.Unicode)]
 public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);
}
'@
 $result = [UIntPtr]::Zero
 [void][SAGPathNotify]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
}
$found = @($parts | Where-Object { [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\') -ieq $bin.TrimEnd('\') }).Count -gt 0
switch ($env:SAG_CLI_PATH_ACTION) {
 'add' { if (-not $found) { Set-UserPath $(if ($current -eq '') { $bin } else { $current + ';' + $bin }) } }
 'remove' { if ($found) { $kept = @($parts | Where-Object { [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\') -ine $bin.TrimEnd('\') }); Set-UserPath ($kept -join ';') } }
}
[Console]::Write($found)
"#;
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .env("SAG_CLI_PATH_DIRECTORY", bin)
        .env("SAG_CLI_PATH_ACTION", action)
        .output()
        .context("无法启动 Windows 用户 PATH 配置")?;
    if !output.status.success() {
        bail!(
            "无法配置 Windows 用户 PATH：{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    match String::from_utf8(output.stdout)?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => bail!("Windows PATH 配置返回了无效结果"),
    }
}
