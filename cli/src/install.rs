use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const BINARY_NAME: &str = if cfg!(windows) { "sag.exe" } else { "sag" };
pub const HARNESS_NAMES: [&str; 3] = ["claude-code", "codex", "opencode"];
pub const SKILL: &str = include_str!("../skills/sag/SKILL.md");
const SKILL_FILES: &[(&str, &str)] = &[
    ("SKILL.md", SKILL),
    (
        "references/parameters.md",
        include_str!("../skills/sag/references/parameters.md"),
    ),
    (
        "references/ignores.md",
        include_str!("../skills/sag/references/ignores.md"),
    ),
    (
        "references/setup.md",
        include_str!("../skills/sag/references/setup.md"),
    ),
];
#[path = "terminal_path.rs"]
mod terminal_path;

const OWNER: &str = "com.sag.desktop.cli";
#[derive(Serialize, Deserialize)]
struct Receipt {
    owner: String,
    hash: String,
    version: String,
    #[serde(default)]
    build_id: Option<String>,
    #[serde(default)]
    channel: Option<String>,
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
fn keep_installed(record: &Receipt) -> Result<bool> {
    use std::cmp::Ordering;
    Ok(
        match sag_protocol::compare_versions(&record.version, sag_protocol::BUILD_VERSION)? {
            Ordering::Greater => true,
            Ordering::Equal => record.build_id.as_deref() != Some(sag_protocol::BUILD_ID),
            Ordering::Less => false,
        },
    )
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
    use std::io::Write;
    if path.symlink_metadata().is_ok() {
        check_owned(path)?;
    }
    let directory = path.parent().unwrap();
    fs::create_dir_all(directory)?;
    let original = if path.exists() {
        for file in [path.to_path_buf(), receipt_path(path)] {
            if fs::metadata(&file)?.permissions().readonly() {
                bail!("安装文件只读，未修改：{}", file.display());
            }
            fs::OpenOptions::new().write(true).open(&file)?;
        }
        Some((
            fs::read(path)?,
            fs::metadata(path)?.permissions(),
            fs::read(receipt_path(path))?,
            fs::metadata(receipt_path(path))?.permissions(),
        ))
    } else {
        None
    };
    let mut staged = tempfile::NamedTempFile::new_in(directory)?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    if digest(&fs::read(staged.path())?) != digest(bytes) {
        bail!("安装文件校验失败，原文件未修改")
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        staged
            .as_file()
            .set_permissions(fs::Permissions::from_mode(
                if path.file_name().is_some_and(|name| name == BINARY_NAME) {
                    0o755
                } else {
                    0o600
                },
            ))?;
    }
    let mut record = tempfile::NamedTempFile::new_in(directory)?;
    record.write_all(&serde_json::to_vec(&Receipt {
        owner: OWNER.into(),
        hash: digest(bytes),
        version: sag_protocol::BUILD_VERSION.into(),
        build_id: Some(sag_protocol::BUILD_ID.into()),
        channel: Some(sag_protocol::BUILD_CHANNEL.into()),
    })?)?;
    record.as_file().sync_all()?;
    staged.persist(path)?;
    if let Err(error) = record.persist(receipt_path(path)) {
        if let Some((old, permissions, receipt, receipt_permissions)) = original {
            fs::write(path, old)?;
            fs::set_permissions(path, permissions)?;
            fs::write(receipt_path(path), receipt)?;
            fs::set_permissions(receipt_path(path), receipt_permissions)?;
        } else {
            fs::remove_file(path)?;
        }
        return Err(error).context("安装记录写入失败，已恢复原安装");
    }
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
fn validate_harness(directory: &Path, writing: bool) -> Result<()> {
    let references = directory.join("references");
    match references.symlink_metadata() {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!("参考文档目录是符号链接，未修改：{}", references.display())
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    for (name, _) in SKILL_FILES {
        let file = directory.join(name);
        let record = receipt_path(&file);
        match record.symlink_metadata() {
            Ok(metadata) => {
                if !metadata.file_type().is_file() {
                    bail!("安装记录不是普通文件，未修改：{}", record.display())
                }
                if writing {
                    fs::OpenOptions::new().write(true).open(&record)?;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        match file.symlink_metadata() {
            Ok(_) => {
                check_owned(&file)?;
                if writing {
                    fs::OpenOptions::new()
                        .write(true)
                        .open(&file)
                        .with_context(|| format!("无法更新 Skill：{}", file.display()))?;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    if writing {
        // Check new reference destinations before replacing the installed entry.
        fs::create_dir_all(&references)?;
        tempfile::NamedTempFile::new_in(directory)?;
        tempfile::NamedTempFile::new_in(&references)?;
    }
    Ok(())
}
pub fn harness(home: &Path, name: &str, uninstall: bool) -> Result<PathBuf> {
    let path = skill_path(home, name)?;
    let directory = path.parent().unwrap();
    validate_harness(directory, !uninstall)?;
    for (name, contents) in SKILL_FILES {
        let file = directory.join(name);
        if uninstall {
            remove_owned(&file)?;
        } else {
            write_owned(&file, contents.as_bytes())?;
        }
    }
    Ok(path)
}
pub fn export_skill(parent: &Path) -> Result<PathBuf> {
    let directory = parent.join("sag");
    fs::create_dir(&directory)
        .with_context(|| format!("无法创建 Skill 文件夹：{}", directory.display()))?;
    for (name, contents) in SKILL_FILES {
        let file = directory.join(name);
        fs::create_dir_all(file.parent().unwrap())?;
        fs::write(file, contents)?;
    }
    Ok(directory)
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
            if path.try_exists()? {
                return Ok(path);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(home.join(".local/bin").join(BINARY_NAME))
}
pub fn binary(home: &Path, source: &Path, uninstall: bool) -> Result<PathBuf> {
    let data = crate::cli_protocol::data_dir()?;
    let record = data.join("cli-install-path.json");
    let previous = binary_path(home)?;
    let target = if uninstall {
        previous.clone()
    } else {
        home.join(".local/bin").join(BINARY_NAME)
    };
    if uninstall {
        if target.try_exists()? {
            check_owned(&target)?;
        }
        terminal_path::configure(home, &data, target.parent().unwrap(), true)?;
        remove_owned(&target)?;
        match fs::remove_file(record) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        fs::create_dir_all(&data)?;
        fs::write(data.join("cli-initialized"), b"1")?;
        return Ok(target);
    }
    if target.exists()
        && sag_protocol::BUILD_CHANNEL == "development"
        && receipt(&target)?
            .and_then(|record| record.channel)
            .as_deref()
            == Some("release")
    {
        bail!("开发版不会覆盖已安装的正式版 SAG，请使用独立开发产物")
    }
    if previous.try_exists()? {
        check_owned(&previous)?;
        let installed = receipt(&previous)?.unwrap();
        if sag_protocol::compare_versions(&installed.version, sag_protocol::BUILD_VERSION)?
            == std::cmp::Ordering::Greater
        {
            bail!(
                "已安装较新的 SAG {}，当前版本 {} 不会覆盖它",
                installed.version,
                sag_protocol::BUILD_VERSION
            )
        }
    }
    let bytes = if source == target {
        check_owned(&target)?;
        None
    } else {
        if target.try_exists()? {
            check_owned(&target)?;
        }
        Some(fs::read(source).context("未找到 sag 二进制文件，请重新下载安装包")?)
    };
    // Finish terminal setup before changing the binary or its recorded location.
    terminal_path::configure(home, &data, target.parent().unwrap(), false)?;
    if let Some(bytes) = bytes {
        write_owned(&target, &bytes)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755))?;
    }
    fs::create_dir_all(&data)?;
    if previous != target {
        remove_owned(&previous)?;
    }
    fs::write(record, serde_json::to_vec(&target)?)?;
    fs::write(data.join("cli-initialized"), b"1")?;
    Ok(target)
}

// Keep managed installations current while respecting explicit uninstall and user edits.
pub fn bootstrap(home: &Path, source: &Path) -> Result<()> {
    let record = crate::cli_protocol::data_dir()?.join("cli-initialized");
    let previous = binary_path(home)?;
    let installed = inspect("cli", &previous)?.installed;
    if installed && keep_installed(&receipt(&previous)?.unwrap())? {
        // A matching version with a different build has no reliable ordering.
        // Preserve its CLI and Skills rather than letting an older desktop downgrade them.
        return Ok(());
    }
    let mut skill_updates = Vec::new();
    for name in HARNESS_NAMES {
        let path = skill_path(home, name)?;
        if inspect(name, &path)?.installed {
            if keep_installed(&receipt(&path)?.unwrap())? {
                continue;
            }
            let directory = path.parent().unwrap();
            let mut changed = false;
            for (file, contents) in SKILL_FILES {
                match fs::read(directory.join(file)) {
                    Ok(bytes) => changed |= bytes != contents.as_bytes(),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => changed = true,
                    Err(error) => return Err(error.into()),
                }
            }
            if changed {
                // Validate every replacement before changing the installed command.
                validate_harness(directory, true)?;
                skill_updates.push(name);
            }
        }
    }
    let update_binary = installed && fs::read(source)? != fs::read(&previous)?;
    if !record.try_exists()?
        || update_binary
        || (previous.try_exists()?
            && previous.file_name() != Some(std::ffi::OsStr::new(BINARY_NAME)))
    {
        binary(home, source, false)?;
    }
    for name in skill_updates {
        harness(home, name, false)?;
    }
    Ok(())
}

#[cfg(test)]
mod export_tests {
    use super::*;

    #[test]
    fn export_skill_includes_every_bundled_file_without_installation_receipts() {
        let parent = tempfile::tempdir().unwrap();
        let directory = export_skill(parent.path()).unwrap();
        assert_eq!(directory, parent.path().join("sag"));
        for (name, contents) in SKILL_FILES {
            let path = directory.join(name);
            assert_eq!(fs::read_to_string(&path).unwrap(), *contents);
            assert!(!receipt_path(&path).exists());
        }
        assert_eq!(fs::read_dir(directory.join("references")).unwrap().count(), 3);
    }

    #[test]
    fn export_skill_preserves_an_existing_folder() {
        let parent = tempfile::tempdir().unwrap();
        let directory = parent.path().join("sag");
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("SKILL.md"), "user skill").unwrap();
        assert!(export_skill(parent.path()).is_err());
        assert_eq!(fs::read_to_string(directory.join("SKILL.md")).unwrap(), "user skill");
        assert!(!directory.join("references").exists());
    }
}
