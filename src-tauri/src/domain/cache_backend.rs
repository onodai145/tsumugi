use serde::{Deserialize, Serialize};
use specta::Type;

/// note cacheのバックエンド選択(Issue #115 Phase 2/3)。パスワードはここに含まず、
/// OS keyringへ別途保存する(`session`モジュール参照。Postgres/MySQLで単一スロットを共用)。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CacheBackendConfig {
    #[default]
    Sqlite,
    Postgres {
        host: String,
        port: u16,
        database: String,
        user: String,
    },
    MySql {
        host: String,
        port: u16,
        database: String,
        user: String,
    },
}
