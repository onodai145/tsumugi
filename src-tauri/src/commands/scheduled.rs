//! 予約投稿(サーバー側、Issue #60)の Tauri command。

use crate::api::drafts::{create_scheduled, delete_draft, list_scheduled};
use crate::api::notes::NoteDraft;
use crate::commands::column::cached_server_version;
use crate::domain::{schedule_capabilities, LocalScheduledNote, ScheduleCapabilities, ScheduledNote};
use crate::error::Result;
use crate::scheduler;
use crate::state::AppState;
use crate::store::draft::DraftInput;
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

/// クライアント側(ローカル)予約を作る(Issue #60 B)。`scheduled_at` は epoch 秒。
/// 添付は呼び出し側でアップロード済みの `fileIds`(`DraftInput.file_ids`)。
#[tauri::command]
#[specta::specta]
pub async fn schedule_note_local(
    state: State<'_, AppState>,
    account_id: String,
    input: DraftInput,
    scheduled_at: u32,
) -> Result<LocalScheduledNote> {
    scheduler::schedule_local(&state, &account_id, input, i64::from(scheduled_at), scheduler::now_sec())
}

/// そのアカウントのローカル予約(予約日時の昇順)。
#[tauri::command]
#[specta::specta]
pub async fn list_local_scheduled_notes(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Vec<LocalScheduledNote>> {
    Ok(scheduler::list_local(&state, &account_id))
}

/// ローカル予約の取り消し(削除)。送信中のものは取り消せない。
#[tauri::command]
#[specta::specta]
pub async fn cancel_local_scheduled_note(
    state: State<'_, AppState>,
    account_id: String,
    id: String,
) -> Result<()> {
    scheduler::cancel_local(&state, &account_id, &id)
}

/// 期限切れ・失敗したローカル予約を、今すぐ 1 回だけ送る。結果はイベントでも通知する。
#[tauri::command]
#[specta::specta]
pub async fn post_local_scheduled_now(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    id: String,
) -> Result<()> {
    let outcome = scheduler::post_now(&state, &account_id, &id, scheduler::now_sec()).await?;
    scheduler::emit_outcome(&app, &outcome);
    Ok(())
}
