//! SQLite 接続とスキーマ初期化。設定（Account/Column）とノートキャッシュを永続化する。
//! ノートキャッシュは TQL§9 の正規化スキーマ（SQL 射影の前提）＋表示復元用の payload(JSON)。

use crate::error::Result;
use rusqlite::{Connection, OptionalExtension};
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

-- 視覚的なカラム（タブの集合）。幅と並び順を持つ。
CREATE TABLE IF NOT EXISTS column_group (
    id    TEXT PRIMARY KEY,
    ord   INTEGER NOT NULL,
    width INTEGER NOT NULL
);

-- タブ（1タイムライン）。group_id で視覚カラムに属し、ord はグループ内順序。
CREATE TABLE IF NOT EXISTS column_def (
    id             TEXT PRIMARY KEY,
    account_id     TEXT NOT NULL,
    kind           TEXT NOT NULL,   -- ColumnKind の JSON
    ord            INTEGER NOT NULL,
    width          INTEGER NOT NULL,  -- 旧: カラム幅（現在は column_group.width が正）
    filter         TEXT NOT NULL,   -- FilterQuery の JSON
    notify_sound   INTEGER NOT NULL,
    notify_desktop INTEGER NOT NULL,
    group_id       TEXT,            -- 所属する column_group.id
    title          TEXT             -- ユーザ設定のタブ名（NULL=自動生成名）
);
CREATE INDEX IF NOT EXISTS idx_column_account ON column_def(account_id);

-- 汎用 key-value 設定（NG設定などの JSON を格納）
CREATE TABLE IF NOT EXISTS app_setting (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

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

/// DB を開き（無ければ作成し）、スキーマを適用してマイグレーションを行う。
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(SCHEMA)?;
    migrate(&conn)?;
    Ok(conn)
}

