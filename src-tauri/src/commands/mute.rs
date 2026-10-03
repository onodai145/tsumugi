//! NG（ミュート）・通知設定の取得・更新。

use crate::api::mutes::{fetch_muted_and_blocked, fetch_muted_words};
use crate::domain::{BackgroundKind, MuteConfig, NotifyConfig, UiPrefs};
use crate::error::{Error, Result};
use crate::state::AppState;
use crate::store::settings::ServerMuteSnapshot;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};
#[cfg(target_os = "android")]
use tauri_plugin_fs::FsExt;

/// 通知音として許容する最大サイズ（短い効果音程度を想定）。
const MAX_NOTIFY_SOUND_BYTES: usize = 5 * 1024 * 1024;

/// 拡張子から背景メディアの種類を判定する。戻り値の2つ目は保存時に使う正規化済み拡張子
/// (例: "jpeg"も"jpg"に揃える)。拡張子が無い/未知の場合は None
/// (Androidのフォトピッカーが返す`content://`URIのように拡張子を持たないパスはここでは
/// 判定できない。呼び出し側で`classify_by_magic_bytes`にフォールバックすること。Issue #400)。
fn classify_by_extension(path: &str) -> Option<(BackgroundKind, &'static str)> {
    match extension_lower(path).as_str() {
        "png" => Some((BackgroundKind::Image, "png")),
        "jpg" | "jpeg" => Some((BackgroundKind::Image, "jpg")),
        "gif" => Some((BackgroundKind::Image, "gif")),
        "webp" => Some((BackgroundKind::Image, "webp")),
        "avif" => Some((BackgroundKind::Image, "avif")),
        "bmp" => Some((BackgroundKind::Image, "bmp")),
        "svg" => Some((BackgroundKind::Image, "svg")),
        "mp4" => Some((BackgroundKind::Video, "mp4")),
        "webm" => Some((BackgroundKind::Video, "webm")),
        "mov" => Some((BackgroundKind::Video, "mov")),
        "m4v" => Some((BackgroundKind::Video, "m4v")),
        "mkv" => Some((BackgroundKind::Video, "mkv")),
        "avi" => Some((BackgroundKind::Video, "avi")),
        "ogv" => Some((BackgroundKind::Video, "ogv")),
        _ => None,
    }
}

/// 拡張子で判定できなかった場合のフォールバック。`infer`クレート(マジックバイトによる
/// ファイル種別判定)で画像/動画を判定する(Androidのフォトピッカーが返す`content://` URI
/// のように拡張子を持たないパスのため。Issue #400)。AVIF/HEICもISO base media形式
/// (ftypボックス)を使うが、`infer`はブランド文字列まで見て画像/動画を正しく区別する。
/// SVGや、コンテナだけでは動画/音声を区別できないOgg動画(ogv)は`infer`でも判定できないため、
/// この経路では対応しない(拡張子が付いている場合は`classify_by_extension`が先に処理するため
/// 実害は小さい)。
fn classify_by_magic_bytes(bytes: &[u8]) -> Option<(BackgroundKind, &'static str)> {
    let kind = infer::get(bytes)?;
    match kind.matcher_type() {
        infer::MatcherType::Image => Some((BackgroundKind::Image, kind.extension())),
        infer::MatcherType::Video => Some((BackgroundKind::Video, kind.extension())),
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
    let (kind, ext) = match classify_by_extension(&path) {
        Some(v) => v,
        None => {
            let header = peek_file_header(&app, &path).await?;
            classify_by_magic_bytes(&header)
                .ok_or_else(|| Error::Invalid(format!("対応していないファイル形式です: {path}")))?
        }
    };

    let backgrounds_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| Error::Invalid(format!("no app data dir: {e}")))?
        .join("backgrounds");
    tokio::fs::create_dir_all(&backgrounds_dir).await?;

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
/// 拡張子で種類判定できなかった場合のフォールバック用に、ファイル先頭の数百バイトを覗き見る。
/// `infer`クレートのwebm判定は先頭256バイト超を要求するため、それより余裕を持たせた
/// `PEEK_HEADER_BYTES`バイトを読む。非Android環境ではファイル全体を読まず先頭のみ部分読み込み
/// する(大きな動画でもメモリを圧迫しない)。Androidの`content://` URIは部分読み込みに対応する
/// 手段が無いため`read_file_bytes`でファイル全体を読む(直後の`copy_media_file`でもう一度全体を
/// 読むため二重読み込みにはなるが、拡張子が無い=Androidのフォトピッカー経由の場合にのみ発生する
/// 稀なパスなので許容する。Issue #400)。
const PEEK_HEADER_BYTES: usize = 1024;

async fn peek_file_header(
    #[cfg_attr(not(target_os = "android"), allow(unused_variables))] app: &AppHandle,
    path: &str,
) -> Result<Vec<u8>> {
    #[cfg(target_os = "android")]
    {
        read_file_bytes(app, path).await
    }
    #[cfg(not(target_os = "android"))]
    {
        use tokio::io::AsyncReadExt;
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|e| Error::Invalid(format!("cannot open file {path}: {e}")))?;
        let mut buf = vec![0u8; PEEK_HEADER_BYTES];
        let n = file.read(&mut buf).await.unwrap_or(0);
        buf.truncate(n);
        Ok(buf)
    }
}

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
    apply_mute_config(&state, config).await
}

