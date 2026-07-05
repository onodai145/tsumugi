//! SQLite 接続とスキーマ初期化。設定（Account/Column）を永続化する。
//! ノートキャッシュ（NQL§9）は将来 Phase 6 で同じ DB に追加する。

use crate::error::Result;
use rusqlite::Connection;
use std::path::Path;

/// 現行スキーマ。将来の移行は `user_version` で管理する。
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS account (
    id            TEXT PRIMARY KEY,
    host          TEXT NOT NULL,
    username      TEXT NOT NULL,
    user_id       TEXT NOT NULL,
    display_name  TEXT NOT NULL,
    avatar_url    TEXT
);

CREATE TABLE IF NOT EXISTS column_def (
    id             TEXT PRIMARY KEY,
    account_id     TEXT NOT NULL,
    kind           TEXT NOT NULL,   -- ColumnKind の JSON
    ord            INTEGER NOT NULL,
    width          INTEGER NOT NULL,
    filter         TEXT NOT NULL,   -- FilterQuery の JSON
    notify_sound   INTEGER NOT NULL,
    notify_desktop INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_column_account ON column_def(account_id);
"#;

/// DB を開き（無ければ作成し）、スキーマを適用する。
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

/// インメモリ DB（テスト用）。
#[cfg(test)]
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}
