# 猫耳アバター表示 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `isCat`なユーザーのアバターに、Misskey本家(`MkAvatar.vue`)同様のCSSトリック(border-radius+rotate+skew)で猫耳を重ね描画する。耳の色はAPIから取得済みの`avatarBlurhash`文字列から抽出した平均色を使う。

**Architecture:** バックエンドは`User`/`Account`ドメインに`avatarBlurhash`(と`Account`には`isCat`も)を追加し、SQLite/Postgres両キャッシュバックエンドへ配線する。フロントエンドは新規`Avatar.svelte`(既存のavatar markupを包む薄いラッパー)と、BlurHashから平均色を取り出す純関数`extractAvgColorFromBlurhash`を追加し、既存6ファイルの avatar 表示箇所を`<Avatar>`でラップする。

**Tech Stack:** Rust(rusqlite, sqlx/sea-query), Svelte 5(runes, snippets), Vitest, `@testing-library/svelte`。

## Global Constraints

- 対象範囲は `NoteCard.svelte` / `ProfileModal.svelte` / `FollowListModal.svelte` / `NotificationCard.svelte` / `ReactionUsersPopover.svelte` / `AccountSelect.svelte` / `AccountsSection.svelte`。`MfmNode.svelte`のメンションアバターは対象外。
- 猫耳表示に設定トグルは設けない(`isCat`のみを条件とする、nyaizeと同じ扱い)。
- BlurHash文字列自体はMisskey APIが既に返す(`avatarBlurhash: string | null`、`src-tauri/openapi/misskey-api-doc.json`で確認済み)。tsumugi側でBlurHashを生成することはしない。平均色抽出のみ自前実装する。
- 耳の色のフォールバック(`avatarBlurhash`が無い場合)は`var(--border)`。
- 新規フィールドは既存キャッシュ済みJSON/設定JSONとの後方互換のため`#[serde(default)]`を付与する(`User.bio`/`Account.instance`と同じパターン)。
- コミットはタスムごとに小さく、都度`cargo test`(Rust変更時)または`pnpm test`/`pnpm check`(フロントエンド変更時)を通してから次のタスクへ進む。

---

## File Structure

**変更(バックエンド):**
- `src-tauri/src/domain/user.rs` — `User`に`avatar_blurhash`追加
- `src-tauri/src/api/normalize.rs` — `RawUser`に`avatar_blurhash`追加、`From<RawUser> for User`にマッピング追加
- `src-tauri/src/domain/account.rs` — `Account`に`is_cat`・`avatar_blurhash`追加
- `src-tauri/src/commands/account.rs` — `build_account`で新フィールドを埋める
- `src-tauri/src/state.rs` / `src-tauri/src/stream/connection.rs` / `src-tauri/src/session/account_manager.rs` / `src-tauri/src/store/settings.rs` — `Account {}`リテラル構築箇所に新フィールドを追加
- `src-tauri/src/store/db.rs` — `migrate_cache`にSQLite `user`テーブルへの列追加マイグレーション
- `src-tauri/src/store/user_ref.rs` — SQLite版upsert/fetch/fill_from_snapshotに列を追加
- `src-tauri/src/store/postgres_backend.rs` — `ensure_schema`のDDLと既存テーブル向け`ALTER TABLE ... ADD COLUMN IF NOT EXISTS`
- `src-tauri/src/store/postgres_user_ref.rs` — Postgres版upsert/fetch/fill_from_snapshotに列を追加

**新規(フロントエンド):**
- `frontend/src/lib/blurhash.ts` — `extractAvgColorFromBlurhash`
- `frontend/src/lib/blurhash.test.ts`
- `frontend/src/ui/Avatar.svelte` — 猫耳ラッパーコンポーネント
- `frontend/src/ui/Avatar.test.ts`

**変更(フロントエンド、既存avatar表示箇所を`<Avatar>`でラップ):**
- `frontend/src/ui/NoteCard.svelte`
- `frontend/src/ui/ProfileModal.svelte`
- `frontend/src/ui/FollowListModal.svelte`
- `frontend/src/ui/NotificationCard.svelte`
- `frontend/src/ui/ReactionUsersPopover.svelte`
- `frontend/src/ui/AccountSelect.svelte`
- `frontend/src/ui/settings/AccountsSection.svelte`

---

### Task 1: `domain::User` に `avatar_blurhash` を追加し、API正規化に配線する

**Files:**
- Modify: `src-tauri/src/domain/user.rs`
- Modify: `src-tauri/src/api/normalize.rs`
- Test: 上記2ファイル内の`#[cfg(test)]`モジュール

**Interfaces:**
- Produces: `User.avatar_blurhash: Option<String>`(`#[serde(default)]`、camelCaseで`avatarBlurhash`)。以降の全タスクがこのフィールド名を使う。

- [ ] **Step 1: `User`に後方互換テストを追加(失敗させる)**

`src-tauri/src/domain/user.rs`の`mod tests`に追加:

```rust
    /// avatarBlurhash フィールド追加前に保存されたキャッシュ済みJSONを読み込めること。
    #[test]
    fn deserializes_without_avatar_blurhash_for_backward_compat() {
        let json = r#"{
            "id":"u1","username":"alice","host":null,"name":"Alice",
            "avatarUrl":null,"isBot":false,"isCat":false,
            "followersCount":0,"followingCount":0,"notesCount":0
        }"#;
        let u: User = serde_json::from_str(json).unwrap();
        assert_eq!(u.avatar_blurhash, None);
    }
```

- [ ] **Step 2: テストを実行し失敗を確認**

Run: `cd src-tauri && cargo test --lib domain::user::tests::deserializes_without_avatar_blurhash_for_backward_compat`
Expected: FAIL（`avatar_blurhash`フィールドが存在せずコンパイルエラー）

- [ ] **Step 3: `User`構造体にフィールドを追加**

`src-tauri/src/domain/user.rs`の`pub instance: Option<InstanceInfo>,`の直後に追加:

```rust
    /// アバター画像のBlurHash文字列。猫耳の色抽出に使う(フロント側 `extractAvgColorFromBlurhash`)。
    /// 追加前に保存されたキャッシュ済みJSONとの後方互換のため default。
    #[serde(default)]
    pub avatar_blurhash: Option<String>,
```

- [ ] **Step 4: `RawUser`・`From<RawUser> for User`を更新**

`src-tauri/src/api/normalize.rs`の`RawUser`構造体、`pub instance: Option<RawInstanceInfo>,`の直後に追加:

```rust
    #[serde(default)]
    pub avatar_blurhash: Option<String>,
```

`impl From<RawUser> for User`の`User { ... }`リテラル内、`instance,`の直前に追加:

```rust
            avatar_blurhash: r.avatar_blurhash.clone(),
```

(`r.avatar_blurhash`を先にcloneしてから`r`の他フィールドを使うため、`instance`計算より前に`let avatar_blurhash = r.avatar_blurhash.clone();`としてから使っても良いが、`r.instance`は`map`で消費されるだけで`r.avatar_blurhash`とは独立したフィールドなので、リテラル内で直接`r.avatar_blurhash.clone()`を書けば問題ない。)

- [ ] **Step 5: 全テストを再実行して通ることを確認**

Run: `cd src-tauri && cargo test --lib domain::user:: api::normalize::`
Expected: PASS(全件)

- [ ] **Step 6: normalize.rs 側にもJSON→Userの取り込みテストを追加**

