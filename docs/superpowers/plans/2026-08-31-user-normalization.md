# ユーザー情報正規化(Issue #263) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ノートキャッシュの `payload` に埋め込まれているユーザー情報(`instance`含む)を `user` テーブルへ正規化し、payloadは `{"id": ...}` スタブのみ持たせる。既存キャッシュは読み込み時に自己修復する。

**Architecture:** 書き込み時は保存用JSONの `user`/`renote.user` をスタブへ差し替えつつ、フルユーザーは `user` テーブルへ`ON CONFLICT`で更新する。読み込み時は payload をまとめて取得し、旧形式(フルユーザー埋め込み)を検知した行はその場で `user` テーブルへ抽出＆payload書き戻しをしてから、全行分の `user.id` をバッチで引いてスタブへ埋め戻し、`Note` へデシリアライズする。`user`参照解決とJSON操作は新設 `store/user_ref.rs` に切り出し、`store/note_cache.rs` はオーケストレーションに専念する。

**Tech Stack:** Rust, rusqlite, serde_json（既存スタック。新規依存なし）

## Global Constraints

- `src-tauri/src/domain/note.rs` / `src-tauri/src/domain/user.rs` のRust型・TSバインディングは変更しない（設計docの「影響範囲外」節）。
- `user`テーブルのUserLite常在フィールド(`username/host/name/avatar_url/is_bot/is_cat/*_count/emojis`)は常に上書き、`UserLite`で省略されうるフィールド(`bio/banner_url/instance_*`)は`COALESCE`で既存値を保持する（設計doc「書き込みパス」節）。
- 既存キャッシュの移行は起動時一括バッチではなく読み込み時の遅延自己修復のみ（設計doc「方針」節）。
- user解決のための `SELECT ... WHERE id IN (...)` は1回の読み込み呼び出しにつき1クエリ(N+1にしない)。
- 参照先の`user`行が見つからない場合はそのノートをログ警告してスキップし、呼び出し元をエラーにしない（既存の`deserialize_note_or_warn`と同じポリシー）。

---

## Task 1: `user` テーブルへの列追加マイグレーション

**Files:**
- Modify: `src-tauri/src/store/db.rs:230-245`（`migrate_cache`）
- Test: `src-tauri/src/store/db.rs`（既存 `#[cfg(test)] mod tests` 内に追加）

**Interfaces:**
- Consumes: 既存の `column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool>`
- Produces: `user` テーブルに以下の列が存在すること（以降の全タスクが依存）:
  `avatar_url TEXT`, `bio TEXT`, `banner_url TEXT`, `emojis TEXT NOT NULL DEFAULT '{}'`,
  `instance_name TEXT`, `instance_icon_url TEXT`, `instance_theme_color TEXT`

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/store/db.rs` の `mod tests` 内に追加:

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
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd src-tauri && cargo test migrate_cache_adds_user_normalization_columns -- --nocapture`
Expected: FAIL（`avatar_url` 等の列が無い）

- [ ] **Step 3: 最小実装**

`migrate_cache` に以下を追加（既存の `column_note.created_at` ブロックの後、インデックス作成の前）:

```rust
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
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd src-tauri && cargo test migrate_cache_adds_user_normalization_columns -- --nocapture`
Expected: PASS

- [ ] **Step 5: 既存テストが壊れていないことを確認**

Run: `cd src-tauri && cargo test --lib store::db`
Expected: PASS（全件）

- [ ] **Step 6: Commit**

```bash
cd src-tauri && git add src/store/db.rs
git commit -m "feat: userテーブルにinstance/bio/banner等の正規化列を追加"
```

---

## Task 2: `store/user_ref.rs` 新設 — `upsert_user`（COALESCE保持ポリシー）

**Files:**
- Create: `src-tauri/src/store/user_ref.rs`
- Modify: `src-tauri/src/store/mod.rs`（`pub mod user_ref;` を追加）
- Test: `src-tauri/src/store/user_ref.rs` 内 `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `crate::domain::{User, InstanceInfo}`, `crate::store::db::open_cache_in_memory`（テストのみ）
- Produces: `pub(crate) fn upsert_user(conn: &rusqlite::Connection, user: &User) -> crate::error::Result<()>`
  （Task 6 の `upsert_note`、Task 7 の自己修復パスから呼ばれる）

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/store/user_ref.rs` を新規作成:

