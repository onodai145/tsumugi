//! Tauri が管理するアプリ状態（command から `State<AppState>` で参照）。

use crate::domain::{EmojiDef, MuteConfig, Note};
use crate::fence::ColumnFence;
use crate::filter::mute::WordMuteRule;
use crate::session::{AccountManager, SecretStore};
use crate::sound::SoundPlayer;
use crate::store::{DraftStore, NoteCacheStore, SettingsStore};
use crate::stream::ConnectionManager;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// REST/WebSocket 双方で送る User-Agent。
pub const USER_AGENT: &str = concat!(
    "tsumugi/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/onodai145/tsumugi)"
);

/// 認可待ちの MiAuth セッション（session_id -> 発行先 host）。
pub struct PendingMiAuth {
    pub host: String,
}

/// `fetch_backfill`のキャッシュhit/fallback理由(Issue #241)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackfillOutcome {
    Hit,
    /// cache_eligibleだがいずれかのソースのbackfill境界(get_fetch_boundaries)が未確定でAPIへ。
    /// Issue #228のPR #237で残課題として記載された「実は機能が働いていない」ケースを可視化する。
    /// 注意: 取得に失敗(`Failed`)し続けるソースの境界は確定しないため、このカウンタが増え続ける。
    /// 例えばTQLにREST非対応のソースが入った場合。現状は全ソースがREST対応なので起きない(Issue #429)。
    FallbackBoundaryUnset,
    /// cache_eligibleだが境界は確定済み、範囲外/件数不足でAPIへ。
    /// 境界の読み出し自体がDBエラーで失敗した場合もここに数える(「未確定」とは区別する)。
    FallbackOther,
}

/// キャッシュhit/fallback回数のプロセス内カウンタ(Issue #241)。DB永続化はせず、
/// アプリ再起動でリセットされるセッション単位の統計という位置付け。
#[derive(Default)]
pub struct CacheMetrics {
    backfill_hit: AtomicU64,
    backfill_fallback_boundary: AtomicU64,
    backfill_fallback_other: AtomicU64,
    resume_hit: AtomicU64,
    resume_fallback: AtomicU64,
}

