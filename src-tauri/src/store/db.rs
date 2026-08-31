//! SQLite 接続とスキーマ初期化。設定（Account/Column）とノートキャッシュは別ファイルに分離し、
//! それぞれ `open_settings` / `open_cache` で開く（バックアップ対象を小さな設定ファイルに絞るため）。
//! ノートキャッシュは TQL§9 の正規化スキーマ（SQL 射影の前提）＋表示復元用の payload(JSON)。

use crate::error::Result;
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

/// 設定スキーマ（Account/Column/汎用設定）。将来の移行は `migrate()` で管理する。
const SETTINGS_SCHEMA: &str = r#"
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
    width INTEGER NOT NULL,
    auto  INTEGER NOT NULL DEFAULT 0
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
    title          TEXT,            -- ユーザ設定のタブ名（NULL=自動生成名）
    notify_sound_choice TEXT NOT NULL DEFAULT ''  -- プリセットIDまたはdata URL。空=グローバル継承
);
CREATE INDEX IF NOT EXISTS idx_column_account ON column_def(account_id);

-- 汎用 key-value 設定（NG設定などの JSON を格納）
CREATE TABLE IF NOT EXISTS app_setting (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

/// ノートキャッシュスキーマ（TQL§9）。SQL 射影用の正規化カラム＋表示復元用 payload。
/// 破棄しても再取得で復元できるため、設定と異なりマイグレーションは持たない。
const CACHE_SCHEMA: &str = r#"
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
    created_at  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (column_id, note_id)
);
CREATE INDEX IF NOT EXISTS idx_cn_column ON column_note(column_id);

-- カラムごとの「これより新しいノートはAPI取得済みで完全」境界（Issue #228）
CREATE TABLE IF NOT EXISTS column_fetch_boundary (
    column_id         TEXT PRIMARY KEY,
    oldest_fetched_id TEXT NOT NULL
);
"#;

/// 設定DBを開き（無ければ作成し）、スキーマを適用してマイグレーションを行う。
pub fn open_settings(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(SETTINGS_SCHEMA)?;
    migrate(&conn)?;
    Ok(conn)
}

/// ノートキャッシュDBを開き（無ければ作成し）、スキーマを適用する。
pub fn open_cache(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "cache_size", -20_000i64)?;
    conn.pragma_update(None, "mmap_size", 67_108_864i64)?;
    conn.execute_batch(CACHE_SCHEMA)?;
    migrate_cache(&conn)?;
    enable_incremental_vacuum(&conn)?;
    Ok(conn)
}