```rust
//! `user` テーブル(正規化済みユーザー情報)への読み書きと、note payload 内の
//! user 参照(スタブ `{"id": ...}` ⇔ フル `User`)の変換ヘルパー(Issue #263)。

use crate::domain::User;
use crate::error::Result;
use rusqlite::{params, Connection};

/// `user` テーブルへ upsert する。UserLite に常に含まれる列は常に最新値で上書きし、
/// UserLite では省略されうる列(`bio`/`banner_url`/`instance_*`)は、新しい値が
/// `NULL` のときは既存値を保持する(COALESCE)。これにより:
/// - フルユーザー取得(bio/banner込み)の後、ノート受信(UserLiteのみ)の `NULL` で
///   既存の bio/banner_url を踏み潰さない。
/// - `instance` フェッチが一時的に失敗した投稿(`"instance":null`)が、既に分かっている
///   instance を消さない。
pub(crate) fn upsert_user(conn: &Connection, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    let (instance_name, instance_icon_url, instance_theme_color) = match &user.instance {
        Some(i) => (i.name.clone(), i.icon_url.clone(), i.theme_color.clone()),
        None => (None, None, None),
    };
    conn.execute(
        "INSERT INTO user (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
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
            instance_name = COALESCE(excluded.instance_name, user.instance_name),
            instance_icon_url = COALESCE(excluded.instance_icon_url, user.instance_icon_url),
            instance_theme_color = COALESCE(excluded.instance_theme_color, user.instance_theme_color)",
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
            instance_name,
            instance_icon_url,
            instance_theme_color,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::InstanceInfo;
    use crate::store::db::open_cache_in_memory;
    use std::collections::HashMap;

    fn user_lite(id: &str, name: &str) -> User {
        User {
            id: id.into(),
            username: "alice".into(),
            host: Some("remote.example".into()),
            name: Some(name.into()),
            avatar_url: Some("https://remote.example/a.png".into()),
            is_bot: false,
            is_cat: false,
            followers_count: 1,
            following_count: 2,
            notes_count: 3,
            emojis: HashMap::new(),
            bio: None,
            banner_url: None,
            instance: None,
        }
    }

    fn row(conn: &Connection, id: &str) -> (Option<String>, Option<String>, Option<String>) {
        conn.query_row(
            "SELECT bio, banner_url, instance_name FROM user WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
    }

    #[test]
    fn upsert_user_preserves_bio_when_later_write_has_none() {
        let conn = open_cache_in_memory().unwrap();
        let mut full = user_lite("u1", "Alice");
        full.bio = Some("hello".into());
        upsert_user(&conn, &full).unwrap();

        // 後続の UserLite 由来の書き込み(bio=None)
        let lite = user_lite("u1", "Alice (updated)");
        upsert_user(&conn, &lite).unwrap();

        let (bio, _, _) = row(&conn, "u1");
        assert_eq!(bio, Some("hello".to_string()));
    }

    #[test]
    fn upsert_user_overwrites_always_present_fields() {
        let conn = open_cache_in_memory().unwrap();
        upsert_user(&conn, &user_lite("u1", "Alice")).unwrap();
        upsert_user(&conn, &user_lite("u1", "Alice2")).unwrap();

        let name: String = conn
            .query_row("SELECT name FROM user WHERE id = 'u1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "Alice2");
    }

    #[test]
    fn upsert_user_preserves_instance_when_later_fetch_fails() {
        let conn = open_cache_in_memory().unwrap();
        let mut with_instance = user_lite("u1", "Alice");
        with_instance.instance = Some(InstanceInfo {
            name: Some("Remote".into()),
            icon_url: Some("https://remote.example/icon.png".into()),
            theme_color: Some("#ff8800".into()),
        });
        upsert_user(&conn, &with_instance).unwrap();

        // instance フェッチ失敗(null)の投稿を後から受信
        let mut failed_fetch = user_lite("u1", "Alice");
        failed_fetch.instance = None;
        upsert_user(&conn, &failed_fetch).unwrap();

        let (_, _, instance_name) = row(&conn, "u1");
        assert_eq!(instance_name, Some("Remote".to_string()));
    }
}
```

- [ ] **Step 2: `store/mod.rs` に登録**

`src-tauri/src/store/mod.rs` に追加:

```rust
pub mod user_ref;
```

- [ ] **Step 3: テストが失敗することを確認**

Run: `cd src-tauri && cargo test --lib store::user_ref`
Expected: コンパイルエラー、または `user` テーブルに列が無くFAIL（Task 1が先に完了していれば列はあるはずなのでコンパイルは通るがロジック未実装なら関数はもう書いてあるのでこのタスクの実装ステップと一致。Task 1を先に完了させておくこと）

- [ ] **Step 4: 上記実装で通ることを確認**

Run: `cd src-tauri && cargo test --lib store::user_ref`
Expected: PASS（3件とも）

- [ ] **Step 5: Commit**

```bash
cd src-tauri && git add src/store/user_ref.rs src/store/mod.rs
git commit -m "feat: userテーブルへのCOALESCE保持upsertを実装"
```

---

## Task 3: `store/user_ref.rs` — 型付き `collect_users`（本体+renote再帰）

**Files:**
- Modify: `src-tauri/src/store/user_ref.rs`

**Interfaces:**
- Consumes: `crate::domain::{Note, User}`
- Produces: `pub(crate) fn collect_users(note: &Note) -> Vec<&User>`
  （Task 6 の `upsert_note` が「本体+renote分すべてのuserをupsertする」ために使う）

- [ ] **Step 1: 失敗するテストを書く**

`user_ref.rs` の `use` に `crate::domain::Note` を追加し、`mod tests` 内に追加:

