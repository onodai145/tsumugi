//! SQLite 接続とスキーマ初期化。設定（Account/Column）とノートキャッシュを永続化する。
//! ノートキャッシュは TQL§9 の正規化スキーマ（SQL 射影の前提）＋表示復元用の payload(JSON)。

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

-- ノートキャッシュ（TQL§9）。SQL 射影用の正規化カラム＋表示復元用 payload。
CREATE TABLE IF NOT EXISTS note (
    id            TEXT PRIMARY KEY,
    created_at    INTEGER NOT NULL,
    text          TEXT,
    text_length   INTEGER NOT NULL DEFAULT 0,
    cw            TEXT,
    visibility    TEXT NOT NULL,
    local_only    INTEGER NOT NULL DEFAULT 0,
    user_id       TEXT NOT NULL,
    reply_id      TEXT,
    reply_user_id TEXT,
    renote_id     TEXT,
    channel_id    TEXT,
    via           TEXT,
    lang          TEXT,
    files_count   INTEGER NOT NULL DEFAULT 0,
    has_poll      INTEGER NOT NULL DEFAULT 0,
    has_link      INTEGER NOT NULL DEFAULT 0,
    is_pinned     INTEGER NOT NULL DEFAULT 0,
    reaction_count     INTEGER NOT NULL DEFAULT 0,
    renote_count       INTEGER NOT NULL DEFAULT 0,
    reply_count        INTEGER NOT NULL DEFAULT 0,
    my_reaction        TEXT,
    is_renoted_by_me   INTEGER NOT NULL DEFAULT 0,
    is_favorited_by_me INTEGER NOT NULL DEFAULT 0,
    payload       TEXT NOT NULL     -- 完全な domain::Note の JSON（表示復元用）
);
CREATE INDEX IF NOT EXISTS idx_note_created ON note(created_at);
CREATE INDEX IF NOT EXISTS idx_note_user ON note(user_id);

CREATE TABLE IF NOT EXISTS user (
    id TEXT PRIMARY KEY, username TEXT NOT NULL, host TEXT, name TEXT,
    is_bot INTEGER NOT NULL DEFAULT 0, is_cat INTEGER NOT NULL DEFAULT 0,
    followers_count INTEGER NOT NULL DEFAULT 0,
    following_count INTEGER NOT NULL DEFAULT 0,
    notes_count     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS note_reaction (note_id TEXT, emoji_key TEXT, count INTEGER);
CREATE TABLE IF NOT EXISTS note_tag      (note_id TEXT, tag TEXT);
CREATE TABLE IF NOT EXISTS note_mention  (note_id TEXT, user_id TEXT);
CREATE TABLE IF NOT EXISTS note_emoji    (note_id TEXT, emoji TEXT);
CREATE TABLE IF NOT EXISTS note_file     (note_id TEXT, mime_type TEXT, mime_category TEXT, is_sensitive INTEGER);
CREATE INDEX IF NOT EXISTS idx_nr_note ON note_reaction(note_id);
CREATE INDEX IF NOT EXISTS idx_nt_note ON note_tag(note_id);
CREATE INDEX IF NOT EXISTS idx_nm_note ON note_mention(note_id);
CREATE INDEX IF NOT EXISTS idx_ne_note ON note_emoji(note_id);
CREATE INDEX IF NOT EXISTS idx_nf_note ON note_file(note_id);

-- どのカラムにどのノートが流れたか（起動時の即時復元用）
CREATE TABLE IF NOT EXISTS column_note (
    column_id   TEXT NOT NULL,
    note_id     TEXT NOT NULL,
    received_at INTEGER NOT NULL,
    PRIMARY KEY (column_id, note_id)
);
CREATE INDEX IF NOT EXISTS idx_cn_column ON column_note(column_id);
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
