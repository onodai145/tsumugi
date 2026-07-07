//! NG（ミュート）・通知設定の取得・更新。

use crate::api::mutes::fetch_muted_and_blocked;
use crate::domain::{MuteConfig, NotifyConfig, UiPrefs};
use crate::error::Result;
use crate::state::AppState;
use tauri::State;

/// 現在の NG 設定を取得。
#[tauri::command]
#[specta::specta]
pub async fn get_mute(state: State<'_, AppState>) -> Result<MuteConfig> {
    Ok(state.mute.lock().unwrap().clone())
}

/// NG 設定を更新（永続化＋以降の受信に即反映）。
#[tauri::command]
#[specta::specta]
pub async fn set_mute(state: State<'_, AppState>, config: MuteConfig) -> Result<()> {
    state.settings.save_mute(&config)?;
    *state.mute.lock().unwrap() = config;
    Ok(())
}

/// デスクトップ通知・音の設定を取得。
#[tauri::command]
#[specta::specta]
pub async fn get_notify(state: State<'_, AppState>) -> Result<NotifyConfig> {
    state.settings.load_notify()
}

/// デスクトップ通知・音の設定を更新（永続化）。
#[tauri::command]
#[specta::specta]
pub async fn set_notify(state: State<'_, AppState>, config: NotifyConfig) -> Result<()> {
    state.settings.save_notify(&config)
}

/// 表示設定（テーマ・既定カラム幅）を取得。
#[tauri::command]
#[specta::specta]
pub async fn get_ui_prefs(state: State<'_, AppState>) -> Result<UiPrefs> {
    state.settings.load_ui()
}

/// 表示設定を更新（永続化）。
#[tauri::command]
#[specta::specta]
pub async fn set_ui_prefs(state: State<'_, AppState>, prefs: UiPrefs) -> Result<()> {
    state.settings.save_ui(&prefs)
}

/// サーバ側のミュート/ブロックを取得して AppState に反映する。返り値は対象ユーザ数。
/// 起動時とアカウント追加時にフロントから呼ぶ（Krile MuteBlockManager 相当）。
#[tauri::command]
#[specta::specta]
pub async fn sync_server_mutes(state: State<'_, AppState>, account_id: String) -> Result<u32> {
    let client = state.client_for(&account_id)?;
    let ids = fetch_muted_and_blocked(&client).await?;
    let n = ids.len() as u32;
    state.set_server_mutes(&account_id, ids);
    Ok(n)
}