```rust
    use crate::domain::{DriveFile, Visibility};
    use std::collections::HashMap as Map;

    fn bare_note(id: &str, user: User) -> Note {
        Note {
            id: id.into(),
            created_at: 100,
            text: Some("hi".into()),
            cw: None,
            visibility: Visibility::Home,
            local_only: false,
            user,
            reply_id: None,
            renote_id: None,
            renote: None,
            files: Vec::<DriveFile>::new(),
            poll: None,
            tags: vec![],
            mentions: vec![],
            emojis: Map::new(),
            channel_id: None,
            via: None,
            lang: None,
            reactions: Map::new(),
            reaction_count: 0,
            renote_count: 0,
            reply_count: 0,
            my_reaction: None,
            is_renoted_by_me: false,
            is_favorited_by_me: false,
            is_pinned: false,
        }
    }

    #[test]
    fn collect_users_returns_just_author_when_no_renote() {
        let n = bare_note("n1", user_lite("u1", "Alice"));
        let users = collect_users(&n);
        assert_eq!(users.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["u1"]);
    }

    #[test]
    fn collect_users_includes_renote_author() {
        let mut n = bare_note("n1", user_lite("u1", "Alice"));
        n.renote = Some(Box::new(bare_note("n0", user_lite("u2", "Bob"))));
        let users = collect_users(&n);
        assert_eq!(users.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["u1", "u2"]);
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd src-tauri && cargo test --lib store::user_ref::tests::collect_users`
Expected: FAIL（`collect_users` 未定義でコンパイルエラー）

- [ ] **Step 3: 実装**

`user_ref.rs` の `use crate::domain::User;` を `use crate::domain::{Note, User};` に変更し、`upsert_user` の下に追加:

```rust
/// ノート本体+renote(入れ子)分の User をすべて集める(重複排除はしない)。
/// upsert_note が「note.payload に埋め込まれる全ユーザー」をキャッシュへ反映するために使う。
pub(crate) fn collect_users(note: &Note) -> Vec<&User> {
    let mut out = vec![&note.user];
    if let Some(renote) = &note.renote {
        out.extend(collect_users(renote));
    }
    out
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib store::user_ref`
Expected: PASS（5件: Task2の3件+今回の2件）

- [ ] **Step 5: Commit**

```bash
cd src-tauri && git add src/store/user_ref.rs
git commit -m "feat: ノート+renote分のUserを再帰的に集めるcollect_usersを実装"
```

---

## Task 4: `store/user_ref.rs` — JSON `stub_user_refs` / `is_legacy_full_user` / `has_legacy_full_user`

**Files:**
- Modify: `src-tauri/src/store/user_ref.rs`

**Interfaces:**
- Consumes: `serde_json::Value`
- Produces:
  - `pub(crate) fn stub_user_refs(note_value: &mut serde_json::Value)`（Task 6の`upsert_note`が使う）
  - `pub(crate) fn is_legacy_full_user(user_value: &serde_json::Value) -> bool`（Task 7が使う）
  - `pub(crate) fn has_legacy_full_user(note_value: &serde_json::Value) -> bool`（Task 7が使う）

- [ ] **Step 1: 失敗するテストを書く**

`user_ref.rs` の `mod tests` 内に追加:

```rust
    use serde_json::json;

    #[test]
    fn stub_user_refs_replaces_top_level_user_with_id_only() {
        let mut v = json!({
            "id": "n1",
            "user": { "id": "u1", "username": "alice", "isBot": false }
        });
        stub_user_refs(&mut v);
        assert_eq!(v["user"], json!({ "id": "u1" }));
    }

    #[test]
    fn stub_user_refs_recurses_into_renote() {
        let mut v = json!({
            "id": "n1",
            "user": { "id": "u1", "username": "alice" },
            "renote": {
                "id": "n0",
                "user": { "id": "u2", "username": "bob" },
                "renote": null
            }
        });
        stub_user_refs(&mut v);
        assert_eq!(v["user"], json!({ "id": "u1" }));
        assert_eq!(v["renote"]["user"], json!({ "id": "u2" }));
    }

    #[test]
    fn is_legacy_full_user_true_for_object_with_username_false_for_stub() {
        assert!(is_legacy_full_user(&json!({ "id": "u1", "username": "alice" })));
        assert!(!is_legacy_full_user(&json!({ "id": "u1" })));
    }

    #[test]
    fn has_legacy_full_user_detects_legacy_shape_in_renote_only() {
        let v = json!({
            "id": "n1",
            "user": { "id": "u1" },
            "renote": { "id": "n0", "user": { "id": "u2", "username": "bob" } }
        });
        assert!(has_legacy_full_user(&v));

        let already_stubbed = json!({
            "id": "n1",
            "user": { "id": "u1" },
            "renote": { "id": "n0", "user": { "id": "u2" } }
        });
        assert!(!has_legacy_full_user(&already_stubbed));
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd src-tauri && cargo test --lib store::user_ref::tests::stub_user_refs`
Expected: FAIL（未定義関数でコンパイルエラー）

- [ ] **Step 3: 実装**

`user_ref.rs` に追加:

```rust
/// note_value の `user`(本体+renote分)を `{"id": ...}` スタブへ差し替える。
/// upsert_note の保存直前に呼び、payload に生ユーザー情報を持たせない。
pub(crate) fn stub_user_refs(note_value: &mut serde_json::Value) {
    if let Some(id) = note_value.get("user").and_then(|u| u.get("id")).cloned() {
        note_value["user"] = serde_json::json!({ "id": id });
    }
    if note_value.get("renote").map(|r| r.is_object()).unwrap_or(false) {
        stub_user_refs(&mut note_value["renote"]);
    }
}

/// user_value が旧形式(フルオブジェクト埋め込み)かどうか。スタブは `id` のみなので
/// `username` の有無で判定する(UserLiteは常にusernameを含む)。
pub(crate) fn is_legacy_full_user(user_value: &serde_json::Value) -> bool {
    user_value.get("username").is_some()
}

/// note_value の `user`(本体+renote分)のいずれかが旧形式かどうかを再帰的に判定する。
pub(crate) fn has_legacy_full_user(note_value: &serde_json::Value) -> bool {
    if note_value.get("user").map(is_legacy_full_user).unwrap_or(false) {
        return true;
    }
    note_value.get("renote").map(has_legacy_full_user).unwrap_or(false)
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib store::user_ref`
Expected: PASS（9件: これまでの5件+今回の4件）

