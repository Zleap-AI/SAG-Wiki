use crate::{error, CommandResult};
use sag_protocol::types::Candidate;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Mutex};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Clone, Serialize, Deserialize)]
pub struct Preferences {
    pub enabled: bool,
    pub shortcut: String,
}
impl Default for Preferences {
    fn default() -> Self {
        Self { enabled: true, shortcut: "CommandOrControl+Shift+Space".into() }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct OpenRequest {
    pub query: String,
    pub candidate: Option<Candidate>,
    #[serde(default)]
    pub deep: bool,
}
pub struct QuickSearch {
    path: PathBuf,
    preferences: Mutex<Preferences>,
    registration_error: Mutex<Option<String>>,
    previous: Mutex<Option<isize>>,
    pending: Mutex<Option<OpenRequest>>,
}

pub fn initialize(app: &tauri::AppHandle, directory: &std::path::Path) -> anyhow::Result<()> {
    let path = directory.join("quick-search.json");
    let preferences: Preferences = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Preferences::default(),
        Err(e) => return Err(e.into()),
    };
    // A conflict must leave the app and shortcut settings available.
    let registration_error = if preferences.enabled {
        app.global_shortcut().register(preferences.shortcut.as_str()).err().map(|e| format!("无法注册全局快捷键：{e}"))
    } else { None };
    app.manage(QuickSearch { path, preferences: Mutex::new(preferences), registration_error: Mutex::new(registration_error), previous: Mutex::new(None), pending: Mutex::new(None) });
    Ok(())
}

pub fn handle(app: &tauri::AppHandle, _: &Shortcut, event: tauri_plugin_global_shortcut::ShortcutEvent) {
    if event.state == ShortcutState::Pressed {
        if let Err(e) = toggle(app) { crate::emit(app, "app-error", e); }
    }
}

fn quick_search_window(app: &tauri::AppHandle) -> CommandResult<tauri::WebviewWindow> {
    app.get_webview_window("quick-search").ok_or_else(|| "快捷搜索窗口不存在".to_string())
}

pub fn toggle(app: &tauri::AppHandle) -> CommandResult<()> {
    let window = quick_search_window(app)?;
    if window.is_visible().map_err(error)? && window.is_focused().map_err(error)? {
        return dismiss(app, true);
    }
    *app.state::<QuickSearch>().previous.lock().unwrap() = previous_application();
    let cursor = app.cursor_position().map_err(error)?;
    if let Some(monitor) = app.monitor_from_point(cursor.x, cursor.y).map_err(error)? {
        let area = monitor.work_area();
        let size = window.outer_size().map_err(error)?;
        window.set_position(tauri::PhysicalPosition::new(
            area.position.x + (area.size.width as i32 - size.width as i32) / 2,
            area.position.y + ((area.size.height as i32 - size.height as i32) / 3).max(0),
        )).map_err(error)?;
    }
    window.show().map_err(error)?;
    window.set_focus().map_err(error)?;
    window.emit("quick-search-shown", ()).map_err(error)
}

pub fn dismiss(app: &tauri::AppHandle, restore: bool) -> CommandResult<()> {
    let previous = app.state::<QuickSearch>().previous.lock().unwrap().take();
    quick_search_window(app)?.hide().map_err(error)?;
    if let Some(previous) = previous {
        if restore { restore_application(previous); }
    }
    app.emit_to("quick-search", "quick-search-hidden", ()).map_err(error)
}

#[tauri::command]
pub fn get_quick_search(app: tauri::AppHandle) -> serde_json::Value {
    let state = app.state::<QuickSearch>();
    serde_json::json!({"preferences": *state.preferences.lock().unwrap(), "error": *state.registration_error.lock().unwrap()})
}

#[tauri::command]
pub fn save_quick_search(app: tauri::AppHandle, preferences: Preferences) -> CommandResult<()> {
    let shortcut: Shortcut = preferences.shortcut.parse().map_err(|e| format!("快捷键格式不正确：{e}"))?;
    let state = app.state::<QuickSearch>();
    let mut old = state.preferences.lock().unwrap();
    let old_registered = old.enabled && state.registration_error.lock().unwrap().is_none();
    let same = old.shortcut.parse::<Shortcut>().map_err(error)? == shortcut;
    let registered_new = preferences.enabled && (!same || !old_registered);
    if registered_new { app.global_shortcut().register(shortcut).map_err(|e| format!("无法注册全局快捷键：{e}"))?; }
    let persisted = (|| -> anyhow::Result<()> {
        use std::io::Write;
        std::fs::create_dir_all(state.path.parent().unwrap())?;
        let mut file = tempfile::NamedTempFile::new_in(state.path.parent().unwrap())?;
        file.write_all(&serde_json::to_vec_pretty(&preferences)?)?;
        file.persist(&state.path)?;
        Ok(())
    })();
    if let Err(e) = persisted {
        if registered_new { app.global_shortcut().unregister(shortcut).map_err(error)?; }
        return Err(error(e));
    }
    if old_registered && (!preferences.enabled || !same) {
        app.global_shortcut().unregister(old.shortcut.as_str()).map_err(error)?;
    }
    *old = preferences;
    *state.registration_error.lock().unwrap() = None;
    Ok(())
}

#[tauri::command]
pub fn show_quick_search(app: tauri::AppHandle) -> CommandResult<()> { toggle(&app) }
#[tauri::command]
pub fn hide_quick_search(app: tauri::AppHandle) -> CommandResult<()> { dismiss(&app, true) }
#[tauri::command]
pub fn open_quick_search_result(app: tauri::AppHandle, request: OpenRequest) -> CommandResult<()> {
    *app.state::<QuickSearch>().pending.lock().unwrap() = Some(request);
    dismiss(&app, false)?;
    crate::show_main_window(&app).map_err(error)?;
    app.emit_to("main", "quick-search-open", ()).map_err(error)
}
#[tauri::command]
pub fn take_quick_search_request(app: tauri::AppHandle) -> Option<OpenRequest> {
    app.state::<QuickSearch>().pending.lock().unwrap().take()
}
#[tauri::command]
pub fn quick_search_main_ready(app: tauri::AppHandle) -> CommandResult<()> {
    if app.state::<QuickSearch>().pending.lock().unwrap().is_some() {
        app.emit_to("main", "quick-search-open", ()).map_err(error)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn previous_application() -> Option<isize> {
    objc2_app_kit::NSWorkspace::sharedWorkspace().frontmostApplication().map(|app| app.processIdentifier() as isize)
}
#[cfg(target_os = "macos")]
fn restore_application(pid: isize) {
    if let Some(app) = objc2_app_kit::NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32) {
        if !app.activateWithOptions(objc2_app_kit::NSApplicationActivationOptions::empty()) {
            eprintln!("快捷搜索：无法恢复之前的应用焦点");
        }
    }
}
#[cfg(windows)]
fn previous_application() -> Option<isize> {
    let window = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
    (!window.is_invalid()).then_some(window.0 as isize)
}
#[cfg(windows)]
fn restore_application(handle: isize) {
    if !unsafe { windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(windows::Win32::Foundation::HWND(handle as *mut _)) }.as_bool() {
        eprintln!("快捷搜索：无法恢复之前的窗口焦点");
    }
}
#[cfg(not(any(target_os = "macos", windows)))]
fn previous_application() -> Option<isize> { None }
#[cfg(not(any(target_os = "macos", windows)))]
fn restore_application(_: isize) {}
