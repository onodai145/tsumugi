# ユーザープロフィールページ Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ノートのアバター/表示名/acct、および本文中の`@mention`をクリックすると、そのユーザーのプロフィール（バナー・自己紹介・フォロー統計・フォローボタン・ノート一覧）をモーダルで閲覧できるようにする。

**Architecture:** バックエンドは `users/show`（プロフィール本体＋フォロー関係フラグを1回で取得）・`following/create`/`following/delete`（フォロー操作）・`users/followers`/`users/following`（一覧）を新規ラップし、既存の `ColumnKind::User.rest_request()` をそのまま再利用してノート一覧も取得する。フロントエンドは Svelte 5 runes によるグローバル `$state` ストア（`profileModal.svelte.ts`）でモーダルの開閉状態を持ち、`NoteCard.svelte`（アバター/名前クリック）と `MfmNode.svelte`（mentionクリック）という経路の異なる2箇所からも同じストアを叩けるようにする。

**Tech Stack:** Rust (Tauri v2 commands, `specta`/`tauri-specta` for TS binding generation), Svelte 5 (runes), Vitest + `@testing-library/svelte`, `cargo test`。

## Global Constraints

- コマンド追加時は `specta_builder()`（`src-tauri/src/lib.rs`）に登録する。フロントの `frontend/src/bindings/tauri.gen.ts` は生成物なので手で編集しない（`cargo test` の `generates_frontend_bindings` で再生成される）。
- Misskey REST 呼び出しは全て POST、トークンは `MisskeyClient` が JSON body に埋め込む（`src-tauri/src/api/client.rs`）。
- 自分自身のプロフィールを開いた場合はフォローボタンを非表示にする（自分をフォローする操作は存在しない）。
- ブロック機能・ミュート操作・自己紹介以外のプロフィールフィールド（ピン留めノート等）は対象外（YAGNI、spec参照）。
- コミットメッセージは1行のみ（本文・箇条書き禁止）。

---

## File Structure

### バックエンド（`src-tauri/`）

- Modify: `src-tauri/src/domain/user.rs` — `User` に `bio`/`banner_url` を追加。
- Modify: `src-tauri/src/api/normalize.rs` — `RawUser` に同フィールドを追加し `From<RawUser>` を更新。
- Modify: `src-tauri/src/api/users.rs` — `show`/`follow`/`unfollow`/`followers`/`following` を追加。
- Create: `src-tauri/src/commands/user.rs` — `get_user_profile`/`follow_user`/`unfollow_user`/`get_user_followers`/`get_user_following`/`get_user_notes` の各 `#[tauri::command]`。
- Modify: `src-tauri/src/commands/mod.rs` — `pub mod user;` と re-export 追加。
- Modify: `src-tauri/src/lib.rs` — `specta_builder()` に新コマンドを登録。

### フロントエンド（`frontend/src/`）

- Create: `frontend/src/lib/userDisplay.ts` — `acct`/`displayName` ヘルパー（`NoteCard.svelte` から抽出・共有化）。
- Modify: `frontend/src/ui/NoteCard.svelte` — 上記ヘルパーへの置き換え＋アバター/名前/acctのクリックハンドラ追加。
- Create: `frontend/src/lib/profileModal.svelte.ts` — グローバルなプロフィールモーダル開閉状態。
- Modify: `frontend/src/lib/store.svelte.ts` — `AppStore` に `getUserProfile`/`followUser`/`unfollowUser`/`getUserFollowers`/`getUserFollowing`/`getUserNotes` を追加。
- Create: `frontend/src/ui/ProfileModal.svelte` — プロフィール本体＋フォローボタン＋埋め込みノート一覧＋「カラムとして追加」。
- Create: `frontend/src/ui/FollowListModal.svelte` — フォロー中/フォロワー一覧。
- Modify: `frontend/src/render/MfmNode.svelte` — mentionノードのクリックハンドラ追加。
- Modify: `frontend/src/App.svelte` — グローバル状態を見て `ProfileModal` をマウント。

---

### Task 1: ドメイン層に bio/banner_url を追加

**Files:**
- Modify: `src-tauri/src/domain/user.rs`
- Test: `src-tauri/src/domain/user.rs`（同ファイル内 `#[cfg(test)] mod tests`）

**Interfaces:**
- Produces: `User { .., bio: Option<String>, banner_url: Option<String> }`（既存フィールドは変更なし）

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/domain/user.rs` の `tests` モジュールに追加:

```rust
    /// bio/bannerUrl フィールド追加前に保存されたキャッシュ済みJSONを読み込めること。
    #[test]
    fn deserializes_without_bio_or_banner_for_backward_compat() {
        let json = r#"{
            "id":"u1","username":"alice","host":null,"name":"Alice",
            "avatarUrl":null,"isBot":false,"isCat":false,
            "followersCount":0,"followingCount":0,"notesCount":0
        }"#;
        let u: User = serde_json::from_str(json).unwrap();
        assert_eq!(u.bio, None);
        assert_eq!(u.banner_url, None);
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd src-tauri && cargo test deserializes_without_bio_or_banner_for_backward_compat`
Expected: FAIL（`bio`フィールドが存在しない）

- [ ] **Step 3: `User` に フィールドを追加**

`src-tauri/src/domain/user.rs` の `User` struct を編集:

```rust
pub struct User {
    pub id: String,
    /// @なしのユーザ名
    pub username: String,
    /// None=ローカル
    pub host: Option<String>,
    /// 表示名
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_bot: bool,
    pub is_cat: bool,
    pub followers_count: u32,
    pub following_count: u32,
    pub notes_count: u32,
    /// 表示名(`name`)中のカスタム絵文字ショートコード解決用 {name: url}。
    /// 既存キャッシュ済みJSON(このフィールド追加前に保存されたもの)との後方互換のため default。
    #[serde(default)]
    pub emojis: HashMap<String, String>,
    /// 自己紹介（Misskeyの`description`）。UserLiteコンテキスト（ノート本文の著者等）では取得されない。
    #[serde(default)]
    pub bio: Option<String>,
    /// バナー画像URL。同上、UserLiteコンテキストでは取得されない。
    #[serde(default)]
    pub banner_url: Option<String>,
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd src-tauri && cargo test deserializes_without_bio_or_banner_for_backward_compat`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/domain/user.rs
git commit -m "feat: Userドメイン型にbio/banner_urlを追加"
```

---

### Task 2: RawUser に bio/banner_url を追加

**Files:**
- Modify: `src-tauri/src/api/normalize.rs`

**Interfaces:**
- Consumes: Task 1 の `User { .., bio, banner_url }`
- Produces: `RawUser { .., description: Option<String>, banner_url: Option<String> }`、`impl From<RawUser> for User` が `bio`/`banner_url` を引き継ぐ

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/api/normalize.rs` の末尾（既存 `#[cfg(test)]` があればそこに、無ければ新規に）に追加:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_user_maps_description_to_bio_and_carries_banner_url() {
        let json = r#"{
            "id":"u1","username":"alice","host":null,"name":"Alice",
            "avatarUrl":null,"isBot":false,"isCat":false,
            "followersCount":0,"followingCount":0,"notesCount":0,
            "description":"hello world","bannerUrl":"https://example.com/banner.png"
        }"#;
        let raw: RawUser = serde_json::from_str(json).unwrap();
        let user: User = raw.into();
        assert_eq!(user.bio, Some("hello world".to_string()));
        assert_eq!(user.banner_url, Some("https://example.com/banner.png".to_string()));
    }

    #[test]
    fn raw_user_without_description_or_banner_defaults_to_none() {
        let json = r#"{
            "id":"u1","username":"alice","host":null,"name":"Alice",
            "avatarUrl":null,"isBot":false,"isCat":false,
            "followersCount":0,"followingCount":0,"notesCount":0
        }"#;
        let raw: RawUser = serde_json::from_str(json).unwrap();
        let user: User = raw.into();
        assert_eq!(user.bio, None);
        assert_eq!(user.banner_url, None);
    }
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd src-tauri && cargo test raw_user_maps_description_to_bio_and_carries_banner_url`
Expected: FAIL（コンパイルエラー: `description`フィールドが `RawUser` に無い）

- [ ] **Step 3: `RawUser` にフィールドを追加し `From` を更新**

`src-tauri/src/api/normalize.rs` の `RawUser` struct・`impl From<RawUser> for User` を編集:

```rust
pub struct RawUser {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub is_cat: bool,
    #[serde(default)]
    pub followers_count: u32,
    #[serde(default)]
    pub following_count: u32,
    #[serde(default)]
    pub notes_count: u32,
    /// 表示名(`name`)中のカスタム絵文字 {name: url}。
    #[serde(default)]
    pub emojis: HashMap<String, String>,
    /// Misskey側のフィールド名は `description`。UserDetailed系レスポンスにのみ存在。
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub banner_url: Option<String>,
}

