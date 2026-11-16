use crate::{error, CommandResult};
use sag_client::share::{
    self, PreparedShare, ReceivedShare, SharePreview, ShareSettings, UploadedShare,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::State;

#[derive(Default)]
pub struct Shares(Mutex<HashMap<String, Arc<PreparedShare>>>);
#[tauri::command]
pub async fn prepare_share(path: String, state: State<'_, Shares>) -> CommandResult<SharePreview> {
    let prepared =
        tauri::async_runtime::spawn_blocking(move || share::prepare(&PathBuf::from(path)))
            .await
            .map_err(error)?
            .map_err(error)?;
    let preview = prepared.preview.clone();
    state
        .0
        .lock()
        .unwrap()
        .insert(preview.id.clone(), Arc::new(prepared));
    Ok(preview)
}
#[tauri::command]
pub async fn upload_share(id: String, state: State<'_, Shares>) -> CommandResult<UploadedShare> {
    let item = state
        .0
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or("分享准备已失效，请重新选择")?;
    share::upload(&item).await.map_err(error)
}
#[tauri::command]
pub fn close_share(id: String, state: State<'_, Shares>) {
    state.0.lock().unwrap().remove(&id);
}
#[tauri::command]
pub async fn receive_share(code: String, directory: String) -> CommandResult<ReceivedShare> {
    share::receive(&code, &PathBuf::from(directory))
        .await
        .map_err(error)
}
#[tauri::command]
pub fn get_share_settings() -> CommandResult<ShareSettings> {
    share::settings().map_err(error)
}
#[tauri::command]
pub fn save_share_settings(settings: ShareSettings) -> CommandResult<()> {
    share::save_settings(&settings).map_err(error)
}
#[tauri::command]
pub fn share_clipboard() -> CommandResult<Option<String>> {
    let settings = share::settings().map_err(error)?;
    if !settings.clipboard {
        return Ok(None);
    }
    let text = arboard::Clipboard::new().map_err(error)?.get_text();
    match text {
        Ok(text)
            if ["SAG-", "分享码：", "SHARE CODE:"]
                .iter()
                .any(|prefix| text.trim().to_ascii_uppercase().starts_with(prefix)) =>
        {
            Ok(share::normalize_code(&text).ok())
        }
        Ok(_) | Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(e) => Err(error(e)),
    }
}

#[tauri::command]
pub fn get_share_history() -> CommandResult<Vec<share::ShareRecord>> { share::history().map_err(error) }
#[tauri::command]
pub async fn cancel_share(code: String) -> CommandResult<()> { share::cancel_share(&code).await.map_err(error) }
