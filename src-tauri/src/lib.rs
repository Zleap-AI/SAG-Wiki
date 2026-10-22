mod cli_commands;
mod folder_picker;
mod updates;
#[cfg(windows)]
mod windows_icon;
#[path = "../../cli/src/install.rs"]
mod cli_install;
use sag_client::Client;
use sag_protocol as cli_protocol;
use sag_protocol::{self as protocol, types::*, DoclingProfile, Operation, Output, Query, ServiceEvent};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, State,
};
type CommandResult<T> = Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
struct AppState {
    client: Mutex<Client>,
    reconnect: tokio::sync::Mutex<()>,
    app: tauri::AppHandle,
    data_dir: PathBuf,
    service_binary: PathBuf,
    cli_install_error: Mutex<Option<String>>,
}
fn emit(app: &tauri::AppHandle, event: &str, payload: impl serde::Serialize + Clone) {
    if let Err(error) = app.emit(event, payload) {
        eprintln!("{event}: {error}");
    }
}
fn output_value(output: Output) -> CommandResult<serde_json::Value> {
    use Output::*;
    let value = match output {
        Unit => serde_json::Value::Null,
        Info(value) => serde_json::to_value(value).map_err(error)?,
        Snapshot(value) => serde_json::to_value(value).map_err(error)?,
        Cli(value) => serde_json::to_value(value).map_err(error)?,
        Access(value) => serde_json::to_value(value).map_err(error)?,
        Roots(value) => serde_json::to_value(value).map_err(error)?,
        Entries(value) => serde_json::to_value(value).map_err(error)?,
        Files(value) => serde_json::to_value(value).map_err(error)?,
        Document(value) => serde_json::to_value(value).map_err(error)?,
        Strings(value) => serde_json::to_value(value).map_err(error)?,
        Text(value) => serde_json::to_value(value).map_err(error)?,
        Drafts(value) => serde_json::to_value(value).map_err(error)?,
        Settings(value) => serde_json::to_value(value).map_err(error)?,
        Docling(value) => serde_json::to_value(value).map_err(error)?,
        IndexStatus(value) => serde_json::to_value(value).map_err(error)?,
        IndexHistory(value) => serde_json::to_value(value).map_err(error)?,
        IndexHistoryLog(value) => serde_json::to_value(value).map_err(error)?,
        FolderIndexOptions(value) => serde_json::to_value(value).map_err(error)?,
        McpTools(value) => serde_json::to_value(value).map_err(error)?,
        Conversations(value) => serde_json::to_value(value).map_err(error)?,
        Conversation(value) => serde_json::to_value(value).map_err(error)?,
        Query(value) => serde_json::to_value(value).map_err(error)?,
        SearchSummary(value) => serde_json::to_value(value).map_err(error)?,
        Overview(value) => serde_json::to_value(value).map_err(error)?,
        Graph(value) => serde_json::to_value(value).map_err(error)?,
        Neighbors(value) => serde_json::to_value(value).map_err(error)?,
        Evidence(value) => serde_json::to_value(value).map_err(error)?,
    };
    Ok(value)
}
async fn rpc(state: &AppState, operation: Operation) -> CommandResult<serde_json::Value> {
    let mut client = state.client.lock().unwrap().clone();
    if !client.is_connected() {
        let _guard = state.reconnect.lock().await;
        client = state.client.lock().unwrap().clone();
        if !client.is_connected() {
            client = sag_client::ensure(&state.data_dir, &state.service_binary)
                .await
                .map_err(error)?;
            client
                .call(Operation::Snapshot, Duration::from_secs(30))
                .await
                .map_err(error)?;
            *state.client.lock().unwrap() = client.clone();
            emit(&state.app, "data-changed", ());
            emit(&state.app, "history-changed", ());
            emit(&state.app, "settings-changed", ());
        }
    }
    output_value(
        client
            .call(operation, Duration::from_secs(600))
            .await
            .map_err(error)?,
    )
}
macro_rules! remote {
    ($name:ident() => $variant:ident) => {
        #[tauri::command]
        async fn $name(state:State<'_,Arc<AppState>>)->CommandResult<serde_json::Value>{rpc(&state,Operation::$variant).await}
    };
    ($name:ident($($arg:ident:$ty:ty),+) => $variant:ident) => {
        #[tauri::command]
        async fn $name($($arg:$ty,)+state:State<'_,Arc<AppState>>)->CommandResult<serde_json::Value>{rpc(&state,Operation::$variant{$($arg),+}).await}
    };
}
/// Connects to the service once at startup and localizes the tray menu.
///
/// Connection failures are reported to the window instead of aborting
/// setup: a stale service record, an older daemon that is still running,
/// or a slow shutdown during upgrade must leave the user with a visible
/// error and a retry, not a silently closed application.
fn connect_on_startup(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        match rpc(&state, Operation::GetSettings).await {
            Ok(value) => {
                if let Some(language) = value.get("language").and_then(|v| v.as_str()) {
                    if let Some(tray) = state.app.tray_by_id("main") {
                        match tray_menu(&state.app, language) {
                            Ok(menu) => {
                                if let Err(e) = tray.set_menu(Some(menu)) {
                                    eprintln!("更新托盘菜单失败：{e}");
                                }
                            }
                            Err(e) => eprintln!("构建托盘菜单失败：{e}"),
                        }
                    }
                }
            }
            Err(error) => eprintln!("启动时连接 SAG 服务失败：{error}"),
        }
    });
}
fn forward_events(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut reported: Option<String> = None;
        loop {
            let mut client = state.client.lock().unwrap().clone();
            if !client.is_connected() {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let _guard = state.reconnect.lock().await;
                client = state.client.lock().unwrap().clone();
                if !client.is_connected() {
                    // Keep trying: the user may stop an old service or the
                    // window may trigger `rpc`, which restarts it. Each
                    // distinct failure is reported once.
                    let failure = match Client::connect(&state.data_dir).await {
                        Ok(Some(connected)) => {
                            match protocol::BuildInfo::from(connected.info())
                                .check_compatible(&protocol::BuildInfo::current())
                            {
                                Ok(()) => {
                                    client = connected;
                                    *state.client.lock().unwrap() = client.clone();
                                    reported = None;
                                    None
                                }
                                Err(error) => Some(error.to_string()),
                            }
                        }
                        Ok(None) => continue,
                        Err(error) => Some(format!("SAG 服务重连失败：{error}")),
                    };
                    if let Some(message) = failure {
                        if reported.as_deref() != Some(&message) {
                            emit(&state.app, "app-error", message.clone());
                            reported = Some(message);
                        }
                        drop(_guard);
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                }
            }
            let mut events = client.subscribe();
            if let Err(error) = client
                .call(Operation::Snapshot, Duration::from_secs(30))
                .await
            {
                emit(
                    &state.app,
                    "app-error",
                    format!("无法恢复 SAG 状态：{error}"),
                );
                continue;
            }
            emit(&state.app, "data-changed", ());
            emit(&state.app, "history-changed", ());
            emit(&state.app, "settings-changed", ());
            loop {
                match events.recv().await {
                    Ok(envelope) => match envelope.event {
                        ServiceEvent::DataChanged => emit(&state.app, "data-changed", ()),
                        ServiceEvent::SettingsChanged => emit(&state.app, "settings-changed", ()),
                        ServiceEvent::HistoryChanged => emit(&state.app, "history-changed", ()),
                        ServiceEvent::IndexProgress(value) => {
                            emit(&state.app, "index-progress", value)
                        }
                        ServiceEvent::AppError(value) => emit(&state.app, "app-error", value),
                        ServiceEvent::QueryProgress(value) => {
                            emit(&state.app, "query-progress", value)
                        }
                        ServiceEvent::ToolApproval(value) => {
                            emit(&state.app, "tool-approval", value)
                        }
                        ServiceEvent::ServiceDisconnected(value) => {
                            emit(&state.app, "app-error", value);
                            break;
                        }
                    },
                    Err(error) => {
                        emit(
                            &state.app,
                            "app-error",
                            format!("SAG 事件连接中断：{error}"),
                        );
                        break;
                    }
                }
            }
        }
    });
}
fn show_main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let window = app.get_webview_window("main").unwrap();
    window.show()?;
    window.unminimize()?;
    window.set_focus()
}
fn tray_menu(app: &tauri::AppHandle, language: &str) -> tauri::Result<Menu<tauri::Wry>> {
    let english = language == "en";
    let open = MenuItem::with_id(
        app,
        "sag-open",
        if english { "Open SAG Wiki" } else { "打开 SAG Wiki" },
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        app,
        "sag-quit",
        if english { "Quit SAG Wiki" } else { "退出 SAG Wiki" },
        true,
        None::<&str>,
    )?;
    Menu::with_items(app, &[&open, &quit])
}
#[tauri::command]
fn reveal_main_window(app: tauri::AppHandle) -> CommandResult<()> {
    show_main_window(&app).map_err(error)
}
#[tauri::command]
fn quit_application(app: tauri::AppHandle) {
    app.exit(0);
}

