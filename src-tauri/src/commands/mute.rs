//! NG（ミュート）・通知設定の取得・更新。

use crate::api::mutes::{fetch_muted_and_blocked, fetch_muted_words};
use crate::domain::{BackgroundKind, MuteConfig, NotifyConfig, UiPrefs};
use crate::error::{Error, Result};
use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};
#[cfg(target_os = "android")]
use tauri_plugin_fs::FsExt;

/// 通知音として許容する最大サイズ（短い効果音程度を想定）。
const MAX_NOTIFY_SOUND_BYTES: usize = 5 * 1024 * 1024;

/// 背景メディアとして許容する拡張子から種類を判定する。未知の拡張子は None。
fn classify_background_kind(path: &str) -> Option<BackgroundKind> {
    match extension_lower(path).as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "svg" => {
            Some(BackgroundKind::Image)
        }
        "mp4" | "webm" | "mov" | "m4v" | "mkv" | "avi" | "ogv" => Some(BackgroundKind::Video),
        _ => None,
    }
}

/// `import_background_media` の戻り値。
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundMedia {
    pub kind: BackgroundKind,
    pub absolute_path: String,
}

/// 背景画像/動画として選んだファイルを `app_data_dir()/backgrounds/` にコピーし、
/// 種類判定した上でコピー後の絶対パスを返す。`backgroundKind`/`backgroundPath` の永続化は
/// フロント側の「保存」(`setUiPrefs`)まで行われないため、ここでは旧ファイルの削除は行わない
/// (未保存のまま閉じられた場合に備えて残しておく)。使われなくなった旧ファイルは起動時の
/// `gc_unused_background_files` でまとめて掃除する。
/// サイズ上限は設けない（コピーは非同期コマンドなのでUIをブロックしない）。
#[tauri::command]
#[specta::specta]
pub async fn import_background_media(app: AppHandle, path: String) -> Result<BackgroundMedia> {
    let kind = classify_background_kind(&path)
        .ok_or_else(|| Error::Invalid(format!("対応していないファイル形式です: {path}")))?;

    let backgrounds_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| Error::Invalid(format!("no app data dir: {e}")))?
        .join("backgrounds");
    tokio::fs::create_dir_all(&backgrounds_dir).await?;

    let ext = extension_lower(&path);
    let file_name = format!("{}.{ext}", uuid::Uuid::new_v4());
    let dest_path = backgrounds_dir.join(&file_name);

    copy_media_file(&app, &path, &dest_path).await?;

    Ok(BackgroundMedia {
        kind,
        absolute_path: dest_path.to_string_lossy().into_owned(),
    })
}

/// `backgrounds_dir` 配下のうち、現在の設定(`keep_path`)から参照されていないファイルを
/// 起動時に削除する。`import_background_media` は新ファイルをコピーするだけで旧ファイルを
/// 即削除しないため(未保存で閉じた場合に備える。I1)、ここでまとめてゴミ掃除する。
///
/// `keep_path` との一致判定は `canonicalize()` した実体パスで比較する。`keep_path` が
/// 存在しない/不正なパスの場合は安全側(=削除する側)に倒す。`backgrounds_dir` はこの関数が
/// 管理する専用ディレクトリであり、他人のファイルを消す心配はないため。
///
/// `backgrounds_dir` 自体が存在しない場合は何もせず `Ok(())` を返す(起動をブロックしないため、
/// 呼び出し側でも失敗を warn ログに留めて継続する設計だが、この関数自体もここで吸収する)。
pub(crate) fn gc_unused_background_files(
    backgrounds_dir: &std::path::Path,
    keep_path: Option<&str>,
) -> Result<()> {
    if !backgrounds_dir.exists() {
        return Ok(());
    }

    let keep_canonical = keep_path.and_then(|p| std::path::Path::new(p).canonicalize().ok());

    for entry in std::fs::read_dir(backgrounds_dir)? {
        let entry = entry?;
        let entry_path = entry.path();
        if !entry_path.is_file() {
            continue;
        }
        let entry_canonical = entry_path.canonicalize().ok();
        let is_kept = match (&entry_canonical, &keep_canonical) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        };
        if !is_kept {
            if let Err(e) = std::fs::remove_file(&entry_path) {
                log::warn!(
                    "gc_unused_background_files: failed to remove {}: {e}",
                    entry_path.display()
                );
            }
        }
    }
    Ok(())
}

