use crate::{error, stop_background, AppState, CommandResult};
use std::{path::PathBuf, sync::{atomic::Ordering, Arc}};
use tauri::State;

fn environment_override() -> bool {
    std::env::var_os("SAG_DATA_DIR").is_some() || std::env::var_os("SAG_DESKTOP_DATA_DIR").is_some()
}

#[derive(serde::Serialize)]
pub(crate) struct DataDirectory {
    directory: PathBuf,
    environment_override: bool,
}

#[tauri::command]
pub(crate) fn get_data_directory(state: State<'_, Arc<AppState>>) -> DataDirectory {
    DataDirectory { directory: state.data_dir.clone(), environment_override: environment_override() }
}

#[tauri::command]
pub(crate) async fn migrate_data_directory(directory: PathBuf, state: State<'_, Arc<AppState>>) -> CommandResult<()> {
    if environment_override() {
        return Err("数据位置由环境变量指定，请移除 SAG_DATA_DIR / SAG_DESKTOP_DATA_DIR 后重启再调整".into());
    }
    // Reserve shutdown before yielding, preventing concurrent migration requests.
    if state.quitting.swap(true, Ordering::Relaxed) {
        return Err("SAG 正在退出或迁移数据".into());
    }
    let result = async {
        stop_background(&state).await?;
        let source = state.data_dir.clone();
        let binary = state.service_binary.clone();
        let output = tauri::async_runtime::spawn_blocking(move || {
            sag_client::run_captured(std::process::Command::new(binary)
                .args(["service", "migrate", "--source"]).arg(source)
                .arg("--destination").arg(directory))
        }).await.map_err(error)?.map_err(error)?;
        if !output.status.success() {
            return Err(format!("迁移失败：{}", String::from_utf8_lossy(&output.stderr).trim()));
        }
        state.app.request_restart();
        Ok(())
    }.await;
    if result.is_err() {
        state.quitting.store(false, Ordering::Relaxed);
    }
    result
}