`src-tauri/src/api/normalize.rs`の`#[cfg(test)]`モジュールに追加(既存の`RawUser`デシリアライズテストの近くに配置。既存テストの名前・構成は`cargo test --lib api::normalize:: -- --list`で事前に確認してから、同じ`json!`ヘルパーやフィクスチャがあれば流用する):

```rust
    #[test]
    fn raw_user_maps_avatar_blurhash_into_user() {
        let json = r#"{"id":"u1","username":"alice","avatarBlurhash":"LEHV6nWB2yk8pyo0adR*.7kCMdnj"}"#;
        let raw: RawUser = serde_json::from_str(json).unwrap();
        let user: User = raw.into();
        assert_eq!(user.avatar_blurhash.as_deref(), Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj"));
    }
```

- [ ] **Step 7: テスト実行**

Run: `cd src-tauri && cargo test --lib api::normalize::tests::raw_user_maps_avatar_blurhash_into_user`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/domain/user.rs src-tauri/src/api/normalize.rs
git commit -m "feat: UserにavatarBlurhashを追加(Issue #41)"
```

(コミットメッセージ末尾に `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` を別引数で付与すること。)

---

### Task 2: `domain::Account` に `is_cat` / `avatar_blurhash` を追加する

**Files:**
- Modify: `src-tauri/src/domain/account.rs`
- Modify: `src-tauri/src/state.rs:209-217`
- Modify: `src-tauri/src/stream/connection.rs:1180-1188`
- Modify: `src-tauri/src/session/account_manager.rs:96-106`
- Modify: `src-tauri/src/store/settings.rs`(`migrate_from_legacy_sqlite`内のリテラルと、テストヘルパー`fn account(id: &str) -> Account`)
- Modify: `src-tauri/src/commands/user.rs:120-130`(テストヘルパー`fn make_account`)

**Interfaces:**
- Consumes: なし(Task 1と独立)
- Produces: `Account.is_cat: bool`・`Account.avatar_blurhash: Option<String>`(両方`#[serde(default)]`、camelCaseで`isCat`/`avatarBlurhash`)。Task 3以降が使う。

- [ ] **Step 1: `Account`構造体にフィールドを追加**

`src-tauri/src/domain/account.rs`の`pub instance: Option<crate::domain::InstanceInfo>,`の直後に追加:

```rust
    /// ログイン中ユーザー自身が`isCat`か(アカウント切替UIでの猫耳表示用)。
    #[serde(default)]
    pub is_cat: bool,
    /// アバター画像のBlurHash文字列(猫耳の色抽出用)。
    #[serde(default)]
    pub avatar_blurhash: Option<String>,
```

- [ ] **Step 2: ビルドして全リテラル構築箇所のコンパイルエラーを確認**

Run: `cd src-tauri && cargo build 2>&1 | grep "missing structure fields\|error\[E0063\]" -A 3`
Expected: `state.rs`・`stream/connection.rs`・`session/account_manager.rs`・`store/settings.rs`(2箇所)・`commands/user.rs`の`Account { ... }`リテラルで`is_cat`・`avatar_blurhash`が無いというE0063エラーが出る

- [ ] **Step 3: 各リテラルにフィールドを追加**

`src-tauri/src/state.rs:216`の`avatar_url: None,`と`instance: None,`の間、または`instance: None,`の直後に追加(以下すべて同じパターン):

```rust
                is_cat: false,
                avatar_blurhash: None,
```

対象箇所:
- `src-tauri/src/state.rs:209-217`の`Account { ... instance: None, }`
- `src-tauri/src/stream/connection.rs:1180-1188`の`Account { ... instance: None, }`
- `src-tauri/src/session/account_manager.rs:96-106`の`fn acc(...) -> Account { ... instance: None, }`
- `src-tauri/src/commands/user.rs:120-130`の`fn make_account(...) -> Account { ... instance: None, }`
- `src-tauri/src/store/settings.rs`の`fn account(id: &str) -> Account { ... instance: None, }`(テストヘルパー)
- `src-tauri/src/store/settings.rs`の`migrate_from_legacy_sqlite`内、`Ok(Account { ... instance: None, })`(旧SQLite`account`テーブルには該当列が無いため、常に`is_cat: false, avatar_blurhash: None`で固定する)

インデント幅は各ファイルの既存の`instance: None,`と揃える。

- [ ] **Step 4: ビルドが通ることを確認**

Run: `cd src-tauri && cargo build`
Expected: 成功(E0063エラーが消える)

- [ ] **Step 5: 全テスト実行**

Run: `cd src-tauri && cargo test --lib`
Expected: PASS(全件。既存テストの挙動は変えていないので新規失敗は無いはず)

