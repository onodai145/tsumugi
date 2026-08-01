//! 投稿・リアクション系 command（Phase 3）。

use crate::api::drive::{
    list_files as api_list_files, list_folders as api_list_folders, upload_bytes as api_upload_bytes,
};
use crate::api::hashtags::search_hashtags as api_search_hashtags;
use crate::api::meta::list_emojis;
use crate::api::notes::{
    create_favorite, create_note, create_reaction, delete_favorite, delete_note, delete_reaction,
    get_reactions, get_renotes, renote as api_renote, vote_poll as api_vote_poll, NoteDraft,
    VisibilityInput,
};
use crate::api::users::search_users as api_search_users;
use crate::domain::{DriveFile, EmojiDef, Note, ReactionUser, SourceItem, User};
use crate::error::{Error, Result};
use crate::state::AppState;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

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
    create_reaction(&client, &note_id, &reaction).await?;
    // キャッシュへ反映しないと、再起動後に load_cached が取得直後の古い payload を返し、
    // ここまでのセッション中に付けたリアクションが消えて見える(Issue #89)。
    if let Ok(Some(mut note)) = state.cache.get_note(&note_id) {
        note.apply_my_reaction(&reaction);
        let _ = state.cache.update_note(&note);
    }
    Ok(())
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
    delete_reaction(&client, &note_id).await?;
    if let Ok(Some(mut note)) = state.cache.get_note(&note_id) {
        note.clear_my_reaction();
        let _ = state.cache.update_note(&note);
    }
    Ok(())
}

/// リアクション付与ユーザー一覧取得（絵文字ごと、最大100件）。
#[tauri::command]
#[specta::specta]
pub async fn get_note_reactions(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
    reaction_type: Option<String>,
) -> Result<Vec<ReactionUser>> {
    let client = state.client_for(&account_id)?;
    get_reactions(&client, &note_id, reaction_type.as_deref()).await
}

/// Renoteしたユーザー一覧取得（最大100件）。
#[tauri::command]
#[specta::specta]
pub async fn get_note_renotes(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
) -> Result<Vec<User>> {
    let client = state.client_for(&account_id)?;
    get_renotes(&client, &note_id).await
}

/// お気に入り登録。
#[tauri::command]
#[specta::specta]
pub async fn favorite_note(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    create_favorite(&client, &note_id).await
}

