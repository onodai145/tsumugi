# instance テーブル正規化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** キャッシュDBの `user.instance_name` / `instance_icon_url` / `instance_theme_color` を、`host` をキーにした `instance` テーブルへ切り出す(SQLite / PostgreSQL / MySQL 全バックエンド、既存データの一度きりの移行つき)。

**Architecture:** 書き込みは「`instance` を先に upsert → `user` を upsert」、読み出しは `user LEFT JOIN instance ON host`。移行は「旧 `user.instance_name` 列がまだ存在する」ことを未移行マーカーとし、競合時に何もしない INSERT…SELECT でコピーしてから3列を DROP する(再実行しても再受信した新しい値を潰さない)。`domain::InstanceInfo` / `domain::User` / フロント / TQL は変更しない。

**Tech Stack:** Rust、rusqlite 0.40.1(bundled)、sqlx 0.9(postgres/mysql)、sea-query、testcontainers(PostgreSQL/MySQL の実DBテストは `#[ignore]`、Docker 必須)。

**Spec:** `docs/superpowers/specs/2026-09-29-instance-table-normalization-design.md`

## Global Constraints

- `instance` テーブル: SQLite/PostgreSQL は `host TEXT PRIMARY KEY, name TEXT, icon_url TEXT, theme_color TEXT`。MySQL は `host VARCHAR(255) PRIMARY KEY`(他3列は TEXT)。
- 書き込みは `user.instance` が `Some` **かつ** `user.host` が `Some` のときだけ `instance` を upsert する(ローカルユーザーは `instance` 行を作らない)。
- `upsert_user`(ライブ経路)は列ごとに `COALESCE(新, 既存)`、`fill_user_from_snapshot`(自己修復経路)は列ごとに `COALESCE(既存, 新)`。
- `InstanceInfo` を `Some` にする条件は現行のまま(3列のいずれかが非NULL)。
- 移行は旧 `user.instance_name` 列の有無で判定し、競合時は何もしない(`ON CONFLICT (host) DO NOTHING` / MySQL は `INSERT IGNORE`)。移行後に3列を `DROP COLUMN` する。同一ホストで旧行の値が食い違う場合は列ごとの `MAX` で1つに決める。
- SQLite の `migrate_cache` の Issue #263 ブロックは `instance_name` ではなく `emojis` の有無で判定し、`instance_*` を追加しない(DROP 後の再起動で全列を再追加しないため)。
- ダウングレード不可(旧バージョンが同じ PostgreSQL/MySQL に接続すると壊れる)は許容済み。
- コミットメッセージは subject 1行のみ(本文・箇条書き禁止)。セッションの帰属トレーラー(`Co-Authored-By: ...`)は別途付与する。
- `main` へ直接コミットしない(作業ブランチは `refactor/409-normalize-instance-table`)。
- `cargo test` を走らせるたびに `frontend/src/bindings/tauri.gen.ts` が再生成される。差分が出たら生成物なのでそのままコミットに含めず、`git status` で意図しない差分が無いか確認する(型は変えないので通常は差分なし)。

## Review Focus

1. ローカルユーザー(`host = NULL`)が `instance: Some` で upsert されても、孤児の `instance` 行を作らず、読み出しは `instance: None`(Task 1〜3 のテストが固定)。
2. 移行の再実行で、移行後に更新した `instance` の値が旧列の値で戻らない(Task 1〜3 のテストが固定)。
3. 同一ホストの旧行で値が食い違う/一部だけ NULL でも、`instance` は1行になりエラーにならない(Task 1〜3 のテストが固定)。
4. SQLite で DROP 後に `migrate_cache` を再実行しても `instance_*` 列が再追加されない(Task 1 のテストが固定)。
5. `instance` の一部の列だけが `Some` の後続受信(再取得の部分失敗)で、既知の他の列が消えない(Task 1〜3 のテストが固定)。

---

## File Structure

- `src-tauri/src/store/db.rs` — SQLite の `CACHE_SCHEMA` に `instance` を追加、`migrate_cache` の #263 ブロック修正と新しい `migrate_instance_table`。
- `src-tauri/src/store/user_ref.rs` — SQLite の `upsert_user` / `fill_user_from_snapshot` / `fetch_users_by_ids` と `upsert_instance` / `fill_instance`。
- `src-tauri/src/store/postgres_backend.rs` — `InstanceTable` Iden、`ensure_schema` の `instance` 作成と `migrate_instance_columns`、`UserTable` から `Instance*` を削除。
- `src-tauri/src/store/postgres_user_ref.rs` — PostgreSQL 版の同 3 関数と `upsert_instance` / `fill_instance`。
- `src-tauri/src/store/mysql_backend.rs` / `mysql_user_ref.rs` — MySQL 版(PostgreSQL と同じ構成)。

前提: 作業ブランチ `refactor/409-normalize-instance-table` 上で、リポジトリルートから作業する(`cd src-tauri` してテストを実行する)。

---

### Task 1: SQLite バックエンド

**Files:**
- Modify: `src-tauri/src/store/db.rs`(`CACHE_SCHEMA` 約84–90行、`migrate_cache` 約230–277行、テスト約498–532行)
- Modify: `src-tauri/src/store/user_ref.rs`(1–129行、209–256行、テスト部)

**Interfaces:**
- Consumes: なし
- Produces: SQLite の `instance(host, name, icon_url, theme_color)` テーブル。`user_ref::upsert_user` / `fill_user_from_snapshot` / `fetch_users_by_ids` のシグネチャは不変(`(conn: &Connection, ...) -> Result<...>`)。

- [ ] **Step 1: 失敗するテストを書く(`user_ref.rs` のテスト)**

`src-tauri/src/store/user_ref.rs` の `mod tests` 内、`fn row(...)` を次に置き換える(`instance_name` 列はもう `user` に無いので取らない)。

```rust
    fn row(conn: &Connection, id: &str) -> (Option<String>, Option<String>) {
        conn.query_row(
            "SELECT bio, banner_url FROM user WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    }

    fn remote_instance() -> InstanceInfo {
        InstanceInfo {
            name: Some("Remote".into()),
            icon_url: Some("https://remote.example/favicon.ico".into()),
            theme_color: Some("#ff8800".into()),
        }
    }

    fn fetch_one(conn: &Connection, id: &str) -> User {
        fetch_users_by_ids(conn, &[id.to_string()]).unwrap().remove(id).unwrap()
    }

    fn instance_row_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM instance", [], |r| r.get(0)).unwrap()
    }
```

`upsert_user_preserves_bio_when_later_write_has_none` の `let (bio, _, _) = row(&conn, "u1");` を `let (bio, _) = row(&conn, "u1");` に変える。

既存の `upsert_user_preserves_instance_when_later_fetch_fails` を次に置き換える。

```rust
    #[test]
    fn upsert_user_preserves_instance_when_later_fetch_fails() {
        let conn = open_cache_in_memory().unwrap();
        let mut with_instance = user_lite("u1", "Alice");
        with_instance.instance = Some(remote_instance());
        upsert_user(&conn, &with_instance).unwrap();

        // instance フェッチ失敗(null)の投稿を後から受信
        let mut failed_fetch = user_lite("u1", "Alice");
        failed_fetch.instance = None;
        upsert_user(&conn, &failed_fetch).unwrap();

        assert_eq!(fetch_one(&conn, "u1").instance, Some(remote_instance()));
    }
```

同じ `mod tests` の末尾(`upsert_user_and_fetch_roundtrips_avatar_blurhash` の後)に次のテストを追加する。