- [ ] **Step 5: Commit**

```bash
cd src-tauri && git add src/store/user_ref.rs
git commit -m "feat: note payload内のuser参照をスタブ化/旧形式検知するヘルパーを実装"
```

---

## Task 5: `store/user_ref.rs` — `collect_user_id_refs` / `hydrate_user_refs` / `fetch_users_by_ids`

**Files:**
- Modify: `src-tauri/src/store/user_ref.rs`

**Interfaces:**
- Consumes: `serde_json::Value`, `rusqlite::Connection`, `crate::domain::{User, InstanceInfo}`
- Produces:
  - `pub(crate) fn collect_user_id_refs(note_value: &serde_json::Value, out: &mut Vec<String>)`
  - `pub(crate) fn hydrate_user_refs(note_value: &mut serde_json::Value, users: &std::collections::HashMap<String, User>) -> bool`
    （`false` = 参照先IDがusersに無い＝このノートは復元不可）
  - `pub(crate) fn fetch_users_by_ids(conn: &Connection, ids: &[String]) -> Result<std::collections::HashMap<String, User>>`
  （すべてTask 7の`resolve_payload_rows`が使う）

- [ ] **Step 1: 失敗するテストを書く**

`user_ref.rs` の `mod tests` 内に追加:

```rust
    use std::collections::HashMap;

    #[test]
    fn collect_user_id_refs_collects_top_level_and_renote() {
        let v = json!({
            "id": "n1",
            "user": { "id": "u1" },
            "renote": { "id": "n0", "user": { "id": "u2" }, "renote": null }
        });
        let mut ids = Vec::new();
        collect_user_id_refs(&v, &mut ids);
        assert_eq!(ids, vec!["u1".to_string(), "u2".to_string()]);
    }

    #[test]
    fn hydrate_user_refs_fills_in_full_user_and_returns_true() {
        let mut v = json!({ "id": "n1", "user": { "id": "u1" } });
        let mut users = HashMap::new();
        users.insert("u1".to_string(), user_lite("u1", "Alice"));

        assert!(hydrate_user_refs(&mut v, &users));
        assert_eq!(v["user"]["username"], json!("alice"));
        assert_eq!(v["user"]["name"], json!("Alice"));
    }

    #[test]
    fn hydrate_user_refs_returns_false_when_user_missing() {
        let mut v = json!({ "id": "n1", "user": { "id": "u1" } });
        let users = HashMap::new();
        assert!(!hydrate_user_refs(&mut v, &users));
    }

    #[test]
    fn hydrate_user_refs_hydrates_renote_author_too() {
        let mut v = json!({
            "id": "n1",
            "user": { "id": "u1" },
            "renote": { "id": "n0", "user": { "id": "u2" }, "renote": null }
        });
        let mut users = HashMap::new();
        users.insert("u1".to_string(), user_lite("u1", "Alice"));
        users.insert("u2".to_string(), user_lite("u2", "Bob"));

        assert!(hydrate_user_refs(&mut v, &users));
        assert_eq!(v["renote"]["user"]["name"], json!("Bob"));
    }

    #[test]
    fn fetch_users_by_ids_returns_stored_instance_info() {
        let conn = open_cache_in_memory().unwrap();
        let mut with_instance = user_lite("u1", "Alice");
        with_instance.instance = Some(InstanceInfo {
            name: Some("Remote".into()),
            icon_url: Some("https://remote.example/icon.png".into()),
            theme_color: Some("#ff8800".into()),
        });
        upsert_user(&conn, &with_instance).unwrap();
        upsert_user(&conn, &user_lite("u2", "Bob")).unwrap(); // instance無し

        let ids = vec!["u1".to_string(), "u2".to_string(), "u3".to_string()];
        let users = fetch_users_by_ids(&conn, &ids).unwrap();

        assert_eq!(users.len(), 2); // u3 は存在しないので含まれない
        let instance = users["u1"].instance.as_ref().unwrap();
        assert_eq!(instance.name.as_deref(), Some("Remote"));
        assert!(users["u2"].instance.is_none());
    }

    #[test]
    fn fetch_users_by_ids_returns_empty_map_for_empty_input() {
        let conn = open_cache_in_memory().unwrap();
        let users = fetch_users_by_ids(&conn, &[]).unwrap();
        assert!(users.is_empty());
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd src-tauri && cargo test --lib store::user_ref::tests::hydrate_user_refs`
Expected: FAIL（未定義関数でコンパイルエラー）

- [ ] **Step 3: 実装**

`user_ref.rs` の `use` を以下に変更・追加:

