//! Protect the endpoint using the process token, never account-name resolution.
use anyhow::{Context, Result};
use std::{
    fs::OpenOptions,
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{LocalFree, ERROR_INSUFFICIENT_BUFFER},
    Security::{
        Authorization::{
            SetEntriesInAclW, SetSecurityInfo, EXPLICIT_ACCESS_W, SET_ACCESS, SE_FILE_OBJECT,
            TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
        },
        GetTokenInformation, TokenUser, DACL_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER,
    },
    Storage::FileSystem::{FILE_ALL_ACCESS, READ_CONTROL, WRITE_DAC},
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

pub(super) fn protect(path: &Path) -> Result<()> {
    // Request WRITE_DAC explicitly: the temporary file's ordinary read/write
    // handle does not necessarily have permission to update its own DACL.
    let file = OpenOptions::new()
        .access_mode(READ_CONTROL | WRITE_DAC)
        .open(path)
        .context("无法打开连接记录以设置权限")?;
    // SAFETY: token owns a valid handle; the aligned buffer stays alive until
    // after SetEntriesInAclW copies its SID. The allocated ACL is freed exactly
    // once, after SetSecurityInfo has copied it into the file's descriptor.
    unsafe {
        let mut raw = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) == 0 {
            return Err(std::io::Error::last_os_error()).context("无法读取当前进程用户");
        }
        let token = OwnedHandle::from_raw_handle(raw);
        let mut needed = 0;
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            ptr::null_mut(),
            0,
            &mut needed,
        );
        if std::io::Error::last_os_error().raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
        {
            return Err(std::io::Error::last_os_error()).context("无法读取当前用户 SID 长度");
        }
        let mut buffer = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
        if GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        ) == 0
        {
            return Err(std::io::Error::last_os_error()).context("无法读取当前用户 SID");
        }
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let entry = EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS,
            grfAccessMode: SET_ACCESS,
            grfInheritance: 0,
            Trustee: TRUSTEE_W {
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_USER,
                ptstrName: user.User.Sid.cast(),
                ..Default::default()
            },
        };
        let mut acl = ptr::null_mut();
        let result = SetEntriesInAclW(1, &entry, ptr::null(), &mut acl);
        if result != 0 {
            return Err(std::io::Error::from_raw_os_error(result as i32))
                .context("无法创建连接记录权限");
        }
        let result = SetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            acl,
            ptr::null(),
        );
        LocalFree(acl.cast());
        if result != 0 {
            return Err(std::io::Error::from_raw_os_error(result as i32))
                .context("无法设置连接记录权限");
        }
    }
    Ok(())
}