impl From<RawUser> for User {
    fn from(r: RawUser) -> Self {
        User {
            id: r.id,
            username: r.username,
            host: r.host,
            name: r.name,
            avatar_url: r.avatar_url,
            is_bot: r.is_bot,
            is_cat: r.is_cat,
            followers_count: r.followers_count,
            following_count: r.following_count,
            notes_count: r.notes_count,
            emojis: r.emojis,
            bio: r.description,
            banner_url: r.banner_url,
        }
    }
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd src-tauri && cargo test raw_user_maps_description_to_bio_and_carries_banner_url raw_user_without_description_or_banner_defaults_to_none`
Expected: 両方 PASS

- [ ] **Step 5: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/api/normalize.rs
git commit -m "feat: RawUserのdescription/bannerUrlをUser.bio/banner_urlへ正規化"
```

---

### Task 3: プロフィール取得・フォロー操作のAPIラッパーを追加

**Files:**
- Modify: `src-tauri/src/api/users.rs`

**Interfaces:**
- Consumes: `crate::api::MisskeyClient`（`post<B, R>(&self, endpoint: &str, body: &B) -> Result<R>`）、`crate::api::normalize::RawUser`、`crate::domain::User`
- Produces:
  - `pub async fn show(client: &MisskeyClient, user_id: &str) -> Result<RawUserShow>`（`RawUserShow` は本タスクで定義、`user: User` + `is_following: Option<bool>` を持つ）
  - `pub async fn follow(client: &MisskeyClient, user_id: &str) -> Result<()>`
  - `pub async fn unfollow(client: &MisskeyClient, user_id: &str) -> Result<()>`

- [ ] **Step 1: `show`/`follow`/`unfollow` を実装（テストなし — 既存の `search_users` 同様、薄いI/Oラッパーはこのリポジトリでは単体テスト対象外。Task 4でこれらを使う `commands/user.rs` 側のロジックをテストする）**

`src-tauri/src/api/users.rs` を編集:

```rust
//! ユーザー検索・プロフィール取得・フォロー操作 REST。

use crate::api::normalize::RawUser;
use crate::api::MisskeyClient;
use crate::domain::User;
use crate::error::Result;
use serde::Deserialize;
use serde_json::json;

pub async fn search_users(client: &MisskeyClient, query: &str, limit: u32) -> Result<Vec<User>> {
    let body = json!({
        "query": query,
        "limit": limit,
        "origin": "combined",
        "detail": false,
    });
    let raw: Vec<RawUser> = client.post("users/search", &body).await?;
    Ok(raw.into_iter().map(Into::into).collect())
}

/// `users/show` のレスポンス。UserDetailedNotMe は `RawUser` にないフォロー関係フラグを
/// 追加で持つため、`#[serde(flatten)]` で `RawUser` の全フィールド + 関係フラグを一度に受ける。
/// 自分自身を対象にした場合（`MeDetailed`）は関係フラグが存在せず `None` になる。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawUserShow {
    #[serde(flatten)]
    pub user: RawUser,
    #[serde(default)]
    pub is_following: Option<bool>,
}

/// ユーザーIDからプロフィール詳細（フォロー関係フラグ込み）を取得する。
pub async fn show(client: &MisskeyClient, user_id: &str) -> Result<RawUserShow> {
    client.post("users/show", &json!({ "userId": user_id })).await
}

/// フォローする。
pub async fn follow(client: &MisskeyClient, user_id: &str) -> Result<()> {
    let _: serde_json::Value = client.post("following/create", &json!({ "userId": user_id })).await?;
    Ok(())
}

/// フォロー解除する。
pub async fn unfollow(client: &MisskeyClient, user_id: &str) -> Result<()> {
    let _: serde_json::Value = client.post("following/delete", &json!({ "userId": user_id })).await?;
    Ok(())
}
```

- [ ] **Step 2: ビルドが通ることを確認**

Run: `cd src-tauri && cargo build`
Expected: エラーなし（`show`/`follow`/`unfollow`/`RawUserShow` は本タスク時点では未使用なので `#[allow(dead_code)]` 警告が出る場合があるが、Task 4で使用開始するため無視してよい）

- [ ] **Step 3: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/api/users.rs
git commit -m "feat: users/show, following/create, following/deleteのAPIラッパーを追加"
```

---

### Task 4: フォロワー/フォロー中一覧のAPIラッパーを追加

**Files:**
- Modify: `src-tauri/src/api/users.rs`

**Interfaces:**
- Produces: `pub async fn followers(client, user_id, until_id: Option<&str>) -> Result<Vec<User>>`、`pub async fn following(client, user_id, until_id: Option<&str>) -> Result<Vec<User>>`

- [ ] **Step 1: 実装**

`src-tauri/src/api/users.rs` の末尾に追加:

```rust
/// `users/followers` / `users/following` の1件（Followingオブジェクト）。
/// 一覧の主体（followers なら相手=follower、following なら相手=followee）のみ使う。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFollowing {
    #[serde(default)]
    followee: Option<RawUser>,
    #[serde(default)]
    follower: Option<RawUser>,
}

const FOLLOW_LIST_PAGE_SIZE: u32 = 20;

/// フォロワー一覧（新しい順、`until_id` でページング）。
pub async fn followers(client: &MisskeyClient, user_id: &str, until_id: Option<&str>) -> Result<Vec<User>> {
    let mut body = json!({ "userId": user_id, "limit": FOLLOW_LIST_PAGE_SIZE });
    if let Some(u) = until_id {
        body["untilId"] = json!(u);
    }
    let raw: Vec<RawFollowing> = client.post("users/followers", &body).await?;
    Ok(raw.into_iter().filter_map(|f| f.follower).map(Into::into).collect())
}

/// フォロー中一覧（新しい順、`until_id` でページング）。
pub async fn following(client: &MisskeyClient, user_id: &str, until_id: Option<&str>) -> Result<Vec<User>> {
    let mut body = json!({ "userId": user_id, "limit": FOLLOW_LIST_PAGE_SIZE });
    if let Some(u) = until_id {
        body["untilId"] = json!(u);
    }
    let raw: Vec<RawFollowing> = client.post("users/following", &body).await?;
    Ok(raw.into_iter().filter_map(|f| f.followee).map(Into::into).collect())
}
```

- [ ] **Step 2: ビルドが通ることを確認**

Run: `cd src-tauri && cargo build`
Expected: エラーなし

- [ ] **Step 3: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/api/users.rs
git commit -m "feat: users/followers, users/followingのAPIラッパーを追加"
```

---

### Task 5: `get_user_profile` コマンドを実装

**Files:**
- Create: `src-tauri/src/commands/user.rs`

**Interfaces:**
- Consumes: `state.accounts.lock().unwrap().list() -> Vec<Account>`（`Account { id, host, username, user_id, display_name, avatar_url }`, `src-tauri/src/domain/account.rs`）、`state.client_for(&account_id) -> Result<MisskeyClient>`、Task 3 の `crate::api::users::show`
- Produces:
  - `pub struct UserProfile { pub user: User, pub is_following: Option<bool>, pub is_self: bool }`（`specta::Type` 付き、`#[serde(rename_all = "camelCase")]`）
  - `pub async fn get_user_profile(state: State<'_, AppState>, account_id: String, user_id: String) -> Result<UserProfile>`
  - `pub(crate) fn is_self_user(accounts: &[crate::domain::Account], account_id: &str, user_id: &str) -> bool`（純粋関数、単体テスト対象）

