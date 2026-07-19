//! 投稿・リアクション系 command（Phase 3）。

use crate::api::drive::{list_files as api_list_files, list_folders as api_list_folders, upload_file as api_upload_file};
use crate::api::meta::list_emojis;
use crate::api::notes::{
    create_note, create_reaction, delete_note, delete_reaction, renote as api_renote, vote_poll as api_vote_poll,
    NoteDraft, VisibilityInput,
};
use crate::domain::{DriveFile, EmojiDef, Note, SourceItem};
use crate::error::{Error, Result};
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

/// アンケートに投票する（choice は 0-based index）。
#[tauri::command]
#[specta::specta]
pub async fn vote_poll(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
    choice: u32,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    api_vote_poll(&client, &note_id, choice).await
}

/// ローカルファイルをドライブへアップロードし、DriveFile を返す（投稿添付用）。
#[tauri::command]
#[specta::specta]
pub async fn upload_file(
    state: State<'_, AppState>,
    account_id: String,
    path: String,
) -> Result<DriveFile> {
    let (host, token) = state.host_token(&account_id)?;
    api_upload_file(&state.http, &host, &token, &path).await
}

/// ドライブのファイル一覧（添付ピッカー用）。folder_id: None はルート直下、
/// until_id は直前に取得した最後のファイルIDを渡してページングする。
#[tauri::command]
#[specta::specta]
pub async fn list_drive_files(
    state: State<'_, AppState>,
    account_id: String,
    folder_id: Option<String>,
    until_id: Option<String>,
) -> Result<Vec<DriveFile>> {
    let client = state.client_for(&account_id)?;
    api_list_files(&client, folder_id.as_deref(), until_id.as_deref()).await
}

/// ドライブのフォルダ一覧（添付ピッカーのフォルダナビゲーション用）。
/// folder_id: None はルート直下のフォルダ一覧。
#[tauri::command]
#[specta::specta]
pub async fn list_drive_folders(
    state: State<'_, AppState>,
    account_id: String,
    folder_id: Option<String>,
) -> Result<Vec<SourceItem>> {
    let client = state.client_for(&account_id)?;
    api_list_folders(&client, folder_id.as_deref()).await
}

/// 添付ファイル(画像/動画等)を上限サイズまで超えていないか調べつつダウンロードし、
/// 指定パスへ保存する（メディアビューワーの「保存」ボタン用）。
/// ドライブの添付URLは公開直リンクのため、認証トークンは不要。
const MAX_SAVE_FILE_BYTES: u64 = 200 * 1024 * 1024;

#[tauri::command]
#[specta::specta]
pub async fn save_url_to_file(state: State<'_, AppState>, url: String, path: String) -> Result<()> {
    let resp = state.http.get(&url).send().await?;
    if !resp.status().is_success() {
        return Err(Error::Api(format!("failed to fetch file: {}", resp.status())));
    }
    if resp.content_length().is_some_and(|len| len > MAX_SAVE_FILE_BYTES) {
        return Err(Error::Invalid(format!(
            "ファイルが大きすぎます（{}MB超）",
            MAX_SAVE_FILE_BYTES / 1024 / 1024
        )));
    }
    let bytes = resp.bytes().await?;
    if bytes.len() as u64 > MAX_SAVE_FILE_BYTES {
        return Err(Error::Invalid(format!(
            "ファイルが大きすぎます（{}MB超）",
            MAX_SAVE_FILE_BYTES / 1024 / 1024
        )));
    }
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|e| Error::Invalid(format!("cannot write file {path}: {e}")))?;
    Ok(())
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