```rust
    #[test]
    fn upsert_user_shares_instance_across_users_of_same_host() {
        let conn = open_cache_in_memory().unwrap();
        let mut u1 = user_lite("u1", "Alice");
        u1.instance = Some(remote_instance());
        upsert_user(&conn, &u1).unwrap();
        upsert_user(&conn, &user_lite("u2", "Bob")).unwrap(); // instance無しでも同じhost

        assert_eq!(fetch_one(&conn, "u2").instance, Some(remote_instance()));
        assert_eq!(instance_row_count(&conn), 1, "同一ホストは1行に集約される");
    }

    #[test]
    fn upsert_user_propagates_instance_update_to_all_users_of_host() {
        let conn = open_cache_in_memory().unwrap();
        let mut u1 = user_lite("u1", "Alice");
        u1.instance = Some(remote_instance());
        upsert_user(&conn, &u1).unwrap();
        upsert_user(&conn, &user_lite("u2", "Bob")).unwrap();

        // 別ユーザー(同一ホスト)の受信で取得元が変わった(#406のような変更)
        let mut u3 = user_lite("u3", "Carol");
        u3.instance = Some(InstanceInfo {
            name: Some("Remote".into()),
            icon_url: Some("https://remote.example/new.png".into()),
            theme_color: Some("#ff8800".into()),
        });
        upsert_user(&conn, &u3).unwrap();

        for id in ["u1", "u2", "u3"] {
            assert_eq!(
                fetch_one(&conn, id).instance.unwrap().icon_url.as_deref(),
                Some("https://remote.example/new.png"),
                "{id} にも反映される"
            );
        }
    }

    #[test]
    fn upsert_user_keeps_known_instance_columns_when_later_value_is_partial() {
        let conn = open_cache_in_memory().unwrap();
        let mut full = user_lite("u1", "Alice");
        full.instance = Some(remote_instance());
        upsert_user(&conn, &full).unwrap();

        // 再取得の部分失敗: name だけ Some
        let mut partial = user_lite("u1", "Alice");
        partial.instance = Some(InstanceInfo {
            name: Some("Renamed".into()),
            icon_url: None,
            theme_color: None,
        });
        upsert_user(&conn, &partial).unwrap();

        let got = fetch_one(&conn, "u1").instance.unwrap();
        assert_eq!(got.name.as_deref(), Some("Renamed"));
        assert_eq!(got.icon_url.as_deref(), Some("https://remote.example/favicon.ico"));
        assert_eq!(got.theme_color.as_deref(), Some("#ff8800"));
    }

    #[test]
    fn upsert_user_does_not_store_instance_for_local_user() {
        let conn = open_cache_in_memory().unwrap();
        let mut local = user_lite("u1", "Alice");
        local.host = None;
        local.instance = Some(remote_instance());
        upsert_user(&conn, &local).unwrap();

        assert_eq!(instance_row_count(&conn), 0, "ローカルユーザーは instance 行を作らない");
        assert!(fetch_one(&conn, "u1").instance.is_none());
    }

    #[test]
    fn fill_user_from_snapshot_fills_missing_instance_but_keeps_existing() {
        let conn = open_cache_in_memory().unwrap();
        let mut fresh = user_lite("u1", "Alice");
        fresh.instance = Some(remote_instance());
        upsert_user(&conn, &fresh).unwrap();

        // 古いスナップショット(同一ホストの別ユーザー): iconもthemeも古い/別の値
        let mut stale = user_lite("u2", "Bob");
        stale.instance = Some(InstanceInfo {
            name: Some("Old".into()),
            icon_url: Some("https://remote.example/old.png".into()),
            theme_color: Some("#000000".into()),
        });
        fill_user_from_snapshot(&conn, &stale).unwrap();
        assert_eq!(
            fetch_one(&conn, "u2").instance,
            Some(remote_instance()),
            "既存値は古いスナップショットで上書きされない"
        );

        // 欠けている列だけは埋まる
        conn.execute("UPDATE instance SET theme_color = NULL WHERE host = 'remote.example'", [])
            .unwrap();
        fill_user_from_snapshot(&conn, &stale).unwrap();
        let got = fetch_one(&conn, "u2").instance.unwrap();
        assert_eq!(got.theme_color.as_deref(), Some("#000000"));
        assert_eq!(got.name.as_deref(), Some("Remote"));
    }
```

- [ ] **Step 2: 失敗するテストを書く(`db.rs` のテスト)**

`src-tauri/src/store/db.rs` の `migrate_cache_adds_user_normalization_columns` を次に置き換える(`instance_*` はもう追加されない)。

```rust
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

        for col in ["avatar_url", "bio", "banner_url", "emojis"] {
            assert!(column_exists(&conn, "user", col).unwrap(), "missing column: {col}");
        }
        // Issue #409: instance_* は instance テーブルへ移したため user には追加しない
        assert!(!column_exists(&conn, "user", "instance_name").unwrap());
        // 冪等: 2回目呼んでもエラーにならない
        migrate_cache(&conn).unwrap();
    }
```

同じ `mod tests` の `migrate_cache_adds_avatar_blurhash_column` の直後に次の2テストを追加する。

```rust
    #[test]
    fn open_cache_in_memory_has_instance_table_and_never_readds_instance_columns() {
        let conn = open_cache_in_memory().unwrap();
        assert!(column_exists(&conn, "instance", "host").unwrap());
        for col in ["instance_name", "instance_icon_url", "instance_theme_color"] {
            assert!(!column_exists(&conn, "user", col).unwrap(), "{col} は存在しない");
        }
        // 再起動相当: 再実行しても旧列が再追加されない
        migrate_cache(&conn).unwrap();
        for col in ["instance_name", "instance_icon_url", "instance_theme_color"] {
            assert!(!column_exists(&conn, "user", col).unwrap(), "{col} が再追加された");
        }
    }

    /// Issue #409: 旧 `user.instance_*` の値を `instance` テーブルへ一度だけ移し、旧列を消す。
    #[test]
    fn migrate_cache_moves_instance_columns_to_instance_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE user (
                id TEXT PRIMARY KEY, username TEXT NOT NULL, host TEXT, name TEXT,
                is_bot INTEGER NOT NULL DEFAULT 0, is_cat INTEGER NOT NULL DEFAULT 0,
                followers_count INTEGER NOT NULL DEFAULT 0,
                following_count INTEGER NOT NULL DEFAULT 0,
                notes_count INTEGER NOT NULL DEFAULT 0,
                avatar_url TEXT, bio TEXT, banner_url TEXT, emojis TEXT NOT NULL DEFAULT '{}',
                instance_name TEXT, instance_icon_url TEXT, instance_theme_color TEXT,
                avatar_blurhash TEXT
            );
            CREATE TABLE note (id TEXT PRIMARY KEY, created_at INTEGER NOT NULL);
            CREATE TABLE column_note (
                column_id TEXT NOT NULL, note_id TEXT NOT NULL, received_at INTEGER NOT NULL,
                PRIMARY KEY (column_id, note_id)
            );
            INSERT INTO user (id, username, host, instance_name, instance_icon_url, instance_theme_color) VALUES
              ('u1', 'a', 'remote.example', 'Remote', 'https://remote.example/favicon.ico', '#ff8800'),
              ('u2', 'b', 'remote.example', 'Remote', 'https://remote.example/favicon.ico', '#ff8800'),
              ('u3', 'c', 'conflict.example', 'Aaa', 'https://conflict.example/a.ico', NULL),
              ('u4', 'd', 'conflict.example', 'Zzz', NULL, '#123456'),
              ('u5', 'e', NULL, 'Local', NULL, NULL),
              ('u6', 'f', 'empty.example', NULL, NULL, NULL);",
        )
        .unwrap();

        migrate_cache(&conn).unwrap();

        let get = |host: &str| -> Option<(Option<String>, Option<String>, Option<String>)> {
            conn.query_row(
                "SELECT name, icon_url, theme_color FROM instance WHERE host = ?1",
                params![host],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .unwrap()
        };
        assert_eq!(
            get("remote.example"),
            Some((
                Some("Remote".into()),
                Some("https://remote.example/favicon.ico".into()),
                Some("#ff8800".into())
            ))
        );
        // 同一ホストで値が食い違う旧行は列ごとの MAX で1行に集約される
        assert_eq!(
            get("conflict.example"),
            Some((
                Some("Zzz".into()),
                Some("https://conflict.example/a.ico".into()),
                Some("#123456".into())
            ))
        );
        // ローカル(host NULL)と全列NULLのホストは移さない
        assert_eq!(get("empty.example"), None);
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM instance", [], |r| r.get(0)).unwrap();
        assert_eq!(total, 2);
        for col in ["instance_name", "instance_icon_url", "instance_theme_color"] {
            assert!(!column_exists(&conn, "user", col).unwrap(), "{col} は DROP される");
        }

        // 移行後に更新した値は、再実行しても旧列の値で戻らない(冪等)
        conn.execute("UPDATE instance SET name = 'Renamed' WHERE host = 'remote.example'", [])
            .unwrap();
        migrate_cache(&conn).unwrap();
        assert_eq!(get("remote.example").unwrap().0.as_deref(), Some("Renamed"));
    }
```

- [ ] **Step 3: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib store:: 2>&1 | tail -40`
Expected: FAIL(`no such table: instance` / `instance_name` 列が残っている等)。コンパイルエラーになる場合は Step 1–2 のコードの誤りなので直す(ここでの失敗は実行時失敗であるべき)。

- [ ] **Step 4: `db.rs` のスキーマと移行を実装する**

`CACHE_SCHEMA` の `user` テーブルの直後に `instance` を追加する。

old:
```
    notes_count     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS note_reaction
```
new:
```
    notes_count     INTEGER NOT NULL DEFAULT 0
);

-- Issue #409: リモートインスタンス単位の表示情報(Instance Ticker)。host をキーにユーザー間で共有する。
CREATE TABLE IF NOT EXISTS instance (
    host TEXT PRIMARY KEY, name TEXT, icon_url TEXT, theme_color TEXT
);