/// `src` を `dest` へコピーする。Android では `content://` URI を扱うため
/// `read_file_bytes`(ContentResolverブリッジ)経由でメモリを介してコピーするが、
/// それ以外の環境では動画のような大きなファイルでもメモリを圧迫しないよう
/// ストリーミングコピー(`tokio::fs::copy`)を使う。
async fn copy_media_file(
    #[cfg_attr(not(target_os = "android"), allow(unused_variables))] app: &AppHandle,
    src: &str,
    dest: &std::path::Path,
) -> Result<()> {
    #[cfg(target_os = "android")]
    {
        let bytes = read_file_bytes(app, src).await?;
        tokio::fs::write(dest, &bytes).await?;
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        tokio::fs::copy(src, dest)
            .await
            .map_err(|e| Error::Invalid(format!("cannot copy file {src}: {e}")))?;
        Ok(())
    }
}

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
    let _ = state.cache.clear_all_fetch_boundaries().await;
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
                avatar_blurhash: None,
                instance: None,
            },
            reply_id: None,
            reply_user_id: None,
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

#[cfg(test)]
mod background_media_tests {
    use super::*;

    #[test]
    fn classify_background_kind_recognizes_common_extensions() {
        assert_eq!(
            classify_background_kind("a.png"),
            Some(BackgroundKind::Image)
        );
        assert_eq!(
            classify_background_kind("a.GIF"),
            Some(BackgroundKind::Image)
        );
        assert_eq!(
            classify_background_kind("a.mp4"),
            Some(BackgroundKind::Video)
        );
        assert_eq!(
            classify_background_kind("a.webm"),
            Some(BackgroundKind::Video)
        );
        assert_eq!(classify_background_kind("a.txt"), None);
    }

    fn make_tmp_backgrounds_dir() -> std::path::PathBuf {
        let tmp = std::env::temp_dir().join(format!("tsumugi-gc-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        tmp
    }

    #[test]
    fn gc_unused_background_files_keeps_only_matching_keep_path() {
        let backgrounds_dir = make_tmp_backgrounds_dir();
        let kept = backgrounds_dir.join("kept.png");
        let other1 = backgrounds_dir.join("other1.png");
        let other2 = backgrounds_dir.join("other2.mp4");
        std::fs::write(&kept, b"dummy").unwrap();
        std::fs::write(&other1, b"dummy").unwrap();
        std::fs::write(&other2, b"dummy").unwrap();

        gc_unused_background_files(&backgrounds_dir, Some(kept.to_str().unwrap())).unwrap();

        assert!(kept.exists());
        assert!(!other1.exists());
        assert!(!other2.exists());

        std::fs::remove_dir_all(&backgrounds_dir).unwrap();
    }

    #[test]
    fn gc_unused_background_files_removes_all_when_keep_path_is_none() {
        let backgrounds_dir = make_tmp_backgrounds_dir();
        let f1 = backgrounds_dir.join("f1.png");
        let f2 = backgrounds_dir.join("f2.mp4");
        std::fs::write(&f1, b"dummy").unwrap();
        std::fs::write(&f2, b"dummy").unwrap();

        gc_unused_background_files(&backgrounds_dir, None).unwrap();

        assert!(!f1.exists());
        assert!(!f2.exists());

        std::fs::remove_dir_all(&backgrounds_dir).unwrap();
    }

    #[test]
    fn gc_unused_background_files_removes_all_when_keep_path_does_not_exist() {
        let backgrounds_dir = make_tmp_backgrounds_dir();
        let f1 = backgrounds_dir.join("f1.png");
        std::fs::write(&f1, b"dummy").unwrap();

        gc_unused_background_files(
            &backgrounds_dir,
            Some(backgrounds_dir.join("does-not-exist.png").to_str().unwrap()),
        )
        .unwrap();

        assert!(!f1.exists());

        std::fs::remove_dir_all(&backgrounds_dir).unwrap();
    }

    #[test]
    fn gc_unused_background_files_ok_when_dir_missing() {
        let backgrounds_dir =
            std::env::temp_dir().join(format!("tsumugi-gc-test-missing-{}", uuid::Uuid::new_v4()));
        assert!(!backgrounds_dir.exists());

        gc_unused_background_files(&backgrounds_dir, None).unwrap();
    }
}
