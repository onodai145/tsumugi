//! Tauri が管理するアプリ状態（command から `State<AppState>` で参照）。

use crate::domain::{EmojiDef, MuteConfig, Note};
use crate::filter::mute::WordMuteRule;
use crate::session::{AccountManager, SecretStore};
use crate::sound::SoundPlayer;
use crate::store::{DraftStore, NoteCacheStore, SettingsStore};
use crate::stream::ConnectionManager;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

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

pub struct AppState {
    pub http: reqwest::Client,
    pub accounts: Mutex<AccountManager>,
    pub secrets: Box<dyn SecretStore>,
    pub pending: Mutex<HashMap<String, PendingMiAuth>>,
    pub connections: ConnectionManager,
    /// host -> カスタム絵文字一覧（インスタンス単位でキャッシュ）
    pub emoji_cache: Mutex<HashMap<String, Vec<EmojiDef>>>,
    /// ローカル NG（ミュート）設定。ストリーム/REST の受信ノートに適用する
    pub mute: Mutex<MuteConfig>,
    /// account_id -> サーバ側でミュート/ブロックしているユーザの userId 集合。
    /// 起動時/アカウント追加時に同期し、受信ノート・通知の抑制に使う（Krile MuteBlockManager 相当）。
    pub server_mutes: Mutex<HashMap<String, HashSet<String>>>,
    /// account_id -> サーバ側ワードミュート(mutedWords)のルール一覧。
    /// server_mutes と同じタイミングで同期し、ノート本文/CWの追加フィルタに使う(Issue #11)。
    pub server_word_mutes: Mutex<HashMap<String, Vec<WordMuteRule>>>,
    pub settings: SettingsStore,
    pub drafts: DraftStore,
    pub cache: NoteCacheStore,
    /// 再接続ギャップ埋め(Issue #147)が実行中の column_id 集合。フラッピング再接続で同一
    /// カラムに対する多重実行を防ぐためのガード（commands/column.rs 側で挿入/削除する）。
    pub gap_fill_in_flight: Mutex<HashSet<String>>,
    /// 通知音のネイティブ再生(Issue #12)。
    pub sound: SoundPlayer,
}

impl AppState {
    /// 永続化済みアカウントを読み込んで初期化する。
    pub fn new(
        secrets: Box<dyn SecretStore>,
        settings: SettingsStore,
        drafts: DraftStore,
        cache: NoteCacheStore,
    ) -> Self {
        Self::new_with_sound(secrets, settings, drafts, cache, SoundPlayer::spawn())
    }

    /// `sound` フィールドの構築方法を差し替え可能にした内部コンストラクタ。
    /// 本番経路は `new`(実デバイスを開くスレッドを立てる)、テスト経路は
    /// `new_for_test`(何も立てない `SoundPlayer::new_for_test`)から呼ばれる。
    fn new_with_sound(
        secrets: Box<dyn SecretStore>,
        settings: SettingsStore,
        drafts: DraftStore,
        cache: NoteCacheStore,
        sound: SoundPlayer,
    ) -> Self {
        let accounts = settings.load_accounts().unwrap_or_else(|e| {
            log::error!("failed to load accounts: {e}");
            Vec::new()
        });
        let mute = settings.load_mute().unwrap_or_default();
        Self {
            http: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .expect("failed to build reqwest client"),
            accounts: Mutex::new(AccountManager::with_accounts(accounts)),
            secrets,
            pending: Mutex::new(HashMap::new()),
            connections: ConnectionManager::default(),
            emoji_cache: Mutex::new(HashMap::new()),
            mute: Mutex::new(mute),
            server_mutes: Mutex::new(HashMap::new()),
            server_word_mutes: Mutex::new(HashMap::new()),
            settings,
            drafts,
            cache,
            gap_fill_in_flight: Mutex::new(HashSet::new()),
            sound,
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

    #[cfg(test)]
    /// テスト用: keyring を使わずインメモリ DB で構築する。他モジュールのテストからも使う。
    /// `sound` は実デバイスを開くスレッドを立てない `SoundPlayer::new_for_test` を使う
    /// (ヘッドレス CI でのテストごとのデバイスプローブ/ALSA ノイズを避けるため)。
    pub(crate) fn new_for_test(settings: SettingsStore) -> Self {
        let cache = NoteCacheStore::new(crate::store::db::open_cache_in_memory().unwrap());
        Self::new_with_sound(
            Box::new(crate::session::MemoryStore::default()),
            settings,
            DraftStore::new_in_memory(),
            cache,
            SoundPlayer::new_for_test(),
        )
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
        Ok(crate::api::MisskeyClient::new(
            self.http.clone(),
            host,
            Some(token),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Account;

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
            })
            .unwrap();

        // 「再起動」相当: 既存 DB から AppState を作り直す
        let state = AppState::new_for_test(settings);
        let mgr = state.accounts.lock().unwrap();
        assert_eq!(mgr.list().len(), 1);
        assert_eq!(mgr.active_id(), Some("acc1")); // 先頭が active
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
        };

        assert!(!state.is_word_muted("acc1", &note)); // 未同期なら常に false
        state.set_server_word_mutes("acc1", vec![WordMuteRule::Words(vec!["spoiler".into()])]);
        assert!(state.is_word_muted("acc1", &note));
        assert!(!state.is_word_muted("other-acc", &note)); // 別アカウントには影響しない
    }
}
