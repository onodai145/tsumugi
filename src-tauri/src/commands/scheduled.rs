//! 予約投稿(サーバー側、Issue #60)の Tauri command。

use crate::api::drafts::{create_scheduled, delete_draft, list_scheduled};
use crate::api::notes::NoteDraft;
use crate::commands::column::cached_server_version;
use crate::domain::{schedule_capabilities, ScheduleCapabilities, ScheduledNote};
use crate::error::Result;
use crate::state::AppState;
use tauri::State;

/// アカウントの接続先サーバーが予約投稿に対応するか。取得失敗は非対応扱い。
#[tauri::command]
#[specta::specta]
pub async fn get_schedule_capabilities(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<ScheduleCapabilities> {
    let version = cached_server_version(&state, &account_id).await?;
    Ok(schedule_capabilities(version.as_deref()))
}

/// 予約投稿を作る。`scheduled_at` は epoch 秒。添付は呼び出し側でアップロード済みの `fileIds`。
#[tauri::command]
#[specta::specta]
pub async fn schedule_note(
    state: State<'_, AppState>,
    account_id: String,
    draft: NoteDraft,
    scheduled_at: u32,
) -> Result<ScheduledNote> {
    let client = state.client_for(&account_id)?;
    create_scheduled(&client, &draft, i64::from(scheduled_at)).await
}

/// 予約中の投稿一覧。`until_id` は前のページの最後の ID。`limit` は 1〜100 に丸める。
#[tauri::command]
#[specta::specta]
pub async fn list_scheduled_notes(
    state: State<'_, AppState>,
    account_id: String,
    until_id: Option<String>,
    limit: u32,
) -> Result<Vec<ScheduledNote>> {
    let client = state.client_for(&account_id)?;
    list_scheduled(&client, until_id.as_deref(), limit.clamp(1, 100)).await
}

/// 予約の取り消し(サーバー側の下書きを削除する)。
#[tauri::command]
#[specta::specta]
pub async fn cancel_scheduled_note(
    state: State<'_, AppState>,
    account_id: String,
    draft_id: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    delete_draft(&client, &draft_id).await
}