- [ ] **Step 1: `is_self_user` の失敗するテストを書く**

新規ファイル `src-tauri/src/commands/user.rs` を作成し、まずテストのみ書く:

```rust
//! ユーザープロフィール取得・フォロー操作。

use crate::api;
use crate::domain::{Account, Note, User};
use crate::error::Result;
use crate::state::AppState;
use specta::Type;
use tauri::State;

/// プロフィールモーダル用のレスポンス。`is_following` は自分自身の場合 `None`。
#[derive(Debug, Clone, serde::Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub user: User,
    pub is_following: Option<bool>,
    pub is_self: bool,
}

/// `account_id` に紐づくアカウント自身のプロフィールかどうかを判定する（純粋関数、テスト用に分離）。
pub(crate) fn is_self_user(accounts: &[Account], account_id: &str, user_id: &str) -> bool {
    accounts
        .iter()
        .find(|a| a.id == account_id)
        .is_some_and(|a| a.user_id == user_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_account(id: &str, user_id: &str) -> Account {
        Account {
            id: id.to_string(),
            host: "misskey.example".to_string(),
            username: "alice".to_string(),
            user_id: user_id.to_string(),
            display_name: "Alice".to_string(),
            avatar_url: None,
        }
    }

    #[test]
    fn is_self_user_true_when_user_id_matches_account() {
        let accounts = vec![make_account("acc1", "u1"), make_account("acc2", "u2")];
        assert!(is_self_user(&accounts, "acc1", "u1"));
    }

    #[test]
    fn is_self_user_false_when_user_id_differs() {
        let accounts = vec![make_account("acc1", "u1")];
        assert!(!is_self_user(&accounts, "acc1", "u2"));
    }

    #[test]
    fn is_self_user_false_when_account_id_unknown() {
        let accounts = vec![make_account("acc1", "u1")];
        assert!(!is_self_user(&accounts, "unknown", "u1"));
    }
}
```

- [ ] **Step 2: モジュールを登録してテストが通ることを確認**

`src-tauri/src/commands/mod.rs` の先頭付近に `pub mod user;` を追加（既存の `pub mod note;` の下）:

```rust
pub mod account;
pub mod app;
pub mod clip;
pub mod column;
pub mod mute;
pub mod note;
pub mod user;
```

Run: `cd src-tauri && cargo test is_self_user`
Expected: 3件とも PASS

- [ ] **Step 3: `get_user_profile` コマンドを実装**

`src-tauri/src/commands/user.rs` に追記（`is_self_user` の下、`#[cfg(test)]` の上）:

```rust
/// ユーザープロフィール（bio/バナー/フォロー関係フラグ込み）を取得する。
#[tauri::command]
#[specta::specta]
pub async fn get_user_profile(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
) -> Result<UserProfile> {
    let client = state.client_for(&account_id)?;
    let raw = api::users::show(&client, &user_id).await?;
    let accounts = state.accounts.lock().unwrap().list();
    let is_self = is_self_user(&accounts, &account_id, &user_id);
    Ok(UserProfile {
        user: raw.user.into(),
        is_following: if is_self { None } else { raw.is_following },
        is_self,
    })
}
```

- [ ] **Step 4: ビルドが通ることを確認**

Run: `cd src-tauri && cargo build`
Expected: エラーなし（`Note` インポートは Task 6 で使うためこの時点では未使用警告が出る場合があるので、`use crate::domain::{Account, Note, User};` の `Note` は Task 6 まで削除せず残す）

- [ ] **Step 5: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/commands/user.rs src-tauri/src/commands/mod.rs
git commit -m "feat: get_user_profileコマンドを追加"
```

---

### Task 6: フォロー操作・ノート一覧コマンドを実装

**Files:**
- Modify: `src-tauri/src/commands/user.rs`

**Interfaces:**
- Consumes: Task 3 の `api::users::follow`/`unfollow`、`crate::domain::column::ColumnKind`、`crate::api::notes::fetch_notes`
- Produces:
  - `pub async fn follow_user(state, account_id, user_id) -> Result<()>`
  - `pub async fn unfollow_user(state, account_id, user_id) -> Result<()>`
  - `pub async fn get_user_notes(state, account_id, user_id, until_id: Option<String>) -> Result<Vec<Note>>`

- [ ] **Step 1: 実装**

`src-tauri/src/commands/user.rs` の `get_user_profile` の下に追記:

```rust
/// フォローする。
#[tauri::command]
#[specta::specta]
pub async fn follow_user(state: State<'_, AppState>, account_id: String, user_id: String) -> Result<()> {
    let client = state.client_for(&account_id)?;
    api::users::follow(&client, &user_id).await
}

/// フォロー解除する。
#[tauri::command]
#[specta::specta]
pub async fn unfollow_user(state: State<'_, AppState>, account_id: String, user_id: String) -> Result<()> {
    let client = state.client_for(&account_id)?;
    api::users::unfollow(&client, &user_id).await
}

/// プロフィールモーダルに埋め込むノート一覧。既存の `ColumnKind::User` と同じ
/// `users/notes` エンドポイントを使う（カラムとして常設せず、都度取得する）。
#[tauri::command]
#[specta::specta]
pub async fn get_user_notes(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
    until_id: Option<String>,
) -> Result<Vec<Note>> {
    let client = state.client_for(&account_id)?;
    let kind = crate::domain::ColumnKind::User { user_id };
    let (endpoint, body) = kind.rest_request(20, until_id.as_deref()).expect("User kind always has rest_request");
    api::notes::fetch_notes(&client, endpoint, &body).await
}
```

- [ ] **Step 2: ビルドが通ることを確認**

Run: `cd src-tauri && cargo build`
Expected: エラーなし

- [ ] **Step 3: 既存のRustテストが全て通ることを確認**

Run: `cd src-tauri && cargo test`
Expected: 全件 PASS（`generates_frontend_bindings` も含む。このテストが `frontend/src/bindings/tauri.gen.ts` を再生成するが、Task 5-6 のコマンドはまだ `specta_builder()` に未登録なのでTS側には現れない。Task 8で登録する）

- [ ] **Step 4: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/commands/user.rs
git commit -m "feat: follow_user/unfollow_user/get_user_notesコマンドを追加"
```

---

### Task 7: フォロワー/フォロー中一覧コマンドを実装

**Files:**
- Modify: `src-tauri/src/commands/user.rs`

**Interfaces:**
- Consumes: Task 4 の `api::users::followers`/`following`
- Produces: `pub async fn get_user_followers(state, account_id, user_id, until_id: Option<String>) -> Result<Vec<User>>`、`pub async fn get_user_following(state, account_id, user_id, until_id: Option<String>) -> Result<Vec<User>>`

- [ ] **Step 1: 実装**

`src-tauri/src/commands/user.rs` の `get_user_notes` の下に追記:

```rust
/// フォロワー一覧（ページング）。
#[tauri::command]
#[specta::specta]
pub async fn get_user_followers(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
    until_id: Option<String>,
) -> Result<Vec<User>> {
    let client = state.client_for(&account_id)?;
    api::users::followers(&client, &user_id, until_id.as_deref()).await
}

/// フォロー中一覧（ページング）。
#[tauri::command]
#[specta::specta]
pub async fn get_user_following(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
    until_id: Option<String>,
) -> Result<Vec<User>> {
    let client = state.client_for(&account_id)?;
    api::users::following(&client, &user_id, until_id.as_deref()).await
}
```

- [ ] **Step 2: ビルドが通ることを確認**

Run: `cd src-tauri && cargo build`
Expected: エラーなし