remote!(list_roots() => ListRoots);
remote!(list_entries(path:String) => ListEntries);
remote!(list_files() => ListFiles);
remote!(read_document(path:String) => ReadDocument);
remote!(explore_overview(scope:String,query:String,visible:Vec<i64>) => ExploreOverview);
remote!(explore_graph(scope:String,request_id:String) => ExploreGraph);
remote!(explore_neighbors(scope:String,center_id:i64,snapshot:Option<String>,offset:usize,request_id:String) => ExploreNeighbors);
remote!(explore_evidence(scope:String,snapshot:String,source:i64,target:i64) => ExploreEvidence);
remote!(add_root(path:String) => AddRoot);
remote!(add_root_with_options(path:String,options:sag_protocol::types::FolderIndexOptions) => AddRootWithOptions);
remote!(get_folder_index_options(path:String) => GetFolderIndexOptions);
remote!(save_folder_index_options(path:String,options:sag_protocol::types::FolderIndexOptions,rebuild:bool) => SaveFolderIndexOptions);
remote!(remove_root(path:String) => RemoveRoot);
remote!(list_removed_directories() => RemovedDirectories);
remote!(refresh_root(path:String) => RefreshRoot);
remote!(reindex_all() => ReindexAll);
remote!(get_index_status() => GetIndexStatus);
remote!(get_index_history() => GetIndexHistory);
remote!(get_index_history_log(id:i64) => GetIndexHistoryLog);
remote!(set_conversion_workers(workers:usize) => SetConversionWorkers);
remote!(clear_index_history() => ClearIndexHistory);
remote!(set_index_paused(paused:bool) => SetIndexPaused);
remote!(remove_all_roots() => RemoveAllRoots);
remote!(save_document(path:String,body:String,expected_hash:Option<String>) => SaveDocument);
remote!(put_draft(draft:Draft) => PutDraft);
remote!(list_drafts() => ListDrafts);
remote!(delete_draft(path:String) => DeleteDraft);
remote!(get_settings() => GetSettings);
remote!(get_docling_status() => DoclingStatus);
remote!(install_docling() => InstallDocling);
remote!(install_docling_profiles(profiles:Vec<DoclingProfile>) => InstallDoclingProfiles);
remote!(cancel_docling_install() => CancelDoclingInstall);
remote!(uninstall_docling() => UninstallDocling);
remote!(clear_model_key(kind:String) => ClearModelKey);
remote!(test_model(kind:String,config:ModelConfig,key:String) => TestModel);
remote!(test_mcp(server:McpServer,credentials:Option<McpCredentials>) => TestMcp);
remote!(resolve_tool_approval(id:String,allow:bool) => ResolveToolApproval);
remote!(list_conversations() => ListConversations);
remote!(clear_conversations() => ClearConversations);
remote!(clear_all_data() => ClearAllData);
remote!(edit_conversation(id:String,title:Option<String>,deleted:Option<bool>) => EditConversation);
remote!(replace_messages(id:String,messages:Vec<Message>) => ReplaceMessages);
remote!(cancel_query(id:String) => CancelQuery);
remote!(run_query(query:Query) => RunQuery);
remote!(summarize_search(id:String,question:String,results:Vec<Candidate>) => SummarizeSearch);
remote!(load_image(path:String) => LoadImage);
remote!(get_service_info() => Info);