```rust
use crate::domain::{InstanceInfo, Note, User};
use crate::error::Result;
use rusqlite::{params, Connection};
use std::collections::HashMap;
```

続けて追加:

```rust
/// note_value の `user.id`(本体+renote分)を出現順にすべて集める(重複可、呼び出し元で
/// dedupする想定)。stub_user_refs 済み・旧形式どちらの形にも対応する(常に `["user"]["id"]`
/// を見るだけなので形式を問わない)。
pub(crate) fn collect_user_id_refs(note_value: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(id) = note_value.get("user").and_then(|u| u.get("id")).and_then(|v| v.as_str()) {
        out.push(id.to_string());
    }
    if let Some(renote) = note_value.get("renote") {
        if renote.is_object() {
            collect_user_id_refs(renote, out);
        }
    }
}

/// note_value の `user` スタブ(本体+renote分)を users から引いてフルオブジェクトへ埋め戻す。
/// 参照先のいずれかが users に無ければ false を返す(このノートは復元不可、呼び出し元でスキップする)。
pub(crate) fn hydrate_user_refs(note_value: &mut serde_json::Value, users: &HashMap<String, User>) -> bool {
    let Some(id) = note_value
        .get("user")
        .and_then(|u| u.get("id"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
    else {
        return false;
    };
    let Some(user) = users.get(&id) else {
        return false;
    };
    note_value["user"] = serde_json::to_value(user).unwrap_or(serde_json::Value::Null);

    if note_value.get("renote").map(|r| r.is_object()).unwrap_or(false) {
        return hydrate_user_refs(&mut note_value["renote"], users);
    }
    true
}

/// `user` テーブルから id 一覧に対応する行をまとめて引く(1クエリ、N+1にしない)。
/// 見つからない id は結果のマップに含まれない(呼び出し元は hydrate_user_refs の false で検知する)。
pub(crate) fn fetch_users_by_ids(conn: &Connection, ids: &[String]) -> Result<HashMap<String, User>> {
    let mut out = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, username, host, name, avatar_url, is_bot, is_cat,
                followers_count, following_count, notes_count, emojis,
                bio, banner_url, instance_name, instance_icon_url, instance_theme_color
         FROM user WHERE id IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let bind_params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(bind_params.as_slice(), |r| {
        let emojis_json: String = r.get(10)?;
        let instance_name: Option<String> = r.get(13)?;
        let instance_icon_url: Option<String> = r.get(14)?;
        let instance_theme_color: Option<String> = r.get(15)?;
        let instance = if instance_name.is_some() || instance_icon_url.is_some() || instance_theme_color.is_some() {
            Some(InstanceInfo {
                name: instance_name,
                icon_url: instance_icon_url,
                theme_color: instance_theme_color,
            })
        } else {
            None
        };
        Ok(User {
            id: r.get(0)?,
            username: r.get(1)?,
            host: r.get(2)?,
            name: r.get(3)?,
            avatar_url: r.get(4)?,
            is_bot: r.get::<_, i64>(5)? != 0,
            is_cat: r.get::<_, i64>(6)? != 0,
            followers_count: r.get(7)?,
            following_count: r.get(8)?,
            notes_count: r.get(9)?,
            emojis: serde_json::from_str(&emojis_json).unwrap_or_default(),
            bio: r.get(11)?,
            banner_url: r.get(12)?,
            instance,
        })
    })?;
    for row in rows {
        let user = row?;
        out.insert(user.id.clone(), user);
    }
    Ok(out)
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib store::user_ref`
Expected: PASS（15件: これまでの9件+今回の6件）

- [ ] **Step 5: Commit**

```bash
cd src-tauri && git add src/store/user_ref.rs
git commit -m "feat: user参照のバッチ引き当て/ハイドレーションを実装"
```

---

## Task 6: `upsert_note` を stub化+user upsertへ配線

**Files:**
- Modify: `src-tauri/src/store/note_cache.rs:415-473`（`upsert_note`）

**Interfaces:**
- Consumes: `crate::store::user_ref::{collect_users, stub_user_refs, upsert_user}`（Task 2/3/4で実装済み）
- Produces: `upsert_note` が保存する `payload` は常に `user`(本体+renote分)がスタブ形式であること。以降のタスクが依存する不変条件。

- [ ] **Step 1: 失敗するテストを書く**

`note_cache.rs` の `mod tests` 内、既存 `cache_roundtrip_preserves_note_and_order` の近くに追加:

```rust
    #[test]
    fn upsert_note_stores_stubbed_user_in_payload() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100)]).unwrap();

        let raw_payload: String = {
            let conn = s.conn.lock().unwrap();
            conn.query_row("SELECT payload FROM note WHERE id = 'n1'", [], |r| r.get(0)).unwrap()
        };
        let v: serde_json::Value = serde_json::from_str(&raw_payload).unwrap();
        assert_eq!(v["user"], serde_json::json!({ "id": "u1" }));
    }

    #[test]
    fn upsert_note_upserts_both_note_and_renote_authors_into_user_table() {
        let s = store();
        let mut n = note("n1", 100);
        n.renote = Some(Box::new({
            let mut renoted = note("n0", 50);
            renoted.user = User {
                id: "u2".into(),
                username: "bob".into(),
                host: None,
                name: Some("Bob".into()),
                avatar_url: None,
                is_bot: false,
                is_cat: false,
                followers_count: 0,
                following_count: 0,
                notes_count: 0,
                emojis: HashMap::new(),
                bio: None,
                banner_url: None,
                instance: None,
            };
            renoted
        }));
        s.cache_notes("col1", &[n]).unwrap();

        let conn = s.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM user WHERE id IN ('u1','u2')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd src-tauri && cargo test --lib store::note_cache::tests::upsert_note_stores_stubbed_user`