/// お気に入り解除。
#[tauri::command]
#[specta::specta]
pub async fn unfavorite_note(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    delete_favorite(&client, &note_id).await
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
///
/// Android の SAF ファイルピッカーは `content://` URI を返し `tokio::fs::read` では
/// 開けないため（Issue #137）、`read_file_bytes` の ContentResolver ブリッジ経由で読み、
/// バイト列としてアップロードする。
#[tauri::command]
#[specta::specta]
pub async fn upload_file(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    path: String,
) -> Result<DriveFile> {
    let (host, token) = state.host_token(&account_id)?;
    let bytes = crate::commands::mute::read_file_bytes(&app, &path).await?;
    let filename = std::path::Path::new(&path)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    let filename = ensure_extension(filename, &bytes);
    api_upload_bytes(&state.http, &host, &token, bytes, filename).await
}

/// ファイル名に拡張子が無ければ、内容の先頭バイト列から `infer` クレートで種別を推定して補う。
///
/// `content://` URI 由来のファイル名（Issue #137）は `100006972` のように拡張子を持たない
/// ため、そのままだとドライブ上で種別が分かりにくい表示になる（アップロード自体は失敗しない
/// ― Misskey サーバ側も実バイト列を sniff して種別判定するため機能上は問題ない）。
fn ensure_extension(filename: String, bytes: &[u8]) -> String {
    if std::path::Path::new(&filename).extension().is_some() {
        return filename;
    }
    match infer::get(bytes) {
        Some(kind) => format!("{filename}.{}", kind.extension()),
        None => filename,
    }
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

/// クリップボードから貼り付けたバイト列をドライブへアップロードし、DriveFile を返す。
#[tauri::command]
#[specta::specta]
pub async fn upload_bytes(
    state: State<'_, AppState>,
    account_id: String,
    filename: String,
    bytes: Vec<u8>,
) -> Result<DriveFile> {
    let (host, token) = state.host_token(&account_id)?;
    api_upload_bytes(&state.http, &host, &token, bytes, filename).await
}

/// `read_clipboard_image` の戻り値。アップロードはせず、フロントは投稿時まで保持してから
/// `upload_bytes` へ渡す(Issue #66 の「投稿時アップロード」原則を踏襲するため)。
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardImage {
    pub filename: String,
    pub bytes: Vec<u8>,
}

/// クリップボードの画像を読み、PNGへエンコードして返す(アップロードはしない)。
/// クリップボードに画像が無い場合や PNG エンコードに失敗した場合は `Error::Invalid` を返す
/// (このコマンド内では Invalid を「実質画像が無い/内部処理異常」を表す専用シグナルとして扱う)。
#[tauri::command]
#[specta::specta]
pub async fn read_clipboard_image(app: AppHandle) -> Result<ClipboardImage> {
    let (rgba, width, height) = tauri::async_runtime::spawn_blocking(move || {
        let image = app
            .clipboard()
            .read_image()
            .map_err(|e| Error::Invalid(format!("クリップボードに画像がありません: {e}")))?;
        let width = image.width();
        let height = image.height();
        let rgba = image.rgba().to_vec();
        Ok::<(Vec<u8>, u32, u32), Error>((rgba, width, height))
    })
    .await
    .map_err(|e| Error::Invalid(format!("クリップボード読み取りに失敗しました: {e}")))??;

    let png_bytes = encode_png_rgba(&rgba, width, height)?;

    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let filename = clipboard_filename(millis);

    Ok(ClipboardImage { filename, bytes: png_bytes })
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

/// プレビュー用途に許容する最大サイズ(base64化してフロントに保持するため、実アップロード上限より小さく抑える)。
const MAX_ATTACHMENT_PREVIEW_BYTES: usize = 20 * 1024 * 1024;

/// 投稿添付の未アップロードローカル画像を data URL(base64) に変換する(投稿前プレビュー用)。
/// 動画や未知拡張子は `application/octet-stream` を返す(呼び出し側でバッジ表示にフォールバックする想定)。
#[tauri::command]
#[specta::specta]
pub async fn read_attachment_preview(app: AppHandle, path: String) -> Result<String> {
    crate::commands::mute::read_file_as_data_url(
        &app,
        &path,
        MAX_ATTACHMENT_PREVIEW_BYTES,
        guess_attachment_image_mime,
    )
    .await
}

/// 拡張子から画像 MIME を推定する。動画・未知拡張子は octet-stream(呼び出し側でバッジ表示にフォールバック)。
fn guess_attachment_image_mime(path: &str) -> &'static str {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

/// RGBA8 のピクセル配列を PNG バイト列にエンコードする(クリップボード貼り付け画像用)。
fn encode_png_rgba(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut encoder = png::Encoder::new(&mut buf, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| Error::Invalid(format!("PNGヘッダ書き込みに失敗しました: {e}")))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| Error::Invalid(format!("PNGエンコードに失敗しました: {e}")))?;
    writer
        .finish()
        .map_err(|e| Error::Invalid(format!("PNG書き込みの完了に失敗しました: {e}")))?;
    Ok(buf)
}

/// ミリ秒Unix時刻から `clipboard-YYYYMMDD-HHMMSS-mmm.png` 形式のファイル名を生成する(UTC基準)。
fn clipboard_filename(millis: i64) -> String {
    let dt = chrono::DateTime::from_timestamp_millis(millis)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp_millis(0).expect("timestamp 0 is valid"));
    format!("clipboard-{}.png", dt.format("%Y%m%d-%H%M%S-%3f"))
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

/// メンション補完用のユーザー検索。
#[tauri::command]
#[specta::specta]
pub async fn search_users(
    state: State<'_, AppState>,
    account_id: String,
    query: String,
) -> Result<Vec<User>> {
    let client = state.client_for(&account_id)?;
    api_search_users(&client, &query, 10).await
}

/// ハッシュタグ補完用のハッシュタグ検索。
#[tauri::command]
#[specta::specta]
pub async fn search_hashtags(
    state: State<'_, AppState>,
    account_id: String,
    query: String,
) -> Result<Vec<String>> {
    let client = state.client_for(&account_id)?;
    api_search_hashtags(&client, &query, 10).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guess_attachment_image_mime_maps_known_extensions() {
        assert_eq!(guess_attachment_image_mime("photo.png"), "image/png");
        assert_eq!(guess_attachment_image_mime("photo.JPG"), "image/jpeg");
        assert_eq!(guess_attachment_image_mime("photo.jpeg"), "image/jpeg");
        assert_eq!(guess_attachment_image_mime("photo.gif"), "image/gif");
        assert_eq!(guess_attachment_image_mime("photo.webp"), "image/webp");
    }

    #[test]
    fn guess_attachment_image_mime_falls_back_for_unknown_or_video_extensions() {
        assert_eq!(guess_attachment_image_mime("clip.mp4"), "application/octet-stream");
        assert_eq!(guess_attachment_image_mime("clip.webm"), "application/octet-stream");
        assert_eq!(guess_attachment_image_mime("noext"), "application/octet-stream");
    }

    #[test]
    fn encode_png_rgba_produces_valid_png_signature() {
        // 2x1 の赤・青ピクセル(RGBA)
        let rgba = [255u8, 0, 0, 255, 0, 0, 255, 255];
        let png_bytes = encode_png_rgba(&rgba, 2, 1).expect("encode should succeed");
        // PNG シグネチャ: 89 50 4E 47 0D 0A 1A 0A
        assert_eq!(&png_bytes[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn clipboard_filename_formats_utc_datetime_with_millis() {
        // 2026-07-25T15:30:45.123Z の Unix ミリ秒
        let millis = 1784993445123;
        assert_eq!(clipboard_filename(millis), "clipboard-20260725-153045-123.png");
    }

    #[test]
    fn ensure_extension_appends_sniffed_extension_when_filename_has_none() {
        // content:// URI 由来のファイル名は "100006972" のように拡張子を持たない
        let png_bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        assert_eq!(ensure_extension("100006972".to_string(), &png_bytes), "100006972.png");
    }

    #[test]
    fn ensure_extension_leaves_filename_with_existing_extension_untouched() {
        let png_bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        assert_eq!(ensure_extension("photo.jpg".to_string(), &png_bytes), "photo.jpg");
    }

    #[test]
    fn ensure_extension_leaves_filename_unchanged_when_type_undetectable() {
        assert_eq!(ensure_extension("100006972".to_string(), &[0, 1, 2, 3]), "100006972");
    }
}