/// ミュート設定を保存して差し替え、全カラムの backfill 境界を捨てる(`set_mute` の本体)。
///
/// `state.mute` の差し替えは、境界を捨てる**前**に行う。書き込み側は、ミュート設定を読む前に
/// 世代を控えるので、境界の世代が進んだ時点で、新しい設定がすでに反映されている(Issue #452)。
/// 境界を捨てる処理は、境界の書きロックの中で行う。実行中の取得が、旧ミュートの結果に基づく境界を
/// 直後に書き込んで復活させないため(`ColumnFence::invalidate_boundaries`)。
async fn apply_mute_config(state: &AppState, config: MuteConfig) -> Result<()> {
    state.settings.save_mute(&config)?;
    *state.mute.lock().unwrap() = config;
    // ミュート解除方向の変更は、除外済み(=キャッシュされていない)ノートを読み直せないため
    // キャッシュ提供パスでは反映できない。境界を捨てて次回backfillをAPI経由に倒す(Issue #228)。
    state
        .column_fence
        .invalidate_boundaries(|| async {
            let _ = state.cache.clear_all_fetch_boundaries().await;
        })
        .await;
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
    let snapshot = ServerMuteSnapshot::new(ids.iter().cloned(), word_rules.iter().map(|r| r.key()));
    // メモリ上の集合の差し替えは、境界を捨てる前に行う。書き込み側は、ミュート設定を読む前に世代を
    // 控えるので、境界の世代が進んだ時点で、新しい集合がすでに反映されている(Issue #454, #452)。
    state.set_server_mutes(account_id, ids);
    state.set_server_word_mutes(account_id, word_rules);
    reflect_server_mute_change(state, account_id, snapshot).await;
    Ok(result)
}

/// サーバー側ミュートの**解除**を検出して、そのアカウントのカラムの backfill 境界を捨てる(Issue #454)。
///
/// キャッシュには、取得時のフィルタを通ったノートだけが入る。ミュートを解除しても、除外済みのノートは
/// キャッシュに無く読み直せないので、境界を捨てて、次回の backfill を API 経由に倒す(ローカル NG の
/// `set_mute` と同じ理由、Issue #228)。ミュートの追加は、提供時に再適用されるので、捨てない。
///
/// - 前回の保存値が無い(アップグレード直後、新しいアカウント)場合は、捨てずに保存だけする。
/// - 境界の破棄が失敗したら、保存値を更新しない(次回の同期で、もう一度解除を検出して再試行する)。
/// - 保存値の書き込みの失敗は、同期を失敗にしない(次回、もう一度検出するだけで、捨てるのは安全側)。
async fn reflect_server_mute_change(state: &AppState, account_id: &str, snapshot: ServerMuteSnapshot) {
    let previous = match state.settings.load_server_mute_snapshot(account_id) {
        Ok(previous) => previous,
        Err(e) => {
            log::warn!("failed to load the server mute snapshot for {account_id}: {e}");
            return;
        }
    };
    if let Some(previous) = &previous {
        if snapshot.removed_any_since(previous) {
            if let Err(e) = clear_account_boundaries(state, account_id).await {
                log::warn!("failed to clear backfill boundaries after a server mute was removed ({account_id}): {e}");
                return;
            }
        }
    }
    if previous.as_ref() != Some(&snapshot) {
        if let Err(e) = state.settings.save_server_mute_snapshot(account_id, &snapshot) {
            log::warn!("failed to save the server mute snapshot for {account_id}: {e}");
        }
    }
}

