use crate::{error, stop_background, AppState, CommandResult};
use std::{path::PathBuf, sync::{atomic::Ordering, Arc}};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

fn environment_override() -> bool {
    std::env::var_os("SAG_DATA_DIR").is_some() || std::env::var_os("SAG_DESKTOP_DATA_DIR").is_some()
}

pub(crate) fn recover_at_startup(app: &AppHandle, cause: String) {
    let chinese = sag_protocol::locales::default_interface_language() == "zh-CN";
    let (title, message, choose, quit) = if chinese {
        ("应用数据目录不可用", "请重新连接原磁盘，或选择包含现有 SAG 数据库的文件夹，例如迁移时保留的旧目录。选用旧目录会恢复到备份时的状态。", "选择数据目录", "退出")
    } else {
        ("Application data folder unavailable", "Reconnect the original drive, or choose a folder containing an existing SAG database, such as the retained backup. Choosing the backup restores its earlier state.", "Choose data folder", "Quit")
    };
    let handle = app.clone();
    app.dialog().message(format!("{message}\n\n{cause}"))
        .title(title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(choose.into(), quit.into()))
        .show(move |select| {
            if !select {
                handle.exit(0);
                return;
            }
            let app = handle.clone();
            handle.dialog().file().set_title(choose).pick_folder(move |folder| {
                let Some(folder) = folder else {
                    app.exit(0);
                    return;
                };
                tauri::async_runtime::spawn_blocking(move || {
                    let result = (|| -> anyhow::Result<()> {
                        let directory = folder.into_path()?;
                        let binary = crate::cli_commands::cli_source(&app).map_err(anyhow::Error::msg)?;
                        let output = sag_client::run_captured(std::process::Command::new(binary)
                            .args(["service", "recover", "--directory"]).arg(directory))?;
                        anyhow::ensure!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr).trim());
                        Ok(())
                    })();
                    match result {
                        Ok(()) => app.request_restart(),
                        Err(err) => recover_at_startup(&app, error(err)),
                    }
                });
            });
        });
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
