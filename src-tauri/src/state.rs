//! Tauri が管理するアプリ状態（command から `State<AppState>` で参照）。

use crate::session::{AccountManager, SecretStore};
use std::collections::HashMap;
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
}

impl AppState {
    pub fn new(secrets: Box<dyn SecretStore>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .user_agent(concat!("tsumugi/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("failed to build reqwest client"),
            accounts: Mutex::new(AccountManager::default()),
            secrets,
            pending: Mutex::new(HashMap::new()),
        }
    }
}
