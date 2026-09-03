//! NG（ミュート）・通知設定の取得・更新。

use crate::api::mutes::{fetch_muted_and_blocked, fetch_muted_words};
use crate::domain::{MuteConfig, NotifyConfig, UiPrefs};
use crate::error::{Error, Result};
use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use specta::Type;
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
    // ミュート解除方向の変更は、除外済み(=キャッシュされていない)ノートを読み直せないため
    // キャッシュ提供パスでは反映できない。境界を捨てて次回backfillをAPI経由に倒す(Issue #228)。
    let _ = state.cache.clear_all_fetch_boundaries();
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

/// ファイルを読む共通処理。
///
/// Android は SAF のファイルピッカーが `content://` URI を返し、通常のファイルシステム
/// パスとして開けない（`std::fs`/`tokio::fs` では ENOENT になる）ため、
/// `tauri-plugin-fs` 経由でネイティブの ContentResolver ブリッジを使って読む。
pub(crate) async fn read_file_bytes(
    #[cfg_attr(not(target_os = "android"), allow(unused_variables))] app: &AppHandle,
    path: &str,
) -> Result<Vec<u8>> {
    #[cfg(target_os = "android")]
    {
        let app = app.clone();
        let path_owned = path.to_string();
        // "content://..." は Url、それ以外は通常のファイルパスとして解釈される
        // (`FilePath::from_str` は `Infallible` を返すため unwrap で安全)。
        let file_path: tauri_plugin_fs::FilePath = path.parse().unwrap();
        tauri::async_runtime::spawn_blocking(move || app.fs().read(file_path))
            .await
            .map_err(|e| Error::Invalid(format!("cannot read file {path_owned}: {e}")))?
            .map_err(|e| Error::Invalid(format!("cannot read file {path_owned}: {e}")))
    }
    #[cfg(not(target_os = "android"))]
    {
        tokio::fs::read(path)
            .await
            .map_err(|e| Error::Invalid(format!("cannot read file {path}: {e}")))
    }
}

/// ファイルを読み、上限サイズを検査して data URL(base64) にする共通処理。
pub(crate) async fn read_file_as_data_url(
    app: &AppHandle,
    path: &str,
    max_bytes: usize,
    guess_mime: fn(&str) -> &'static str,
) -> Result<String> {
    let bytes = read_file_bytes(app, path).await?;
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

/// `sync_server_mutes` の戻り値。ユーザ/ブロックミュート数とワードミュートのルール数を
/// 別々に返す(フロントのログ表示用。Issue #11)。
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncMuteResult {
    pub blocked_users: u32,
    pub word_rules: u32,
}

/// サーバ側のミュート/ブロック・ワードミュート(mutedWords)を取得して AppState に反映する。
/// 起動時とアカウント追加時にフロントから呼ぶ（Krile MuteBlockManager 相当。Issue #11）。
#[tauri::command]
#[specta::specta]
pub async fn sync_server_mutes(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<SyncMuteResult> {
    let client = state.client_for(&account_id)?;
    sync_server_mutes_core(&state, &account_id, &client).await
}

/// `sync_server_mutes` の中核ロジック。`AppState` を `State<'_, _>` ではなく `&AppState` で
/// 受け取り、`client` も呼び出し側から渡すことで `tauri::State`(テストから構築不可)と
/// `client_for`(登録済みアカウント+keyringが必要)の両方を経由せずに単体テスト可能にしている
/// (`commands/column.rs::search_cache_core` と同じ狙い)。
async fn sync_server_mutes_core(
    state: &AppState,
    account_id: &str,
    client: &crate::api::MisskeyClient,
) -> Result<SyncMuteResult> {
    let ids = fetch_muted_and_blocked(client).await?;
    let word_rules = fetch_muted_words(client).await?;
    let result = SyncMuteResult {
        blocked_users: ids.len() as u32,
        word_rules: word_rules.len() as u32,
    };
    state.set_server_mutes(account_id, ids);
    state.set_server_word_mutes(account_id, word_rules);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::MisskeyClient;
    use crate::domain::{Note, User, Visibility};
    use crate::store::SettingsStore;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn note(text: &str) -> Note {
        Note {
            id: "n1".into(),
            created_at: 0,
            text: Some(text.into()),
            cw: None,
            visibility: Visibility::Public,
            local_only: false,
            user: User {
                id: "u1".into(),
                username: "alice".into(),
                host: None,
                name: None,
                avatar_url: None,
                is_bot: false,
                is_cat: false,
                followers_count: 0,
                following_count: 0,
                notes_count: 0,
                emojis: std::collections::HashMap::new(),
                bio: None,
                banner_url: None,
                instance: None,
            },
            reply_id: None,
            renote_id: None,
            renote: None,
            files: vec![],
            poll: None,
            tags: vec![],
            mentions: vec![],
            emojis: std::collections::HashMap::new(),
            channel_id: None,
            via: None,
            lang: None,
            reactions: std::collections::HashMap::new(),
            reaction_count: 0,
            renote_count: 0,
            reply_count: 0,
            my_reaction: None,
            is_renoted_by_me: false,
            is_favorited_by_me: false,
            is_pinned: false,
        }
    }

    /// `sync_server_mutes_core` の結合テスト(Issue #11)。実HTTP経由(wiremockモック)で
    /// `mute/list`/`blocking/list`/`i` を叩き、レスポンスが `AppState` まで正しく届いて
    /// `is_word_muted` が実際に効くことを検証する。`parse_muted_words` 単体の網羅は
    /// `api::mutes::tests` 側の8ケースに任せ、ここでは「実HTTPレスポンス→state反映」という
    /// 単体テストでは埋まらない結合部分だけを見る。
    #[tokio::test(flavor = "multi_thread")]
    async fn sync_server_mutes_core_populates_state_from_real_http_responses() {
        let mock = MockServer::start().await;
        // mute/list・blocking/list は空配列を返す(ページングループを1回で終わらせる)。
        Mock::given(method("POST"))
            .and(path("/mute/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/blocking/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock)
            .await;
        // /i は2グループ(AND語群 + 正規表現)を持つ mutedWords を返す。
        Mock::given(method("POST"))
            .and(path("/i"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "mutedWords": [["foo", "bar"], "/spoiler/i"]
            })))
            .mount(&mock)
            .await;

        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());

        let result = sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        assert_eq!(result.blocked_users, 0);
        assert_eq!(result.word_rules, 2);
        // AND群("foo"かつ"bar")が実際に効く
        assert!(state.is_word_muted("acc1", &note("foo and bar here")));
        // 正規表現("/spoiler/i")が実際に効く(大小無視)
        assert!(state.is_word_muted("acc1", &note("BIG SPOILER")));
        // どちらにも該当しなければミュートされない
        assert!(!state.is_word_muted("acc1", &note("nothing matches")));
        // 未同期の別アカウントには影響しない
        assert!(!state.is_word_muted("other-acc", &note("foo and bar here")));
    }
}