/// 旧スキーマ（group_id 無し）からの移行。既存カラムを各自 1 グループへ割り当てる。
fn migrate(conn: &Connection) -> Result<()> {
    // 既存 DB で group_id 列が無ければ追加（列追加後にインデックスを張る）
    if !column_exists(conn, "column_def", "group_id")? {
        conn.execute_batch("ALTER TABLE column_def ADD COLUMN group_id TEXT")?;
    }
    conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_column_group ON column_def(group_id)")?;
    // タブのカスタム名（無い旧 DB には追加）
    if !column_exists(conn, "column_def", "title")? {
        conn.execute_batch("ALTER TABLE column_def ADD COLUMN title TEXT")?;
    }
    // group_id が未設定のタブを、それぞれ新規グループへ（新規 DB では該当なし）
    let orphans: Vec<(String, i32, i32)> = {
        let mut stmt =
            conn.prepare("SELECT id, ord, width FROM column_def WHERE group_id IS NULL")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (id, ord, width) in orphans {
        let gid = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO column_group (id, ord, width) VALUES (?1, ?2, ?3)",
            rusqlite::params![gid, ord, width],
        )?;
        conn.execute(
            "UPDATE column_def SET group_id = ?1, ord = 0 WHERE id = ?2",
            rusqlite::params![gid, id],
        )?;
    }

    // notify_sound/notify_desktop 列は元々未実装で常に false のまま保存されていた。
    // 通知種別(Notifications)カラムは「通知カラムがあれば全部鳴る」というグローバル挙動
    // だったので、それをタブ単位のフィルタとして実際に使うようにした今、既存ユーザの
    // 見た目（＝これまで通り全部通知される）を壊さないよう通知カラムに限り一度だけ
    // true に migrate する。他種別(Home/List等)は今回追加した新機能なので false のまま
    // （新規タブと同じオプトイン）。以後はユーザ操作で変わり得るので再実行しないよう
    // マーカーを立てる。
    let migrated: Option<String> = conn
        .query_row(
            "SELECT value FROM app_setting WHERE key = 'notify_flags_migrated_v1'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if migrated.is_none() {
        conn.execute_batch(
            "UPDATE column_def SET notify_sound = 1, notify_desktop = 1
             WHERE json_extract(kind, '$.type') = 'notifications'",
        )?;
        conn.execute(
            "INSERT INTO app_setting (key, value) VALUES ('notify_flags_migrated_v1', '1')",
            [],
        )?;
    }
    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
    for n in names {
        if n? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// インメモリ DB（テスト用）。
#[cfg(test)]
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(SCHEMA)?;
    migrate(&conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_old_column_def_to_groups() {
        let conn = Connection::open_in_memory().unwrap();
        // 旧スキーマ（group_id 無し）＋既存カラム1件
        conn.execute_batch(
            "CREATE TABLE column_def (
                id TEXT PRIMARY KEY, account_id TEXT NOT NULL, kind TEXT NOT NULL,
                ord INTEGER NOT NULL, width INTEGER NOT NULL, filter TEXT NOT NULL,
                notify_sound INTEGER NOT NULL, notify_desktop INTEGER NOT NULL);
             INSERT INTO column_def VALUES('c1','a1','{}',2,360,'{}',0,0);",
        )
        .unwrap();
        // 新スキーマ適用（column_def は IF NOT EXISTS で維持、column_group は作成）＋移行
        conn.execute_batch(SCHEMA).unwrap();
        migrate(&conn).unwrap();

        // タブに group_id が付与され、グループが作られている
        let gid: Option<String> = conn
            .query_row("SELECT group_id FROM column_def WHERE id='c1'", [], |r| r.get(0))
            .unwrap();
        let gid = gid.expect("group_id should be set");
        let (gord, gwidth): (i32, i32) = conn
            .query_row("SELECT ord, width FROM column_group WHERE id=?1", [&gid], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(gord, 2); // 旧 ord をグループ順に引き継ぐ
        assert_eq!(gwidth, 360); // 旧 width をグループ幅に
        let tab_ord: i32 = conn
            .query_row("SELECT ord FROM column_def WHERE id='c1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tab_ord, 0); // グループ内では単独タブ

        // 冪等: 再度 migrate しても増えない
        migrate(&conn).unwrap();
        let groups: i32 = conn
            .query_row("SELECT COUNT(*) FROM column_group", [], |r| r.get(0))
            .unwrap();
        assert_eq!(groups, 1);
    }

    #[test]
    fn migrates_notify_flags_to_true_once() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE column_def (
                id TEXT PRIMARY KEY, account_id TEXT NOT NULL, kind TEXT NOT NULL,
                ord INTEGER NOT NULL, width INTEGER NOT NULL, filter TEXT NOT NULL,
                notify_sound INTEGER NOT NULL, notify_desktop INTEGER NOT NULL);
             INSERT INTO column_def VALUES('c1','a1','{\"type\":\"notifications\"}',0,300,'{}',0,0);
             INSERT INTO column_def VALUES('c2','a1','{\"type\":\"home\"}',1,300,'{}',0,0);",
        )
        .unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        migrate(&conn).unwrap();

        // 通知カラム(c1)は旧「常にfalse」から一度だけ true へ移行
        let (sound, desktop): (i64, i64) = conn
            .query_row(
                "SELECT notify_sound, notify_desktop FROM column_def WHERE id='c1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((sound, desktop), (1, 1));

        // 通知カラム以外(c2)は新機能のため false のまま（オプトイン）
        let (sound2, desktop2): (i64, i64) = conn
            .query_row(
                "SELECT notify_sound, notify_desktop FROM column_def WHERE id='c2'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((sound2, desktop2), (0, 0));

        // ユーザが明示的に false へ戻した後、再 migrate しても上書きされない（冪等）
        conn.execute_batch("UPDATE column_def SET notify_sound = 0 WHERE id='c1'").unwrap();
        migrate(&conn).unwrap();
        let sound: i64 = conn
            .query_row("SELECT notify_sound FROM column_def WHERE id='c1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sound, 0);
    }
}
