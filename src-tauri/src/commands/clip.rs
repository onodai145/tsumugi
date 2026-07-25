//! クリップ系 command（一覧取得・作成・ノート追加）。

use crate::api::clips::{add_note_to_clip as api_add_note_to_clip, create_clip as api_create_clip, list_clips as api_list_clips};
use crate::domain::Clip;
use crate::error::Result;
use crate::state::AppState;
use tauri::State;

/// 自分のクリップ一覧を取得。
#[tauri::command]
#[specta::specta]
pub async fn list_clips(state: State<'_, AppState>, account_id: String) -> Result<Vec<Clip>> {
    let client = state.client_for(&account_id)?;
    api_list_clips(&client).await
}

/// クリップを新規作成する。
#[tauri::command]
#[specta::specta]
pub async fn create_clip(state: State<'_, AppState>, account_id: String, name: String) -> Result<Clip> {
    let client = state.client_for(&account_id)?;
    api_create_clip(&client, &name).await
}

/// ノートをクリップへ追加する。
#[tauri::command]
#[specta::specta]
pub async fn add_note_to_clip(
    state: State<'_, AppState>,
    account_id: String,
    clip_id: String,
    note_id: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    api_add_note_to_clip(&client, &clip_id, &note_id).await
}
