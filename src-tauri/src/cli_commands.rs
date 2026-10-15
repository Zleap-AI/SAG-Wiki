use crate::{cli_install as install, cli_service::Access, error, AppState, CommandResult};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tauri::{Manager, State};

#[derive(Serialize)]
pub struct IntegrationSettings {
    access: Access,
    installations: Vec<install::Installation>,
}
#[tauri::command]
pub fn get_cli_settings(state: State<Arc<AppState>>) -> CommandResult<IntegrationSettings> {
    let home = dirs::home_dir().ok_or("无法获取用户目录")?;
    let mut installations =
        vec![install::inspect("cli", &install::binary_path(&home)).map_err(error)?];
    for name in install::HARNESS_NAMES {
        installations.push(
            install::inspect(name, &install::skill_path(&home, name).map_err(error)?)
                .map_err(error)?,
        );
    }
    Ok(IntegrationSettings {
        access: state.cli.access(),
        installations,
    })
}
#[tauri::command]
pub fn save_cli_settings(access: Access, state: State<Arc<AppState>>) -> CommandResult<()> {
    state.cli.configure(access).map_err(error)
}
#[tauri::command]
pub async fn install_cli_integration(
    target: String,
    uninstall: bool,
    app: tauri::AppHandle,
) -> CommandResult<String> {
    let home = dirs::home_dir().ok_or("无法获取用户目录")?;
    let source = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../cli/target/debug")
            .join(if cfg!(windows) { "sag.exe" } else { "sag" })
    } else {
        app.path()
            .resource_dir()
            .map_err(error)?
            .join("bin")
            .join(if cfg!(windows) { "sag.exe" } else { "sag" })
    };
    tauri::async_runtime::spawn_blocking(move || {
        if target == "cli" {
            let path = install::binary_path(&home);
            install::binary(&source, &path, uninstall).map_err(error)?;
            Ok(format!(
                "{}：{}。请确保此目录在 PATH 中：{}",
                if uninstall { "已卸载" } else { "已安装" },
                path.display(),
                path.parent().unwrap().display()
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
pub async fn test_cli_connection(path: String) -> CommandResult<String> {
    let home = dirs::home_dir().ok_or("无法获取用户目录")?;
    let binary = install::binary_path(&home);
    if !install::inspect("cli", &binary).map_err(error)?.installed {
        return Err("请先安装 SAG CLI".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let output = std::process::Command::new(binary)
            .arg("status")
            .current_dir(path)
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