CREATE TABLE IF NOT EXISTS note_reaction
```

`migrate_cache` の Issue #263 ブロック(`if !column_exists(conn, "user", "instance_name")? { ... }`)を次に置き換える。

```rust
    // Issue #263: user テーブルをフル正規化テーブルに格上げする列を追加。
    // note.payload に埋め込まれていたユーザー情報をここへ集約する。
    // instance_* は Issue #409 で instance テーブルへ移したため追加しない
    // (判定を instance_name にすると、旧列 DROP 後の再起動で全列を再追加してしまう)。
    if !column_exists(conn, "user", "emojis")? {
        conn.execute_batch(
            "ALTER TABLE user ADD COLUMN avatar_url TEXT;
             ALTER TABLE user ADD COLUMN bio TEXT;
             ALTER TABLE user ADD COLUMN banner_url TEXT;
             ALTER TABLE user ADD COLUMN emojis TEXT NOT NULL DEFAULT '{}';",
        )?;
    }
```

同じ関数の avatar_blurhash ブロック(`if !column_exists(conn, "user", "avatar_blurhash")? { ... }`)の直後に呼び出しを追加する。

```rust
    migrate_instance_table(conn)?;
```

`migrate_cache` の直後(`add_unique_index_with_dedup` の前)に関数を追加する。

```rust
/// Issue #409: `user.instance_*` を `instance` テーブルへ移す(一度きり)。
/// 旧 `user.instance_name` 列が残っていることを「未移行」のマーカーとして使う。
/// コピーは競合時に何もしないため、途中で中断して再実行しても、その間に
/// 再受信した新しい値を旧列の値で潰さない。同一ホストで旧行の値が食い違う場合は
/// 列ごとの MAX で1つに決める(次回の受信で自己修復される)。
fn migrate_instance_table(conn: &Connection) -> Result<()> {
    if !column_exists(conn, "user", "instance_name")? {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS instance (
             host TEXT PRIMARY KEY, name TEXT, icon_url TEXT, theme_color TEXT
         );
         INSERT INTO instance (host, name, icon_url, theme_color)
         SELECT host, MAX(instance_name), MAX(instance_icon_url), MAX(instance_theme_color)
         FROM user
         WHERE host IS NOT NULL
           AND (instance_name IS NOT NULL OR instance_icon_url IS NOT NULL OR instance_theme_color IS NOT NULL)
         GROUP BY host
         ON CONFLICT(host) DO NOTHING;
         ALTER TABLE user DROP COLUMN instance_name;
         ALTER TABLE user DROP COLUMN instance_icon_url;
         ALTER TABLE user DROP COLUMN instance_theme_color;",
    )?;
    tx.commit()?;
    Ok(())
}
```

- [ ] **Step 5: `user_ref.rs` の書き込み・読み出しを実装する**

ファイル冒頭のモジュールdocは変更しない。`upsert_user` の直前のdocコメントと `upsert_user`・`fill_user_from_snapshot` の2関数(1〜129行付近、`/// \`user\` テーブルへ upsert する。` から `fill_user_from_snapshot` の閉じ括弧まで)を、次に置き換える。

```rust
/// `instance` テーブルへ upsert する(Issue #409)。列ごとに `COALESCE(新, 既存)`:
/// 再取得の失敗(NULL)が既知の値を消さない。インスタンス単位なので、同一ホストの
/// 誰か1人の受信でそのホスト全ユーザーの表示が更新される。
fn upsert_instance(conn: &Connection, host: &str, info: &InstanceInfo) -> Result<()> {
    conn.execute(
        "INSERT INTO instance (host, name, icon_url, theme_color) VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(host) DO UPDATE SET
            name = COALESCE(excluded.name, instance.name),
            icon_url = COALESCE(excluded.icon_url, instance.icon_url),
            theme_color = COALESCE(excluded.theme_color, instance.theme_color)",
        params![host, info.name, info.icon_url, info.theme_color],
    )?;
    Ok(())
}

/// 自己修復パス用: 既存値が無い列だけ埋める(古いノートのスナップショットで、
/// 直近の値を上書きしない)。
fn fill_instance(conn: &Connection, host: &str, info: &InstanceInfo) -> Result<()> {
    conn.execute(
        "INSERT INTO instance (host, name, icon_url, theme_color) VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(host) DO UPDATE SET
            name = COALESCE(instance.name, excluded.name),
            icon_url = COALESCE(instance.icon_url, excluded.icon_url),
            theme_color = COALESCE(instance.theme_color, excluded.theme_color)",
        params![host, info.name, info.icon_url, info.theme_color],
    )?;
    Ok(())
}

/// `user` テーブルへ upsert する。UserLite に常に含まれる列は常に最新値で上書きし、
/// UserLite では省略されうる列(`bio`/`banner_url`)は、新しい値が `NULL` のときは
/// 既存値を保持する(COALESCE)。これにより、フルユーザー取得(bio/banner込み)の後、
/// ノート受信(UserLiteのみ)の `NULL` で既存の bio/banner_url を踏み潰さない。
/// インスタンス情報は `instance` テーブル(host キー)へ書く(Issue #409)。`instance` の
/// フェッチが一時的に失敗した投稿(`"instance":null`)は何もしないので、既知の値は消えない。
/// ローカルユーザー(`host` が None)は instance を持たないため `instance` 行を作らない。
pub(crate) fn upsert_user(conn: &Connection, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    if let (Some(host), Some(instance)) = (&user.host, &user.instance) {
        upsert_instance(conn, host, instance)?;
    }
    conn.execute(
        "INSERT INTO user (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, avatar_blurhash
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        ON CONFLICT(id) DO UPDATE SET
            username = excluded.username,
            host = excluded.host,
            name = excluded.name,
            avatar_url = excluded.avatar_url,
            is_bot = excluded.is_bot,
            is_cat = excluded.is_cat,
            followers_count = excluded.followers_count,
            following_count = excluded.following_count,
            notes_count = excluded.notes_count,
            emojis = excluded.emojis,
            bio = COALESCE(excluded.bio, user.bio),
            banner_url = COALESCE(excluded.banner_url, user.banner_url),
            avatar_blurhash = COALESCE(excluded.avatar_blurhash, user.avatar_blurhash)",
        params![
            user.id,
            user.username,
            user.host,
            user.name,
            user.avatar_url,
            user.is_bot as i64,
            user.is_cat as i64,
            user.followers_count,
            user.following_count,
            user.notes_count,
            emojis_json,
            user.bio,
            user.banner_url,
            user.avatar_blurhash,
        ],
    )?;
    Ok(())
}

/// 自己修復パス専用のupsert。ペイロードは「そのノートがキャッシュされた時点のスナップショット」
/// であり最新とは限らないため、`upsert_user`(ライブ書き込みパス、常に最新のUserLiteを前提に
/// 常時上書き)と異なり、**全列**を「既存値が無い場合のみ埋める」方針にする(Issue #263 最終レビュー指摘)。
/// これにより、古いノートを読んだだけで直近の name/avatar_url/emojis/*_count が
/// 古いスナップショットで上書きされる回帰を防ぐ。インスタンス情報も `fill_instance` で
/// 同じ方針(既存値が無い列だけ埋める)にする。
/// `is_bot`/`is_cat`/`*_count`は0がデフォルト値であり「値が無い」ことを表現できないため、
/// これらは常に既存値を維持する(=スナップショット側の値は無視する)。
/// `emojis`は`NOT NULL DEFAULT '{}'`で明示的なNULLを取れないため、
/// 「既存値が空オブジェクト('{}')なら埋める」という扱いにする(NULLIFで擬似NULL化)。
pub(crate) fn fill_user_from_snapshot(conn: &Connection, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    if let (Some(host), Some(instance)) = (&user.host, &user.instance) {
        fill_instance(conn, host, instance)?;
    }
    conn.execute(
        "INSERT INTO user (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, avatar_blurhash
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        ON CONFLICT(id) DO UPDATE SET
            username = COALESCE(user.username, excluded.username),
            host = COALESCE(user.host, excluded.host),
            name = COALESCE(user.name, excluded.name),
            avatar_url = COALESCE(user.avatar_url, excluded.avatar_url),
            is_bot = user.is_bot,
            is_cat = user.is_cat,
            followers_count = user.followers_count,
            following_count = user.following_count,
            notes_count = user.notes_count,
            emojis = COALESCE(NULLIF(user.emojis, '{}'), excluded.emojis),
            bio = COALESCE(user.bio, excluded.bio),
            banner_url = COALESCE(user.banner_url, excluded.banner_url),
            avatar_blurhash = COALESCE(user.avatar_blurhash, excluded.avatar_blurhash)",
        params![
            user.id,
            user.username,
            user.host,
            user.name,
            user.avatar_url,
            user.is_bot as i64,
            user.is_cat as i64,
            user.followers_count,
            user.following_count,
            user.notes_count,
            emojis_json,
            user.bio,
            user.banner_url,
            user.avatar_blurhash,
        ],
    )?;
    Ok(())
}
```

