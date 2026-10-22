//! Atomic document publication across the app/NAS-user privilege boundary.
// Shared by the gateway and worker binaries, which use different entry points.
#![allow(dead_code)]
use anyhow::{bail, Result};
#[cfg(target_os = "linux")]
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub uid: u32,
    pub temporary: PathBuf,
    pub target: PathBuf,
    pub expected_hash: Option<String>,
}

#[cfg(target_os = "linux")]
pub fn request(socket: &Path, request: &Request) -> Result<()> {
    use std::io::{BufRead, BufReader, Write};
    let mut connection = std::os::unix::net::UnixStream::connect(socket)?;
    connection.set_read_timeout(Some(std::time::Duration::from_secs(30)))?;
    connection.set_write_timeout(Some(std::time::Duration::from_secs(30)))?;
    serde_json::to_writer(&mut connection, &serde_json::json!({"document_publish":request}))?;
    connection.write_all(b"\n")?;
    let mut line = String::new();
    BufReader::new(connection).read_line(&mut line)?;
    let response: serde_json::Value = serde_json::from_str(&line)?;
    if response["ok"] != true { bail!("{}", response["error"].as_str().unwrap_or("无法保存文档")); }
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn prepare(request: &Request, app_uid: u32, user_gid: u32) -> Result<std::fs::Metadata> {
    use std::os::{fd::AsRawFd, unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt}};
    let parent = request.target.parent().context("文档缺少父目录")?.canonicalize()?;
    if request.temporary.parent().context("临时文件缺少父目录")?.canonicalize()? != parent
        || !request.temporary.file_name().is_some_and(|name| name.to_string_lossy().starts_with(".sag-save-"))
        || request.target.extension().is_none_or(|extension| !extension.eq_ignore_ascii_case("md")) {
        bail!("文档发布路径无效");
    }
    let temporary = std::fs::OpenOptions::new().read(true).write(true)
        .custom_flags(libc::O_NOFOLLOW).open(&request.temporary)?;
    let metadata = temporary.metadata()?;
    if !metadata.is_file() || metadata.uid() != app_uid || metadata.nlink() != 1 {
        bail!("文档临时文件无效");
    }
    let directory = std::fs::File::open(&parent)?;
    // fnOS volumes implement native ACL inheritance through system.trim_acl.
    // chmod materializes a different explicit native ACL even for mode 0600.
    // ENODATA is a supported, inherited ACL; ordinary Linux returns ENOTSUP.
    if !matches!(read_attribute(&directory, c"system.trim_acl")?, Attribute::Unsupported) {
        let source = if request.expected_hash.is_some() {
            Some(std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(&request.target)?)
        } else { None };
        let (uid, gid, native, posix) = if let Some(source) = source {
            let metadata = source.metadata()?;
            if !metadata.is_file() || metadata.uid() == 0 || metadata.mode() & 0o7000 != 0 {
                bail!("不能替换系统或特殊权限文档");
            }
            (metadata.uid(), metadata.gid(), read_attribute(&source, c"system.trim_acl")?, read_attribute(&source, c"system.posix_acl_access")?)
        } else {
            (request.uid, user_gid, Attribute::Supported(None), Attribute::Supported(None))
        };
        if unsafe { libc::fchown(temporary.as_raw_fd(), uid, gid) } != 0 { return Err(std::io::Error::last_os_error().into()); }
        apply_attribute(&temporary, c"system.posix_acl_access", posix)?;
        // Apply native ACL last, including removal when the source inherits.
        // This also removes the explicit ACL created by tempfile's 0600 mode.
        apply_attribute(&temporary, c"system.trim_acl", native)?;
        temporary.sync_all()?;
        return Ok(temporary.metadata()?);
    }
    let (uid, gid, mode, acl) = if request.expected_hash.is_some() {
        let source = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(&request.target)?;
        let metadata = source.metadata()?;
        if !metadata.is_file() || metadata.uid() == 0 || metadata.mode() & 0o7000 != 0 {
            bail!("不能替换系统或特殊权限文档");
        }
        let attribute = c"system.posix_acl_access";
        let size = unsafe { libc::fgetxattr(source.as_raw_fd(), attribute.as_ptr(), std::ptr::null_mut(), 0) };
        let acl = if size < 0 {
            let error = std::io::Error::last_os_error();
            if !matches!(error.raw_os_error(), Some(libc::ENODATA | libc::ENOTSUP)) { return Err(error.into()); }
            None
        } else {
            let mut bytes = vec![0u8; size as usize];
            let count = unsafe { libc::fgetxattr(source.as_raw_fd(), attribute.as_ptr(), bytes.as_mut_ptr().cast(), bytes.len()) };
            if count < 0 { return Err(std::io::Error::last_os_error().into()); }
            bytes.truncate(count as usize);
            Some(bytes)
        };
        (metadata.uid(), metadata.gid(), metadata.mode() & 0o777, acl)
    } else {
        // Restore the parent's default ACL (the 0600 temporary file has already
        // narrowed its access mask). Keep every inherited effective grant before
        // expanding the mask for app access, and exclude execute bits on a file.
        use std::collections::BTreeMap;
        let directory = std::fs::File::open(&parent)?;
        let attribute = c"system.posix_acl_default";
        let size = unsafe { libc::fgetxattr(directory.as_raw_fd(), attribute.as_ptr(), std::ptr::null_mut(), 0) };
        let mut entries = BTreeMap::new();
        if size < 0 {
            let error = std::io::Error::last_os_error();
            if !matches!(error.raw_os_error(), Some(libc::ENODATA | libc::ENOTSUP)) { return Err(error.into()); }
            entries.extend([((1u16, u32::MAX), 6u16), ((4, u32::MAX), 0), ((32, u32::MAX), 0)]);
        } else {
            let mut bytes = vec![0u8; size as usize];
            let count = unsafe { libc::fgetxattr(directory.as_raw_fd(), attribute.as_ptr(), bytes.as_mut_ptr().cast(), bytes.len()) };
            if count < 0 { return Err(std::io::Error::last_os_error().into()); }
            bytes.truncate(count as usize);
            if bytes.len() < 4 || bytes[..4] != 2u32.to_le_bytes() || (bytes.len() - 4) % 8 != 0 { bail!("父目录默认 ACL 格式无效"); }
            for entry in bytes[4..].chunks_exact(8) {
                entries.insert((u16::from_le_bytes(entry[..2].try_into()?), u32::from_le_bytes(entry[4..].try_into()?)), u16::from_le_bytes(entry[2..4].try_into()?));
            }
        }
        let mask = entries.get(&(16, u32::MAX)).copied().unwrap_or(7) & 6;
        for (&(tag, _), permission) in &mut entries {
            *permission &= if matches!(tag, 2 | 4 | 8) { mask } else { 6 };
        }
        entries.insert((2, app_uid), 6);
        entries.insert((16, u32::MAX), mask | 6);
        let mut acl = 2u32.to_le_bytes().to_vec();
        for ((tag, id), permission) in entries {
            acl.extend(tag.to_le_bytes()); acl.extend(permission.to_le_bytes()); acl.extend(id.to_le_bytes());
        }
        let parent_metadata = directory.metadata()?;
        let gid = if parent_metadata.mode() & 0o2000 != 0 { parent_metadata.gid() } else { user_gid };
        (request.uid, gid, 0o600, Some(acl))
    };
    if unsafe { libc::fchown(temporary.as_raw_fd(), uid, gid) } != 0 { return Err(std::io::Error::last_os_error().into()); }
    temporary.set_permissions(std::fs::Permissions::from_mode(mode))?;
    let attribute = c"system.posix_acl_access";
    match acl {
        Some(bytes) => {
            if unsafe { libc::fsetxattr(temporary.as_raw_fd(), attribute.as_ptr(), bytes.as_ptr().cast(), bytes.len(), 0) } != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        },
        None => {
            // A parent default ACL may have been inherited by the temporary file.
            // An existing file without an ACL must not acquire new named grants.
            let result = unsafe { libc::fremovexattr(temporary.as_raw_fd(), attribute.as_ptr()) };
            if result != 0 && !matches!(std::io::Error::last_os_error().raw_os_error(), Some(libc::ENODATA | libc::ENOTSUP)) {
                return Err(std::io::Error::last_os_error().into());
            }
        },
    }
    temporary.sync_all()?;
    Ok(temporary.metadata()?)
}