/// サイズ上限による間引き(Issue #6)は DELETE だけでは解放ページがファイル内に残ってしまい
/// 実際のファイルサイズが縮まらないため、incremental_vacuum で明示的に回収できるモードへ
/// 切り替えておく。新規DBは即座に反映されるが、既に非空のDBでは一度 VACUUM しないと
/// auto_vacuum の変更が反映されないため、未設定の場合だけ実行する（初回のみの一過性コスト）。
fn enable_incremental_vacuum(conn: &Connection) -> Result<()> {
    let mode: i64 = conn.query_row("PRAGMA auto_vacuum", [], |r| r.get(0))?;
    const INCREMENTAL: i64 = 2;
    if mode != INCREMENTAL {
        conn.pragma_update(None, "auto_vacuum", "INCREMENTAL")?;
        conn.execute_batch("VACUUM")?;
    }
    Ok(())
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
    // タブごとの通知音選択（無い旧 DB には追加。既定は空文字＝グローバル継承）
    if !column_exists(conn, "column_def", "notify_sound_choice")? {
        conn.execute_batch(
            "ALTER TABLE column_def ADD COLUMN notify_sound_choice TEXT NOT NULL DEFAULT ''",
        )?;
    }
    // カラム幅の固定/自動調整（無い旧 DB には追加。既定は0=固定、従来どおり）
    if !column_exists(conn, "column_group", "auto")? {
        conn.execute_batch("ALTER TABLE column_group ADD COLUMN auto INTEGER NOT NULL DEFAULT 0")?;
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

/// `column_note` にソート用の `created_at` を非正規化し、`load_cached` が
/// カラム別に「新しい順」をインデックスだけで取り出せるようにする。
/// 既存DBには列が無いため、`note` から逆算してバックフィルしてからインデックスを張る。
/// 新規DBでは `column_exists` が true を返しバックフィルはスキップされ、インデックス作成のみ行う。
fn migrate_cache(conn: &Connection) -> Result<()> {
    if !column_exists(conn, "column_note", "created_at")? {
        conn.execute_batch("ALTER TABLE column_note ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0")?;
        conn.execute_batch(
            "UPDATE column_note SET created_at = (
                SELECT created_at FROM note WHERE note.id = column_note.note_id
            )
            WHERE EXISTS (SELECT 1 FROM note WHERE note.id = column_note.note_id)",
        )?;
    }
    // Issue #263: user テーブルをフル正規化テーブルに格上げする列を追加。
    // note.payload に埋め込まれていたユーザー情報(instance含む)をここへ集約する。
    if !column_exists(conn, "user", "instance_name")? {
        conn.execute_batch(
            "ALTER TABLE user ADD COLUMN avatar_url TEXT;
             ALTER TABLE user ADD COLUMN bio TEXT;
             ALTER TABLE user ADD COLUMN banner_url TEXT;
             ALTER TABLE user ADD COLUMN emojis TEXT NOT NULL DEFAULT '{}';
             ALTER TABLE user ADD COLUMN instance_name TEXT;
             ALTER TABLE user ADD COLUMN instance_icon_url TEXT;
             ALTER TABLE user ADD COLUMN instance_theme_color TEXT;",
        )?;
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_cn_column_created \
         ON column_note(column_id, created_at DESC, note_id DESC)",
    )?;
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

/// インメモリキャッシュDB（テスト用）。
#[cfg(test)]
pub fn open_cache_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(CACHE_SCHEMA)?;
    migrate_cache(&conn)?;
    enable_incremental_vacuum(&conn)?;
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
        conn.execute_batch(SETTINGS_SCHEMA).unwrap();
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
        conn.execute_batch(SETTINGS_SCHEMA).unwrap();
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

    #[test]
    fn migrate_cache_backfills_created_at_from_note() {
        let conn = Connection::open_in_memory().unwrap();
        // 旧スキーマ（column_note に created_at 列が無い状態）を模倣
        conn.execute_batch(
            "CREATE TABLE note (id TEXT PRIMARY KEY, created_at INTEGER NOT NULL);
             CREATE TABLE user (id TEXT PRIMARY KEY, username TEXT NOT NULL);
             CREATE TABLE column_note (
                 column_id TEXT NOT NULL, note_id TEXT NOT NULL, received_at INTEGER NOT NULL,
                 PRIMARY KEY (column_id, note_id)
             );
             INSERT INTO note (id, created_at) VALUES ('n1', 12345);
             INSERT INTO column_note (column_id, note_id, received_at) VALUES ('c1', 'n1', 999);",
        )
        .unwrap();

        migrate_cache(&conn).unwrap();

        let created_at: i64 = conn
            .query_row(
                "SELECT created_at FROM column_note WHERE column_id='c1' AND note_id='n1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(created_at, 12345);

        // 冪等: 再実行してもエラーにならず値は変わらない
        migrate_cache(&conn).unwrap();
        let created_at2: i64 = conn
            .query_row(
                "SELECT created_at FROM column_note WHERE column_id='c1' AND note_id='n1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(created_at2, 12345);

        // idx_cn_column_created が作成されていること
        let idx_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_cn_column_created'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx_count, 1);
    }

    #[test]
    fn open_cache_applies_pragma_tuning() {
        let path = std::env::temp_dir().join(format!("tsumugi_test_{}.db", uuid::Uuid::new_v4()));
        let conn = open_cache(&path).unwrap();

        let synchronous: i64 = conn.query_row("PRAGMA synchronous", [], |r| r.get(0)).unwrap();
        assert_eq!(synchronous, 1); // NORMAL

        let temp_store: i64 = conn.query_row("PRAGMA temp_store", [], |r| r.get(0)).unwrap();
        assert_eq!(temp_store, 2); // MEMORY

        let cache_size: i64 = conn.query_row("PRAGMA cache_size", [], |r| r.get(0)).unwrap();
        assert_eq!(cache_size, -20000);

        let mmap_size: i64 = conn.query_row("PRAGMA mmap_size", [], |r| r.get(0)).unwrap();
        assert_eq!(mmap_size, 67_108_864);

        drop(conn);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn migrate_cache_adds_user_normalization_columns() {
        let conn = Connection::open_in_memory().unwrap();
        // 列追加前の旧 user テーブル
        conn.execute_batch(
            "CREATE TABLE user (
                id TEXT PRIMARY KEY, username TEXT NOT NULL, host TEXT, name TEXT,
                is_bot INTEGER NOT NULL DEFAULT 0, is_cat INTEGER NOT NULL DEFAULT 0,
                followers_count INTEGER NOT NULL DEFAULT 0,
                following_count INTEGER NOT NULL DEFAULT 0,
                notes_count INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE note (id TEXT PRIMARY KEY, created_at INTEGER NOT NULL);
            CREATE TABLE column_note (
                column_id TEXT NOT NULL, note_id TEXT NOT NULL, received_at INTEGER NOT NULL,
                PRIMARY KEY (column_id, note_id)
            );",
        )
        .unwrap();

        migrate_cache(&conn).unwrap();

        for col in [
            "avatar_url",
            "bio",
            "banner_url",
            "emojis",
            "instance_name",
            "instance_icon_url",
            "instance_theme_color",
        ] {
            assert!(column_exists(&conn, "user", col).unwrap(), "missing column: {col}");
        }
        // 冪等: 2回目呼んでもエラーにならない
        migrate_cache(&conn).unwrap();
    }
}
