//! NG（ミュート）・通知設定の取得・更新。

use crate::api::mutes::fetch_muted_and_blocked;
use crate::domain::{MuteConfig, NotifyConfig, UiPrefs};
use crate::error::{Error, Result};
use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use tauri::{AppHandle, State};
#[cfg(target_os = "android")]
use tauri_plugin_fs::FsExt;

/// 背景画像として許容する最大サイズ（DB肥大化を防ぐ）。
const MAX_BACKGROUND_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// 通知音として許容する最大サイズ（短い効果音程度を想定）。
const MAX_NOTIFY_SOUND_BYTES: usize = 5 * 1024 * 1024;

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

/// ローカル画像ファイルを data URL(base64)へ変換する（背景画像設定用）。
/// UiPrefs.background_image に直接保存できる形にする。拡張子から MIME を推定する。
#[tauri::command]
#[specta::specta]
pub async fn read_image_data_url(app: AppHandle, path: String) -> Result<String> {
    read_file_as_data_url(&app, &path, MAX_BACKGROUND_IMAGE_BYTES, guess_image_mime).await
}

/// ローカル音声ファイルを data URL(base64)へ変換する（通知音設定用）。
#[tauri::command]
#[specta::specta]
pub async fn read_audio_data_url(app: AppHandle, path: String) -> Result<String> {
    read_file_as_data_url(&app, &path, MAX_NOTIFY_SOUND_BYTES, guess_audio_mime).await
}

/// ファイルを読み、上限サイズを検査して data URL(base64) にする共通処理。
///
/// Android は SAF のファイルピッカーが `content://` URI を返し、通常のファイルシステム
/// パスとして開けない（`std::fs`/`tokio::fs` では ENOENT になる）ため、
/// `tauri-plugin-fs` 経由でネイティブの ContentResolver ブリッジを使って読む。
pub(crate) async fn read_file_as_data_url(
    #[cfg_attr(not(target_os = "android"), allow(unused_variables))] app: &AppHandle,
    path: &str,
    max_bytes: usize,
    guess_mime: fn(&str) -> &'static str,
) -> Result<String> {
    #[cfg(target_os = "android")]
    let bytes = {
        let app = app.clone();
        // "content://..." は Url、それ以外は通常のファイルパスとして解釈される
        // (`FilePath::from_str` は `Infallible` を返すため unwrap で安全)。
        let file_path: tauri_plugin_fs::FilePath = path.parse().unwrap();
        tauri::async_runtime::spawn_blocking(move || app.fs().read(file_path))
            .await
            .map_err(|e| Error::Invalid(format!("cannot read file {path}: {e}")))?
            .map_err(|e| Error::Invalid(format!("cannot read file {path}: {e}")))?
    };
    #[cfg(not(target_os = "android"))]
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| Error::Invalid(format!("cannot read file {path}: {e}")))?;
    if bytes.len() > max_bytes {
        return Err(Error::Invalid(format!(
            "ファイルが大きすぎます（{}MB超）。{}MB以下のファイルを選んでください",
            max_bytes / 1024 / 1024,
            max_bytes / 1024 / 1024
        )));
    }
    let mime = guess_mime(path);
    let b64 = STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

/// 拡張子から画像 MIME を推定する。不明な拡張子は octet-stream(ブラウザ側で概ね表示可)。
fn guess_image_mime(path: &str) -> &'static str {
    match extension_lower(path).as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// 拡張子から音声 MIME を推定する。不明な拡張子は octet-stream。
fn guess_audio_mime(path: &str) -> &'static str {
    match extension_lower(path).as_str() {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "webm" => "audio/webm",
        _ => "application/octet-stream",
    }
}

fn extension_lower(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default()
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