#[tauri::command]
async fn save_settings(
    settings: Settings,
    answer_key: String,
    jev_key: String,
    mcp_credentials: Option<std::collections::BTreeMap<String, McpCredentials>>,
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> CommandResult<serde_json::Value> {
    let language = settings.language.clone();
    let result = rpc(
        &state,
        Operation::SaveSettings {
            settings,
            answer_key,
            jev_key,
            mcp_credentials,
        },
    )
    .await?;
    app.tray_by_id("main")
        .unwrap()
        .set_menu(Some(tray_menu(&app, &language).map_err(error)?))
        .map_err(error)?;
    Ok(result)
}
#[tauri::command]
async fn reveal_path(path: String, state: State<'_, Arc<AppState>>) -> CommandResult<()> {
    let value = rpc(&state, Operation::AuthorizePath { path }).await?;
    reveal(Path::new(value.as_str().unwrap())).map_err(error)
}
#[tauri::command]
async fn reveal_docling_log(state: State<'_, Arc<AppState>>) -> CommandResult<()> {
    let log = state.data_dir.join("docling").join("install-diagnostics.log");
    if !log.is_file() {
        return Err("尚未生成安装日志".into());
    }
    reveal(&log).map_err(error)
}
#[tauri::command]
async fn open_index_history_file(id: i64, path: String, state: State<'_, Arc<AppState>>) -> CommandResult<()> {
    let value = rpc(&state, Operation::AuthorizeIndexHistoryFile { id, path }).await?;
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    if !command.arg(value.as_str().unwrap()).status().map_err(error)?.success() {
        return Err("无法用系统默认应用打开原文件".into());
    }
    Ok(())
}
#[tauri::command]
fn open_data_directory(state: State<Arc<AppState>>) -> CommandResult<()> {
    reveal(&state.data_dir).map_err(error)
}
#[tauri::command]
fn open_about_link(target: &str) -> CommandResult<()> {
    let url = match target {
        "project" => "https://github.com/Zleap-AI/SAG-Wiki",
        "company" => "https://zleap.com",
        "docling" => "https://docling.ai/",
        "302-api-keys" => "https://302.ai/api-keys/list",
        _ => return Err("未知的关于页面链接".into()),
    };
    open_browser_url(url)
}
fn open_browser_url(url: &str) -> CommandResult<()> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    if !command.arg(url).status().map_err(error)?.success() {
        return Err("无法在浏览器中打开链接".into());
    }
    Ok(())
}
fn reveal(path: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let mut c = std::process::Command::new("open");
        if path.is_file() {
            c.arg("-R");
        }
        if !c.arg(path).status()?.success() {
            anyhow::bail!("Finder 未能打开此位置")
        }
    }
    #[cfg(target_os = "windows")]
    {
        let mut c = std::process::Command::new("explorer.exe");
        if path.is_file() {
            c.arg(format!("/select,{}", path.display()));
        } else {
            c.arg(path);
        }
        c.spawn()?;
    }
    Ok(())
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::UpdateSession::default())
        .setup(|app| {
            #[cfg(windows)]
            windows_icon::set_taskbar_icon(app.get_webview_window("main").unwrap().hwnd()?)?;
            let data = protocol::data_dir()?;
            let cli_install_error = cli_commands::initialize_cli(app.handle()).err();
            let source = cli_commands::service_source(app.handle(), cli_install_error.is_none())
                .map_err(anyhow::Error::msg)?;
            // The service is connected asynchronously by `connect_on_startup`
            // and on demand by `rpc`; the tray starts with the default language.
            let language = "zh-CN";
            let icon = app.default_window_icon().unwrap().clone();
            #[cfg(target_os = "macos")]
            let icon = {
                // Crop app-icon padding so the menu-bar silhouette remains readable.
                let width = icon.width() as usize;
                let height = icon.height() as usize;
                let left = width / 8;
                let top = height / 8;
                let tray_width = width - 2 * left;
                let tray_height = height - 2 * top;
                let mut rgba: Vec<u8> = icon
                    .rgba()
                    .chunks_exact(width * 4)
                    .skip(top)
                    .take(tray_height)
                    .flat_map(|row| row[left * 4..(left + tray_width) * 4].iter().copied())
                    .collect();
                for pixel in rgba.chunks_exact_mut(4) {
                    pixel[3] = (u16::from(pixel[3]) * (255 - u16::from(pixel[0])) / 255) as u8;
                    pixel[..3].fill(0);
                }
                tauri::image::Image::new_owned(rgba, tray_width as u32, tray_height as u32)
            };
            TrayIconBuilder::with_id("main")
                .icon(icon)
                .icon_as_template(cfg!(target_os = "macos"))
                .tooltip("SAG Wiki")
                .menu(&tray_menu(app.handle(), language)?)
                .show_menu_on_left_click(!cfg!(target_os = "windows"))
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "sag-open" => {
                        if let Err(e) = show_main_window(app) {
                            emit(app, "app-error", e.to_string());
                        }
                    }
                    "sag-quit" => emit(app, "quit-requested", ()),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    #[cfg(target_os = "windows")]
                    if matches!(
                        event,
                        tauri::tray::TrayIconEvent::Click {
                            button: tauri::tray::MouseButton::Left,
                            button_state: tauri::tray::MouseButtonState::Up,
                            ..
                        }
                    ) {
                        if let Err(e) = show_main_window(tray.app_handle()) {
                            emit(tray.app_handle(), "app-error", e.to_string());
                        }
                    }
                    #[cfg(not(target_os = "windows"))]
                    let _ = (tray, event);
                })
                .build(app)?;

            let state = Arc::new(AppState {
                client: Mutex::new(Client::disconnected()),
                reconnect: tokio::sync::Mutex::new(()),
                app: app.handle().clone(),
                data_dir: data,
                service_binary: source,
                cli_install_error: Mutex::new(cli_install_error),
            });
            app.manage(state.clone());
            connect_on_startup(state.clone());
            forward_events(state);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Err(e) = window.hide() {
                    emit(window.app_handle(), "app-error", e.to_string());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            cli_commands::get_cli_settings,
            cli_commands::save_cli_settings,
            cli_commands::install_cli_integration,
            cli_commands::download_cli_skill,
            cli_commands::test_cli_connection,
            reveal_main_window,
            quit_application,
            list_roots,
            list_entries,
            folder_picker::browse_folders,
            list_files,
            read_document,
            explore_overview,
            explore_graph,
            explore_neighbors,
            explore_evidence,
            add_root,
            add_root_with_options,
            get_folder_index_options,
            save_folder_index_options,
            remove_root,
            list_removed_directories,
            refresh_root,
            reindex_all,
            get_index_status,
            get_index_history,
            get_index_history_log,
            set_conversion_workers,
            open_index_history_file,
            clear_index_history,
            set_index_paused,
            remove_all_roots,
            save_document,
            put_draft,
            list_drafts,
            delete_draft,
            get_settings,
            get_docling_status,
            install_docling,
            install_docling_profiles,
            cancel_docling_install,
            uninstall_docling,
            save_settings,
            test_model,
            test_mcp,
            resolve_tool_approval,
            clear_model_key,
            list_conversations,
            clear_conversations,
            clear_all_data,
            edit_conversation,
            replace_messages,
            cancel_query,
            run_query,
            summarize_search,
            reveal_path,
            reveal_docling_log,
            open_data_directory,
            open_about_link,
            updates::check_app_update,
            updates::download_app_update,
            updates::install_app_update,
            load_image,
            get_service_info
        ])
        .build(tauri::generate_context!())
        .expect("无法启动 SAG Wiki");
    app.run(|app, event| match event {
        tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } => {
            api.prevent_exit();
            emit(app, "quit-requested", ());
        }
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => {
            if let Err(e) = show_main_window(app) {
                emit(app, "app-error", e.to_string());
            }
        }
        _ => {}
    });
}
