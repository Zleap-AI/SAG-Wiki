use crate::{cli_install as install, error, rpc, AppState, CommandResult};
use sag_protocol::{Access, Operation};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

#[derive(Serialize)]
pub struct IntegrationSettings {
    access: Access,
    installations: Vec<install::Installation>,
    install_error: Option<String>,
}
#[tauri::command]
pub async fn get_cli_settings(
    state: State<'_, Arc<AppState>>,
) -> CommandResult<IntegrationSettings> {
    let home = dirs::home_dir().ok_or("无法获取用户目录")?;
    let mut installations =
        vec![install::inspect("cli", &install::binary_path(&home).map_err(error)?).map_err(error)?];
    for name in install::HARNESS_NAMES {
        installations.push(
            install::inspect(name, &install::skill_path(&home, name).map_err(error)?)
                .map_err(error)?,
        );
    }
    Ok(IntegrationSettings {
        access: serde_json::from_value(rpc(&state, Operation::GetAccess).await?).map_err(error)?,
        install_error: state.cli_install_error.lock().unwrap().clone(),
        installations,
    })
}
#[tauri::command]
pub async fn save_cli_settings(
    access: Access,
    state: State<'_, Arc<AppState>>,
) -> CommandResult<()> {
    rpc(&state, Operation::SetAccess { access }).await?;
    Ok(())
}
pub(crate) fn cli_source(app: &tauri::AppHandle) -> CommandResult<PathBuf> {
    let source = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("bin/development")
            .join(install::BINARY_NAME)
    } else {
        app.path()
            .resource_dir()
            .map_err(error)?
            .join("bin")
            .join(install::BINARY_NAME)
    };
    Ok(source)
}
pub fn initialize_cli(app: &tauri::AppHandle) -> CommandResult<()> {
    let home = dirs::home_dir().ok_or("无法获取用户目录")?;
    if cfg!(debug_assertions) {
        return Ok(());
    }
    install::bootstrap(&home, &cli_source(app)?).map_err(error)
}
pub(crate) fn service_source(
    app: &tauri::AppHandle,
    installation_succeeded: bool,
) -> CommandResult<PathBuf> {
    if !cfg!(debug_assertions) && installation_succeeded {
        let home = dirs::home_dir().ok_or("无法获取用户目录")?;
        let installed = install::binary_path(&home).map_err(error)?;
        if install::inspect("cli", &installed)
            .map_err(error)?
            .installed
        {
            return Ok(installed);
        }
    }
    cli_source(app)
}
#[tauri::command]
pub async fn install_cli_integration(
    target: String,
    uninstall: bool,
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> CommandResult<String> {
    let home = dirs::home_dir().ok_or("无法获取用户目录")?;
    let source = cli_source(&app)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if target == "cli" {
            let path = install::binary(&home, &source, uninstall).map_err(error)?;
            *state.cli_install_error.lock().unwrap() = None;
            Ok(format!(
                "{}：{}{}",
                if uninstall { "已卸载" } else { "已安装" },
                path.display(),
                if uninstall {
                    ""
                } else {
                    "。终端路径已自动配置，请新开终端运行 sag。"
                }
            ))
        } else {
            let path = install::harness(&home, &target, uninstall).map_err(error)?;
            Ok(format!(
                "{}：{}。请重新启动对应 Harness。",
                if uninstall { "已卸载" } else { "已安装" },
                path.display()
            ))
        }
    })
    .await
    .map_err(error)?
}
#[tauri::command]
pub async fn download_cli_skill(
    title: String,
    app: tauri::AppHandle,
) -> CommandResult<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(folder) = app.dialog().file().set_title(title).blocking_pick_folder() else {
            return Ok(None);
        };
        let folder = folder.into_path().map_err(error)?;
        let path = install::export_skill(&folder).map_err(error)?;
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(error)?
}
#[tauri::command]
pub async fn test_cli_connection(state: State<'_, Arc<AppState>>) -> CommandResult<String> {
    let info = rpc(&state, Operation::Info).await?;
    Ok(format!(
        "SAG 服务已连接\n{}",
        serde_json::to_string_pretty(&info).map_err(error)?
    ))
}
