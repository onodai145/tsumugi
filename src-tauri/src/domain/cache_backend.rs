use serde::{Deserialize, Serialize};
use specta::Type;

/// note cacheのバックエンド選択(Issue #115 Phase 2)。パスワードはここに含まず、
/// OS keyringへ別途保存する(`session`モジュール参照)。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CacheBackendConfig {
    Sqlite,
    Postgres { host: String, port: u16, database: String, user: String },
}

impl Default for CacheBackendConfig {
    fn default() -> Self {
        CacheBackendConfig::Sqlite
    }
}