- [ ] **Step 6: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/domain/account.rs src-tauri/src/state.rs src-tauri/src/stream/connection.rs src-tauri/src/session/account_manager.rs src-tauri/src/store/settings.rs src-tauri/src/commands/user.rs
git commit -m "feat: AccountにisCat/avatarBlurhashを追加(Issue #41)"
```

---

### Task 3: `build_account` で `is_cat` / `avatar_blurhash` を埋める

**Files:**
- Modify: `src-tauri/src/commands/account.rs:141-149`

**Interfaces:**
- Consumes: `RawUser.is_cat: bool`（既存）、`RawUser.avatar_blurhash: Option<String>`（Task 1で追加）、`Account.is_cat`/`Account.avatar_blurhash`（Task 2で追加）
- Produces: ログイン時に構築される`Account`が`isCat`/`avatarBlurhash`を正しく持つ

- [ ] **Step 1: 失敗するテストを追加**

`src-tauri/src/commands/account.rs`の`mod tests`、`build_account_uses_name_then_username`の近くに追加:

```rust
    #[test]
    fn build_account_carries_is_cat_and_avatar_blurhash() {
        let raw: RawUser = serde_json::from_str(
            r#"{"id":"u1","username":"alice","isCat":true,"avatarBlurhash":"LEHV6nWB2yk8pyo0adR*.7kCMdnj"}"#,
        )
        .unwrap();
        let a = build_account(None, "misskey.io", &raw);
        assert!(a.is_cat);
        assert_eq!(a.avatar_blurhash.as_deref(), Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj"));
    }
```

- [ ] **Step 2: テスト実行し失敗を確認**

Run: `cd src-tauri && cargo test --lib commands::account::tests::build_account_carries_is_cat_and_avatar_blurhash`
Expected: FAIL(`a.is_cat`が`false`、`a.avatar_blurhash`が`None`のままでassert失敗)

- [ ] **Step 3: `build_account`を更新**

`src-tauri/src/commands/account.rs:141-149`を以下に置き換え:

```rust
fn build_account(existing_id: Option<String>, host: &str, raw: &RawUser) -> Account {
    Account {
        id: existing_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        host: host.to_string(),
        username: raw.username.clone(),
        user_id: raw.id.clone(),
        display_name: raw.name.clone().unwrap_or_else(|| raw.username.clone()),
        avatar_url: raw.avatar_url.clone(),
        instance: None,
        is_cat: raw.is_cat,
        avatar_blurhash: raw.avatar_blurhash.clone(),
    }
}
```

- [ ] **Step 4: テスト実行して通ることを確認**

Run: `cd src-tauri && cargo test --lib commands::account::`
Expected: PASS(全件)

- [ ] **Step 5: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/commands/account.rs
git commit -m "feat: build_accountでisCat/avatarBlurhashを埋める(Issue #41)"
```

---

### Task 4: SQLiteキャッシュ(`user`テーブル)に `avatar_blurhash` 列を追加する

**Files:**
- Modify: `src-tauri/src/store/db.rs`(`migrate_cache`関数、230-273行目付近)
- Modify: `src-tauri/src/store/user_ref.rs`

**Interfaces:**
- Consumes: `User.avatar_blurhash`(Task 1)
- Produces: SQLiteキャッシュに保存された`User`を`fetch_users_by_ids`で読み戻したとき`avatar_blurhash`が復元される

- [ ] **Step 1: `db.rs`に失敗するマイグレーションテストを追加**

`src-tauri/src/store/db.rs`の`migrate_cache_adds_user_normalization_columns`の直後に追加:

```rust
    #[test]
    fn migrate_cache_adds_avatar_blurhash_column() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE user (
                id TEXT PRIMARY KEY, username TEXT NOT NULL, host TEXT, name TEXT,
                is_bot INTEGER NOT NULL DEFAULT 0, is_cat INTEGER NOT NULL DEFAULT 0,
                followers_count INTEGER NOT NULL DEFAULT 0,
                following_count INTEGER NOT NULL DEFAULT 0,
                notes_count INTEGER NOT NULL DEFAULT 0,
                avatar_url TEXT, bio TEXT, banner_url TEXT, emojis TEXT NOT NULL DEFAULT '{}',
                instance_name TEXT, instance_icon_url TEXT, instance_theme_color TEXT
            );
            CREATE TABLE note (id TEXT PRIMARY KEY, created_at INTEGER NOT NULL);
            CREATE TABLE column_note (
                column_id TEXT NOT NULL, note_id TEXT NOT NULL, received_at INTEGER NOT NULL,
                PRIMARY KEY (column_id, note_id)
            );",
        )
        .unwrap();

        migrate_cache(&conn).unwrap();

        assert!(column_exists(&conn, "user", "avatar_blurhash").unwrap());
        // 冪等
        migrate_cache(&conn).unwrap();
    }
```

- [ ] **Step 2: テスト実行し失敗を確認**

Run: `cd src-tauri && cargo test --lib store::db::tests::migrate_cache_adds_avatar_blurhash_column`
Expected: FAIL(`avatar_blurhash`列が存在しない)

- [ ] **Step 3: `migrate_cache`にマイグレーションブロックを追加**

`src-tauri/src/store/db.rs`の`migrate_cache`関数内、Issue #263の列追加ブロック(242-252行目)の直後に追加:

```rust
    // Issue #41: 猫耳表示の色抽出用。avatarBlurhash を正規化テーブルにも保持する。
    if !column_exists(conn, "user", "avatar_blurhash")? {
        conn.execute_batch("ALTER TABLE user ADD COLUMN avatar_blurhash TEXT;")?;
    }
```

- [ ] **Step 4: テスト実行して通ることを確認**

Run: `cd src-tauri && cargo test --lib store::db::tests::migrate_cache_adds_avatar_blurhash_column`
Expected: PASS

- [ ] **Step 5: `user_ref.rs`の`upsert_user`を更新**

`src-tauri/src/store/user_ref.rs`の`upsert_user`関数(16-64行目)を以下に置き換え:

```rust
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
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color,
            avatar_blurhash
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
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
            instance_theme_color = COALESCE(excluded.instance_theme_color, user.instance_theme_color),
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
            instance_name,
            instance_icon_url,
            instance_theme_color,
            user.avatar_blurhash,
        ],
    )?;
    Ok(())
}
```

(`avatar_blurhash`は`bio`/`banner_url`と同様、UserLiteでは省略されうる値としてCOALESCEで既存値を保護する扱いにする。)

- [ ] **Step 6: `fill_user_from_snapshot`を更新**

同ファイルの`fill_user_from_snapshot`関数(75-123行目)も同じパターンで`avatar_blurhash`列を追加する(INSERT列リスト・VALUESプレースホルダ・`ON CONFLICT`の`avatar_blurhash = COALESCE(user.avatar_blurhash, excluded.avatar_blurhash)`・`params![]`末尾に`user.avatar_blurhash`を追加)。

- [ ] **Step 7: `fetch_users_by_ids`を更新**

同ファイルの`fetch_users_by_ids`関数(198-248行目)のSQLの`SELECT`列リストに`avatar_blurhash`を追加し、`query_map`のクロージャで`r.get(16)?`として読み取り、`User { ... }`リテラルに`avatar_blurhash: r.get(16)?,`を追加する:

```rust
    let sql = format!(
        "SELECT id, username, host, name, avatar_url, is_bot, is_cat,
                followers_count, following_count, notes_count, emojis,
                bio, banner_url, instance_name, instance_icon_url, instance_theme_color,
                avatar_blurhash
         FROM user WHERE id IN ({placeholders})"
    );
```

`Ok(User { ... })`リテラル内、`instance,`の直前に追加:

```rust
            avatar_blurhash: r.get(16)?,
```

- [ ] **Step 8: 既存のテストヘルパー`user_lite`を更新**

同ファイルの`#[cfg(test)]`内`fn user_lite(...)`の`instance: None,`の直後に追加:

```rust
            avatar_blurhash: None,
```

- [ ] **Step 9: ラウンドトリップテストを追加**

`user_ref.rs`の`mod tests`に追加:

```rust
    #[test]
    fn upsert_user_and_fetch_roundtrips_avatar_blurhash() {
        let conn = open_cache_in_memory().unwrap();
        let mut u = user_lite("u1", "Alice");
        u.avatar_blurhash = Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj".into());
        upsert_user(&conn, &u).unwrap();

        let got = fetch_users_by_ids(&conn, &["u1".to_string()]).unwrap();
        assert_eq!(
            got["u1"].avatar_blurhash.as_deref(),
            Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj")
        );
    }
```

- [ ] **Step 10: 全テスト実行**

Run: `cd src-tauri && cargo test --lib store::db:: store::user_ref::`
Expected: PASS(全件)

- [ ] **Step 11: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/store/db.rs src-tauri/src/store/user_ref.rs
git commit -m "feat: SQLiteユーザーキャッシュにavatarBlurhash列を追加(Issue #41)"
```

---

### Task 5: Postgres/MySQLキャッシュバックエンドに `avatar_blurhash` 列を追加する

> **注記(Task 1実行後に判明):** `src-tauri/src/store/mod.rs`にはPostgresに加えてMySQLバックエンド(`mysql_backend.rs`・`mysql_user_ref.rs`、`postgres_backend.rs`/`postgres_user_ref.rs`と全く同じ構造の並行実装)も存在する。当初の設計時に見落としており、本タスクはPostgresとMySQLの両方を対象にする(Step 1-10がPostgres、Step 11-19がMySQL)。Task 1のコミットで`mysql_backend.rs`/`mysql_user_ref.rs`のテスト用`User {}`リテラルには既に`avatar_blurhash: None,`が追加済み(コンパイルを通すための機械的な追加)なので、本タスクはDDL・upsert・fetchのロジック配線のみを行う。

**Files:**
- Modify: `src-tauri/src/store/postgres_backend.rs`(`ensure_schema`関数、`UserTable` enum)
- Modify: `src-tauri/src/store/postgres_user_ref.rs`
- Modify: `src-tauri/src/store/mysql_backend.rs`(`ensure_schema`関数、`UserTable` enum)
- Modify: `src-tauri/src/store/mysql_user_ref.rs`

**Interfaces:**
- Consumes: `User.avatar_blurhash`(Task 1)
- Produces: Postgres/MySQLキャッシュに保存された`User`を`fetch_users_by_ids`で読み戻したとき`avatar_blurhash`が復元される

- [ ] **Step 1: `UserTable` enumに列を追加**

`src-tauri/src/store/postgres_backend.rs`の`enum UserTable`(342-360行目)、`InstanceThemeColor,`の直後に追加:

```rust
    AvatarBlurhash,
```

- [ ] **Step 2: `ensure_schema`のDDLに列を追加**

同ファイルの`ensure_schema`関数内、`user`テーブル定義(107-127行目)の`.col(ColumnDef::new(UserTable::InstanceThemeColor).text())`の直後に追加:

```rust
        .col(ColumnDef::new(UserTable::AvatarBlurhash).text())
```

- [ ] **Step 3: 既存テーブル向けの明示的マイグレーションを追加**

`sea_query`の`Table::create().if_not_exists()`は既存テーブルへの列追加を行わないため、`ensure_schema`関数の`pool.execute(user.as_str()).await?;`の直後に追加:

```rust
    // Issue #41: 猫耳表示の色抽出用。sea_query の CREATE TABLE IF NOT EXISTS は既存テーブルへの
    // 列追加を行わないため、`user` テーブルが既に存在する既存インストール向けに明示的な
    // ALTER TABLE ... ADD COLUMN IF NOT EXISTS を別途実行する(Postgres専用構文、冪等)。
    pool.execute("ALTER TABLE \"user\" ADD COLUMN IF NOT EXISTS avatar_blurhash TEXT").await?;
```

- [ ] **Step 4: `postgres_user_ref.rs`の`upsert_user`・`fill_user_from_snapshot`・`fetch_users_by_ids`を更新**

`src-tauri/src/store/postgres_user_ref.rs`を、Task 4でSQLite版に加えた変更と同じ内容でPostgres構文(`$1`等のプレースホルダ、`sqlx::query`)に合わせて更新する:

- `upsert_user`(9-57行目): INSERT列リストに`avatar_blurhash`を追加、`VALUES`のプレースホルダを`$17`まで拡張、`ON CONFLICT ... DO UPDATE SET`に`avatar_blurhash = excluded.avatar_blurhash`を追加、末尾の`.bind(&user.avatar_blurhash)`を追加。
- `fill_user_from_snapshot`(61-109行目): 同様に列追加。`ON CONFLICT`側は`avatar_blurhash = COALESCE("user".avatar_blurhash, excluded.avatar_blurhash)`。
- `fetch_users_by_ids`(111-154行目): `SELECT`列リストとタプル型引数リストに`avatar_blurhash`(`Option<String>`)を追加し、`User { ... }`リテラルに`avatar_blurhash`を追加。

- [ ] **Step 5: テストヘルパー`fn user(id: &str) -> User`を更新**

`postgres_user_ref.rs`の`#[cfg(test)]`内`fn user(...)`の`instance: None,`の直後に追加:

```rust
            avatar_blurhash: None,
```

- [ ] **Step 6: `#[ignore]`のラウンドトリップテストを追加**

`postgres_user_ref.rs`の`mod tests`、`upsert_user_roundtrip`の直後に追加:

```rust
    #[tokio::test]
    #[ignore]
    async fn upsert_user_roundtrips_avatar_blurhash() {
        let pool = pool().await;
        let mut u = user("u1");
        u.avatar_blurhash = Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj".into());
        upsert_user(&pool, &u).await.unwrap();

        let got = fetch_users_by_ids(&pool, &["u1".to_string()]).await.unwrap();
        assert_eq!(
            got.get("u1").unwrap().avatar_blurhash.as_deref(),
            Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj")
        );
    }
```

- [ ] **Step 7: コンパイルを確認(Postgres実テストは`#[ignore]`でCIでは通常実行しない)**

Run: `cd src-tauri && cargo test --lib --no-run`
Expected: 成功(コンパイルエラー無し)

- [ ] **Step 8: Docker等でPostgresが使える場合のみ、無視テストも実行して確認**

Run: `cd src-tauri && cargo test --lib store::postgres_user_ref:: -- --ignored`
Expected: PASS(Postgresが使えない環境ではスキップして構わない。既存の`upsert_user_roundtrip`等と同じ扱い)

- [ ] **Step 9: 通常のテストスイートを実行**

Run: `cd src-tauri && cargo test --lib`
Expected: PASS(全件)

- [ ] **Step 10: Postgres分をCommit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/store/postgres_backend.rs src-tauri/src/store/postgres_user_ref.rs
git commit -m "feat: PostgresユーザーキャッシュにavatarBlurhash列を追加(Issue #41)"
```

- [ ] **Step 11: MySQL `UserTable` enumに列を追加**

`src-tauri/src/store/mysql_backend.rs`の`enum UserTable`(282-288行目付近)、`InstanceThemeColor,`の直後に追加:

```rust
    AvatarBlurhash,
```

- [ ] **Step 12: MySQLの`ensure_schema`のDDLに列を追加**

同ファイルの`ensure_schema`関数内、`user`テーブル定義(150-169行目付近)の`.col(ColumnDef::new(UserTable::InstanceThemeColor).text())`の直後に追加:

```rust
        .col(ColumnDef::new(UserTable::AvatarBlurhash).text())
```

- [ ] **Step 13: MySQLの既存テーブル向け明示的マイグレーションを追加**

Postgres同様`sea_query`の`Table::create().if_not_exists()`は既存テーブルへの列追加を行わないため、`ensure_schema`関数の`pool.execute(user.as_str()).await?;`の直後に追加(MySQL 8.0.29+はPostgres同様`ADD COLUMN IF NOT EXISTS`をサポートする):

```rust
    // Issue #41: 猫耳表示の色抽出用。sea_query の CREATE TABLE IF NOT EXISTS は既存テーブルへの
    // 列追加を行わないため、`user` テーブルが既に存在する既存インストール向けに明示的な
    // ALTER TABLE ... ADD COLUMN IF NOT EXISTS を別途実行する(冪等)。
    pool.execute("ALTER TABLE `user` ADD COLUMN IF NOT EXISTS avatar_blurhash TEXT").await?;
```

- [ ] **Step 14: `mysql_user_ref.rs`の`upsert_user`・`fill_user_from_snapshot`・`fetch_users_by_ids`を更新**

`src-tauri/src/store/mysql_user_ref.rs`を、Step 4でPostgres版に加えた変更と同じ内容でMySQL構文(`?`プレースホルダ、`ON DUPLICATE KEY UPDATE ... = VALUES(...)`)に合わせて更新する:

- `upsert_user`: INSERT列リストに`avatar_blurhash`を追加、`VALUES`のプレースホルダを1つ増やす、`ON DUPLICATE KEY UPDATE`に`avatar_blurhash = COALESCE(VALUES(avatar_blurhash), avatar_blurhash)`を追加、末尾の`.bind(&user.avatar_blurhash)`を追加。
- `fill_user_from_snapshot`: 同様に列追加(ファイル内のPostgres版と対になる自己修復パス専用関数。無ければ`postgres_user_ref.rs`の`fill_user_from_snapshot`と同じ「既存値が無い場合のみ埋める」規約で実装されている箇所を探して合わせる)。
- `fetch_users_by_ids`: `SELECT`列リストとタプル型引数リストに`avatar_blurhash`(`Option<String>`)を追加し、`User { ... }`リテラルに`avatar_blurhash`を追加。

- [ ] **Step 15: MySQLのテストヘルパーを更新**

`mysql_user_ref.rs`の`#[cfg(test)]`内、ユーザー生成用テストヘルパー(`postgres_user_ref.rs`の`fn user(id: &str) -> User`に相当するもの)の`instance: None,`の直後に追加(既に存在すれば変更不要):

```rust
            avatar_blurhash: None,
```

- [ ] **Step 16: `#[ignore]`のラウンドトリップテストを追加**

`mysql_user_ref.rs`の`mod tests`、既存のupsert roundtripテストの直後に追加:

```rust
    #[tokio::test]
    #[ignore]
    async fn upsert_user_roundtrips_avatar_blurhash() {
        let pool = pool().await;
        let mut u = user("u1");
        u.avatar_blurhash = Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj".into());
        upsert_user(&pool, &u).await.unwrap();

        let got = fetch_users_by_ids(&pool, &["u1".to_string()]).await.unwrap();
        assert_eq!(
            got.get("u1").unwrap().avatar_blurhash.as_deref(),
            Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj")
        );
    }
```

- [ ] **Step 17: コンパイルを確認**

Run: `cd src-tauri && cargo test --lib --no-run`
Expected: 成功(コンパイルエラー無し)

- [ ] **Step 18: MySQLが使える場合のみ、無視テストも実行して確認**

Run: `cd src-tauri && cargo test --lib store::mysql_user_ref:: -- --ignored`
Expected: PASS(MySQLが使えない環境ではスキップして構わない)

- [ ] **Step 19: 通常のテストスイートを実行してMySQL分をCommit**

Run: `cd src-tauri && cargo test --lib`
Expected: PASS(全件)

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/store/mysql_backend.rs src-tauri/src/store/mysql_user_ref.rs
git commit -m "feat: MySQLユーザーキャッシュにavatarBlurhash列を追加(Issue #41)"
```

---

### Task 6: TSバインディングを再生成し、フロントの型を確認する

**Files:**
- Modify(自動生成、手編集禁止): `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: Task 1〜5で確定した`User`/`Account`のRust側フィールド
- Produces: `User.avatarBlurhash?: string | null`・`Account.isCat?: boolean`・`Account.avatarBlurhash?: string | null`がTS側に反映される

- [ ] **Step 1: `cargo test`でバインディングを再生成させる**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS(このテストが`frontend/src/bindings/tauri.gen.ts`を書き換える)

- [ ] **Step 2: 生成された型を確認**

Run: `grep -n "avatarBlurhash\|isCat" /home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/bindings/tauri.gen.ts`
Expected: `User`型に`avatarBlurhash?: string | null`、`Account`型に`isCat?: boolean`・`avatarBlurhash?: string | null`が出力されている

- [ ] **Step 3: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/bindings/tauri.gen.ts
git commit -m "chore: TSバインディングを再生成(avatarBlurhash/isCat追加分)"
```

---

### Task 7: BlurHash平均色抽出ユーティリティ

**Files:**
- Create: `frontend/src/lib/blurhash.ts`
- Test: `frontend/src/lib/blurhash.test.ts`

**Interfaces:**
- Produces: `extractAvgColorFromBlurhash(hash: string | null | undefined): string | undefined`。Task 8が使う。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/blurhash.test.ts`を新規作成:

```ts
import { describe, expect, it } from "vitest";
import { extractAvgColorFromBlurhash } from "./blurhash";

describe("extractAvgColorFromBlurhash", () => {
  it("BlurHash文字列から平均色のhexコードを抽出する", () => {
    // Misskey本家 extract-avg-color-from-blurhash.ts と同一ロジックの既知の入出力例。
    // "LEHV6nWB2yk8pyo0adR*.7kCMdnj" の3〜6文字目("HV6n")をbase83デコードした値。
    const result = extractAvgColorFromBlurhash("LEHV6nWB2yk8pyo0adR*.7kCMdnj");
    expect(result).toMatch(/^#[0-9a-f]{6}$/);
  });

  it("null/undefinedはundefinedを返す", () => {
    expect(extractAvgColorFromBlurhash(null)).toBeUndefined();
    expect(extractAvgColorFromBlurhash(undefined)).toBeUndefined();
  });

  it("同じ入力に対して常に同じ色を返す(決定的)", () => {
    const a = extractAvgColorFromBlurhash("LEHV6nWB2yk8pyo0adR*.7kCMdnj");
    const b = extractAvgColorFromBlurhash("LEHV6nWB2yk8pyo0adR*.7kCMdnj");
    expect(a).toBe(b);
  });
});
```

- [ ] **Step 2: テスト実行し失敗を確認**

Run: `cd frontend && pnpm vitest run src/lib/blurhash.test.ts`
Expected: FAIL(`./blurhash`モジュールが存在しない)

- [ ] **Step 3: 実装を書く**

`frontend/src/lib/blurhash.ts`を新規作成:

```ts
// Misskey本家 packages/frontend-shared/js/extract-avg-color-from-blurhash.ts を移植。
// BlurHash文字列の先頭数文字(DC成分)は画像全体の平均色を符号化しているため、
// フルデコードせずこの計算だけで平均色が求まる。

const BLURHASH_CHARS =
  "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~";

/** BlurHash文字列から画像全体の平均色を `#rrggbb` 形式で抽出する。無効な入力は undefined。 */
export function extractAvgColorFromBlurhash(hash: string | null | undefined): string | undefined {
  if (typeof hash !== "string" || hash.length < 6) return undefined;
  const value = [...hash.slice(2, 6)]
    .map((c) => BLURHASH_CHARS.indexOf(c))
    .reduce((a, c) => a * 83 + c, 0);
  return "#" + value.toString(16).padStart(6, "0");
}
```

- [ ] **Step 4: テスト実行して通ることを確認**

Run: `cd frontend && pnpm vitest run src/lib/blurhash.test.ts`
Expected: PASS(全件)

- [ ] **Step 5: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/blurhash.ts frontend/src/lib/blurhash.test.ts
git commit -m "feat: BlurHash平均色抽出ユーティリティを追加(Issue #41)"
```

---

### Task 8: `Avatar.svelte` 猫耳ラッパーコンポーネント

**Files:**
- Create: `frontend/src/ui/Avatar.svelte`
- Test: `frontend/src/ui/Avatar.test.ts`

**Interfaces:**
- Consumes: `extractAvgColorFromBlurhash`(Task 7)
- Produces: `<Avatar isCat avatarBlurhash class>` — Svelteコンポーネント。`isCat`/`avatarBlurhash`/`class`props と`children: Snippet`を受け取る。Task 9〜15が使う。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/Avatar.test.ts`を新規作成:

```ts
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import Avatar from "./Avatar.svelte";
import AvatarTestHost from "./Avatar.test.host.svelte";

afterEach(() => cleanup());

describe("Avatar", () => {
  it("isCat=trueのとき耳要素を描画する", () => {
    const { container } = render(AvatarTestHost, { props: { isCat: true } });
    expect(container.querySelector(".ears")).not.toBeNull();
  });

  it("isCat=falseのとき耳要素を描画しない", () => {
    const { container } = render(AvatarTestHost, { props: { isCat: false } });
    expect(container.querySelector(".ears")).toBeNull();
  });

  it("childrenをそのまま描画する", () => {
    const { getByTestId } = render(AvatarTestHost, { props: { isCat: false } });
    expect(getByTestId("inner-img")).not.toBeNull();
  });
});
```

Svelte 5の`children: Snippet`propは呼び出し側の`<Avatar>...</Avatar>`構文でしか渡せず、`render()`に直接`children`スニペットを渡すテストは書けないため、テスト専用のホストコンポーネント`frontend/src/ui/Avatar.test.host.svelte`を新規作成する:

```svelte
<script lang="ts">
  import Avatar from "./Avatar.svelte";
  let { isCat }: { isCat: boolean } = $props();
</script>

<Avatar {isCat} class="h-8 w-8">
  <img data-testid="inner-img" src="https://example.com/a.png" alt="" />
</Avatar>
```

- [ ] **Step 2: テスト実行し失敗を確認**

Run: `cd frontend && pnpm vitest run src/ui/Avatar.test.ts`
Expected: FAIL(`./Avatar.svelte`が存在しない)

- [ ] **Step 3: `Avatar.svelte`を実装**

`frontend/src/ui/Avatar.svelte`を新規作成:

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import { extractAvgColorFromBlurhash } from "../lib/blurhash";

  let {
    isCat = false,
    avatarBlurhash = null,
    class: className = "",
    children,
  }: {
    isCat?: boolean;
    avatarBlurhash?: string | null;
    class?: string;
    children: Snippet;
  } = $props();

  const earColor = $derived(extractAvgColorFromBlurhash(avatarBlurhash) ?? "var(--border)");
</script>

<span class="avatar-frame relative inline-block {className}">
  {@render children()}
  {#if isCat}
    <span class="ears" style="color: {earColor}" aria-hidden="true">
      <span class="ear-left"></span>
      <span class="ear-right"></span>
    </span>
  {/if}
</span>

<style>
  /* Misskey本家 MkAvatar.vue の .cat > .ears 相当を移植。%ベースなので
     avatar-frame のサイズ(呼び出し側の class で決まる)に自動追従する。 */
  .ears {
    contain: strict;
    position: absolute;
    top: -50%;
    left: -50%;
    width: 100%;
    height: 100%;
    padding: 50%;
    pointer-events: none;
  }
  .ear-left,
  .ear-right {
    contain: strict;
    display: inline-block;
    height: 50%;
    width: 50%;
    background: currentColor;
  }
  .ear-left::after,
  .ear-right::after {
    content: "";
    display: block;
    width: 60%;
    height: 60%;
    margin: 20%;
    background: #df548f;
  }
  .ear-left {
    transform: rotate(37.5deg) skew(30deg);
  }
  .ear-left,
  .ear-left::after {
    border-radius: 25% 75% 75%;
  }
  .ear-right {
    transform: rotate(-37.5deg) skew(-30deg);
  }
  .ear-right,
  .ear-right::after {
    border-radius: 75% 25% 75% 75%;
  }

  @keyframes earwiggleleft {
    from,
    to {
      transform: rotate(37.6deg) skew(30deg);
    }
    25% {
      transform: rotate(10deg) skew(30deg);
    }
    50% {
      transform: rotate(20deg) skew(30deg);
    }
    75% {
      transform: rotate(0deg) skew(30deg);
    }
  }
  @keyframes earwiggleright {
    from,
    to {
      transform: rotate(-37.6deg) skew(-30deg);
    }
    30% {
      transform: rotate(-10deg) skew(-30deg);
    }
    55% {
      transform: rotate(-20deg) skew(-30deg);
    }
    75% {
      transform: rotate(0deg) skew(-30deg);
    }
  }
  @media (prefers-reduced-motion: no-preference) {
    .avatar-frame:hover .ear-left {
      animation: earwiggleleft 1s infinite;
    }
    .avatar-frame:hover .ear-right {
      animation: earwiggleright 1s infinite;
    }
  }
</style>
```

- [ ] **Step 4: テスト実行して通ることを確認**

Run: `cd frontend && pnpm vitest run src/ui/Avatar.test.ts`
Expected: PASS(全件)

- [ ] **Step 5: `pnpm check`で型エラーが無いことを確認**

Run: `cd frontend && pnpm check`
Expected: エラー無し

- [ ] **Step 6: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/Avatar.svelte frontend/src/ui/Avatar.test.ts frontend/src/ui/Avatar.test.host.svelte
git commit -m "feat: 猫耳ラッパーコンポーネントAvatar.svelteを追加(Issue #41)"
```

---

### Task 9: `NoteCard.svelte` に猫耳を組み込む

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte`(321-337行目付近)
- Modify: `frontend/src/ui/NoteCard.test.ts`

**Interfaces:**
- Consumes: `Avatar.svelte`(Task 8)、`User.isCat`(既存)、`User.avatarBlurhash`(Task 1・6)

- [ ] **Step 1: `Avatar`をimportする**

`frontend/src/ui/NoteCard.svelte`の先頭付近、`import ReactionUsersPopover from "./ReactionUsersPopover.svelte";`の直後に追加:

```svelte
  import Avatar from "./Avatar.svelte";
```

- [ ] **Step 2: 投稿者アバターを`<Avatar>`でラップする**

`frontend/src/ui/NoteCard.svelte`の321-337行目を以下に置き換え:

```svelte
    <Avatar isCat={inner.user.isCat} avatarBlurhash={inner.user.avatarBlurhash} class="h-[34px] w-[34px] flex-none">
      {#if inner.user.avatarUrl}
        <img
          class="h-full w-full rounded-[var(--avatar-radius,20%)] object-cover"
          data-testid="note-avatar"
          src={inner.user.avatarUrl}
          alt=""
          loading="lazy"
          onclick={() => openProfile({ userId: inner.user.id }, accountId)}
          style="cursor: pointer"
        />
      {:else}
        <!-- role="button"だがButtonプリミティブ非経由のため、キーボードフォーカス時の視認性を
             Buttonのfocus-visibleパターン（スタイルガイド§7、border-ringは無枠のため省略）で個別に補う -->
        <div
          class="avatar h-full w-full rounded-[var(--avatar-radius,20%)] outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
          data-testid="note-avatar"
          onclick={() => openProfile({ userId: inner.user.id }, accountId)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === "Enter" && openProfile({ userId: inner.user.id }, accountId)}
        ></div>
      {/if}
    </Avatar>
```

- [ ] **Step 3: 既存テストがまだ通ることを確認(`data-testid="note-avatar"`はimg/div自身に残しているので壊れないはず)**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts`
Expected: PASS(既存の全ケース)

- [ ] **Step 4: 猫耳表示の新規テストを追加**

`frontend/src/ui/NoteCard.test.ts`の`makeUser`利用テストの近くに追加:

```ts
  it("投稿者がisCatのとき猫耳(.ears)を描画する", () => {
    const note = makeNote({ user: makeUser({ isCat: true }) });
    const { container } = render(NoteCard, { note, accountId: "a1", instanceHost: "misskey.io" });
    expect(container.querySelector(".ears")).not.toBeNull();
  });

  it("投稿者がisCatでないとき猫耳(.ears)を描画しない", () => {
    const note = makeNote({ user: makeUser({ isCat: false }) });
    const { container } = render(NoteCard, { note, accountId: "a1", instanceHost: "misskey.io" });
    expect(container.querySelector(".ears")).toBeNull();
  });
```

(既存テストで`render(NoteCard, {...})`に渡しているprops名が異なる場合は、同ファイル内の既存呼び出し例に合わせること。)

- [ ] **Step 5: テスト実行して通ることを確認**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts`
Expected: PASS(全件)

- [ ] **Step 6: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/NoteCard.svelte frontend/src/ui/NoteCard.test.ts
git commit -m "feat: NoteCardのアバターに猫耳を表示(Issue #41)"
```

---

### Task 10: `ProfileModal.svelte` に猫耳を組み込む

**Files:**
- Modify: `frontend/src/ui/ProfileModal.svelte`(150-154行目付近)
- Modify: `frontend/src/ui/ProfileModal.test.ts`

**Interfaces:**
- Consumes: `Avatar.svelte`(Task 8)、`User.isCat`・`User.avatarBlurhash`

- [ ] **Step 1: `Avatar`をimportする**

`frontend/src/ui/ProfileModal.svelte`の`import FollowListModal from "./FollowListModal.svelte";`の直後に追加:

```svelte
  import Avatar from "./Avatar.svelte";
```

- [ ] **Step 2: プロフィールアバターを`<Avatar>`でラップする**

150-154行目を以下に置き換え:

```svelte
      <Avatar
        isCat={profile.user.isCat}
        avatarBlurhash={profile.user.avatarBlurhash}
        class="h-14 w-14 flex-none"
      >
        {#if profile.user.avatarUrl}
          <img class="h-full w-full rounded-[var(--avatar-radius,20%)] border-2 border-background object-cover" src={profile.user.avatarUrl} alt="" />
        {:else}
          <div class="avatar-ph h-full w-full rounded-[var(--avatar-radius,20%)] border-2 border-background"></div>
        {/if}
      </Avatar>
```

- [ ] **Step 3: 既存テストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/ui/ProfileModal.test.ts`
Expected: PASS(全件)

- [ ] **Step 4: 猫耳表示のテストを追加**

`frontend/src/ui/ProfileModal.test.ts`の`describe("ProfileModal", ...)`ブロック内、末尾のテストの直後に追加:

```ts
  it("プロフィールのuser.isCatがtrueのとき猫耳(.ears)を描画する", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_user_profile") return Promise.resolve(profileResponse({ user: { ...profileResponse().user, isCat: true } }));
      if (cmd === "get_user_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const { container } = render(ProfileModal, {
      props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(container.querySelector(".ears")).not.toBeNull());
  });
```

- [ ] **Step 5: テスト実行して通ることを確認**

Run: `cd frontend && pnpm vitest run src/ui/ProfileModal.test.ts`
Expected: PASS(全件)

- [ ] **Step 6: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/ProfileModal.svelte frontend/src/ui/ProfileModal.test.ts
git commit -m "feat: ProfileModalのアバターに猫耳を表示(Issue #41)"
```

---

### Task 11: `FollowListModal.svelte` に猫耳を組み込む

**Files:**
- Modify: `frontend/src/ui/FollowListModal.svelte`(104-108行目付近)
- Modify: `frontend/src/ui/FollowListModal.test.ts`

**Interfaces:**
- Consumes: `Avatar.svelte`(Task 8)、`FollowListEntry.user.isCat`・`avatarBlurhash`

- [ ] **Step 1: `Avatar`をimportする**

`frontend/src/ui/FollowListModal.svelte`の`import Modal from "./Modal.svelte";`の直後に追加:

```svelte
  import Avatar from "./Avatar.svelte";
```

- [ ] **Step 2: 一覧アバターを`<Avatar>`でラップする**

104-108行目を以下に置き換え:

```svelte
          <Avatar isCat={entry.user.isCat} avatarBlurhash={entry.user.avatarBlurhash} class="h-10 w-10 flex-none">
            {#if entry.user.avatarUrl}
              <img class="h-full w-full rounded-[var(--avatar-radius,20%)] object-cover" src={entry.user.avatarUrl} alt="" />
            {:else}
              <div class="avatar-ph h-full w-full rounded-[var(--avatar-radius,20%)]"></div>
            {/if}
          </Avatar>
```

- [ ] **Step 3: 既存テストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/ui/FollowListModal.test.ts`
Expected: PASS(全件)

- [ ] **Step 4: 猫耳表示のテストを追加**

`frontend/src/ui/FollowListModal.test.ts`の`makeUser`関数を、`isCat`を上書きできるよう拡張する:

```ts
function makeUser(id: string, username: string, overrides: Partial<{ isCat: boolean }> = {}) {
  return {
    id,
    username,
    host: null,
    name: username,
    avatarUrl: null,
    isBot: false,
    isCat: false,
    followersCount: 0,
    followingCount: 0,
    notesCount: 0,
    emojis: {},
    bio: null,
    bannerUrl: null,
    ...overrides,
  };
}

function makeEntry(userId: string, username: string, cursor: string, userOverrides: Partial<{ isCat: boolean }> = {}) {
  return { user: makeUser(userId, username, userOverrides), cursor };
}
```

(`makeEntry`の既存呼び出し箇所は引数を追加していないので、そのまま動く。)

`describe("FollowListModal", ...)`ブロックの末尾に追加:

```ts
  it("エントリのuser.isCatがtrueのとき猫耳(.ears)を描画する", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_user_followers") return Promise.resolve([makeEntry("u2", "bob", "f2", { isCat: true })]);
      return Promise.resolve(null);
    });
    const { container, getByText } = render(FollowListModal, {
      props: { kind: "followers", userId: "u1", accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(getByText("bob")).toBeTruthy());
    expect(container.querySelector(".ears")).not.toBeNull();
  });
```

- [ ] **Step 5: テスト実行して通ることを確認**

Run: `cd frontend && pnpm vitest run src/ui/FollowListModal.test.ts`
Expected: PASS(全件)

- [ ] **Step 6: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/FollowListModal.svelte frontend/src/ui/FollowListModal.test.ts
git commit -m "feat: FollowListModalのアバターに猫耳を表示(Issue #41)"
```

---

### Task 12: `NotificationCard.svelte` に猫耳を組み込む

**Files:**
- Modify: `frontend/src/ui/NotificationCard.svelte`(81-92行目付近)
- Modify: `frontend/src/ui/NotificationCard.test.ts`

**Interfaces:**
- Consumes: `Avatar.svelte`(Task 8)、`Notification.user.isCat`・`avatarBlurhash`(`user`はoptional)

- [ ] **Step 1: `Avatar`をimportする**

`frontend/src/ui/NotificationCard.svelte`の`import NoteCard from "./NoteCard.svelte";`の直後に追加:

```svelte
  import Avatar from "./Avatar.svelte";
```

- [ ] **Step 2: 通知アバターを`<Avatar>`でラップする**

81-92行目を以下に置き換え:

```svelte
    {#if n.user?.avatarUrl}
      <Avatar isCat={n.user.isCat} avatarBlurhash={n.user.avatarBlurhash} class="h-6 w-6 flex-none">
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <img
          class="h-full w-full rounded-[var(--avatar-radius,20%)] object-cover"
          data-testid="notification-avatar"
          src={n.user.avatarUrl}
          alt=""
          loading="lazy"
          onclick={() => n.user && openProfile({ userId: n.user.id }, accountId)}
          style="cursor: pointer"
        />
      </Avatar>
    {/if}
```

- [ ] **Step 3: 既存テストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/ui/NotificationCard.test.ts`
Expected: PASS(全件)

- [ ] **Step 4: 猫耳表示のテストを追加**

`frontend/src/ui/NotificationCard.test.ts`の`describe("NotificationCard note actions", ...)`ブロックの末尾に追加(通知アバターは`n.user?.avatarUrl`がある場合のみ描画されるため、`avatarUrl`も設定する):

```ts
  it("通知元ユーザーのisCatがtrueのとき猫耳(.ears)を描画する", () => {
    const notification = makeNotification({
      type: "mention",
      user: makeUser({ id: "u2", name: "Bob", avatarUrl: "https://example.com/b.png", isCat: true }),
    });
    const { container } = render(NotificationCard, {
      props: { notification, accountId: "a1" },
    });
    expect(container.querySelector(".ears")).not.toBeNull();
  });
```

- [ ] **Step 5: テスト実行して通ることを確認**

Run: `cd frontend && pnpm vitest run src/ui/NotificationCard.test.ts`
Expected: PASS(全件)

- [ ] **Step 6: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/NotificationCard.svelte frontend/src/ui/NotificationCard.test.ts
git commit -m "feat: NotificationCardのアバターに猫耳を表示(Issue #41)"
```

---

### Task 13: `ReactionUsersPopover.svelte` に猫耳を組み込む

**Files:**
- Modify: `frontend/src/ui/ReactionUsersPopover.svelte`(76-80行目付近)
- (テストファイルなし。Task 8の`Avatar.test.ts`がisCat分岐をカバー済み)

**Interfaces:**
- Consumes: `Avatar.svelte`(Task 8)、`User.isCat`・`avatarBlurhash`

- [ ] **Step 1: `Avatar`をimportする**

`frontend/src/ui/ReactionUsersPopover.svelte`の`import { fetchReactionUsers } from "../lib/reactionUsersCache";`の直後に追加:

```svelte
  import Avatar from "./Avatar.svelte";
```

- [ ] **Step 2: リアクションユーザーのアバターを`<Avatar>`でラップする**

76-80行目を以下に置き換え:

```svelte
            <Avatar isCat={u.isCat} avatarBlurhash={u.avatarBlurhash} class="h-5 w-5 flex-shrink-0">
              {#if u.avatarUrl}
                <img class="h-full w-full rounded-[var(--avatar-radius,20%)] object-cover" src={u.avatarUrl} alt="" loading="lazy" />
              {:else}
                <div class="h-full w-full rounded-[var(--avatar-radius,20%)] bg-border"></div>
              {/if}
            </Avatar>
```

`ReactionUsersPopover.svelte`には現時点でテストファイルが存在しない(`Avatar.svelte`自体のisCat分岐は既にTask 8で単体テスト済みのため、ここでは新規テストファイルは作らずビルド確認のみ行う)。

- [ ] **Step 3: 型チェックとビルドが通ることを確認**

Run: `cd frontend && pnpm check`
Expected: エラー無し

- [ ] **Step 4: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/ReactionUsersPopover.svelte
git commit -m "feat: ReactionUsersPopoverのアバターに猫耳を表示(Issue #41)"
```

---

### Task 14: `AccountSelect.svelte` に猫耳を組み込む(トリガー・ドロップダウンの2箇所)

**Files:**
- Modify: `frontend/src/ui/AccountSelect.svelte`(70-83行目、122-129行目付近)
- (テストファイルなし。Task 8の`Avatar.test.ts`がisCat分岐をカバー済み)

**Interfaces:**
- Consumes: `Avatar.svelte`(Task 8)、`Account.isCat`・`Account.avatarBlurhash`(Task 2・6)

- [ ] **Step 1: `Avatar`をimportする**

`frontend/src/ui/AccountSelect.svelte`の`import { Button } from "$lib/components/ui/button";`の直後に追加:

```svelte
  import Avatar from "./Avatar.svelte";
```

- [ ] **Step 2: トリガーボタンのアバターを`<Avatar>`でラップする**

70-83行目(`{#if selected.avatarUrl} ... {/if}`)を以下に置き換え:

```svelte
    {#if selected.avatarUrl}
      <Avatar
        isCat={selected.isCat}
        avatarBlurhash={selected.avatarBlurhash}
        class={large ? "size-9 flex-none" : "size-[22px] flex-none"}
      >
        <img src={selected.avatarUrl} alt="" class={large ? "h-full w-full rounded-lg object-cover" : "h-full w-full rounded-md object-cover"} />
      </Avatar>
    {:else}
```

(`{:else}`以降のイニシャル表示部分は変更しない。)

- [ ] **Step 3: ドロップダウン内リストのアバターを`<Avatar>`でラップする**

122-129行目を以下に置き換え:

```svelte
          {#if a.avatarUrl}
            <Avatar isCat={a.isCat} avatarBlurhash={a.avatarBlurhash} class="size-7 flex-none">
              <img src={a.avatarUrl} alt="" class="h-full w-full rounded-[var(--avatar-radius,20%)] object-cover" />
            </Avatar>
          {:else}
```

(`{:else}`以降のイニシャル表示部分は変更しない。)

`AccountSelect.svelte`には現時点でテストファイルが存在しない(`Avatar.svelte`自体のisCat分岐は既にTask 8で単体テスト済みのため、ここでは新規テストファイルは作らずビルド確認のみ行う)。

- [ ] **Step 4: 型チェックとビルドが通ることを確認**

Run: `cd frontend && pnpm check`
Expected: エラー無し

- [ ] **Step 5: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/AccountSelect.svelte
git commit -m "feat: AccountSelectのアバターに猫耳を表示(Issue #41)"
```

---

### Task 15: `AccountsSection.svelte` に猫耳を組み込む

**Files:**
- Modify: `frontend/src/ui/settings/AccountsSection.svelte`(46-50行目付近)
- (テストファイルなし。Task 8の`Avatar.test.ts`がisCat分岐をカバー済み)

**Interfaces:**
- Consumes: `Avatar.svelte`(Task 8)、`Account.isCat`・`Account.avatarBlurhash`

- [ ] **Step 1: `Avatar`をimportする**

`frontend/src/ui/settings/AccountsSection.svelte`の`import { Button } from "$lib/components/ui/button";`の直後に追加:

```svelte
  import Avatar from "../Avatar.svelte";
```

- [ ] **Step 2: アカウント一覧のアバターを`<Avatar>`でラップする**

46-50行目を以下に置き換え:

```svelte
        <Avatar isCat={a.isCat} avatarBlurhash={a.avatarBlurhash} class="h-[34px] w-[34px] flex-none">
          {#if a.avatarUrl}
            <img class="h-full w-full rounded-[var(--avatar-radius,20%)] object-cover" src={a.avatarUrl} alt="" />
          {:else}
            <div class="grid h-full w-full place-items-center rounded-[var(--avatar-radius,20%)] bg-accent font-bold text-muted-foreground">{(a.displayName || a.username).charAt(0)}</div>
          {/if}
        </Avatar>
```

`AccountsSection.svelte`には現時点でテストファイルが存在しない(`Avatar.svelte`自体のisCat分岐は既にTask 8で単体テスト済みのため、ここでは新規テストファイルは作らずビルド確認のみ行う)。

- [ ] **Step 3: 型チェックとビルドが通ることを確認**

Run: `cd frontend && pnpm check`
Expected: エラー無し

- [ ] **Step 4: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/settings/AccountsSection.svelte
git commit -m "feat: AccountsSectionのアバターに猫耳を表示(Issue #41)"
```

---

### Task 16: 最終検証

**Files:** なし(検証のみ)

- [ ] **Step 1: Rust側フルテスト**

Run: `cd src-tauri && cargo test`
Expected: PASS(全件。Postgres実接続を要する`#[ignore]`テストは除く)

- [ ] **Step 2: フロントエンド型チェック**

Run: `cd frontend && pnpm check`
Expected: エラー無し

- [ ] **Step 3: フロントエンド全テスト**

Run: `cd frontend && pnpm test`
Expected: PASS(全件)

- [ ] **Step 4: `cargo tauri dev`で実機確認**

`cargo tauri dev`(リポジトリルートから)を起動し、`isCat`なユーザーの投稿・プロフィール・フォロー一覧・通知・リアクションユーザー一覧・アカウント切替UIそれぞれで猫耳が表示され、ホバーで耳が揺れることを目視確認する。確認後、検証用に自分で起動した`cargo tauri dev`はkillする。

- [ ] **Step 5: 最終Commit(残作業があれば)**

上記検証で修正が必要になった場合のみ、修正して追加コミットする。