Expected: FAIL（`v["user"]` がまだフルオブジェクトのまま）

- [ ] **Step 3: 実装**

`note_cache.rs` 冒頭の `use` に追加:

```rust
use crate::store::user_ref::{collect_users, stub_user_refs, upsert_user};
```

`upsert_note` の冒頭を変更（payload生成部分のみ差し替え、note insert・関連テーブル処理は不変）:

```rust
fn upsert_note(conn: &Connection, n: &Note) -> Result<()> {
    let mut payload_value = serde_json::to_value(n)?;
    stub_user_refs(&mut payload_value);
    let payload = serde_json::to_string(&payload_value)?;
    let text_length = n.text.as_deref().map(|t| t.chars().count()).unwrap_or(0) as i64;
    let has_link = n.text.as_deref().map(has_url).unwrap_or(false) as i64;
```

（この後の `conn.execute("INSERT OR REPLACE INTO note (...)", ...)` はそのまま。`payload` 変数の中身が変わるだけ）

既存の以下のブロックを:

```rust
    let u = &n.user;
    conn.execute(
        "INSERT OR REPLACE INTO user (
            id, username, host, name, is_bot, is_cat,
            followers_count, following_count, notes_count
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            u.id, u.username, u.host, u.name, u.is_bot as i64, u.is_cat as i64,
            u.followers_count, u.following_count, u.notes_count
        ],
    )?;
```

以下に置き換える:

```rust
    // 本体+renote分すべての User を正規化テーブルへ反映する(Issue #263)。
    for user in collect_users(n) {
        upsert_user(conn, user)?;
    }
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib store::note_cache`
Expected: PASS（既存の全テスト含む。ただし `cache_roundtrip_preserves_note_and_order` 等、payloadの中身を直接文字列比較していないテストは影響を受けない想定。もし壊れるテストがあれば、そのテストが `payload` の生JSONに `user.username` 等を期待していないか確認して調整する）

- [ ] **Step 5: Commit**

```bash
cd src-tauri && git add src/store/note_cache.rs
git commit -m "feat: upsert_noteでpayloadのuserをスタブ化しuserテーブルへ集約"
```

---

## Task 7: 読み込み経路の統合(`resolve_payload_rows`)と自己修復

**Files:**
- Modify: `src-tauri/src/store/note_cache.rs`
  - `load_cached` (:73-88), `load_cached_before` (:92-111), `get_note` (:116-125), `search_cache` (:251-289)
  - `deserialize_note_or_warn` (:292-302) を置き換え

**Interfaces:**
- Consumes: `crate::store::user_ref::{collect_user_id_refs, has_legacy_full_user, hydrate_user_refs, fetch_users_by_ids, is_legacy_full_user, upsert_user}`
- Produces: `fn resolve_payload_rows(conn: &Connection, rows: Vec<(String, String)>) -> Result<Vec<Note>>`
  （4つの読み込み関数すべてがこれを使う。以降このモジュール内だけで完結し、他モジュールから直接呼ばれない）

- [ ] **Step 1: 失敗するテストを書く**

`note_cache.rs` の `mod tests` 内に追加:

```rust
    /// 旧形式(userフルオブジェクト埋め込み)の行を素のSQLで作る(upsert_noteを経由しない=
    /// Issue #263 以前に保存された実データの形を再現する)。
    fn insert_legacy_row(conn: &Connection, note_id: &str, created_at: i64, user_json: serde_json::Value) {
        let mut n = note(note_id, created_at);
        let mut v = serde_json::to_value(&n).unwrap();
        v["user"] = user_json;
        let payload = serde_json::to_string(&v).unwrap();
        n.user.id = v["user"]["id"].as_str().unwrap_or("").to_string();
        conn.execute(
            "INSERT INTO note (
                id, created_at, text, text_length, cw, visibility, local_only, user_id,
                reply_id, reply_user_id, renote_id, channel_id, via, lang,
                files_count, has_poll, has_link, is_pinned,
                reaction_count, renote_count, reply_count, my_reaction,
                is_renoted_by_me, is_favorited_by_me, payload
            ) VALUES (?1, ?2, '', 0, NULL, 'home', 0, ?3, NULL, NULL, NULL, NULL, NULL, NULL,
                0, 0, 0, 0, 0, 0, 0, NULL, 0, 0, ?4)",
            params![note_id, created_at, n.user.id, payload],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO column_note (column_id, note_id, received_at, created_at) VALUES ('col1', ?1, 0, ?2)",
            params![note_id, created_at],
        )
        .unwrap();
    }

    #[test]
    fn load_cached_self_heals_legacy_full_user_payload() {
        let s = store();
        {
            let conn = s.conn.lock().unwrap();
            insert_legacy_row(
                &conn,
                "n_legacy",
                100,
                serde_json::json!({
                    "id": "u_legacy", "username": "carol", "host": "remote.example",
                    "name": "Carol", "avatarUrl": null, "isBot": false, "isCat": false,
                    "followersCount": 0, "followingCount": 0, "notesCount": 0,
                    "emojis": {}, "bio": null, "bannerUrl": null,
                    "instance": { "name": "Remote", "iconUrl": "https://remote.example/icon.png", "themeColor": "#ff8800" }
                }),
            );
        }

        let got = s.load_cached("col1", 10).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].user.id, "u_legacy");
        let instance = got[0].user.instance.as_ref().expect("instance should be hydrated");
        assert_eq!(instance.name.as_deref(), Some("Remote"));

        // payload がスタブ形式へ書き戻されていること
        let conn = s.conn.lock().unwrap();
        let raw: String = conn.query_row("SELECT payload FROM note WHERE id = 'n_legacy'", [], |r| r.get(0)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["user"], serde_json::json!({ "id": "u_legacy" }));

        // user テーブルへ抽出されていること
        let name: String = conn.query_row("SELECT name FROM user WHERE id = 'u_legacy'", [], |r| r.get(0)).unwrap();
        assert_eq!(name, "Carol");
    }

    #[test]
    fn load_cached_skips_note_when_referenced_user_row_missing() {
        let s = store();
        {
            let conn = s.conn.lock().unwrap();
            insert_legacy_row(&conn, "n_orphan", 100, serde_json::json!({ "id": "u_orphan" }));
        }

        let got = s.load_cached("col1", 10).unwrap();
        assert!(got.is_empty());
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd src-tauri && cargo test --lib store::note_cache::tests::load_cached_self_heals`
Expected: FAIL（`got[0].user.instance` が `None`。まだスタブ解決していない）

