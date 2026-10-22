use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            LoadImageW, SendMessageW, ICON_BIG, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED, WM_SETICON,
        },
    },
};

pub fn set_taskbar_icon(hwnd: HWND) -> windows::core::Result<()> {
    // Tauri sets ICON_SMALL; Windows also needs ICON_BIG for the running app.
    // LR_SHARED keeps the resource icon valid for the lifetime of the process.
    unsafe {
        let icon = LoadImageW(
            Some(GetModuleHandleW(PCWSTR::null())?.into()),
            PCWSTR(tauri::utils::platform::WINDOWS_APP_ICON_RESOURCE_ID as usize as *const u16),
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE | LR_SHARED,
        )?;
        SendMessageW(
            hwnd,
            WM_SETICON,
            Some(WPARAM(ICON_BIG as usize)),
            Some(LPARAM(icon.0 as isize)),
        );
    }
    Ok(())
}

