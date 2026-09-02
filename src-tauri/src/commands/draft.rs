//! 下書き(未送信の投稿の保存/復元)系 command。

use crate::state::AppState;
use crate::store::draft::{Draft, DraftInput};
use crate::error::Result;
use tauri::State;

/// アカウントの手動保存下書き一覧(更新日時降順)。
#[tauri::command]
#[specta::specta]
pub async fn list_drafts(state: State<'_, AppState>, account_id: String) -> Result<Vec<Draft>> {
    state.drafts.list_manual(&account_id)
}

/// 手動下書きを新規保存する(既存の上書きはしない)。
#[tauri::command]
#[specta::specta]
pub async fn save_draft(state: State<'_, AppState>, account_id: String, input: DraftInput) -> Result<()> {
    state.drafts.save_manual(&account_id, &input)
}

/// 手動下書きを削除する。
#[tauri::command]
#[specta::specta]
pub async fn delete_draft(state: State<'_, AppState>, account_id: String, draft_id: String) -> Result<()> {
    state.drafts.delete_manual(&account_id, &draft_id)
}

/// アカウントの自動一時下書き(あれば1件)。
#[tauri::command]
#[specta::specta]
pub async fn get_auto_draft(state: State<'_, AppState>, account_id: String) -> Result<Option<Draft>> {
    state.drafts.get_auto(&account_id)
}

/// 自動一時下書きをupsertする(アカウントにつき1件)。
#[tauri::command]
#[specta::specta]
pub async fn save_auto_draft(state: State<'_, AppState>, account_id: String, input: DraftInput) -> Result<()> {
    state.drafts.save_auto(&account_id, &input)
}

/// 自動一時下書きを消す。
#[tauri::command]
#[specta::specta]
pub async fn clear_auto_draft(state: State<'_, AppState>, account_id: String) -> Result<()> {
    state.drafts.clear_auto(&account_id)
}
