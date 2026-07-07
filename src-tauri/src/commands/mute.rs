//! NG（ミュート）設定の取得・更新。

use crate::domain::MuteConfig;
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
