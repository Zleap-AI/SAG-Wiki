//! Give the requesting NAS user access to app-created conversion output.
//! Other ACL entries retain their effective permissions when the mask grows.
use anyhow::Result;
#[cfg(target_os = "linux")]
use anyhow::{bail, Context};
use std::path::Path;

#[cfg(target_os = "linux")]
pub fn share_conversion(path: &Path, uid: u32, writable: bool) -> Result<()> {
    use std::{
        collections::BTreeMap,
        os::{
            fd::AsRawFd,
            unix::{
                ffi::OsStrExt,
                fs::{MetadataExt, OpenOptionsExt},
            },
        },
    };
    const USER_OBJ: u16 = 1;
    const USER: u16 = 2;
    const GROUP_OBJ: u16 = 4;
    const GROUP: u16 = 8;
    const MASK: u16 = 16;
    const OTHER: u16 = 32;
    const UNDEFINED: u32 = u32::MAX;
    let attribute = c"system.posix_acl_access";
    if !path
        .components()
        .any(|part| part.as_os_str() == ".sag-converted")
    {
        bail!("共享权限仅用于转换目录");
    }
    for item in path.ancestors() {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(item)
            .with_context(|| format!("无法设置转换文件权限：{}", item.display()))?;
        let metadata = file.metadata()?;
        if metadata.uid() == unsafe { libc::geteuid() } {
            let fd = file.as_raw_fd();
            let size = unsafe { libc::fgetxattr(fd, attribute.as_ptr(), std::ptr::null_mut(), 0) };
            let mut entries = BTreeMap::new();
            if size < 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ENODATA) {
                    return Err(error.into());
                }
                for (tag, shift) in [(USER_OBJ, 6), (GROUP_OBJ, 3), (OTHER, 0)] {
                    entries.insert((tag, UNDEFINED), ((metadata.mode() >> shift) & 7) as u16);
                }
            } else {
                let mut bytes = vec![0u8; size as usize];
                let count = unsafe {
                    libc::fgetxattr(
                        fd,
                        attribute.as_ptr(),
                        bytes.as_mut_ptr().cast(),
                        bytes.len(),
                    )
                };
                if count < 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                bytes.truncate(count as usize);
                if bytes.len() < 4 || bytes[..4] != 2u32.to_le_bytes() || (bytes.len() - 4) % 8 != 0
                {
                    bail!("转换文件 ACL 格式无效");
                }
                for entry in bytes[4..].chunks_exact(8) {
                    entries.insert(
                        (
                            u16::from_le_bytes(entry[..2].try_into()?),
                            u32::from_le_bytes(entry[4..].try_into()?),
                        ),
                        u16::from_le_bytes(entry[2..4].try_into()?),
                    );
                }
            }
            let mask = *entries.get(&(MASK, UNDEFINED)).unwrap_or(&7);
            for (&(tag, _), permission) in &mut entries {
                if matches!(tag, USER | GROUP_OBJ | GROUP) {
                    *permission &= mask;
                }
            }
            let permission = if metadata.is_dir() {
                if writable {
                    7
                } else {
                    5
                }
            } else if writable {
                6
            } else {
                4
            };
            entries.insert((USER, uid), permission);
            entries.insert((MASK, UNDEFINED), mask | permission);
            let mut bytes = 2u32.to_le_bytes().to_vec();
            for ((tag, id), permission) in entries {
                bytes.extend(tag.to_le_bytes());
                bytes.extend(permission.to_le_bytes());
                bytes.extend(id.to_le_bytes());
            }
            if unsafe {
                libc::fsetxattr(
                    fd,
                    attribute.as_ptr(),
                    bytes.as_ptr().cast(),
                    bytes.len(),
                    0,
                )
            } != 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        if item
            .file_name()
            .is_some_and(|name| name.as_bytes() == b".sag-converted")
        {
            break;
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn share_conversion(_path: &Path, _uid: u32, _writable: bool) -> Result<()> {
    Ok(())
}
