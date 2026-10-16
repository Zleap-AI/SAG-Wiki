use crate::{cli_install as install, cli_service::Access, error, AppState, CommandResult};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tauri::{Manager, State};

#[derive(Serialize)]
pub struct IntegrationSettings {
    access: Access,
    installations: Vec<install::Installation>,
    install_error: Option<String>,
}
#[tauri::command]
pub fn get_cli_settings(state: State<Arc<AppState>>) -> CommandResult<IntegrationSettings> {
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
        access: state.cli.access(),
        install_error: state.cli_install_error.lock().unwrap().clone(),
        installations,
    })
}
#[tauri::command]
pub fn save_cli_settings(access: Access, state: State<Arc<AppState>>) -> CommandResult<()> {
    state.cli.configure(access).map_err(error)
}
fn cli_source(app: &tauri::AppHandle) -> CommandResult<PathBuf> {
    let source = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../cli/target/debug")
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
    install::bootstrap(&home, &cli_source(app)?).map_err(error)
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
                if uninstall { "" } else { "。终端路径已自动配置，请新开终端运行 sgrep。" }
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
pub async fn test_cli_connection() -> CommandResult<String> {
    let home = dirs::home_dir().ok_or("无法获取用户目录")?;
    let binary = install::binary_path(&home).map_err(error)?;
    if !install::inspect("cli", &binary).map_err(error)?.installed {
        return Err("请先安装 sgrep".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let output = std::process::Command::new(binary)
            .arg("status")
            .output()
            .map_err(error)?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    })
    .await
    .map_err(error)?
}
