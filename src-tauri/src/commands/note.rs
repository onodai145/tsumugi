//! 投稿・リアクション系 command（Phase 3）。

use crate::api::meta::list_emojis;
use crate::api::notes::{
    create_note, create_reaction, delete_note, delete_reaction, renote as api_renote, NoteDraft,
    VisibilityInput,
};
use crate::domain::{EmojiDef, Note};
use crate::error::Result;
use crate::state::AppState;
use tauri::State;

/// 投稿する（本文・CW・可視性・添付・投票・返信/引用/Renote）。作成された Note を返す。
#[tauri::command]
#[specta::specta]
pub async fn post_note(
    state: State<'_, AppState>,
    account_id: String,
    draft: NoteDraft,
) -> Result<Note> {
    let client = state.client_for(&account_id)?;
    create_note(&client, &draft).await
}

/// 純粋 Renote。
#[tauri::command]
#[specta::specta]
pub async fn renote(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
    visibility: VisibilityInput,
) -> Result<Note> {
    let client = state.client_for(&account_id)?;
    api_renote(&client, &note_id, visibility).await
}

/// ノート削除。
#[tauri::command]
#[specta::specta]
pub async fn delete_note_cmd(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    delete_note(&client, &note_id).await
}

/// リアクション付与（カスタム絵文字は `:name:` / `:name@host:`、Unicode は生文字）。
#[tauri::command]
#[specta::specta]
pub async fn react(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
    reaction: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    create_reaction(&client, &note_id, &reaction).await
}

/// リアクション解除。
#[tauri::command]
#[specta::specta]
pub async fn unreact(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    delete_reaction(&client, &note_id).await
}

/// カスタム絵文字一覧（リアクションピッカー用）。host 単位でキャッシュする。
#[tauri::command]
#[specta::specta]
pub async fn list_custom_emojis(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Vec<EmojiDef>> {
    let (host, _) = state.host_token(&account_id)?;
    if let Some(cached) = state.emoji_cache.lock().unwrap().get(&host).cloned() {
        return Ok(cached);
    }
    let client = state.client_for(&account_id)?;
    let emojis = list_emojis(&client).await?;
    state
        .emoji_cache
        .lock()
        .unwrap()
        .insert(host, emojis.clone());
    Ok(emojis)
}
