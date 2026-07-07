//! Tauri が管理するアプリ状態（command から `State<AppState>` で参照）。

use crate::domain::{EmojiDef, MuteConfig};
use crate::session::{AccountManager, SecretStore};
use crate::store::SettingsStore;
use crate::stream::ConnectionManager;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

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
    pub settings: SettingsStore,
}

impl AppState {
    /// 永続化済みアカウントを読み込んで初期化する。
    pub fn new(secrets: Box<dyn SecretStore>, settings: SettingsStore) -> Self {
        let accounts = settings.load_accounts().unwrap_or_else(|e| {
            log::error!("failed to load accounts: {e}");
            Vec::new()
        });
        let mute = settings.load_mute().unwrap_or_default();
        Self {
            http: reqwest::Client::builder()
                .user_agent(concat!("tsumugi/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("failed to build reqwest client"),
            accounts: Mutex::new(AccountManager::with_accounts(accounts)),
            secrets,
            pending: Mutex::new(HashMap::new()),
            connections: ConnectionManager::default(),
            emoji_cache: Mutex::new(HashMap::new()),
            mute: Mutex::new(mute),
            server_mutes: Mutex::new(HashMap::new()),
            settings,
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

    #[cfg(test)]
    /// テスト用: keyring を使わずインメモリ DB で構築する。
    fn new_for_test(settings: SettingsStore) -> Self {
        Self::new(Box::new(crate::session::MemoryStore::default()), settings)
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
    use crate::store::db::open_in_memory;

    #[test]
    fn restores_persisted_accounts_on_construction() {
        let settings = SettingsStore::new(open_in_memory().unwrap());
        settings
            .upsert_account(&Account {
                id: "acc1".into(),
                host: "misskey.io".into(),
                username: "me".into(),
                user_id: "u1".into(),
                display_name: "Me".into(),
                avatar_url: None,
            })
            .unwrap();

        // 「再起動」相当: 既存 DB から AppState を作り直す
        let state = AppState::new_for_test(settings);
        let mgr = state.accounts.lock().unwrap();
        assert_eq!(mgr.list().len(), 1);
        assert_eq!(mgr.active_id(), Some("acc1")); // 先頭が active
    }
}