impl CacheMetrics {
    pub fn record_backfill(&self, outcome: BackfillOutcome) {
        let counter = match outcome {
            BackfillOutcome::Hit => &self.backfill_hit,
            BackfillOutcome::FallbackBoundaryUnset => &self.backfill_fallback_boundary,
            BackfillOutcome::FallbackOther => &self.backfill_fallback_other,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_resume(&self, hit: bool) {
        let counter = if hit { &self.resume_hit } else { &self.resume_fallback };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn backfill_hit(&self) -> i32 {
        self.backfill_hit
            .load(Ordering::Relaxed)
            .try_into()
            .unwrap_or(i32::MAX)
    }

    pub fn backfill_fallback_boundary(&self) -> i32 {
        self.backfill_fallback_boundary
            .load(Ordering::Relaxed)
            .try_into()
            .unwrap_or(i32::MAX)
    }

    pub fn backfill_fallback_other(&self) -> i32 {
        self.backfill_fallback_other
            .load(Ordering::Relaxed)
            .try_into()
            .unwrap_or(i32::MAX)
    }

    pub fn resume_hit(&self) -> i32 {
        self.resume_hit
            .load(Ordering::Relaxed)
            .try_into()
            .unwrap_or(i32::MAX)
    }

    pub fn resume_fallback(&self) -> i32 {
        self.resume_fallback
            .load(Ordering::Relaxed)
            .try_into()
            .unwrap_or(i32::MAX)
    }
}

pub struct AppState {
    pub http: reqwest::Client,
    /// WebViewから渡された任意URLを取得するコマンド専用のHTTPクライアント(Issue #377)。
    /// 禁止IP(loopback/private/link-local等)への接続を `net_guard` が拒否する。API通信は `http` を使う。
    pub fetch_http: reqwest::Client,
    /// `fetch_http` が例外として許可するホスト(登録アカウントのホスト)。`accounts` が変わったら
    /// `refresh_fetch_allowlist` で差し替える。
    pub fetch_allowlist: crate::net_guard::FetchAllowlist,
    pub accounts: Mutex<AccountManager>,
    pub secrets: Box<dyn SecretStore>,
    pub pending: Mutex<HashMap<String, PendingMiAuth>>,
    pub connections: ConnectionManager,
    /// host -> カスタム絵文字一覧（インスタンス単位でキャッシュ）
    pub emoji_cache: Mutex<HashMap<String, Vec<EmojiDef>>>,
    /// account_id -> 接続先サーバーの Misskey バージョン文字列(`/api/meta`)。サーバーサイド検索
    /// (Issue #430)の対応機能判定に使う。取得に成功した値だけ保存し、アプリ再起動まで再取得しない。
    pub server_versions: Mutex<HashMap<String, String>>,
    /// account_id -> 接続先サーバーでノート翻訳が使えるか(`/api/meta` の `translatorAvailable`、Issue #440)。
    /// 取得に成功した値だけ保存し、アプリ再起動まで再取得しない。
    pub translator_available: Mutex<HashMap<String, bool>>,
    /// ローカル NG（ミュート）設定。ストリーム/REST の受信ノートに適用する
    pub mute: Mutex<MuteConfig>,
    /// account_id -> サーバ側でミュート/ブロックしているユーザの userId 集合。
    /// 起動時/アカウント追加時に同期し、受信ノート・通知の抑制に使う（Krile MuteBlockManager 相当）。
    pub server_mutes: Mutex<HashMap<String, HashSet<String>>>,
    /// account_id -> サーバ側ワードミュート(mutedWords)のルール一覧。
    /// server_mutes と同じタイミングで同期し、ノート本文/CWの追加フィルタに使う(Issue #11)。
    pub server_word_mutes: Mutex<HashMap<String, Vec<WordMuteRule>>>,
    /// account_id -> サーバー側ミュート同期の排他ロック。同じアカウントの `sync_server_mutes_core` を、
    /// 取得から保存値の書き込みまで直列にする(Issue #456)。必要になったときに作る。
    /// ロックの順序は「このロック → 境界の書きロック(`ColumnFence::invalidate_boundaries`)」のみ。
    server_mute_sync_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// テスト専用: `client_for` が返すクライアントの API ベースの上書き(`wiremock` の `uri()`)。
    /// 本番のクライアントは `https://{host}/api` 固定で、モック HTTP に向けられないため。
    /// 設定は `set_test_api_base` 経由だけ。全アカウント共通の上書きで、アカウントごとに別のモックは使えない。
    #[cfg(test)]
    test_api_base: Mutex<Option<String>>,
    pub settings: SettingsStore,
    pub drafts: DraftStore,
    /// クライアント側の予約投稿(Issue #60 B)。`AppState::new*` のシグネチャを変えずに済むよう、
    /// 既定はメモリ版で、起動時に `with_scheduled_posts` で永続版へ差し替える。
    pub scheduled_posts: crate::store::ScheduledPostStore,
    /// スケジューラを起こす通知。予約の追加・取り消し・「今すぐ投稿」の後に `notify_one` する。
    pub scheduler_wakeup: std::sync::Arc<tokio::sync::Notify>,
    pub cache: NoteCacheStore,
    /// ノートキャッシュDB(`cache.db`)を置くディレクトリ。`set_cache_backend`がSqliteへ
    /// 切り替える際に`cache_dir.join("cache.db")`を再度開くために保持する(Issue #115 Phase 2)。
    pub cache_dir: std::path::PathBuf,
    /// 再接続ギャップ埋め(Issue #147)が実行中の column_id 集合。フラッピング再接続で同一
    /// カラムに対する多重実行を防ぐためのガード（commands/column.rs 側で挿入/削除する）。
    pub gap_fill_in_flight: Mutex<HashSet<String>>,
    /// 通知音のネイティブ再生(Issue #12)。
    pub sound: SoundPlayer,
    /// キャッシュhit/fallback回数の集計(Issue #241)。Backstageの「メトリクス」タブ用。
    pub cache_metrics: CacheMetrics,
    /// カラムごとの世代とロック。`update_column` / `close_column` と、実行中の REST 取得の
    /// キャッシュ書き込みの競合を防ぐ(Issue #446)。
    pub column_fence: ColumnFence,
}

impl AppState {
    /// 永続化済みアカウントを読み込んで初期化する。
    pub fn new(
        secrets: Box<dyn SecretStore>,
        settings: SettingsStore,
        drafts: DraftStore,
        cache: NoteCacheStore,
        cache_dir: std::path::PathBuf,
    ) -> Self {
        Self::new_with_sound(secrets, settings, drafts, cache, cache_dir, SoundPlayer::spawn())
    }

    /// 予約投稿のストアを差し替える(起動時に永続版へ)。
    pub fn with_scheduled_posts(mut self, store: crate::store::ScheduledPostStore) -> Self {
        self.scheduled_posts = store;
        self
    }

    /// `sound` フィールドの構築方法を差し替え可能にした内部コンストラクタ。
    /// 本番経路は `new`(実デバイスを開くスレッドを立てる)、テスト経路は
    /// `new_for_test`(何も立てない `SoundPlayer::new_for_test`)から呼ばれる。
    fn new_with_sound(
        secrets: Box<dyn SecretStore>,
        settings: SettingsStore,
        drafts: DraftStore,
        cache: NoteCacheStore,
        cache_dir: std::path::PathBuf,
        sound: SoundPlayer,
    ) -> Self {
        let accounts = settings.load_accounts().unwrap_or_else(|e| {
            log::error!("failed to load accounts: {e}");
            Vec::new()
        });
        let mute = settings.load_mute().unwrap_or_default();
        let fetch_allowlist = crate::net_guard::FetchAllowlist::default();
        fetch_allowlist.replace(accounts.iter().map(|a| a.host.as_str()));
        Self {
            http: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .expect("failed to build reqwest client"),
            fetch_http: crate::net_guard::build_fetch_client(USER_AGENT, fetch_allowlist.clone())
                .expect("failed to build fetch client"),
            fetch_allowlist,
            accounts: Mutex::new(AccountManager::with_accounts(accounts)),
            secrets,
            pending: Mutex::new(HashMap::new()),
            connections: ConnectionManager::default(),
            emoji_cache: Mutex::new(HashMap::new()),
            server_versions: Mutex::new(HashMap::new()),
            translator_available: Mutex::new(HashMap::new()),
            mute: Mutex::new(mute),
            server_mutes: Mutex::new(HashMap::new()),
            server_word_mutes: Mutex::new(HashMap::new()),
            server_mute_sync_locks: Mutex::new(HashMap::new()),
            #[cfg(test)]
            test_api_base: Mutex::new(None),
            settings,
            drafts,
            scheduled_posts: crate::store::ScheduledPostStore::new_in_memory(),
            scheduler_wakeup: std::sync::Arc::new(tokio::sync::Notify::new()),
            cache,
            cache_dir,
            gap_fill_in_flight: Mutex::new(HashSet::new()),
            sound,
            cache_metrics: CacheMetrics::default(),
            column_fence: ColumnFence::default(),
        }
    }

    /// account の user_id がサーバ側ミュート/ブロック対象か。
    pub fn is_server_muted(&self, account_id: &str, user_id: &str) -> bool {
        self.server_mutes
            .lock()
            .unwrap()
            .get(account_id)
            .is_some_and(|s| s.contains(user_id))
    }

    /// account のサーバ側ミュート/ブロック集合を差し替える。
    pub fn set_server_mutes(&self, account_id: &str, ids: HashSet<String>) {
        self.server_mutes
            .lock()
            .unwrap()
            .insert(account_id.to_string(), ids);
    }

    /// account の note が サーバ側ワードミュート(mutedWords)に該当するか。
    pub fn is_word_muted(&self, account_id: &str, note: &Note) -> bool {
        self.server_word_mutes
            .lock()
            .unwrap()
            .get(account_id)
            .is_some_and(|rules| crate::filter::mute::is_word_note_muted(note, rules))
    }

    /// account のサーバ側ワードミュートルールを差し替える。
    pub fn set_server_word_mutes(&self, account_id: &str, rules: Vec<WordMuteRule>) {
        self.server_word_mutes
            .lock()
            .unwrap()
            .insert(account_id.to_string(), rules);
    }

    /// account のサーバー側ミュート同期の排他ロック。同じアカウントには同じ `Arc` を返す。
    pub fn server_mute_sync_lock(&self, account_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        Arc::clone(
            self.server_mute_sync_locks
                .lock()
                .unwrap()
                .entry(account_id.to_string())
                .or_default(),
        )
    }

    /// account の同期ロックのエントリを破棄する(アカウント削除時)。実行中の同期は `Arc` を持つので、
    /// そのまま完了する。
    pub fn forget_server_mute_sync_lock(&self, account_id: &str) {
        self.server_mute_sync_locks.lock().unwrap().remove(account_id);
    }

    /// account の接続先サーバーのバージョン（取得済みの場合のみ）。
    pub fn server_version(&self, account_id: &str) -> Option<String> {
        self.server_versions.lock().unwrap().get(account_id).cloned()
    }

    /// account の接続先サーバーのバージョンを保存する。
    pub fn set_server_version(&self, account_id: &str, version: String) {
        self.server_versions
            .lock()
            .unwrap()
            .insert(account_id.to_string(), version);
    }

    /// account のバージョンキャッシュを破棄する（アカウント削除時）。
    pub fn forget_server_version(&self, account_id: &str) {
        self.server_versions.lock().unwrap().remove(account_id);
    }

    /// account の接続先サーバーでノート翻訳が使えるか（取得済みの場合のみ）。
    pub fn translator_available(&self, account_id: &str) -> Option<bool> {
        self.translator_available.lock().unwrap().get(account_id).copied()
    }

    /// account の翻訳可否を保存する。
    pub fn set_translator_available(&self, account_id: &str, available: bool) {
        self.translator_available
            .lock()
            .unwrap()
            .insert(account_id.to_string(), available);
    }

    /// account の翻訳可否キャッシュを破棄する（アカウント削除時）。
    pub fn forget_translator_available(&self, account_id: &str) {
        self.translator_available.lock().unwrap().remove(account_id);
    }

    #[cfg(test)]
    /// テスト用: keyring を使わずインメモリ DB で構築する。他モジュールのテストからも使う。
    /// `sound` は実デバイスを開くスレッドを立てない `SoundPlayer::new_for_test` を使う
    /// (ヘッドレス CI でのテストごとのデバイスプローブ/ALSA ノイズを避けるため)。
    pub(crate) fn new_for_test(settings: SettingsStore) -> Self {
        let cache = NoteCacheStore::new(crate::store::SqliteBackend::new(
            crate::store::db::open_cache_in_memory().unwrap(),
        ));
        Self::new_with_sound(
            Box::new(crate::session::MemoryStore::default()),
            settings,
            DraftStore::new_in_memory(),
            cache,
            std::env::temp_dir(),
            SoundPlayer::new_for_test(),
        )
    }

    /// `accounts` の全ホストで取得先の許可リストを差し替える。アカウントの追加・削除の直後に呼ぶ。
    pub fn refresh_fetch_allowlist(&self) {
        let hosts: Vec<String> = self.accounts.lock().unwrap().list().into_iter().map(|a| a.host).collect();
        self.fetch_allowlist.replace(hosts);
    }

    /// account_id から (host, token) を引く。未登録なら Invalid、token 欠落なら Unauthorized。
    pub fn host_token(&self, account_id: &str) -> crate::error::Result<(String, String)> {
        use crate::error::Error;
        let host = {
            let accounts = self.accounts.lock().unwrap();
            accounts
                .get(account_id)
                .map(|a| a.host.clone())
                .ok_or_else(|| Error::Invalid(format!("unknown account: {account_id}")))?
        };
        let token = self
            .secrets
            .get(account_id)?
            .ok_or_else(|| Error::Unauthorized(format!("no token for account: {account_id}")))?;
        Ok((host, token))
    }

    /// フィルタ評価に使う文脈（全ログインアカウントの userId）を構築する。
    pub fn eval_context(&self) -> crate::filter::eval::EvalContext {
        let my_user_ids = self
            .accounts
            .lock()
            .unwrap()
            .list()
            .iter()
            .map(|a| a.user_id.clone())
            .collect();
        crate::filter::eval::EvalContext {
            my_user_ids,
            following_ids: None,
            local_host: None,
        }
    }

    /// account_id から host + token を引き、REST クライアントを構築する。
    pub fn client_for(&self, account_id: &str) -> crate::error::Result<crate::api::MisskeyClient> {
        let (host, token) = self.host_token(account_id)?;
        #[cfg(test)]
        if let Some(base) = self.test_api_base.lock().unwrap().clone() {
            return Ok(crate::api::MisskeyClient::new_with_api_base(self.http.clone(), base, Some(token)));
        }
        Ok(crate::api::MisskeyClient::new(
            self.http.clone(),
            host,
            Some(token),
        ))
    }

    /// テスト用: 以降の `client_for` を、指定の API ベース(`wiremock` の `uri()`)へ向ける。
    /// 全アカウント共通の上書き。呼び忘れると、`client_for` は実際の `https://{host}/api`(実 DNS)へ向かう。
    #[cfg(test)]
    pub(crate) fn set_test_api_base(&self, base: String) {
        *self.test_api_base.lock().unwrap() = Some(base);
    }

    /// テスト用: アカウント(host は固定で `misskey.test`)とトークンを登録し、`host_token` /
    /// `client_for` が通る状態にする。HTTP をモックに向けるには、`set_test_api_base` も呼ぶこと
    /// (呼ばないと、`client_for` は `https://misskey.test/api` へ向かう)。
    #[cfg(test)]
    pub(crate) fn register_test_account(&self, account_id: &str) {
        self.accounts.lock().unwrap().upsert(crate::domain::Account {
            id: account_id.into(),
            host: "misskey.test".into(),
            username: "me".into(),
            user_id: "u1".into(),
            display_name: "Me".into(),
            avatar_url: None,
            instance: None,
            is_cat: false,
            avatar_blurhash: None,
        });
        self.secrets.set(account_id, "token").unwrap();
        self.refresh_fetch_allowlist();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Account;

    #[test]
    fn server_version_is_remembered_per_account_and_forgettable() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        assert_eq!(state.server_version("a1"), None);

        state.set_server_version("a1", "2026.9.1".into());
        assert_eq!(state.server_version("a1").as_deref(), Some("2026.9.1"));
        assert_eq!(state.server_version("a2"), None);

        state.forget_server_version("a1");
        assert_eq!(state.server_version("a1"), None);
    }

    #[test]
    fn translator_available_is_remembered_per_account_and_forgettable() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        assert_eq!(state.translator_available("a1"), None);

        state.set_translator_available("a1", true);
        state.set_translator_available("a2", false);
        assert_eq!(state.translator_available("a1"), Some(true));
        assert_eq!(state.translator_available("a2"), Some(false));

        state.forget_translator_available("a1");
        assert_eq!(state.translator_available("a1"), None);
        assert_eq!(state.translator_available("a2"), Some(false));
    }

    #[test]
    fn restores_persisted_accounts_on_construction() {
        let settings = SettingsStore::new_in_memory();
        settings
            .upsert_account(&Account {
                id: "acc1".into(),
                host: "misskey.io".into(),
                username: "me".into(),
                user_id: "u1".into(),
                display_name: "Me".into(),
                avatar_url: None,
                instance: None,
                is_cat: false,
                avatar_blurhash: None,
            })
            .unwrap();

        // 「再起動」相当: 既存 DB から AppState を作り直す
        let state = AppState::new_for_test(settings);
        let mgr = state.accounts.lock().unwrap();
        assert_eq!(mgr.list().len(), 1);
        assert_eq!(mgr.active_id(), Some("acc1")); // 先頭が active
    }

    #[test]
    fn refresh_fetch_allowlist_follows_registered_accounts() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        assert!(!state.fetch_allowlist.contains_key("misskey.test"));

        state.register_test_account("acc1");
        assert!(state.fetch_allowlist.contains_key("misskey.test"));

        state.accounts.lock().unwrap().remove("acc1").unwrap();
        state.refresh_fetch_allowlist();
        assert!(!state.fetch_allowlist.contains_key("misskey.test"));
    }

    #[test]
    fn is_word_muted_false_before_sync_and_true_after() {
        use crate::domain::{User, Visibility};
        use crate::filter::mute::WordMuteRule;

        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        let note = crate::domain::Note {
            id: "n1".into(),
            created_at: 0,
            text: Some("spoiler here".into()),
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
            reply: None,
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
        };

        assert!(!state.is_word_muted("acc1", &note)); // 未同期なら常に false
        state.set_server_word_mutes("acc1", vec![WordMuteRule::Words(vec!["spoiler".into()])]);
        assert!(state.is_word_muted("acc1", &note));
        assert!(!state.is_word_muted("other-acc", &note)); // 別アカウントには影響しない
    }

    #[test]
    fn cache_metrics_record_backfill_increments_the_matching_counter() {
        let m = CacheMetrics::default();
        m.record_backfill(BackfillOutcome::Hit);
        m.record_backfill(BackfillOutcome::Hit);
        m.record_backfill(BackfillOutcome::FallbackBoundaryUnset);
        m.record_backfill(BackfillOutcome::FallbackOther);

        assert_eq!(m.backfill_hit(), 2);
        assert_eq!(m.backfill_fallback_boundary(), 1);
        assert_eq!(m.backfill_fallback_other(), 1);
    }

    #[test]
    fn cache_metrics_record_resume_increments_hit_or_fallback() {
        let m = CacheMetrics::default();
        m.record_resume(true);
        m.record_resume(true);
        m.record_resume(false);

        assert_eq!(m.resume_hit(), 2);
        assert_eq!(m.resume_fallback(), 1);
    }

    #[test]
    fn server_mute_sync_lock_is_shared_per_account_and_separate_across_accounts() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());

        let a1 = state.server_mute_sync_lock("a1");
        let a1_again = state.server_mute_sync_lock("a1");
        let a2 = state.server_mute_sync_lock("a2");

        assert!(std::sync::Arc::ptr_eq(&a1, &a1_again));
        assert!(!std::sync::Arc::ptr_eq(&a1, &a2));
    }

    #[test]
    fn forget_server_mute_sync_lock_drops_the_entry() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        let before = state.server_mute_sync_lock("a1");

        state.forget_server_mute_sync_lock("a1");

        assert!(!std::sync::Arc::ptr_eq(&before, &state.server_mute_sync_lock("a1")));
    }
}