#[cfg(target_os = "linux")]
pub fn publish(request: &Request, device: u64, inode: u64) -> Result<()> {
    use std::{ffi::CString, os::unix::fs::{MetadataExt, OpenOptionsExt}};
    let metadata = request.temporary.symlink_metadata()?;
    if !metadata.is_file() || metadata.dev() != device || metadata.ino() != inode { bail!("文档临时文件已被替换"); }
    if let Some(expected) = &request.expected_hash {
        let mut source = std::fs::OpenOptions::new().read(true).write(true).custom_flags(libc::O_NOFOLLOW).open(&request.target)?;
        use std::io::Read;
        let mut bytes = Vec::new(); source.read_to_end(&mut bytes)?;
        if sag_core::store::hash(&bytes) != *expected { bail!("保存前检测到外部修改，未覆盖原文件"); }
        std::fs::rename(&request.temporary, &request.target)?;
    } else {
        let temporary = CString::new(request.temporary.as_os_str().as_encoded_bytes())?;
        let target = CString::new(request.target.as_os_str().as_encoded_bytes())?;
        if unsafe { libc::renameat2(libc::AT_FDCWD, temporary.as_ptr(), libc::AT_FDCWD, target.as_ptr(), libc::RENAME_NOREPLACE) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    // The rename has committed. A directory fsync failure must not be reported
    // as "not saved" after the source has already changed.
    if let Ok(directory) = std::fs::File::open(request.target.parent().unwrap()) { let _ = directory.sync_all(); }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum Attribute {
    Unsupported,
    Supported(Option<Vec<u8>>),
}

fn read_attribute(file: &std::fs::File, name: &std::ffi::CStr) -> Result<Attribute> {
    use std::os::fd::AsRawFd;
    let fd = file.as_raw_fd();
    #[cfg(target_os = "linux")]
    let size = unsafe { libc::fgetxattr(fd, name.as_ptr(), std::ptr::null_mut(), 0) };
    #[cfg(target_os = "macos")]
    let size = unsafe { libc::fgetxattr(fd, name.as_ptr(), std::ptr::null_mut(), 0, 0, 0) };
    if size < 0 {
        let error = std::io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(libc::ENOTSUP) => Ok(Attribute::Unsupported),
            Some(libc::ENODATA) => Ok(Attribute::Supported(None)),
            #[cfg(target_os = "macos")]
            Some(libc::ENOATTR) => Ok(Attribute::Supported(None)),
            _ => Err(error.into()),
        };
    }
    let mut bytes = vec![0u8; size as usize];
    #[cfg(target_os = "linux")]
    let count = unsafe { libc::fgetxattr(fd, name.as_ptr(), bytes.as_mut_ptr().cast(), bytes.len()) };
    #[cfg(target_os = "macos")]
    let count = unsafe { libc::fgetxattr(fd, name.as_ptr(), bytes.as_mut_ptr().cast(), bytes.len(), 0, 0) };
    if count < 0 { return Err(std::io::Error::last_os_error().into()); }
    bytes.truncate(count as usize);
    Ok(Attribute::Supported(Some(bytes)))
}

fn apply_attribute(file: &std::fs::File, name: &std::ffi::CStr, attribute: Attribute) -> Result<()> {
    use std::os::fd::AsRawFd;
    let fd = file.as_raw_fd();
    let code = match attribute {
        Attribute::Unsupported => return Ok(()),
        Attribute::Supported(Some(bytes)) => {
            #[cfg(target_os = "linux")]
            let code = unsafe { libc::fsetxattr(fd, name.as_ptr(), bytes.as_ptr().cast(), bytes.len(), 0) };
            #[cfg(target_os = "macos")]
            let code = unsafe { libc::fsetxattr(fd, name.as_ptr(), bytes.as_ptr().cast(), bytes.len(), 0, 0) };
            code
        },
        Attribute::Supported(None) => {
            #[cfg(target_os = "linux")]
            let code = unsafe { libc::fremovexattr(fd, name.as_ptr()) };
            #[cfg(target_os = "macos")]
            let code = unsafe { libc::fremovexattr(fd, name.as_ptr(), 0) };
            if code != 0 {
                let error = std::io::Error::last_os_error();
                match error.raw_os_error() {
                    Some(libc::ENODATA | libc::ENOTSUP) => return Ok(()),
                    #[cfg(target_os = "macos")]
                    Some(libc::ENOATTR) => return Ok(()),
                    _ => return Err(error.into()),
                }
            }
            code
        },
    };
    if code != 0 { return Err(std::io::Error::last_os_error().into()); }
    Ok(())
}

#[cfg(test)]
mod acl_attribute_tests {
    use super::*;
    use std::os::fd::AsRawFd;
    fn set(file: &std::fs::File, bytes: &[u8]) {
        #[cfg(target_os = "linux")]
        let code = unsafe { libc::fsetxattr(file.as_raw_fd(), c"user.sag-document-acl".as_ptr(), bytes.as_ptr().cast(), bytes.len(), 0) };
        #[cfg(target_os = "macos")]
        let code = unsafe { libc::fsetxattr(file.as_raw_fd(), c"user.sag-document-acl".as_ptr(), bytes.as_ptr().cast(), bytes.len(), 0, 0) };
        assert_eq!(code, 0, "{}", std::io::Error::last_os_error());
    }
    #[test]
    fn preserves_explicit_native_acl_bytes_exactly() {
        let source = tempfile::tempfile().unwrap();
        let target = tempfile::tempfile().unwrap();
        let bytes = [1, 0, 0, 0, 5, 0, 0, 0, 207, 55, 0, 0, 7, 0, 0, 0, 217, 3, 0, 0, 0, 0, 0, 0];
        set(&source, &bytes);
        set(&target, b"temporary explicit ACL");
        let attribute = read_attribute(&source, c"user.sag-document-acl").unwrap();
        apply_attribute(&target, c"user.sag-document-acl", attribute).unwrap();
        assert_eq!(read_attribute(&target, c"user.sag-document-acl").unwrap(), Attribute::Supported(Some(bytes.to_vec())));
    }
    #[test]
    fn absent_native_acl_removes_temporary_override_to_restore_inheritance() {
        let source = tempfile::tempfile().unwrap();
        let target = tempfile::tempfile().unwrap();
        set(&target, b"temporary explicit ACL");
        let attribute = read_attribute(&source, c"user.sag-document-acl").unwrap();
        assert_eq!(attribute, Attribute::Supported(None));
        apply_attribute(&target, c"user.sag-document-acl", attribute).unwrap();
        assert_eq!(read_attribute(&target, c"user.sag-document-acl").unwrap(), Attribute::Supported(None));
    }
}