- [ ] **Step 3: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/commands/user.rs
git commit -m "feat: get_user_followers/get_user_followingコマンドを追加"
```

---

### Task 8: コマンドを specta_builder に登録し TS バインディングを生成

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `frontend/src/bindings/tauri.gen.ts` 内に `commands.getUserProfile`/`followUser`/`unfollowUser`/`getUserNotes`/`getUserFollowers`/`getUserFollowing`、および型 `UserProfile` が生成される

- [ ] **Step 1: `commands/mod.rs` に re-export を追加**

`src-tauri/src/commands/mod.rs` の末尾に追加:

```rust
#[allow(unused_imports)]
pub use user::{
    follow_user, get_user_followers, get_user_following, get_user_notes, get_user_profile,
    unfollow_user, UserProfile,
};
```

- [ ] **Step 2: `lib.rs` の `specta_builder()` に登録**

`specta_builder()` は `.commands(collect_commands![...])` の1箇所だけでコマンドを列挙しており（`tauri::generate_handler!` の並行登録は無い。invoke_handlerへの登録は `specta_builder()` の戻り値を経由して行われるため、ここに追加するだけでTSエクスポートと実行時ハンドラ登録の両方に反映される）、`src-tauri/src/lib.rs:22-97` の `collect_commands![` マクロの引数リストの末尾（`commands::clip::add_note_to_clip,` の直後、`])` の直前）に追加する:

```rust
            commands::clip::list_clips,
            commands::clip::create_clip,
            commands::clip::add_note_to_clip,
            commands::user::get_user_profile,
            commands::user::follow_user,
            commands::user::unfollow_user,
            commands::user::get_user_notes,
            commands::user::get_user_followers,
            commands::user::get_user_following,
        ])
```

- [ ] **Step 3: TSバインディングを再生成して確認**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。これにより `frontend/src/bindings/tauri.gen.ts` が更新される。

Run: `grep -n "getUserProfile\|followUser\|UserProfile" /home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/bindings/tauri.gen.ts`
Expected: `commands.getUserProfile`、`commands.followUser` 等と `export type UserProfile` が出力される

- [ ] **Step 4: 全Rustテストが通ることを確認**

Run: `cd src-tauri && cargo test`
Expected: 全件 PASS

- [ ] **Step 5: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/commands/mod.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: ユーザープロフィール系コマンドをspecta_builderに登録"
```

---

### Task 9: acct/displayNameヘルパーの共通化

**Files:**
- Create: `frontend/src/lib/userDisplay.ts`
- Test: `frontend/src/lib/userDisplay.test.ts`
- Modify: `frontend/src/ui/NoteCard.svelte`

**Interfaces:**
- Produces: `export function acct(u: { username: string; host: string | null }): string`、`export function displayName(u: { name: string | null; username: string }): string`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/userDisplay.test.ts` を作成:

```ts
import { describe, expect, it } from "vitest";
import { acct, displayName } from "./userDisplay";

describe("acct", () => {
  it("ローカルユーザーは @username", () => {
    expect(acct({ username: "alice", host: null })).toBe("@alice");
  });
  it("リモートユーザーは @username@host", () => {
    expect(acct({ username: "alice", host: "example.com" })).toBe("@alice@example.com");
  });
});

describe("displayName", () => {
  it("nameがあればnameを返す", () => {
    expect(displayName({ name: "Alice", username: "alice" })).toBe("Alice");
  });
  it("nameがnullならusernameを返す", () => {
    expect(displayName({ name: null, username: "alice" })).toBe("alice");
  });
});
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && pnpm vitest run src/lib/userDisplay.test.ts`
Expected: FAIL（`./userDisplay` が存在しない）

- [ ] **Step 3: 実装**

`frontend/src/lib/userDisplay.ts` を作成:

```ts
export function displayName(u: { name: string | null; username: string }): string {
  return u.name ?? u.username;
}

