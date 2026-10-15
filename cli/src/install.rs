use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const HARNESS_NAMES: [&str; 3] = ["claude-code", "codex", "opencode"];
pub const SKILL: &str = include_str!("../skills/sag/SKILL.md");
const OWNER: &str = "com.sag.desktop.cli";
#[derive(Serialize, Deserialize)]
struct Receipt {
    owner: String,
    hash: String,
    version: String,
}
#[derive(Serialize)]
pub struct Installation {
    pub name: String,
    pub path: String,
    pub installed: bool,
    pub version: Option<String>,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn skill_path(home: &Path, harness: &str) -> Result<PathBuf> {
    let base = match harness {
        "claude-code" => home.join(".claude/skills"),
        "codex" => home.join(".agents/skills"),
        "opencode" => home.join(".config/opencode/skills"),
        _ => bail!("未知 Harness：{harness}"),
    };
    Ok(base.join("sag/SKILL.md"))
}
fn receipt_path(path: &Path) -> PathBuf {
    path.with_extension("sag-install.json")
}
fn receipt(path: &Path) -> Result<Option<Receipt>> {
    match fs::read(receipt_path(path)) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes).context("安装记录损坏")?,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn check_owned(path: &Path) -> Result<()> {
    if path.symlink_metadata()?.file_type().is_symlink() {
        bail!("安装位置是符号链接，未修改：{}", path.display())
    }
    let old = receipt(path)?.context("发现同名文件，缺少 SAG 安装记录，未修改")?;
    if old.owner != OWNER || old.hash != digest(&fs::read(path)?) {
        bail!("文件来源不同或已经修改，未覆盖或删除：{}", path.display())
    }
    Ok(())
}
fn write_owned(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.symlink_metadata().is_ok() {
        check_owned(path)?;
    }
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, bytes)?;
    fs::write(
        receipt_path(path),
        serde_json::to_vec(&Receipt {
            owner: OWNER.into(),
            hash: digest(bytes),
            version: env!("CARGO_PKG_VERSION").into(),
        })?,
    )?;
    Ok(())
}
fn remove_owned(path: &Path) -> Result<()> {
    if !path.try_exists()? {
        return Ok(());
    }
    check_owned(path)?;
    fs::remove_file(path)?;
    fs::remove_file(receipt_path(path))?;
    Ok(())
}
pub fn harness(home: &Path, name: &str, uninstall: bool) -> Result<PathBuf> {
    let path = skill_path(home, name)?;
    if uninstall {
        remove_owned(&path)?;
    } else {
        write_owned(&path, SKILL.as_bytes())?;
    }
    Ok(path)
}
pub fn inspect(name: &str, path: &Path) -> Result<Installation> {
    let installed = path.try_exists()?;
    let owned = if installed {
        let hash = digest(&fs::read(path)?);
        receipt(path)?.filter(|r| r.owner == OWNER && r.hash == hash)
    } else {
        None
    };
    Ok(Installation {
        name: name.into(),
        path: path.display().to_string(),
        installed: installed && owned.is_some(),
        version: owned.map(|r| r.version),
    })
}
pub fn binary_path(home: &Path) -> Result<PathBuf> {
    let record = crate::cli_protocol::data_dir()?.join("cli-install-path.json");
    match fs::read(record) {
        Ok(bytes) => {
            let path: PathBuf = serde_json::from_slice(&bytes).context("CLI 安装位置记录无效")?;
            if !path.is_absolute() || !path.starts_with(home) {
                bail!("CLI 安装位置不属于当前用户目录")
            }
            return Ok(path);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let name = if cfg!(windows) { "sag.exe" } else { "sag" };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            if dir.is_absolute()
                && dir.starts_with(home)
                && dir.is_dir()
                && fs::metadata(&dir).is_ok_and(|m| !m.permissions().readonly())
            {
                return Ok(dir.join(name));
            }
        }
    }
    Ok(home.join(".local/bin").join(name))
}
pub fn binary(source: &Path, target: &Path, uninstall: bool) -> Result<()> {
    let data = crate::cli_protocol::data_dir()?;
    let record = data.join("cli-install-path.json");
    if uninstall {
        remove_owned(target)?;
        match fs::remove_file(record) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        return Ok(());
    }
    if source == target {
        check_owned(target)?;
        return Ok(());
    }
    write_owned(
        target,
        &fs::read(source).context("未找到随应用分发的 SAG CLI，请重新安装应用")?,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(target, fs::Permissions::from_mode(0o755))?;
    }
    fs::create_dir_all(data)?;
    fs::write(record, serde_json::to_vec(target)?)?;
    Ok(())
}