`fetch_users_by_ids` 内の SQL(`let sql = format!(...)` 部分)を次に置き換える。列の並び(インデックス 13〜16)は変えないので、`query_map` 内のマッピングは触らない。

```rust
    let sql = format!(
        "SELECT u.id, u.username, u.host, u.name, u.avatar_url, u.is_bot, u.is_cat,
                u.followers_count, u.following_count, u.notes_count, u.emojis,
                u.bio, u.banner_url, i.name, i.icon_url, i.theme_color,
                u.avatar_blurhash
         FROM user u LEFT JOIN instance i ON i.host = u.host
         WHERE u.id IN ({placeholders})"
    );
```

- [ ] **Step 6: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib store:: 2>&1 | tail -30`
Expected: PASS(`user_ref` / `db` / `sqlite_backend` / `note_cache` のテストがすべて成功)。`DROP COLUMN` が SQLite 側で未対応というエラー(`near "DROP": syntax error`)が出た場合は、rusqlite の同梱 SQLite が 3.35 未満という意味なので、ここで止めてユーザーへ報告する(設計の前提が崩れる)。

Run: `cd src-tauri && cargo test 2>&1 | tail -15`
Expected: 全体が PASS。`git status` で `frontend/src/bindings/tauri.gen.ts` に差分が出ていないこと。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/store/db.rs src-tauri/src/store/user_ref.rs
git commit -m "refactor: SQLiteキャッシュのインスタンス情報をinstanceテーブルへ正規化する"
```

---

### Task 2: PostgreSQL バックエンド

**Files:**
- Modify: `src-tauri/src/store/postgres_backend.rs`(`ensure_schema` の user 作成部 約99–125行、`UserTable` enum 約352–370行、テスト部)
- Modify: `src-tauri/src/store/postgres_user_ref.rs`(9–182行、テスト部)

**Interfaces:**
- Consumes: Task 1 の仕様(テーブル形・COALESCE 方針)。コードの依存はない。
- Produces: PostgreSQL の `instance` テーブル。`postgres_user_ref::{upsert_user, fill_user_from_snapshot, fetch_users_by_ids}` のシグネチャは不変。

- [ ] **Step 1: 失敗するテストを書く(`postgres_user_ref.rs`)**

`mod tests` 内、`fn user(id: &str)` の直後にヘルパーを追加する。

```rust
    fn remote_user(id: &str) -> User {
        let mut u = user(id);
        u.host = Some("remote.example".into());
        u
    }

    fn remote_instance() -> InstanceInfo {
        InstanceInfo {
            name: Some("Remote".into()),
            icon_url: Some("https://remote.example/favicon.ico".into()),
            theme_color: Some("#ff8800".into()),
        }
    }

    async fn fetch_one(pool: &sqlx::PgPool, id: &str) -> User {
        fetch_users_by_ids(pool, &[id.to_string()]).await.unwrap().remove(id).unwrap()
    }

    async fn instance_row_count(pool: &sqlx::PgPool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM instance").fetch_one(pool).await.unwrap()
    }
```

`mod tests` の末尾に次のテストを追加する。

```rust
    #[tokio::test]
    #[ignore]
    async fn upsert_user_shares_instance_across_users_of_same_host() {
        let pool = pool().await;
        let mut u1 = remote_user("u1");
        u1.instance = Some(remote_instance());
        upsert_user(&pool, &u1).await.unwrap();
        upsert_user(&pool, &remote_user("u2")).await.unwrap(); // instance無しでも同じhost

        assert_eq!(fetch_one(&pool, "u2").await.instance, Some(remote_instance()));
        assert_eq!(instance_row_count(&pool).await, 1, "同一ホストは1行に集約される");
    }

    #[tokio::test]
    #[ignore]
    async fn upsert_user_propagates_instance_update_and_keeps_known_columns() {
        let pool = pool().await;
        let mut u1 = remote_user("u1");
        u1.instance = Some(remote_instance());
        upsert_user(&pool, &u1).await.unwrap();
        upsert_user(&pool, &remote_user("u2")).await.unwrap();

        // 別ユーザー(同一ホスト)の受信で name だけ更新(部分失敗)
        let mut u3 = remote_user("u3");
        u3.instance = Some(InstanceInfo { name: Some("Renamed".into()), icon_url: None, theme_color: None });
        upsert_user(&pool, &u3).await.unwrap();

        for id in ["u1", "u2", "u3"] {
            let got = fetch_one(&pool, id).await.instance.unwrap();
            assert_eq!(got.name.as_deref(), Some("Renamed"), "{id}");
            assert_eq!(got.icon_url.as_deref(), Some("https://remote.example/favicon.ico"), "{id}");
            assert_eq!(got.theme_color.as_deref(), Some("#ff8800"), "{id}");
        }
    }

    #[tokio::test]
    #[ignore]
    async fn upsert_user_does_not_store_instance_for_local_user() {
        let pool = pool().await;
        let mut local = user("u1"); // host None
        local.instance = Some(remote_instance());
        upsert_user(&pool, &local).await.unwrap();

        assert_eq!(instance_row_count(&pool).await, 0, "ローカルユーザーは instance 行を作らない");
        assert!(fetch_one(&pool, "u1").await.instance.is_none());
    }

    #[tokio::test]
    #[ignore]
    async fn fill_user_from_snapshot_fills_missing_instance_but_keeps_existing() {
        let pool = pool().await;
        let mut fresh = remote_user("u1");
        fresh.instance = Some(remote_instance());
        upsert_user(&pool, &fresh).await.unwrap();

        let mut stale = remote_user("u2");
        stale.instance = Some(InstanceInfo {
            name: Some("Old".into()),
            icon_url: Some("https://remote.example/old.png".into()),
            theme_color: Some("#000000".into()),
        });
        fill_user_from_snapshot(&pool, &stale).await.unwrap();
        assert_eq!(fetch_one(&pool, "u2").await.instance, Some(remote_instance()));

        sqlx::query("UPDATE instance SET theme_color = NULL WHERE host = 'remote.example'")
            .execute(&pool)
            .await
            .unwrap();
        fill_user_from_snapshot(&pool, &stale).await.unwrap();
        let got = fetch_one(&pool, "u2").await.instance.unwrap();
        assert_eq!(got.theme_color.as_deref(), Some("#000000"));
        assert_eq!(got.name.as_deref(), Some("Remote"));
    }
```

- [ ] **Step 2: 失敗するテストを書く(`postgres_backend.rs` の移行テスト)**

`postgres_backend.rs` の `mod tests` の `ensure_schema_is_idempotent_and_creates_tables` の直後に追加する。

```rust
    /// Issue #409: 旧スキーマ(`user.instance_*` あり)の既存インストールが、
    /// `ensure_schema` で `instance` テーブルへ移行され、旧列が消え、再実行しても値が戻らないこと。
    #[tokio::test]
    #[ignore]
    async fn ensure_schema_migrates_legacy_instance_columns() {
        let container = Postgres::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect(&format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres"))
            .await
            .unwrap();

        // 旧スキーマの user テーブルを手で作る(instance_* あり、avatar_blurhash は ensure_schema が追加する)
        sqlx::query(
            "CREATE TABLE \"user\" (
                id TEXT PRIMARY KEY, username TEXT NOT NULL, host TEXT, name TEXT, avatar_url TEXT,
                is_bot SMALLINT NOT NULL DEFAULT 0, is_cat SMALLINT NOT NULL DEFAULT 0,
                followers_count BIGINT NOT NULL DEFAULT 0, following_count BIGINT NOT NULL DEFAULT 0,
                notes_count BIGINT NOT NULL DEFAULT 0, emojis TEXT NOT NULL DEFAULT '{}',
                bio TEXT, banner_url TEXT,
                instance_name TEXT, instance_icon_url TEXT, instance_theme_color TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO \"user\" (id, username, host, instance_name, instance_icon_url, instance_theme_color) VALUES
              ('u1', 'a', 'remote.example', 'Remote', 'https://remote.example/favicon.ico', '#ff8800'),
              ('u2', 'b', 'remote.example', 'Remote', 'https://remote.example/favicon.ico', '#ff8800'),
              ('u3', 'c', 'conflict.example', 'Aaa', 'https://conflict.example/a.ico', NULL),
              ('u4', 'd', 'conflict.example', 'Zzz', NULL, '#123456'),
              ('u5', 'e', NULL, 'Local', NULL, NULL),
              ('u6', 'f', 'empty.example', NULL, NULL, NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();

        ensure_schema(&pool).await.unwrap();

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM instance").fetch_one(&pool).await.unwrap();
        assert_eq!(total, 2, "ローカル(host NULL)と全列NULLのホストは移さない");
        let (name, icon, theme): (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT name, icon_url, theme_color FROM instance WHERE host = 'conflict.example'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(name.as_deref(), Some("Zzz"), "食い違う旧行は列ごとのMAXで1行に集約");
        assert_eq!(icon.as_deref(), Some("https://conflict.example/a.ico"));
        assert_eq!(theme.as_deref(), Some("#123456"));

        let old_cols: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.columns
             WHERE table_name = 'user' AND column_name LIKE 'instance\\_%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(old_cols, 0, "旧 instance_* 列は DROP される");

        // 移行後に更新した値は、ensure_schema を再実行しても戻らない
        sqlx::query("UPDATE instance SET name = 'Renamed' WHERE host = 'remote.example'")
            .execute(&pool)
            .await
            .unwrap();
        ensure_schema(&pool).await.unwrap();
        let name: String = sqlx::query_scalar("SELECT name FROM instance WHERE host = 'remote.example'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(name, "Renamed");
    }
```