- [ ] **Step 3: 実装**

`note_cache.rs` 冒頭の `use` を更新:

```rust
use crate::store::user_ref::{
    collect_user_id_refs, collect_users, fetch_users_by_ids, has_legacy_full_user,
    hydrate_user_refs, is_legacy_full_user, stub_user_refs, upsert_user,
};
```

`deserialize_note_or_warn` を削除し、代わりに以下を追加:

```rust
/// (note_id, payload) の行群を Note へ復元する(Issue #263)。
/// - 旧形式(userフルオブジェクト埋め込み)を検知した行は、その場で user テーブルへ抽出し
///   payload をスタブ形式へ書き戻してから復元する(自己修復)。
/// - user参照(本体+renote分)が user テーブルに見つからない行は、ログ警告してスキップする
///   (呼び出し元をエラーにしない。deserialize_note_or_warn と同じポリシー)。
fn resolve_payload_rows(conn: &Connection, rows: Vec<(String, String)>) -> Result<Vec<Note>> {
    let mut values: Vec<(String, serde_json::Value)> = Vec::with_capacity(rows.len());
    for (id, payload) in rows {
        match serde_json::from_str::<serde_json::Value>(&payload) {
            Ok(mut v) => {
                if has_legacy_full_user(&v) {
                    self_heal_legacy_row(conn, &id, &mut v)?;
                }
                values.push((id, v));
            }
            Err(e) => {
                log::warn!("skipping note cache row {id} with unparsable payload: {e}");
            }
        }
    }

    let mut ids = Vec::new();
    for (_, v) in &values {
        collect_user_id_refs(v, &mut ids);
    }
    ids.sort();
    ids.dedup();
    let users = fetch_users_by_ids(conn, &ids)?;

    let mut out = Vec::with_capacity(values.len());
    for (id, mut v) in values {
        if !hydrate_user_refs(&mut v, &users) {
            log::warn!("skipping note cache row {id}: referenced user not found in user table");
            continue;
        }
        match serde_json::from_value::<Note>(v) {
            Ok(note) => out.push(note),
            Err(e) => log::warn!("skipping note cache row {id} with undeserializable payload: {e}"),
        }
    }
    Ok(out)
}

/// 旧形式(userフルオブジェクト埋め込み)の行を検知した際、抽出できた user を
/// user テーブルへ upsert し、抽出できた箇所だけ payload をスタブ形式へ書き戻す
/// (抽出に失敗した箇所は元のまま残す。中途半端な書き換えで既存データを失わないため)。
fn self_heal_legacy_row(conn: &Connection, note_id: &str, value: &mut serde_json::Value) -> Result<()> {
    let changed = self_heal_node(conn, value)?;
    if changed {
        let new_payload = serde_json::to_string(value)?;
        conn.execute("UPDATE note SET payload = ?1 WHERE id = ?2", params![new_payload, note_id])?;
    }
    Ok(())
}

/// 1ノード分(本体 or renote)の user を自己修復する。renote へ再帰する。
/// 戻り値: このノード以下で1箇所でも書き換えたら true。
fn self_heal_node(conn: &Connection, node: &mut serde_json::Value) -> Result<bool> {
    let mut changed = false;
    if let Some(user_value) = node.get("user").cloned() {
        if is_legacy_full_user(&user_value) {
            if let Ok(user) = serde_json::from_value::<crate::domain::User>(user_value.clone()) {
                upsert_user(conn, &user)?;
                if let Some(id) = user_value.get("id").cloned() {
                    node["user"] = serde_json::json!({ "id": id });
                    changed = true;
                }
            }
        }
    }
    if node.get("renote").map(|r| r.is_object()).unwrap_or(false) {
        changed |= self_heal_node(conn, &mut node["renote"])?;
    }
    Ok(changed)
}
```