/// そのアカウントの全カラムの境界を捨てる。境界の書きロックの中で行うので、実行中の取得が、
/// 旧い集合の結果に基づく境界を、直後に書き込んで復活させない(`ColumnFence::invalidate_boundaries`)。
/// 1件でも失敗したら、残りのカラムも実行した上で、最初のエラーを返す。
async fn clear_account_boundaries(state: &AppState, account_id: &str) -> Result<()> {
    let column_ids: Vec<String> = state
        .settings
        .load_columns()?
        .into_iter()
        .filter(|c| c.account_id == account_id)
        .map(|c| c.id)
        .collect();
    state
        .column_fence
        .invalidate_boundaries(|| async {
            let mut first_error = None;
            for column_id in &column_ids {
                if let Err(e) = state.cache.replace_fetch_boundaries(column_id, &[]).await {
                    first_error.get_or_insert(e);
                }
            }
            first_error.map_or(Ok(()), Err)
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::MisskeyClient;
    use crate::domain::{Column, ColumnKind, FilterQuery, Note, User, Visibility};
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

    #[tokio::test]
    async fn apply_mute_config_clears_boundaries_and_stales_earlier_boundary_epochs() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.cache.replace_fetch_boundaries("c1", &[(0, "n500".to_string())]).await.unwrap();
        let epoch = state.column_fence.begin("c1"); // 旧ミュートで取得を始めた

        apply_mute_config(&state, MuteConfig::default()).await.unwrap();

        assert!(state.cache.get_fetch_boundaries("c1").await.unwrap().is_empty());
        let boundaries_ok = state.column_fence.write_if_current("c1", &epoch, |ok| async move { ok }).await;
        assert_eq!(boundaries_ok, Some(false), "旧ミュートで控えた境界の世代は古くなる");
    }

    #[tokio::test]
    async fn apply_mute_config_replaces_the_mute_config() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        let config = MuteConfig { ng_words: vec!["secret".to_string()], ..MuteConfig::default() };

        apply_mute_config(&state, config.clone()).await.unwrap();

        // 新しい設定が `state.mute` に反映される。差し替えが境界を捨てる前に行われる順序(書き込み側の
        // 不変条件)は、このテストでは検証できない。`apply_mute_config` のコードの順序で守る
        assert_eq!(*state.mute.lock().unwrap(), config);
    }

    /// `mute/list` / `blocking/list` / `i` を、指定のユーザーIDとワード群で返すモックに組み直す。
    async fn mount_server_mutes(mock: &MockServer, muted_users: &[&str], words: serde_json::Value) {
        mock.reset().await;
        let rows: Vec<serde_json::Value> = muted_users
            .iter()
            .enumerate()
            .map(|(i, u)| serde_json::json!({ "id": format!("r{i}"), "muteeId": u }))
            .collect();
        Mock::given(method("POST"))
            .and(path("/mute/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(rows))
            .mount(mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/blocking/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/i"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "mutedWords": words })))
            .mount(mock)
            .await;
    }

    /// acc1 のカラム c1・c2 と、acc2 のカラム c3 に、それぞれ境界(n500)がある状態の `AppState`。
    async fn state_with_three_columns_and_boundaries() -> AppState {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        for (column_id, account_id) in [("c1", "acc1"), ("c2", "acc1"), ("c3", "acc2")] {
            state
                .settings
                .upsert_column(&Column {
                    id: column_id.into(),
                    account_id: account_id.into(),
                    kind: ColumnKind::Home,
                    order: 0,
                    filter: FilterQuery::Keywords(vec![]),
                    notify_sound: false,
                    notify_desktop: false,
                    notify_sound_choice: String::new(),
                    group_id: "g1".into(),
                    title: None,
                })
                .unwrap();
            state.cache.replace_fetch_boundaries(column_id, &[(0, "n500".to_string())]).await.unwrap();
        }
        state
    }

    async fn boundaries_of(state: &AppState, column_id: &str) -> Vec<(u32, String)> {
        state.cache.get_fetch_boundaries(column_id).await.unwrap()
    }

    fn snap(users: &[&str], words: &[&str]) -> crate::store::settings::ServerMuteSnapshot {
        crate::store::settings::ServerMuteSnapshot::new(
            users.iter().map(|s| s.to_string()),
            words.iter().map(|s| s.to_string()),
        )
    }

    /// 捨てられていない境界(`state_with_three_columns_and_boundaries` が置いたもの)。
    fn kept() -> Vec<(u32, String)> {
        vec![(0, "n500".to_string())]
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_keeps_boundaries_and_saves_a_snapshot_on_the_first_sync() {
        let mock = MockServer::start().await;
        mount_server_mutes(&mock, &["u1"], serde_json::json!([])).await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;

        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        // 保存値が無い(アップグレード直後)ので、捨てずに保存だけする
        for column_id in ["c1", "c2", "c3"] {
            assert_eq!(boundaries_of(&state, column_id).await, kept());
        }
        assert_eq!(state.settings.load_server_mute_snapshot("acc1").unwrap(), Some(snap(&["u1"], &[])));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_keeps_boundaries_when_server_mutes_were_only_added() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;
        mount_server_mutes(&mock, &["u1"], serde_json::json!(["spoiler"])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        mount_server_mutes(&mock, &["u1", "u2"], serde_json::json!(["spoiler", "alpha"])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        for column_id in ["c1", "c2", "c3"] {
            assert_eq!(boundaries_of(&state, column_id).await, kept(), "追加だけでは捨てない");
        }
        let saved = state.settings.load_server_mute_snapshot("acc1").unwrap().unwrap();
        assert_eq!(saved.users, vec!["u1".to_string(), "u2".to_string()], "保存値は更新される");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_clears_only_the_syncing_accounts_boundaries_when_a_muted_user_was_removed() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;
        mount_server_mutes(&mock, &["u1", "u2"], serde_json::json!([])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        mount_server_mutes(&mock, &["u1"], serde_json::json!([])).await; // u2 のミュートを解除
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        assert!(boundaries_of(&state, "c1").await.is_empty());
        assert!(boundaries_of(&state, "c2").await.is_empty());
        assert_eq!(boundaries_of(&state, "c3").await, kept(), "他のアカウントのカラムは、そのまま");
        assert_eq!(state.settings.load_server_mute_snapshot("acc1").unwrap(), Some(snap(&["u1"], &[])));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_clears_the_accounts_boundaries_when_a_muted_word_was_removed() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;
        mount_server_mutes(&mock, &[], serde_json::json!(["spoiler", "alpha"])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        mount_server_mutes(&mock, &[], serde_json::json!(["spoiler"])).await; // "alpha" を解除
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        assert!(boundaries_of(&state, "c1").await.is_empty());
        assert!(boundaries_of(&state, "c2").await.is_empty());
        assert_eq!(boundaries_of(&state, "c3").await, kept());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_changes_nothing_when_the_fetch_fails() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;
        mount_server_mutes(&mock, &["u1", "u2"], serde_json::json!([])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        mock.reset().await;
        Mock::given(method("POST"))
            .and(path("/mute/list"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock)
            .await;
        let result = sync_server_mutes_core(&state, "acc1", &client).await;

        assert!(result.is_err());
        for column_id in ["c1", "c2", "c3"] {
            assert_eq!(boundaries_of(&state, column_id).await, kept());
        }
        assert_eq!(state.settings.load_server_mute_snapshot("acc1").unwrap(), Some(snap(&["u1", "u2"], &[])));
        assert!(state.is_server_muted("acc1", "u2"), "メモリ上のミュート集合も、そのまま");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_makes_earlier_boundary_epochs_stale_when_it_clears_boundaries() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;
        mount_server_mutes(&mock, &["u1", "u2"], serde_json::json!([])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();
        let epoch = state.column_fence.begin("c1"); // 旧い集合で取得を始めた(再認証のときのように)

        mount_server_mutes(&mock, &["u1"], serde_json::json!([])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        let boundaries_ok = state.column_fence.write_if_current("c1", &epoch, |ok| async move { ok }).await;
        assert_eq!(boundaries_ok, Some(false), "旧い集合で控えた境界の世代は、古くなる");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_saves_the_snapshot_even_when_the_account_has_no_columns() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        mount_server_mutes(&mock, &["u1", "u2"], serde_json::json!([])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        mount_server_mutes(&mock, &["u1"], serde_json::json!([])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        assert_eq!(state.settings.load_server_mute_snapshot("acc1").unwrap(), Some(snap(&["u1"], &[])));
    }
}

#[cfg(test)]
mod background_media_tests {
    use super::*;

    #[test]
    fn classify_by_extension_recognizes_common_extensions() {
        assert_eq!(
            classify_by_extension("a.png"),
            Some((BackgroundKind::Image, "png"))
        );
        assert_eq!(
            classify_by_extension("a.GIF"),
            Some((BackgroundKind::Image, "gif"))
        );
        assert_eq!(
            classify_by_extension("a.mp4"),
            Some((BackgroundKind::Video, "mp4"))
        );
        assert_eq!(
            classify_by_extension("a.webm"),
            Some((BackgroundKind::Video, "webm"))
        );
        assert_eq!(classify_by_extension("a.txt"), None);
    }

    /// Androidのフォトピッカーが返す`content://`URIには拡張子が無いため、拡張子判定が
    /// 失敗した場合はファイル先頭のマジックバイトから種類を判定する(Issue #400)。
    #[test]
    fn classify_by_extension_returns_none_for_extensionless_path() {
        // content://media/picker/0/... のような拡張子の無いパスは拡張子判定できない。
        assert_eq!(
            classify_by_extension("content://media/picker/0/com.android.providers.media.photopicker/media/1000000123"),
            None
        );
    }

    // `infer`クレートの一部の判定(特にwebm)は先頭256バイト超のバイト列を要求するため、
    // 手書きの短いマジックバイト列では正しく再現できない。ffmpeg/Pillowで生成した
    // 実ファイルを`tests/fixtures/background_media/`に配置し、それをそのまま使う。
    const TINY_PNG: &[u8] =
        include_bytes!("../../tests/fixtures/background_media/tiny.png");
    const TINY_JPG: &[u8] =
        include_bytes!("../../tests/fixtures/background_media/tiny.jpg");
    const TINY_WEBP: &[u8] =
        include_bytes!("../../tests/fixtures/background_media/tiny.webp");
    const TINY_AVIF: &[u8] =
        include_bytes!("../../tests/fixtures/background_media/tiny.avif");
    const TINY_MP4: &[u8] =
        include_bytes!("../../tests/fixtures/background_media/tiny.mp4");
    const TINY_WEBM: &[u8] =
        include_bytes!("../../tests/fixtures/background_media/tiny.webm");
    const TINY_AVI: &[u8] =
        include_bytes!("../../tests/fixtures/background_media/tiny.avi");

    #[test]
    fn classify_by_magic_bytes_recognizes_png() {
        assert_eq!(
            classify_by_magic_bytes(TINY_PNG),
            Some((BackgroundKind::Image, "png"))
        );
    }

    #[test]
    fn classify_by_magic_bytes_recognizes_jpeg() {
        assert_eq!(
            classify_by_magic_bytes(TINY_JPG),
            Some((BackgroundKind::Image, "jpg"))
        );
    }

    #[test]
    fn classify_by_magic_bytes_recognizes_webp() {
        assert_eq!(
            classify_by_magic_bytes(TINY_WEBP),
            Some((BackgroundKind::Image, "webp"))
        );
    }

    #[test]
    fn classify_by_magic_bytes_recognizes_avi() {
        assert_eq!(
            classify_by_magic_bytes(TINY_AVI),
            Some((BackgroundKind::Video, "avi"))
        );
    }

    #[test]
    fn classify_by_magic_bytes_recognizes_mp4_ftyp_box() {
        assert_eq!(
            classify_by_magic_bytes(TINY_MP4),
            Some((BackgroundKind::Video, "mp4"))
        );
    }

    /// AVIF/HEICもISO base media形式(ftypボックス)を使うため、ブランド文字列まで見て
    /// 動画と誤判定しないことを確認する。
    #[test]
    fn classify_by_magic_bytes_distinguishes_avif_from_mp4_by_brand() {
        assert_eq!(
            classify_by_magic_bytes(TINY_AVIF),
            Some((BackgroundKind::Image, "avif"))
        );
    }

    #[test]
    fn classify_by_magic_bytes_recognizes_webm() {
        assert_eq!(
            classify_by_magic_bytes(TINY_WEBM),
            Some((BackgroundKind::Video, "webm"))
        );
    }

    #[test]
    fn classify_by_magic_bytes_returns_none_for_unrecognized_bytes() {
        assert_eq!(classify_by_magic_bytes(b"not a media file!!"), None);
        // 短すぎるバイト列も判定不能として扱う。
        assert_eq!(classify_by_magic_bytes(&[0x89, b'P']), None);
    }

    /// 実際の`import_background_media`が使う`PEEK_HEADER_BYTES`だけを覗いた場合でも
    /// 判定できることを確認する(webmはinferの要求(256バイト超)を満たす必要があるため、
    /// 覗き見るバイト数が足りているかの回帰確認を兼ねる)。
    #[test]
    fn classify_by_magic_bytes_works_with_peek_sized_prefix() {
        fn peek(bytes: &[u8]) -> &[u8] {
            &bytes[..bytes.len().min(PEEK_HEADER_BYTES)]
        }
        assert_eq!(
            classify_by_magic_bytes(peek(TINY_MP4)),
            Some((BackgroundKind::Video, "mp4"))
        );
        assert_eq!(
            classify_by_magic_bytes(peek(TINY_WEBM)),
            Some((BackgroundKind::Video, "webm"))
        );
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