- [ ] **Step 3: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib postgres_ -- --ignored 2>&1 | tail -40`
Expected: FAIL(`relation "instance" does not exist` 等)。Docker が動いていること(`docker info`)。コンパイルエラーなら Step 1–2 を直す。

- [ ] **Step 4: `postgres_backend.rs` のスキーマと移行を実装する**

`ensure_schema` の user 作成部から次の3行を削除する。

```rust
        .col(ColumnDef::new(UserTable::InstanceName).text())
        .col(ColumnDef::new(UserTable::InstanceIconUrl).text())
        .col(ColumnDef::new(UserTable::InstanceThemeColor).text())
```

`UserTable` enum から `InstanceName,` `InstanceIconUrl,` `InstanceThemeColor,` の3行を削除し、その直後(`UserTable` の閉じ括弧の後)に追加する。

```rust
#[derive(sea_query::Iden)]
enum InstanceTable {
    #[iden = "instance"]
    Table,
    Host,
    Name,
    IconUrl,
    ThemeColor,
}
```

`ensure_schema` 内、`pool.execute("ALTER TABLE \"user\" ADD COLUMN IF NOT EXISTS avatar_blurhash TEXT").await?;` の直後に追加する。

```rust
    // Issue #409: インスタンス単位の表示情報。user.instance_* から移行する(旧列があるときだけ)。
    let instance = Table::create()
        .table(InstanceTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(InstanceTable::Host).text().primary_key())
        .col(ColumnDef::new(InstanceTable::Name).text())
        .col(ColumnDef::new(InstanceTable::IconUrl).text())
        .col(ColumnDef::new(InstanceTable::ThemeColor).text())
        .build(PostgresQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(instance)).await?;
    migrate_instance_columns(pool).await?;