続けて、4つの読み込み関数を書き換える。

`load_cached`:

```rust
    pub fn load_cached(&self, column_id: &str, limit: u32) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT n.id, n.payload FROM column_note cn
             JOIN note n ON n.id = cn.note_id
             WHERE cn.column_id = ?1
             ORDER BY cn.created_at DESC, cn.note_id DESC
             LIMIT ?2",
        )?;
        let rows: Vec<(String, String)> = stmt
            .query_map(params![column_id, limit], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        resolve_payload_rows(&conn, rows)
    }
```

`load_cached_before`（末尾のソートはそのまま残す）:

```rust
    pub fn load_cached_before(&self, column_id: &str, until_id: &str, limit: u32) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT n.id, n.payload FROM column_note cn
             JOIN note n ON n.id = cn.note_id
             WHERE cn.column_id = ?1 AND cn.note_id < ?2
             ORDER BY cn.note_id DESC
             LIMIT ?3",
        )?;
        let rows: Vec<(String, String)> = stmt
            .query_map(params![column_id, until_id, limit], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut out = resolve_payload_rows(&conn, rows)?;
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
        Ok(out)
    }
```

`get_note`:

```rust
    pub fn get_note(&self, note_id: &str) -> Result<Option<Note>> {
        let conn = self.conn.lock().unwrap();
        let row: Option<(String, String)> = conn
            .query_row("SELECT id, payload FROM note WHERE id = ?1", params![note_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        Ok(match row {
            Some((id, payload)) => resolve_payload_rows(&conn, vec![(id, payload)])?.into_iter().next(),
            None => None,
        })
    }
```

`search_cache`（`SELECT` 部分と読み込みループのみ変更、`WHERE`/`ORDER BY`/bind処理は不変）:

```rust
        let mut sql = String::from(
            "SELECT n.id, n.payload FROM note n JOIN user u ON u.id = n.user_id WHERE (",
        );
        sql.push_str(&where_sql.sql);
        sql.push(')');
        if until_id.is_some() {
            sql.push_str(" AND n.id < ?");
        }
        sql.push_str(" ORDER BY n.created_at DESC, n.id DESC LIMIT ?");

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&sql)?;
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        for p in &where_sql.params {
            binds.push(match p {
                SqlParam::Text(s) => Box::new(s.clone()),
                SqlParam::Real(x) => Box::new(*x),
            });
        }
        if let Some(u) = until_id {
            binds.push(Box::new(u.to_string()));
        }
        binds.push(Box::new(limit));
        let params_ref: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
        let rows: Vec<(String, String)> = stmt
            .query_map(params_ref.as_slice(), |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        resolve_payload_rows(&conn, rows)
    }
```

- [ ] **Step 4: 既存テストの参照を更新**

`load_cached_skips_row_with_legacy_array_emojis_payload` と `search_cache_skips_row_with_legacy_array_emojis_payload` はそのまま(`deserialize_note_or_warn`を直接呼んでいないため変更不要)。`payload_with_array_emojis` ヘルパーも変更不要(引き続き有効なテストフィクスチャ)。

- [ ] **Step 5: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib store::note_cache`
Expected: PASS（全件。旧`deserialize_note_or_warn`直接テストは無かったことを確認）

- [ ] **Step 6: Commit**

```bash
cd src-tauri && git add src/store/note_cache.rs
git commit -m "feat: 読み込み経路でuser参照の自己修復とバッチハイドレーションを実施"
```

---

## Task 8: 全体テスト・実データ検証・仕上げ

**Files:**
- なし(検証のみ。問題が見つかった場合のみ Task 6/7 のファイルを修正)

**Interfaces:**
- Consumes: Task 1〜7 の成果物すべて
- Produces: グリーンな `cargo test`、フロントエンドバインディング無変更の確認、実データでの動作確認レポート

- [ ] **Step 1: Rust全体テスト**

Run: `cd src-tauri && cargo test`
Expected: PASS（`#[ignore]` の実接続テストを除く全件）

- [ ] **Step 2: TSバインディング無変更の確認**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。加えて `git status frontend/src/bindings/tauri.gen.ts` で差分が出ないことを確認する（設計docの「影響範囲外」節どおり、`Note`/`User`型は変えていないはず）。

- [ ] **Step 3: clippy**

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: PASS（新規追加コードに警告が無いこと）

- [ ] **Step 4: 実データでの手動確認**

手元の実 `cache.db`（48,470件、Instance Ticker実装前にキャッシュされた古いノートを含む）を使い、`cargo tauri dev` で実際に起動して以下を確認する:

- 以前 host名フォールバック表示だった古いリモートノートで、本来のInstance Ticker（アイコン・色付き）が表示されること。
- 起動・スクロールの体感速度が変化していないこと。
- 確認後、検証用に立てた `cargo tauri dev` は必ず自分で終了する。

- [ ] **Step 5: Commit（該当があれば）**

Step 1〜4で修正が発生した場合のみ:

```bash
cd src-tauri && git add -A
git commit -m "fix: レビュー指摘を反映"
```

修正が無ければこのステップは不要（コミットするものが無いため）。