export function acct(u: { username: string; host: string | null }): string {
  return u.host ? `@${u.username}@${u.host}` : `@${u.username}`;
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/lib/userDisplay.test.ts`
Expected: PASS

- [ ] **Step 5: `NoteCard.svelte` をこのヘルパー利用に置き換える**

`frontend/src/ui/NoteCard.svelte` の import ブロックに追加:

```ts
  import { acct, displayName } from "../lib/userDisplay";
```

同ファイルのローカル定義2行を削除:

```ts
  const displayName = (u: Note["user"]) => u.name ?? u.username;
```

```ts
  const acct = (u: Note["user"]) => (u.host ? `@${u.username}@${u.host}` : `@${u.username}`);
```

（呼び出し箇所 `displayName(...)`/`acct(...)` はそのまま、シグネチャ互換のため変更不要）

- [ ] **Step 6: 既存のNoteCardテストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts`
Expected: 全件 PASS

- [ ] **Step 7: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/userDisplay.ts frontend/src/lib/userDisplay.test.ts frontend/src/ui/NoteCard.svelte
git commit -m "refactor: acct/displayNameヘルパーをlib/userDisplay.tsへ共通化"
```

---

### Task 10: プロフィールモーダルのグローバル状態

**Files:**
- Create: `frontend/src/lib/profileModal.svelte.ts`
- Test: `frontend/src/lib/profileModal.svelte.test.ts`

**Interfaces:**
- Produces:
  ```ts
  export type ProfileTarget = { userId: string } | { username: string; host: string | null };
  export function openProfile(target: ProfileTarget, accountId?: string): void;
  export function closeProfile(): void;
  export function currentProfileTarget(): ProfileTarget | null;
  export function currentProfileAccountId(): string | null;
  ```

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/profileModal.svelte.test.ts` を作成:

```ts
import { afterEach, describe, expect, it } from "vitest";
import {
  closeProfile,
  currentProfileAccountId,
  currentProfileTarget,
  openProfile,
} from "./profileModal.svelte";

afterEach(() => closeProfile());

describe("profileModal store", () => {
  it("初期状態はnull", () => {
    expect(currentProfileTarget()).toBeNull();
    expect(currentProfileAccountId()).toBeNull();
  });

  it("openProfileでターゲットとaccountIdが設定される", () => {
    openProfile({ userId: "u1" }, "acc1");
    expect(currentProfileTarget()).toEqual({ userId: "u1" });
    expect(currentProfileAccountId()).toBe("acc1");
  });

  it("accountId省略時はnull", () => {
    openProfile({ username: "alice", host: "example.com" });
    expect(currentProfileTarget()).toEqual({ username: "alice", host: "example.com" });
    expect(currentProfileAccountId()).toBeNull();
  });

  it("closeProfileで両方nullに戻る", () => {
    openProfile({ userId: "u1" }, "acc1");
    closeProfile();
    expect(currentProfileTarget()).toBeNull();
    expect(currentProfileAccountId()).toBeNull();
  });
});
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && pnpm vitest run src/lib/profileModal.svelte.test.ts`
Expected: FAIL（`./profileModal.svelte` が存在しない）

- [ ] **Step 3: 実装**

`frontend/src/lib/profileModal.svelte.ts` を作成:

```ts
export type ProfileTarget = { userId: string } | { username: string; host: string | null };

let target = $state<ProfileTarget | null>(null);
let accountId = $state<string | null>(null);

/// プロフィールモーダルを開く。`accountId` を省略した場合、呼び出し側（ProfileModal）が
/// app.defaultAccountId() にフォールバックする（mentionクリック等、経路上にaccountIdが無い場合用）。
export function openProfile(t: ProfileTarget, forAccountId?: string): void {
  target = t;
  accountId = forAccountId ?? null;
}

export function closeProfile(): void {
  target = null;
  accountId = null;
}

export function currentProfileTarget(): ProfileTarget | null {
  return target;
}

export function currentProfileAccountId(): string | null {
  return accountId;
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/lib/profileModal.svelte.test.ts`
Expected: 全件 PASS

- [ ] **Step 5: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/profileModal.svelte.ts frontend/src/lib/profileModal.svelte.test.ts
git commit -m "feat: プロフィールモーダルのグローバル開閉状態を追加"
```

---

### Task 11: AppStoreにプロフィール関連メソッドを追加

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`
- Test: `frontend/src/lib/store.svelte.test.ts`

**Interfaces:**
- Consumes: Task 8 の `commands.getUserProfile`/`followUser`/`unfollowUser`/`getUserNotes`/`getUserFollowers`/`getUserFollowing`（`frontend/src/bindings/tauri.gen.ts`）、`unwrapAcc`（`frontend/src/lib/ipc.ts`）
- Produces（`AppStore` クラスのメソッド、`resolveUser` の直後に追加）:
  - `async getUserProfile(accountId: string, userId: string): Promise<UserProfile>`
  - `async followUser(accountId: string, userId: string): Promise<void>`
  - `async unfollowUser(accountId: string, userId: string): Promise<void>`
  - `async getUserNotes(accountId: string, userId: string, untilId?: string): Promise<Note[]>`
  - `async getUserFollowers(accountId: string, userId: string, untilId?: string): Promise<User[]>`
  - `async getUserFollowing(accountId: string, userId: string, untilId?: string): Promise<User[]>`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/store.svelte.test.ts` に追加（既存の `describe`/`it` ブロック群の末尾、ファイル内の `ACCOUNT_ID`/`makeUser` 等は既存のものをそのまま使う）:

```ts
describe("app.getUserProfile / followUser / unfollowUser", () => {
  it("getUserProfileはコマンド結果をそのまま返す", async () => {
    const profile = { user: makeUser(), isFollowing: false, isSelf: false };
    // invokeMockはbindings生成コードのtypedError()に渡す前のraw invoke()相当。
    // typedError側が{status:"ok",data:...}に包むため、ここでは生の戻り値のみを返す。
    invokeMock.mockResolvedValueOnce(profile);
    const result = await app.getUserProfile(ACCOUNT_ID, "u1");
    expect(result).toEqual(profile);
    expect(invokeMock).toHaveBeenCalledWith(
      "get_user_profile",
      expect.objectContaining({ accountId: ACCOUNT_ID, userId: "u1" }),
    );
  });

  it("followUserはfollow_userコマンドを呼ぶ", async () => {
    invokeMock.mockResolvedValueOnce(null);
    await app.followUser(ACCOUNT_ID, "u1");
    expect(invokeMock).toHaveBeenCalledWith(
      "follow_user",
      expect.objectContaining({ accountId: ACCOUNT_ID, userId: "u1" }),
    );
  });

  it("unfollowUserはunfollow_userコマンドを呼ぶ", async () => {
    invokeMock.mockResolvedValueOnce(null);
    await app.unfollowUser(ACCOUNT_ID, "u1");
    expect(invokeMock).toHaveBeenCalledWith(
      "unfollow_user",
      expect.objectContaining({ accountId: ACCOUNT_ID, userId: "u1" }),
    );
  });
});
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && pnpm vitest run src/lib/store.svelte.test.ts -t "getUserProfile"`
Expected: FAIL（`app.getUserProfile is not a function`）

- [ ] **Step 3: 実装**

`frontend/src/lib/store.svelte.ts` の `resolveUser` メソッドの直後（Line 1089付近）に追加:

```ts
  /// プロフィールモーダル用: ユーザー詳細（bio/バナー/フォロー関係）を取得する。
  async getUserProfile(accountId: string, userId: string) {
    try {
      return await unwrapAcc(accountId, commands.getUserProfile(accountId, userId));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  /// フォローする。
  async followUser(accountId: string, userId: string) {
    try {
      await unwrapAcc(accountId, commands.followUser(accountId, userId));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  /// フォロー解除する。
  async unfollowUser(accountId: string, userId: string) {
    try {
      await unwrapAcc(accountId, commands.unfollowUser(accountId, userId));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  /// プロフィールモーダルに埋め込むノート一覧。
  async getUserNotes(accountId: string, userId: string, untilId?: string) {
    try {
      return await unwrapAcc(accountId, commands.getUserNotes(accountId, userId, untilId ?? null));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  /// フォロワー一覧。
  async getUserFollowers(accountId: string, userId: string, untilId?: string) {
    try {
      return await unwrapAcc(accountId, commands.getUserFollowers(accountId, userId, untilId ?? null));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  /// フォロー中一覧。
  async getUserFollowing(accountId: string, userId: string, untilId?: string) {
    try {
      return await unwrapAcc(accountId, commands.getUserFollowing(accountId, userId, untilId ?? null));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/lib/store.svelte.test.ts`
Expected: 全件 PASS（新規3件含む）

- [ ] **Step 5: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "feat: AppStoreにプロフィール取得・フォロー操作メソッドを追加"
```

---

### Task 12: ProfileModal.svelte（プロフィール本体＋フォローボタン）

**Files:**
- Create: `frontend/src/ui/ProfileModal.svelte`
- Test: `frontend/src/ui/ProfileModal.test.ts`

**Interfaces:**
- Consumes: `app.getUserProfile`/`followUser`/`unfollowUser`/`resolveUser`（Task 11・既存）、`profileModal.svelte.ts` の `ProfileTarget`/`closeProfile`、`Modal.svelte`（`{ title: string; onclose: () => void; children: Snippet }`）
- Props: `{ target: ProfileTarget; accountId: string }`（`accountId` は呼び出し側で `currentProfileAccountId() ?? app.defaultAccountId()` を解決して渡す）

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/ProfileModal.test.ts` を作成:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/svelte";
import type { Note } from "../bindings/tauri.gen";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
const invokeMock = vi.fn().mockResolvedValue({ status: "ok", data: null });
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: ProfileModal } = await import("./ProfileModal.svelte");

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
});

function profileResponse(overrides: Record<string, unknown> = {}) {
  return {
    user: {
      id: "u1",
      username: "alice",
      host: null,
      name: "Alice",
      avatarUrl: null,
      isBot: false,
      isCat: false,
      followersCount: 3,
      followingCount: 5,
      notesCount: 10,
      emojis: {},
      bio: "hello",
      bannerUrl: null,
    },
    isFollowing: false,
    isSelf: false,
    ...overrides,
  };
}

// invokeMockは生成コードのtypedError()に渡される前のraw invoke()相当。
// typedError側が{status:"ok",data:...}に包むため、ここでは生の戻り値(コマンドの実際の返り値そのもの)を返す。
// {status:"ok",data:...}でラップして返すと、typedErrorがそれをさらに包んでしまい
// (unwrapAccが1段階しか剥がせず)コンポーネントが受け取る値が壊れるので絶対にやらないこと。
describe("ProfileModal", () => {
  it("マウント時にget_user_profileを呼び、プロフィールを表示する", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_user_profile") return Promise.resolve(profileResponse());
      if (cmd === "get_user_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const { getByText } = render(ProfileModal, {
      props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(getByText("Alice")).toBeTruthy());
    expect(getByText("hello")).toBeTruthy();
  });

  it("自分自身の場合フォローボタンを表示しない", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_user_profile")
        return Promise.resolve(profileResponse({ isSelf: true, isFollowing: null }));
      if (cmd === "get_user_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const { queryByRole } = render(ProfileModal, {
      props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_user_profile", expect.anything()));
    expect(queryByRole("button", { name: /フォロー/ })).toBeNull();
  });

  it("フォローボタンクリックでfollow_userを呼ぶ", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_user_profile") return Promise.resolve(profileResponse());
      if (cmd === "get_user_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const { getByRole } = render(ProfileModal, {
      props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
    });
    const btn = await waitFor(() => getByRole("button", { name: "フォロー" }));
    btn.click();
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "follow_user",
        expect.objectContaining({ accountId: "acc1", userId: "u1" }),
      ),
    );
  });

  it("target propが変わったら前のユーザーのノート一覧を引き継がない", async () => {
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_user_profile") {
        const userId = args?.userId;
        return Promise.resolve(
          profileResponse({ user: { ...profileResponse().user, id: userId, username: userId } }),
        );
      }
      if (cmd === "get_user_notes") return Promise.resolve([{ id: "n1" } as Note]);
      return Promise.resolve(null);
    });
    const { rerender, getByText } = render(ProfileModal, {
      props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(getByText("u1")).toBeTruthy());
    invokeMock.mockClear();
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_user_profile") {
        const userId = args?.userId;
        return Promise.resolve(
          profileResponse({ user: { ...profileResponse().user, id: userId, username: userId } }),
        );
      }
      if (cmd === "get_user_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    await rerender({ target: { userId: "u2" }, accountId: "acc1", onclose: () => {} });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "get_user_notes",
        expect.objectContaining({ userId: "u2", untilId: null }),
      ),
    );
  });
});
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && pnpm vitest run src/ui/ProfileModal.test.ts`
Expected: FAIL（`./ProfileModal.svelte` が存在しない）

- [ ] **Step 3: 実装**

`frontend/src/ui/ProfileModal.svelte` を作成:

```svelte
<script lang="ts">
  import type { Note } from "../bindings/tauri.gen";
  import type { ProfileTarget } from "../lib/profileModal.svelte";
  import { app } from "../lib/store.svelte";
  import { acct, displayName } from "../lib/userDisplay";
  import Modal from "./Modal.svelte";
  import Mfm from "../render/Mfm.svelte";
  import NoteCard from "./NoteCard.svelte";
  import FollowListModal from "./FollowListModal.svelte";

  let { target, accountId, onclose }: { target: ProfileTarget; accountId: string; onclose: () => void } =
    $props();

  type ProfileState =
    | { status: "loading" }
    | { status: "error"; message: string }
    | { status: "ready"; profile: Awaited<ReturnType<typeof app.getUserProfile>> };

  let state = $state<ProfileState>({ status: "loading" });
  let notes = $state<Note[]>([]);
  let notesBusy = $state(false);
  let notesDone = $state(false);
  let followBusy = $state(false);
  let followErr = $state<string | null>(null);
  let followListKind = $state<"followers" | "following" | null>(null);

  async function resolveUserId(): Promise<string> {
    if ("userId" in target) return target.userId;
    const acctStr = target.host ? `${target.username}@${target.host}` : target.username;
    const u = await app.resolveUser(accountId, acctStr);
    return u.id;
  }

  async function load() {
    // target が変わって同じコンポーネントインスタンスが再利用されるケース（FollowListModalの
    // 行クリック→openProfile→App.svelte側propsのみ更新）があるため、前のユーザーの状態を必ず捨てる。
    state = { status: "loading" };
    notes = [];
    notesDone = false;
    notesBusy = false;
    followErr = null;
    followListKind = null;
    try {
      const userId = await resolveUserId();
      const profile = await app.getUserProfile(accountId, userId);
      state = { status: "ready", profile };
      void loadMoreNotes(profile.user.id);
    } catch (e) {
      state = { status: "error", message: String(e) };
    }
  }

  async function loadMoreNotes(userId: string) {
    if (notesBusy || notesDone) return;
    notesBusy = true;
    try {
      const untilId = notes.length > 0 ? notes[notes.length - 1].id : undefined;
      const page = await app.getUserNotes(accountId, userId, untilId);
      if (page.length === 0) notesDone = true;
      notes = [...notes, ...page];
    } finally {
      notesBusy = false;
    }
  }

  async function toggleFollow() {
    if (state.status !== "ready" || state.profile.isFollowing === null) return;
    followBusy = true;
    followErr = null;
    const wasFollowing = state.profile.isFollowing;
    state.profile.isFollowing = !wasFollowing;
    try {
      if (wasFollowing) {
        await app.unfollowUser(accountId, state.profile.user.id);
      } else {
        await app.followUser(accountId, state.profile.user.id);
      }
    } catch (e) {
      state.profile.isFollowing = wasFollowing;
      followErr = String(e);
    } finally {
      followBusy = false;
    }
  }

  function addAsColumn() {
    if (state.status !== "ready") return;
    void app.addColumn(
      accountId,
      { type: "user", userId: state.profile.user.id },
      { kind: "keywords", value: [] },
      undefined,
      displayName(state.profile.user),
    );
    onclose();
  }

  $effect(() => {
    void load();
  });
</script>

<Modal title="プロフィール" {onclose}>
  {#if state.status === "loading"}
    <p>読み込み中…</p>
  {:else if state.status === "error"}
    <p class="err">{state.message}</p>
    <button onclick={load}>再試行</button>
  {:else}
    {@const profile = state.profile}
    {#if profile.user.bannerUrl}
      <img class="banner" src={profile.user.bannerUrl} alt="" />
    {/if}
    <div class="head">
      {#if profile.user.avatarUrl}
        <img class="avatar" src={profile.user.avatarUrl} alt="" />
      {:else}
        <div class="avatar placeholder"></div>
      {/if}
      <div class="names">
        <span class="name"><Mfm text={displayName(profile.user)} emojis={profile.user.emojis} simple /></span>
        <span class="acct">{acct(profile.user)}</span>
      </div>
      {#if !profile.isSelf}
        <button onclick={toggleFollow} disabled={followBusy}>
          {profile.isFollowing ? "フォロー解除" : "フォロー"}
        </button>
      {/if}
    </div>
    {#if followErr}<p class="err">{followErr}</p>{/if}
    {#if profile.user.bio}
      <p class="bio"><Mfm text={profile.user.bio} emojis={profile.user.emojis} /></p>
    {/if}
    <div class="stats">
      <button onclick={() => (followListKind = "following")}>フォロー中 {profile.user.followingCount}</button>
      <button onclick={() => (followListKind = "followers")}>フォロワー {profile.user.followersCount}</button>
      <span>ノート {profile.user.notesCount}</span>
    </div>
    <button onclick={addAsColumn}>カラムとして追加</button>
    <div class="notes">
      {#each notes as note (note.id)}
        <NoteCard {note} {accountId} />
      {/each}
      {#if !notesDone}
        <button onclick={() => loadMoreNotes(profile.user.id)} disabled={notesBusy}>もっと見る</button>
      {/if}
    </div>
    {#if followListKind}
      <FollowListModal
        kind={followListKind}
        userId={profile.user.id}
        {accountId}
        onclose={() => (followListKind = null)}
      />
    {/if}
  {/if}
</Modal>

<style>
  .banner {
    width: 100%;
    aspect-ratio: 3 / 1;
    object-fit: cover;
    border-radius: 8px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }
  .avatar {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    object-fit: cover;
  }
  .avatar.placeholder {
    background: var(--surface-2);
  }
  .names {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .acct {
    color: var(--text-dim);
    font-size: 0.85em;
  }
  .stats {
    display: flex;
    gap: 12px;
    margin: 8px 0;
  }
  .err {
    color: var(--danger, #d33);
  }
  .notes {
    max-height: 40vh;
    overflow-y: auto;
    margin-top: 8px;
  }
</style>
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/ui/ProfileModal.test.ts`
Expected: 全件 PASS

- [ ] **Step 5: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/ProfileModal.svelte frontend/src/ui/ProfileModal.test.ts
git commit -m "feat: ProfileModalコンポーネントを追加"
```

---

### Task 13: FollowListModal.svelte（フォロワー/フォロー中一覧）

**Files:**
- Create: `frontend/src/ui/FollowListModal.svelte`
- Test: `frontend/src/ui/FollowListModal.test.ts`

**Interfaces:**
- Consumes: `app.getUserFollowers`/`getUserFollowing`（Task 11）、`profileModal.svelte.ts` の `openProfile`
- Props: `{ kind: "followers" | "following"; userId: string; accountId: string; onclose: () => void }`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/FollowListModal.test.ts` を作成:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
const invokeMock = vi.fn().mockResolvedValue({ status: "ok", data: null });
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: FollowListModal } = await import("./FollowListModal.svelte");

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
});

function makeUser(id: string, username: string) {
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
  };
}

// invokeMockは生成コードのtypedError()に渡される前のraw invoke()相当。
// typedError側が{status:"ok",data:...}に包むため、ここでは生の戻り値のみを返す
// ({status:"ok",data:...}でラップして返すと二重ラップになりコンポーネントが壊れた値を受け取る)。
describe("FollowListModal", () => {
  it("kind=followersでget_user_followersを呼び一覧表示する", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_user_followers") return Promise.resolve([makeUser("u2", "bob")]);
      return Promise.resolve(null);
    });
    const { getByText } = render(FollowListModal, {
      props: { kind: "followers", userId: "u1", accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(getByText("bob")).toBeTruthy());
    expect(invokeMock).toHaveBeenCalledWith(
      "get_user_followers",
      expect.objectContaining({ accountId: "acc1", userId: "u1" }),
    );
  });

  it("kind=followingでget_user_followingを呼ぶ", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_user_following") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    render(FollowListModal, {
      props: { kind: "following", userId: "u1", accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "get_user_following",
        expect.objectContaining({ accountId: "acc1", userId: "u1" }),
      ),
    );
  });
});
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && pnpm vitest run src/ui/FollowListModal.test.ts`
Expected: FAIL（`./FollowListModal.svelte` が存在しない）

- [ ] **Step 3: 実装**

`frontend/src/ui/FollowListModal.svelte` を作成:

```svelte
<script lang="ts">
  import type { User } from "../bindings/tauri.gen";
  import { app } from "../lib/store.svelte";
  import { acct, displayName } from "../lib/userDisplay";
  import { openProfile } from "../lib/profileModal.svelte";
  import Modal from "./Modal.svelte";

  let {
    kind,
    userId,
    accountId,
    onclose,
  }: { kind: "followers" | "following"; userId: string; accountId: string; onclose: () => void } = $props();

  let users = $state<User[]>([]);
  let busy = $state(false);
  let done = $state(false);
  let err = $state<string | null>(null);

  async function loadMore() {
    if (busy || done) return;
    busy = true;
    err = null;
    try {
      const untilId = users.length > 0 ? users[users.length - 1].id : undefined;
      const page =
        kind === "followers"
          ? await app.getUserFollowers(accountId, userId, untilId)
          : await app.getUserFollowing(accountId, userId, untilId);
      if (page.length === 0) done = true;
      users = [...users, ...page];
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    void loadMore();
  });
</script>

<Modal title={kind === "followers" ? "フォロワー" : "フォロー中"} {onclose}>
  {#if err}<p class="err">{err}</p>{/if}
  <ul class="list">
    {#each users as u (u.id)}
      <li>
        <button class="row" onclick={() => openProfile({ userId: u.id }, accountId)}>
          {#if u.avatarUrl}
            <img class="avatar" src={u.avatarUrl} alt="" />
          {:else}
            <div class="avatar placeholder"></div>
          {/if}
          <span class="name">{displayName(u)}</span>
          <span class="acct">{acct(u)}</span>
        </button>
      </li>
    {/each}
  </ul>
  {#if !done}
    <button onclick={loadMore} disabled={busy}>もっと見る</button>
  {/if}
</Modal>

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 50vh;
    overflow-y: auto;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 0;
    background: none;
    border: none;
    cursor: pointer;
    text-align: left;
  }
  .avatar {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    object-fit: cover;
  }
  .avatar.placeholder {
    background: var(--surface-2);
  }
  .acct {
    color: var(--text-dim);
    font-size: 0.85em;
  }
  .err {
    color: var(--danger, #d33);
  }
</style>
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/ui/FollowListModal.test.ts`
Expected: 全件 PASS

- [ ] **Step 5: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/FollowListModal.svelte frontend/src/ui/FollowListModal.test.ts
git commit -m "feat: FollowListModalコンポーネントを追加"
```

- [ ] **Step 6: ProfileModal.svelteにFollowListModalを配線する**

Task 12で作成した `frontend/src/ui/ProfileModal.svelte` は、この時点ではまだ存在しなかった `FollowListModal.svelte` への
importと描画を意図的に省略し、フックだけを残してある(`followListKind` state、統計ボタンのonclick、
`// NOTE: フォロー中/フォロワー一覧モーダル(FollowListModal.svelte)はTask 13で作成予定。` というコメント)。
このStepで実際に配線する。

`frontend/src/ui/ProfileModal.svelte` の以下のコメント行(import群の下)を:

```svelte
  // NOTE: フォロー中/フォロワー一覧モーダル(FollowListModal.svelte)はTask 13で作成予定。
  // followListKind はここで統計ボタンのクリック状態を保持するが、Task 13でコンポーネントが
  // 作成され次第この場所で描画を配線する。
```

以下に置き換える:

```svelte
  import FollowListModal from "./FollowListModal.svelte";
```

`</Modal>` 直前(`{/if}` の直後、`</Modal>` の直前)に以下を追加する:

```svelte
    {#if followListKind}
      <FollowListModal
        kind={followListKind}
        userId={profile.user.id}
        {accountId}
        onclose={() => (followListKind = null)}
      />
    {/if}
```

（`profile` は `{#if state.status === "ready"}` 相当のブロック内の `{@const profile = ...}` で束縛されている変数なので、
このifブロックの中に置くこと。実装済みファイルの実際の変数束縛箇所を確認してから挿入すること。）

- [ ] **Step 7: テストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/ui/ProfileModal.test.ts src/ui/FollowListModal.test.ts`
Expected: 全件 PASS

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 8: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/ProfileModal.svelte
git commit -m "feat: ProfileModalにFollowListModalを配線"
```

---

### Task 14: NoteCard・MfmNodeにクリック導線を追加し、App.svelteでモーダルをマウント

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte`
- Modify: `frontend/src/render/MfmNode.svelte`
- Modify: `frontend/src/App.svelte`
- Test: `frontend/src/ui/NoteCard.test.ts`（既存ファイルに追記）
- Test: `frontend/src/render/Mfm.test.ts`（既存ファイルに追記）

**Interfaces:**
- Consumes: Task 10 の `openProfile`/`currentProfileTarget`/`currentProfileAccountId`/`closeProfile`、Task 12 の `ProfileModal`

- [ ] **Step 1: NoteCardクリックの失敗するテストを書く**

`frontend/src/ui/NoteCard.test.ts` に追加（ファイル冒頭で `vi.mock("../lib/profileModal.svelte", ...)` していなければ追加し、既存の `makeNote`/`render` ヘルパーを使う）:

```ts
import { openProfile } from "../lib/profileModal.svelte";

vi.mock("../lib/profileModal.svelte", () => ({ openProfile: vi.fn() }));

// ... 既存の describe ブロック群の末尾に追加 ...
describe("プロフィール導線", () => {
  it("アバタークリックでopenProfileが呼ばれる", () => {
    const note = makeNote();
    const { container } = render(NoteCard, { props: { note, accountId: "acc1" } });
    const avatar = container.querySelector(".avatar") as HTMLElement;
    avatar.click();
    expect(openProfile).toHaveBeenCalledWith({ userId: note.user.id }, "acc1");
  });
});
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts -t "プロフィール導線"`
Expected: FAIL（`.avatar` にクリックハンドラが無く `openProfile` が呼ばれない）

- [ ] **Step 3: NoteCard.svelte にクリックハンドラを追加**

`frontend/src/ui/NoteCard.svelte` の import ブロックに追加:

```ts
  import { openProfile } from "../lib/profileModal.svelte";
```

同ファイルの `<div class="row">` 以下（既存コード、Line 269-282付近）を編集:

```svelte
  <div class="row">
    {#if inner.user.avatarUrl}
      <img
        class="avatar"
        src={inner.user.avatarUrl}
        alt=""
        loading="lazy"
        onclick={() => openProfile({ userId: inner.user.id }, accountId)}
        style="cursor: pointer"
      />
    {:else}
      <div
        class="avatar placeholder"
        onclick={() => openProfile({ userId: inner.user.id }, accountId)}
        role="button"
        tabindex="0"
        onkeydown={(e) => e.key === "Enter" && openProfile({ userId: inner.user.id }, accountId)}
      ></div>
    {/if}
    <div class="body">
      <header class="head">
        <span
          class="name"
          onclick={() => openProfile({ userId: inner.user.id }, accountId)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === "Enter" && openProfile({ userId: inner.user.id }, accountId)}
          style="cursor: pointer"
        ><Mfm
          text={displayName(inner.user)}
          emojis={proxiedEmojiMap(inner.user.emojis, instanceHost)}
          simple
        /></span>
        <span
          class="acct"
          onclick={() => openProfile({ userId: inner.user.id }, accountId)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === "Enter" && openProfile({ userId: inner.user.id }, accountId)}
          style="cursor: pointer"
        >{acct(inner.user)}</span>
        <span class="time" title={new Date(inner.createdAt * 1000).toLocaleString()}>
          {relativeTime(inner.createdAt)}
        </span>
        {#if inner.visibility !== "public"}
          {@const VisIcon = VIS_ICON[inner.visibility]}
          <span class="vis" title={VIS_LABEL[inner.visibility]}><VisIcon size={12} /></span>
        {/if}
```

（`{#if inner.visibility !== "public"}` 以降は既存のまま変更不要）

- [ ] **Step 4: テストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts`
Expected: 全件 PASS

- [ ] **Step 5: MfmNodeのmentionクリックの失敗するテストを書く**

`frontend/src/render/Mfm.test.ts` に追加（ファイル冒頭に `vi.mock` を追加）:

```ts
import { openProfile } from "../lib/profileModal.svelte";

vi.mock("../lib/profileModal.svelte", () => ({ openProfile: vi.fn() }));

// 既存の "renders a mention" テストの近くに追加
it("mentionクリックでopenProfileが呼ばれる", () => {
  const { container } = render(Mfm, { props: { text: "@alice@example.com hi" } });
  const mention = container.querySelector("span.mfm-mention") as HTMLElement;
  mention.click();
  expect(openProfile).toHaveBeenCalledWith({ username: "alice", host: "example.com" });
});

it("ローカルユーザーへのmentionはhost:nullで呼ばれる", () => {
  const { container } = render(Mfm, { props: { text: "@bob hi" } });
  const mention = container.querySelector("span.mfm-mention") as HTMLElement;
  mention.click();
  expect(openProfile).toHaveBeenCalledWith({ username: "bob", host: null });
});
```

- [ ] **Step 6: テストが失敗することを確認**

Run: `cd frontend && pnpm vitest run src/render/Mfm.test.ts -t "mentionクリック"`
Expected: FAIL（`span.mfm-mention` にクリックハンドラが無い）

- [ ] **Step 7: MfmNode.svelte にmentionクリックハンドラを追加**

`frontend/src/render/MfmNode.svelte` の import ブロックに追加:

```ts
  import { openProfile } from "../lib/profileModal.svelte";
```

mentionノードの描画（Line 93-94）を編集:

```svelte
{:else if node.type === "mention"}
  <span
    class="mfm-mention"
    onclick={() => openProfile({ username: p.username, host: p.host ?? null })}
    role="button"
    tabindex="0"
    onkeydown={(e) => e.key === "Enter" && openProfile({ username: p.username, host: p.host ?? null })}
    style="cursor: pointer"
  >{p.acct}</span>
```

（`mfm-js` の mention ノードは `props.username`/`props.host`/`props.acct` を持つ。`p.host` はローカルユーザーなら `null`）

- [ ] **Step 8: テストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/render/Mfm.test.ts`
Expected: 全件 PASS

- [ ] **Step 9: App.svelteでProfileModalをマウント**

`frontend/src/App.svelte` の import ブロックに追加:

```ts
  import ProfileModal from "./ui/ProfileModal.svelte";
  import { currentProfileTarget, currentProfileAccountId, closeProfile } from "./lib/profileModal.svelte";
```

`{#if showSettings}...{/if}` ブロック（Line 200-207付近）の直後、`</div>`（Line 208）の手前に追加:

```svelte
  {#if currentProfileTarget()}
    <ProfileModal
      target={currentProfileTarget()!}
      accountId={currentProfileAccountId() ?? app.defaultAccountId()}
      onclose={closeProfile}
    />
  {/if}
```

- [ ] **Step 10: フロントエンド全体のチェックが通ることを確認**

Run: `cd frontend && pnpm check`
Expected: エラーなし

Run: `cd frontend && pnpm vitest run`
Expected: 全件 PASS

- [ ] **Step 11: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/NoteCard.svelte frontend/src/ui/NoteCard.test.ts frontend/src/render/MfmNode.svelte frontend/src/render/Mfm.test.ts frontend/src/App.svelte
git commit -m "feat: ノートのアバター/名前/mentionクリックでプロフィールモーダルを開けるようにする"
```

---

### Task 15: 手動確認（`cargo tauri dev`）

**Files:** なし（動作確認のみ）

- [ ] **Step 1: 開発サーバーを起動**

Run: `cargo tauri dev`（**`cargo run` や `./target/debug/tsumugi` を直接実行しない** — `CLAUDE.md` 参照）

- [ ] **Step 2: 動作確認項目**

1. タイムライン上のノートのアバター、または表示名/acctをクリック → プロフィールモーダルが開き、バナー・アバター・bio・フォロー統計・ノート一覧が表示される。
2. 他人のプロフィールでフォロー/フォロー解除ボタンが動作する（Misskeyサーバー側の実フォロー状態が変わることを確認）。**実サーバーに繋ぐ場合、フォロー対象は自分のサブアカウント等にすること — 見知らぬ他人を実際にフォローしてしまわないよう注意し、確認後は必ずフォロー解除で元に戻す。**
3. 自分自身のノートのアバターをクリック → フォローボタンが表示されない。
4. ノート本文中の `@mention` をクリック → 該当ユーザーのプロフィールモーダルが開く（ローカル・リモート両方）。
5. 「フォロー中」「フォロワー」の数字をクリック → 一覧モーダルが開き、行クリックでさらにそのユーザーのプロフィールに遷移する。
6. 「カラムとして追加」→ 新しいカラムとして `ColumnKind::User` のノート一覧カラムが追加され、モーダルが閉じる。
7. プロフィールモーダル内のノート一覧をスクロールし、末尾で追加ページが読み込まれる。
8. 存在しないユーザー（削除済み等）のプロフィールを開いた場合にエラー表示＋再試行ボタンが出る。

- [ ] **Step 3: 問題があれば該当タスクに戻って修正し、このタスクのStep 1-2を再実行する**

---

## Self-Review メモ

- **Spec coverage:** spec の全項目（bio/banner表示、フォロー中/フォロワー数・ボタン、自分自身は非表示、埋め込みノート一覧、カラム追加導線、フォロー中/フォロワー一覧、NoteCard/mention両方の起点）に対応するタスクあり。
- **Placeholder scan:** TBD/TODO等なし。全ステップに実コードあり。
- **Type consistency:** `UserProfile { user: User; isFollowing: boolean | null; isSelf: boolean }`（TS側camelCase化後）を Task 5 で定義し、Task 12 のProfileModalで一貫して使用。`ProfileTarget` は Task 10 で定義し Task 12-14 で一貫して使用。
- **advisorレビュー反映済み:** (1) `ProfileModal.load()` が `notes`/`notesDone`/`followListKind` 等をリセットしていなかった不具合を修正（FollowListModal経由でtargetが変わっても前ユーザーの状態が残るバグ）、リグレッションテストも追加。(2) Task 8 は `lib.rs:22-97` の実コードを確認した上で正確な追記位置に修正（`generate_handler!` の並行登録は存在しない）。(3) 埋め込みノート一覧の `NoteCard` に `accountId` を渡すよう修正（渡し忘れると返信/リアクション操作ができない読み取り専用表示になってしまう）。(4) Task 15 に実サーバーでの誤フォロー防止の注意書きを追加。