```

`ensure_schema` の直前(関数のdocコメントより前)に関数を追加する。

```rust
/// Issue #409: `"user".instance_*` を `instance` テーブルへ移す(一度きり)。
/// 旧 `instance_name` 列が残っていることを「未移行」のマーカーとして使う(PostgreSQLには
/// 「一度だけ実行」のマーカー機構が無いため)。コピーは競合時に何もしないので、再実行しても
/// 再受信した新しい値を旧列の値で潰さない。同一ホストで旧行の値が食い違う場合は列ごとの
/// MAX で1つに決める(次回の受信で自己修復される)。コピーとDROPは1トランザクション。
async fn migrate_instance_columns(pool: &sqlx::PgPool) -> Result<()> {
    let legacy: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns
         WHERE table_schema = current_schema() AND table_name = 'user' AND column_name = 'instance_name'",
    )
    .fetch_one(pool)
    .await?;
    if legacy == 0 {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO instance (host, name, icon_url, theme_color)
         SELECT host, MAX(instance_name), MAX(instance_icon_url), MAX(instance_theme_color)
         FROM \"user\"
         WHERE host IS NOT NULL
           AND (instance_name IS NOT NULL OR instance_icon_url IS NOT NULL OR instance_theme_color IS NOT NULL)
         GROUP BY host
         ON CONFLICT (host) DO NOTHING",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "ALTER TABLE \"user\"
         DROP COLUMN IF EXISTS instance_name,
         DROP COLUMN IF EXISTS instance_icon_url,
         DROP COLUMN IF EXISTS instance_theme_color",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
```

- [ ] **Step 5: `postgres_user_ref.rs` を実装する**

ファイル先頭の `use` は変更しない(`InstanceInfo` を引き続き使う)。`upsert_user`・`fill_user_from_snapshot`(9〜115行、`fill_user_from_snapshot` の直前のdocコメントを含む)を次に置き換える。

```rust
/// `instance` テーブルへ upsert する(Issue #409)。列ごとに `COALESCE(新, 既存)`。
async fn upsert_instance(pool: &sqlx::PgPool, host: &str, info: &InstanceInfo) -> Result<()> {
    sqlx::query(
        "INSERT INTO instance (host, name, icon_url, theme_color) VALUES ($1, $2, $3, $4)
        ON CONFLICT (host) DO UPDATE SET
            name = COALESCE(excluded.name, instance.name),
            icon_url = COALESCE(excluded.icon_url, instance.icon_url),
            theme_color = COALESCE(excluded.theme_color, instance.theme_color)",
    )
    .bind(host)
    .bind(&info.name)
    .bind(&info.icon_url)
    .bind(&info.theme_color)
    .execute(pool)
    .await?;
    Ok(())
}

/// 自己修復パス用: 既存値が無い列だけ埋める。
async fn fill_instance(pool: &sqlx::PgPool, host: &str, info: &InstanceInfo) -> Result<()> {
    sqlx::query(
        "INSERT INTO instance (host, name, icon_url, theme_color) VALUES ($1, $2, $3, $4)
        ON CONFLICT (host) DO UPDATE SET
            name = COALESCE(instance.name, excluded.name),
            icon_url = COALESCE(instance.icon_url, excluded.icon_url),
            theme_color = COALESCE(instance.theme_color, excluded.theme_color)",
    )
    .bind(host)
    .bind(&info.name)
    .bind(&info.icon_url)
    .bind(&info.theme_color)
    .execute(pool)
    .await?;
    Ok(())
}

/// `user_ref.rs::upsert_user`と同じ規約。インスタンス情報は `instance` テーブルへ書く。
/// ローカルユーザー(`host` が None)は `instance` 行を作らない。
pub(crate) async fn upsert_user(pool: &sqlx::PgPool, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    if let (Some(host), Some(instance)) = (&user.host, &user.instance) {
        upsert_instance(pool, host, instance).await?;
    }
    sqlx::query(
        "INSERT INTO \"user\" (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, avatar_blurhash
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT (id) DO UPDATE SET
            username = excluded.username,
            host = excluded.host,
            name = excluded.name,
            avatar_url = excluded.avatar_url,
            is_bot = excluded.is_bot,
            is_cat = excluded.is_cat,
            followers_count = excluded.followers_count,
            following_count = excluded.following_count,
            notes_count = excluded.notes_count,
            emojis = excluded.emojis,
            bio = COALESCE(excluded.bio, \"user\".bio),
            banner_url = COALESCE(excluded.banner_url, \"user\".banner_url),
            avatar_blurhash = COALESCE(excluded.avatar_blurhash, \"user\".avatar_blurhash)",
    )
    .bind(&user.id)
    .bind(&user.username)
    .bind(&user.host)
    .bind(&user.name)
    .bind(&user.avatar_url)
    .bind(user.is_bot as i16)
    .bind(user.is_cat as i16)
    .bind(user.followers_count as i64)
    .bind(user.following_count as i64)
    .bind(user.notes_count as i64)
    .bind(&emojis_json)
    .bind(&user.bio)
    .bind(&user.banner_url)
    .bind(&user.avatar_blurhash)
    .execute(pool)
    .await?;
    Ok(())
}

/// 自己修復パス専用のupsert(`user_ref.rs::fill_user_from_snapshot`と同じ規約:
/// 全列を「既存値が無い場合のみ埋める」。詳細は`user_ref.rs`のdocコメント参照)。
pub(crate) async fn fill_user_from_snapshot(pool: &sqlx::PgPool, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    if let (Some(host), Some(instance)) = (&user.host, &user.instance) {
        fill_instance(pool, host, instance).await?;
    }
    sqlx::query(
        "INSERT INTO \"user\" (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, avatar_blurhash
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT (id) DO UPDATE SET
            username = COALESCE(\"user\".username, excluded.username),
            host = COALESCE(\"user\".host, excluded.host),
            name = COALESCE(\"user\".name, excluded.name),
            avatar_url = COALESCE(\"user\".avatar_url, excluded.avatar_url),
            is_bot = \"user\".is_bot,
            is_cat = \"user\".is_cat,
            followers_count = \"user\".followers_count,
            following_count = \"user\".following_count,
            notes_count = \"user\".notes_count,
            emojis = COALESCE(NULLIF(\"user\".emojis, '{}'), excluded.emojis),
            bio = COALESCE(\"user\".bio, excluded.bio),
            banner_url = COALESCE(\"user\".banner_url, excluded.banner_url),
            avatar_blurhash = COALESCE(\"user\".avatar_blurhash, excluded.avatar_blurhash)",
    )
    .bind(&user.id)
    .bind(&user.username)
    .bind(&user.host)
    .bind(&user.name)
    .bind(&user.avatar_url)
    .bind(user.is_bot as i16)
    .bind(user.is_cat as i16)
    .bind(user.followers_count as i64)
    .bind(user.following_count as i64)
    .bind(user.notes_count as i64)
    .bind(&emojis_json)
    .bind(&user.bio)
    .bind(&user.banner_url)
    .bind(&user.avatar_blurhash)
    .execute(pool)
    .await?;
    Ok(())
}
```

`fetch_users_by_ids` の SQL を次に置き換える。`i.name` は `u.name` と列名が衝突するため必ず別名を付ける(以降の `row.try_get("instance_name")` 等はそのまま動く)。

```rust
    let rows = sqlx::query(
        "SELECT u.id, u.username, u.host, u.name, u.avatar_url, u.is_bot, u.is_cat,
                u.followers_count, u.following_count, u.notes_count, u.emojis,
                u.bio, u.banner_url,
                i.name AS instance_name, i.icon_url AS instance_icon_url, i.theme_color AS instance_theme_color,
                u.avatar_blurhash
         FROM \"user\" u LEFT JOIN instance i ON i.host = u.host
         WHERE u.id = ANY($1)",
    )
```

(`.bind(ids).fetch_all(pool).await?;` の続きはそのまま。)

- [ ] **Step 6: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib postgres_ -- --ignored 2>&1 | tail -30`
Expected: PASS(`postgres_user_ref` と `postgres_backend` の全 ignored テスト、既存の `upsert_user_roundtrip` 等を含む)。

Run: `cd src-tauri && cargo test 2>&1 | tail -10`
Expected: 通常テストも PASS(コンパイル警告が増えていないこと。`UserTable::Instance*` を消し忘れていると dead_code 警告が出る)。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/store/postgres_backend.rs src-tauri/src/store/postgres_user_ref.rs
git commit -m "refactor: PostgreSQLキャッシュのインスタンス情報をinstanceテーブルへ正規化する"
```

---

### Task 3: MySQL バックエンド

**Files:**
- Modify: `src-tauri/src/store/mysql_backend.rs`(`ensure_schema` の user 作成部 約183–204行、`UserTable` enum 約321–327行、テスト部)
- Modify: `src-tauri/src/store/mysql_user_ref.rs`(9–180行、テスト部)

**Interfaces:**
- Consumes: Task 1 の仕様。コードの依存はない。
- Produces: MySQL の `instance` テーブル。`mysql_user_ref::{upsert_user, fill_user_from_snapshot, fetch_users_by_ids}` のシグネチャは不変。

- [ ] **Step 1: 失敗するテストを書く(`mysql_user_ref.rs`)**

`mod tests` 内、`fn user(id: &str)` の直後にヘルパーを追加する(`use super::*` で `InstanceInfo` は見える)。

```rust
    fn remote_user(id: &str) -> User {
        let mut u = user(id);
        u.host = Some("remote.example".into());
        u
    }

    fn remote_instance() -> InstanceInfo {
        InstanceInfo {
            name: Some("Remote".into()),
            icon_url: Some("https://remote.example/favicon.ico".into()),
            theme_color: Some("#ff8800".into()),
        }
    }

    async fn fetch_one(pool: &sqlx::MySqlPool, id: &str) -> User {
        fetch_users_by_ids(pool, &[id.to_string()]).await.unwrap().remove(id).unwrap()
    }

    async fn instance_row_count(pool: &sqlx::MySqlPool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM `instance`").fetch_one(pool).await.unwrap()
    }
```

`mod tests` の末尾に次のテストを追加する。

```rust
    #[tokio::test]
    #[ignore]
    async fn upsert_user_shares_instance_across_users_of_same_host() {
        let pool = pool().await;
        let mut u1 = remote_user("u1");
        u1.instance = Some(remote_instance());
        upsert_user(&pool, &u1).await.unwrap();
        upsert_user(&pool, &remote_user("u2")).await.unwrap(); // instance無しでも同じhost

        assert_eq!(fetch_one(&pool, "u2").await.instance, Some(remote_instance()));
        assert_eq!(instance_row_count(&pool).await, 1, "同一ホストは1行に集約される");
    }

    #[tokio::test]
    #[ignore]
    async fn upsert_user_propagates_instance_update_and_keeps_known_columns() {
        let pool = pool().await;
        let mut u1 = remote_user("u1");
        u1.instance = Some(remote_instance());
        upsert_user(&pool, &u1).await.unwrap();
        upsert_user(&pool, &remote_user("u2")).await.unwrap();

        // 別ユーザー(同一ホスト)の受信で name だけ更新(部分失敗)
        let mut u3 = remote_user("u3");
        u3.instance = Some(InstanceInfo { name: Some("Renamed".into()), icon_url: None, theme_color: None });
        upsert_user(&pool, &u3).await.unwrap();

        for id in ["u1", "u2", "u3"] {
            let got = fetch_one(&pool, id).await.instance.unwrap();
            assert_eq!(got.name.as_deref(), Some("Renamed"), "{id}");
            assert_eq!(got.icon_url.as_deref(), Some("https://remote.example/favicon.ico"), "{id}");
            assert_eq!(got.theme_color.as_deref(), Some("#ff8800"), "{id}");
        }
    }

    #[tokio::test]
    #[ignore]
    async fn upsert_user_does_not_store_instance_for_local_user() {
        let pool = pool().await;
        let mut local = user("u1"); // host None
        local.instance = Some(remote_instance());
        upsert_user(&pool, &local).await.unwrap();

        assert_eq!(instance_row_count(&pool).await, 0, "ローカルユーザーは instance 行を作らない");
        assert!(fetch_one(&pool, "u1").await.instance.is_none());
    }

    #[tokio::test]
    #[ignore]
    async fn fill_user_from_snapshot_fills_missing_instance_but_keeps_existing() {
        let pool = pool().await;
        let mut fresh = remote_user("u1");
        fresh.instance = Some(remote_instance());
        upsert_user(&pool, &fresh).await.unwrap();

        let mut stale = remote_user("u2");
        stale.instance = Some(InstanceInfo {
            name: Some("Old".into()),
            icon_url: Some("https://remote.example/old.png".into()),
            theme_color: Some("#000000".into()),
        });
        fill_user_from_snapshot(&pool, &stale).await.unwrap();
        assert_eq!(fetch_one(&pool, "u2").await.instance, Some(remote_instance()));

        sqlx::query("UPDATE `instance` SET theme_color = NULL WHERE host = 'remote.example'")
            .execute(&pool)
            .await
            .unwrap();
        fill_user_from_snapshot(&pool, &stale).await.unwrap();
        let got = fetch_one(&pool, "u2").await.instance.unwrap();
        assert_eq!(got.theme_color.as_deref(), Some("#000000"));
        assert_eq!(got.name.as_deref(), Some("Remote"));
    }
```

- [ ] **Step 2: 失敗するテストを書く(`mysql_backend.rs` の移行テスト)**

`mysql_backend.rs` の `mod tests` の `ensure_schema_is_idempotent_and_creates_tables` の直後に追加する。

```rust
    /// Issue #409: 旧スキーマ(`user.instance_*` あり)の既存インストールが、
    /// `ensure_schema` で `instance` テーブルへ移行され、旧列が消え、再実行しても値が戻らないこと。
    #[tokio::test]
    #[ignore]
    async fn ensure_schema_migrates_legacy_instance_columns() {
        let container = Mysql::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(3306).await.unwrap();
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .connect(&format!("mysql://root@127.0.0.1:{port}/test"))
            .await
            .unwrap();

        // 旧スキーマの user テーブルを手で作る(instance_* あり、avatar_blurhash は ensure_schema が追加する)
        sqlx::query(
            "CREATE TABLE `user` (
                id VARCHAR(64) PRIMARY KEY, username TEXT NOT NULL, host TEXT, name TEXT, avatar_url TEXT,
                is_bot BOOLEAN NOT NULL DEFAULT FALSE, is_cat BOOLEAN NOT NULL DEFAULT FALSE,
                followers_count BIGINT NOT NULL DEFAULT 0, following_count BIGINT NOT NULL DEFAULT 0,
                notes_count BIGINT NOT NULL DEFAULT 0, emojis TEXT NOT NULL,
                bio LONGTEXT, banner_url TEXT,
                instance_name TEXT, instance_icon_url TEXT, instance_theme_color TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO `user` (id, username, host, emojis, instance_name, instance_icon_url, instance_theme_color) VALUES
              ('u1', 'a', 'remote.example', '{}', 'Remote', 'https://remote.example/favicon.ico', '#ff8800'),
              ('u2', 'b', 'remote.example', '{}', 'Remote', 'https://remote.example/favicon.ico', '#ff8800'),
              ('u3', 'c', 'conflict.example', '{}', 'Aaa', 'https://conflict.example/a.ico', NULL),
              ('u4', 'd', 'conflict.example', '{}', 'Zzz', NULL, '#123456'),
              ('u5', 'e', NULL, '{}', 'Local', NULL, NULL),
              ('u6', 'f', 'empty.example', '{}', NULL, NULL, NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();

        ensure_schema(&pool).await.unwrap();

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `instance`").fetch_one(&pool).await.unwrap();
        assert_eq!(total, 2, "ローカル(host NULL)と全列NULLのホストは移さない");
        let (name, icon, theme): (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT name, icon_url, theme_color FROM `instance` WHERE host = 'conflict.example'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(name.as_deref(), Some("Zzz"), "食い違う旧行は列ごとのMAXで1行に集約");
        assert_eq!(icon.as_deref(), Some("https://conflict.example/a.ico"));
        assert_eq!(theme.as_deref(), Some("#123456"));

        let old_cols: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.columns
             WHERE table_schema = DATABASE() AND table_name = 'user' AND column_name LIKE 'instance\\_%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(old_cols, 0, "旧 instance_* 列は DROP される");

        // 移行後に更新した値は、ensure_schema を再実行しても戻らない
        sqlx::query("UPDATE `instance` SET name = 'Renamed' WHERE host = 'remote.example'")
            .execute(&pool)
            .await
            .unwrap();
        ensure_schema(&pool).await.unwrap();
        let name: String = sqlx::query_scalar("SELECT name FROM `instance` WHERE host = 'remote.example'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(name, "Renamed");
    }
```

- [ ] **Step 3: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib mysql_ -- --ignored 2>&1 | tail -40`
Expected: FAIL(`Table 'test.instance' doesn't exist` 等)。Docker が動いていること。コンパイルエラーなら Step 1–2 を直す。

- [ ] **Step 4: `mysql_backend.rs` のスキーマと移行を実装する**

`ensure_schema` の user 作成部から次の3行を削除する。

```rust
        .col(ColumnDef::new(UserTable::InstanceName).text())
        .col(ColumnDef::new(UserTable::InstanceIconUrl).text())
        .col(ColumnDef::new(UserTable::InstanceThemeColor).text())
```

`UserTable` enum を次に置き換える(`InstanceName, InstanceIconUrl, InstanceThemeColor,` を除く)。

```rust
#[derive(sea_query::Iden)]
enum UserTable {
    #[iden = "user"]
    Table, Id, Username, Host, Name, AvatarUrl, IsBot, IsCat, FollowersCount,
    FollowingCount, NotesCount, Emojis, Bio, BannerUrl, AvatarBlurhash,
}

#[derive(sea_query::Iden)]
enum InstanceTable {
    #[iden = "instance"]
    Table, Host, Name, IconUrl, ThemeColor,
}
```

`ensure_schema` 内、`add_column_if_missing(pool, "ALTER TABLE `user` ADD COLUMN avatar_blurhash TEXT").await?;` の直後に追加する。

```rust
    // Issue #409: インスタンス単位の表示情報。user.instance_* から移行する(旧列があるときだけ)。
    // MySQLはTEXTにPKを張れないため host は VARCHAR(255)(ホスト名の上限は253文字)。
    let instance = Table::create()
        .table(InstanceTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(InstanceTable::Host).string_len(255).primary_key())
        .col(ColumnDef::new(InstanceTable::Name).text())
        .col(ColumnDef::new(InstanceTable::IconUrl).text())
        .col(ColumnDef::new(InstanceTable::ThemeColor).text())
        .build(MysqlQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(instance)).await?;
    migrate_instance_columns(pool).await?;
```

`ensure_schema` の直前(`add_column_if_missing` の後、`ensure_schema` のdocコメントより前)に関数を追加する。

```rust
/// Issue #409: `user.instance_*` を `instance` テーブルへ移す(一度きり)。
/// 旧 `instance_name` 列が残っていることを「未移行」のマーカーとして使う(MySQLには
/// 「一度だけ実行」のマーカー機構が無いため)。MySQLのDDLは暗黙コミットされ、コピーとDROPを
/// 1トランザクションにできない。そのためコピーは競合時に何もしない `INSERT IGNORE` にして、
/// コピー後・DROP前に中断して再実行しても、その間に再受信した新しい値を旧列の値で潰さない
/// (旧列が残っている限り再度コピーを試みるが、既存の行は変更されない)。
/// 同一ホストで旧行の値が食い違う場合は列ごとのMAXで1つに決める(次回の受信で自己修復される)。
async fn migrate_instance_columns(pool: &sqlx::MySqlPool) -> Result<()> {
    let legacy: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns
         WHERE table_schema = DATABASE() AND table_name = 'user' AND column_name = 'instance_name'",
    )
    .fetch_one(pool)
    .await?;
    if legacy == 0 {
        return Ok(());
    }
    sqlx::query(
        "INSERT IGNORE INTO `instance` (host, name, icon_url, theme_color)
         SELECT host, MAX(instance_name), MAX(instance_icon_url), MAX(instance_theme_color)
         FROM `user`
         WHERE host IS NOT NULL
           AND (instance_name IS NOT NULL OR instance_icon_url IS NOT NULL OR instance_theme_color IS NOT NULL)
         GROUP BY host",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "ALTER TABLE `user`
         DROP COLUMN instance_name,
         DROP COLUMN instance_icon_url,
         DROP COLUMN instance_theme_color",
    )
    .execute(pool)
    .await?;
    Ok(())
}
```

- [ ] **Step 5: `mysql_user_ref.rs` を実装する**

`upsert_user`・`fill_user_from_snapshot`(9〜115行付近、`fill_user_from_snapshot` の直前のdocコメントを含む)を次に置き換える。

```rust
/// `instance` テーブルへ upsert する(Issue #409)。列ごとに `COALESCE(新, 既存)`。
async fn upsert_instance(pool: &sqlx::MySqlPool, host: &str, info: &InstanceInfo) -> Result<()> {
    sqlx::query(
        "INSERT INTO `instance` (host, name, icon_url, theme_color) VALUES (?, ?, ?, ?)
        ON DUPLICATE KEY UPDATE
            name = COALESCE(VALUES(name), name),
            icon_url = COALESCE(VALUES(icon_url), icon_url),
            theme_color = COALESCE(VALUES(theme_color), theme_color)",
    )
    .bind(host)
    .bind(&info.name)
    .bind(&info.icon_url)
    .bind(&info.theme_color)
    .execute(pool)
    .await?;
    Ok(())
}

/// 自己修復パス用: 既存値が無い列だけ埋める(`ON DUPLICATE KEY UPDATE` では列名=既存値、
/// `VALUES(col)`=新しく挿入しようとした値)。
async fn fill_instance(pool: &sqlx::MySqlPool, host: &str, info: &InstanceInfo) -> Result<()> {
    sqlx::query(
        "INSERT INTO `instance` (host, name, icon_url, theme_color) VALUES (?, ?, ?, ?)
        ON DUPLICATE KEY UPDATE
            name = COALESCE(name, VALUES(name)),
            icon_url = COALESCE(icon_url, VALUES(icon_url)),
            theme_color = COALESCE(theme_color, VALUES(theme_color))",
    )
    .bind(host)
    .bind(&info.name)
    .bind(&info.icon_url)
    .bind(&info.theme_color)
    .execute(pool)
    .await?;
    Ok(())
}

/// `user_ref.rs::upsert_user`と同じ規約。インスタンス情報は `instance` テーブルへ書く。
/// ローカルユーザー(`host` が None)は `instance` 行を作らない。
pub(crate) async fn upsert_user(pool: &sqlx::MySqlPool, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    if let (Some(host), Some(instance)) = (&user.host, &user.instance) {
        upsert_instance(pool, host, instance).await?;
    }
    sqlx::query(
        "INSERT INTO `user` (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, avatar_blurhash
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON DUPLICATE KEY UPDATE
            username = VALUES(username),
            host = VALUES(host),
            name = VALUES(name),
            avatar_url = VALUES(avatar_url),
            is_bot = VALUES(is_bot),
            is_cat = VALUES(is_cat),
            followers_count = VALUES(followers_count),
            following_count = VALUES(following_count),
            notes_count = VALUES(notes_count),
            emojis = VALUES(emojis),
            bio = COALESCE(VALUES(bio), bio),
            banner_url = COALESCE(VALUES(banner_url), banner_url),
            avatar_blurhash = COALESCE(VALUES(avatar_blurhash), avatar_blurhash)",
    )
    .bind(&user.id)
    .bind(&user.username)
    .bind(&user.host)
    .bind(&user.name)
    .bind(&user.avatar_url)
    .bind(user.is_bot)
    .bind(user.is_cat)
    .bind(user.followers_count as i64)
    .bind(user.following_count as i64)
    .bind(user.notes_count as i64)
    .bind(&emojis_json)
    .bind(&user.bio)
    .bind(&user.banner_url)
    .bind(&user.avatar_blurhash)
    .execute(pool)
    .await?;
    Ok(())
}

/// 自己修復パス専用のupsert(`postgres_user_ref.rs::fill_user_from_snapshot`と同じ規約:
/// 全列を「既存値が無い場合のみ埋める」。詳細はそちらのdocコメント参照)。
/// MySQLの`ON DUPLICATE KEY UPDATE`では、挿入直前の既存行の値は列名をそのまま
/// (`col`)、新しく挿入しようとした値は`VALUES(col)`で参照する — Postgresの
/// `"user".col`(既存値)/`excluded.col`(新値)と役割の対応が逆になる点に注意。
pub(crate) async fn fill_user_from_snapshot(pool: &sqlx::MySqlPool, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    if let (Some(host), Some(instance)) = (&user.host, &user.instance) {
        fill_instance(pool, host, instance).await?;
    }
    sqlx::query(
        "INSERT INTO `user` (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, avatar_blurhash
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON DUPLICATE KEY UPDATE
            username = COALESCE(username, VALUES(username)),
            host = COALESCE(host, VALUES(host)),
            name = COALESCE(name, VALUES(name)),
            avatar_url = COALESCE(avatar_url, VALUES(avatar_url)),
            is_bot = is_bot,
            is_cat = is_cat,
            followers_count = followers_count,
            following_count = following_count,
            notes_count = notes_count,
            emojis = COALESCE(NULLIF(emojis, '{}'), VALUES(emojis)),
            bio = COALESCE(bio, VALUES(bio)),
            banner_url = COALESCE(banner_url, VALUES(banner_url)),
            avatar_blurhash = COALESCE(avatar_blurhash, VALUES(avatar_blurhash))",
    )
    .bind(&user.id)
    .bind(&user.username)
    .bind(&user.host)
    .bind(&user.name)
    .bind(&user.avatar_url)
    .bind(user.is_bot)
    .bind(user.is_cat)
    .bind(user.followers_count as i64)
    .bind(user.following_count as i64)
    .bind(user.notes_count as i64)
    .bind(&emojis_json)
    .bind(&user.bio)
    .bind(&user.banner_url)
    .bind(&user.avatar_blurhash)
    .execute(pool)
    .await?;
    Ok(())
}
```

(`fill_user_from_snapshot` の `ON DUPLICATE KEY UPDATE` 節は、既存実装から `instance_*` の3行・bind・カラム・プレースホルダ(17→14個)だけを除いたもの。)

`fetch_users_by_ids` の `let sql = format!(...)` を次に置き換える(別名の理由は Task 2 と同じ)。

```rust
    let sql = format!(
        "SELECT u.id, u.username, u.host, u.name, u.avatar_url, u.is_bot, u.is_cat,
                u.followers_count, u.following_count, u.notes_count, u.emojis,
                u.bio, u.banner_url,
                i.name AS instance_name, i.icon_url AS instance_icon_url, i.theme_color AS instance_theme_color,
                u.avatar_blurhash
         FROM `user` u LEFT JOIN `instance` i ON i.host = u.host
         WHERE u.id IN ({placeholders})"
    );
```

- [ ] **Step 6: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib mysql_ -- --ignored 2>&1 | tail -30`
Expected: PASS(`mysql_user_ref` と `mysql_backend` の全 ignored テスト、既存の `upsert_user_roundtrip` 等を含む)。`Illegal mix of collations`(JOIN の `i.host = u.host`)が出たら、`u.host` と `i.host` の照合順序が食い違っているので、ここで止めてユーザーへ報告する(設計上のリスクとして spec に記載済み)。

Run: `cd src-tauri && cargo test 2>&1 | tail -10`
Expected: 通常テストも PASS、警告が増えていない。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/store/mysql_backend.rs src-tauri/src/store/mysql_user_ref.rs
git commit -m "refactor: MySQLキャッシュのインスタンス情報をinstanceテーブルへ正規化する"
```

---

### Task 4: 全体検証と実データでの移行確認

**Files:** なし(検証のみ。問題が見つかった場合だけ該当ファイルを修正して個別にコミットする)

**Interfaces:**
- Consumes: Task 1〜3 の成果物。
- Produces: 動作確認の証拠(PR 本文の検証項目に転記できる形)。

- [ ] **Step 1: 通常テストと ignored テスト(実DB)をすべて実行する**

Run: `cd src-tauri && cargo test 2>&1 | tail -15`
Expected: 全 PASS。`generates_frontend_bindings` が通り、`git status` で `frontend/src/bindings/tauri.gen.ts` に差分が無い。

Run: `cd src-tauri && cargo test --lib -- --ignored postgres_ mysql_ 2>&1 | tail -30`
Expected: PostgreSQL/MySQL の ignored テストがすべて PASS(Docker 必須)。実Misskey接続の ignored テストは対象外(名前が `postgres_` / `mysql_` を含むものだけ走る)。

Run: `cd frontend && pnpm check && pnpm test 2>&1 | tail -10`
Expected: PASS(フロント側は無変更のため回帰確認のみ)。

- [ ] **Step 2: 実データの複製で SQLite 移行を確認する**

実際のキャッシュDBには絶対に触れない。複製に対してのみ実行する。

1. 実キャッシュDBの場所を特定する: `find ~/.local/share ~/.cache -name '*.db' -path '*tsumugi*' 2>/dev/null`(見つからなければ `app_cache_dir` 系のパスをコードで確認)。
2. スクラッチパッド配下へ複製し、`sqlite3` で移行前の状態を記録する: `SELECT COUNT(DISTINCT host), COUNT(*) FROM user WHERE instance_name IS NOT NULL OR instance_icon_url IS NOT NULL OR instance_theme_color IS NOT NULL;`
3. 複製に対して `open_cache` を通す。一番簡単なのは、使い捨ての `#[test] #[ignore]`(コミットしない)またはスクラッチの小さな Rust スニペットで `store::db::open_cache(<複製パス>)` を呼ぶこと。アプリ本体を起動する場合は、メモリの「dev server verification must use virtual display」と「tauri single-instance blocks XDG isolated verification」に従い、`Xvfb` + `dbus-run-session` + `WAYLAND_DISPLAY` の unset で実画面へ漏らさず、`XDG_*` を複製先へ向け、起動した PID を正確に kill する(`pkill`/`killall` は使わない)。
4. 移行後に確認する:
   - `SELECT COUNT(*) FROM instance;` が手順2の `COUNT(DISTINCT host)` と一致する(値が全列 NULL のホスト・ローカルは除く)。
   - `PRAGMA table_info(user);` に `instance_*` が無い。
   - もう一度 `open_cache` を通しても `instance` の行数・値が変わらない。
5. 実アプリ起動で確認できる場合は、リモートユーザーの投稿に Instance Ticker(名前・アイコン・色)が従来どおり表示されることを目視する。

Expected: 上記すべて満たす。満たさない場合は原因を切り分けて Task 1 を修正する(この確認は「レビュー通過 ≠ 実データで動作確認済み」の穴を塞ぐためのもの)。

- [ ] **Step 3: 仕上げの確認と引き渡し**

- `git log --oneline main..HEAD` で spec/plan/Task 1〜3 のコミットだけが並ぶことを確認する。
- `git status` がクリーンであること(スクラッチの検証物をリポジトリに残していない)。
- 生きたドキュメント(`CLAUDE.md` / `docs/design/` / `docs/guide/`)に `instance_name` 等の記述が無いことは計画時に `grep` で確認済み(過去の spec/plan にだけ残るが履歴なので触らない)。実装中に新たな記述を見つけたら `instance` テーブルに合わせて直す。
- push / PR 作成は **ユーザーの指示があるまでしない**(PR 本文には `Fixes #409` を入れ、`.github/pull_request_template.md` の構造に従う。マージは `gh pr merge --merge`)。superpowers:finishing-a-development-branch で進める。
