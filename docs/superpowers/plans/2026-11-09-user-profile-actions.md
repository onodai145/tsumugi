# ユーザープロフィールからのフォロー以外の操作 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ProfileModal の「…」メニューに、ブロック・リノートミュート・リストへの追加(トグル)・通報・リモートユーザーのブラウザ表示を追加する(Issue #546)。

**Architecture:** Rust 側に Misskey REST のラッパとコマンドを足し(ブロックは再同期付き)、`UserProfile` に `isBlocking` / `isRenoteMuted` / `profileUrl` を載せる。フロントは `UserMenu.svelte` を拡張し、リスト用の `UserListsPanel.svelte` と通報用の `ReportUserDialog.svelte` を新設、`ProfileModal.svelte` の操作ハンドラを `runUserAction` に共通化する。#545 の時限ミュートと同じ構造(`mute_user_core` / `resync_after_mute_change`、`UserMenu` の同一パネル内ビュー切替)に揃える。

**Tech Stack:** Rust(Tauri v2、tauri-specta、wiremock)、Svelte 5 + TypeScript、Vitest、`@lucide/svelte`、`@tauri-apps/plugin-opener`。

**Spec:** `docs/superpowers/specs/2026-11-09-user-profile-actions-design.md`

## Global Constraints

- 作業は `feat/issue-546-profile-actions` ブランチ上で行う。`main` 上では編集しない。
- コミットメッセージは `type(scope): 件名` の1行のみ(本文・箇条書きなし)。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` を付ける。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら停止して報告し、再試行しない。
- `frontend/src/bindings/tauri.gen.ts` は生成物。手で編集しない。`cd src-tauri && cargo test generates_frontend_bindings` で再生成する。
- 新しいコマンドは `src-tauri/src/lib.rs` の `specta_builder()` に登録する。
- 見た目の値は `docs/design/style-guide.md` の既存スケール(`rounded-md` / `rounded-lg` / `rounded-xl` など)だけを使い、`rounded-[Npx]` のような即値は作らない。
- ダイアログの z-index: ドロップダウンメニュー(z-1010)から開くダイアログは `ConfirmDialog` の `z={1020}` を渡す。
- `cargo tauri dev` / `cargo run` を検証で直接起動しない。実 UI の確認は Xvfb 越し(`WAYLAND_DISPLAY` も unset)で行い、起動したプロセスは完了前に正確な PID で止める(`pkill` / `killall` は使わない)。
- ユーザー向けの文言・ドキュメントはすべて日本語。
- MiAuth 権限: `write:blocks`(ブロック)、`write:mutes`(リノートミュート。既存)、`write:account`(リスト。既存)、`write:report-abuse`(通報)。
- 通報のコメントは 1〜2048 文字(上流 `users/report-abuse` の制約)。Rust 側でも空白のみを拒否する。
- 外部サーバーが返す URL は信用しない。`profileUrl` は http/https のみ(Rust 側で `sanitize_profile_url`)。

## Review Focus

- **すでに状態が変わっている**(他クライアントで先にブロック・リノートミュートされていた/解除されていた): `ALREADY_BLOCKING` / `NOT_BLOCKING` / `ALREADY_MUTING` / `NOT_MUTING` で表示を直し、エラーではなく補正メッセージを出す。→ Task 8
- **外部サーバーが返す `url` / `uri` が `javascript:` / `data:` / 空文字 / 不正な URL**: `profileUrl` は `None` になり、「ブラウザで開く」は出ない。→ Task 2
- **リストへの追加で `ALREADY_ADDED` / `YOU_HAVE_BEEN_BLOCKED` / `TOO_MANY_USERS`**: パネル内にエラーを出し、チェックは変えず、メニューは開いたままにする。→ Task 6
- **通報の二重送信・失敗時の入力消失**: 送信中はボタンを無効にして二重送信を防ぎ、失敗したら入力を残してダイアログ内にエラーを出す。→ Task 6
- **プロフィールが切り替わった後に古い操作の応答が返る**: 世代番号で無視し、新しいプロフィールの busy を触らない。→ Task 8
- **既存アカウントに `write:blocks` / `write:report-abuse` が無い(403)**: 既存の `unwrapAcc` の `ForbiddenError` 経路で、エラーとして表示する(クラッシュしない)。→ Task 8

---

### Task 1: MiAuth に `write:blocks` と `write:report-abuse` を追加する

**Files:**
- Modify: `src-tauri/src/session/miauth.rs:11-26`(`PERMISSIONS`)、同ファイル末尾のテスト

**Interfaces:**
- Produces: `PERMISSIONS` に `write:blocks` と `write:report-abuse` が含まれる。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/session/miauth.rs` の `mod tests` の末尾(`requests_write_mutes_for_timed_mutes` の後)に追加する。

```rust
    #[test]
    fn requests_write_blocks_and_write_report_abuse() {
        for permission in ["write:blocks", "write:report-abuse", "write:account", "write:mutes"] {
            assert!(PERMISSIONS.contains(&permission), "{permission} が無い");
        }
        let u = build_miauth_url("misskey.io", "11111111-2222-3333-4444-555555555555");
        assert!(u.contains("write%3Ablocks") || u.contains("write:blocks"));
        assert!(u.contains("write%3Areport-abuse") || u.contains("write:report-abuse"));
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib requests_write_blocks_and_write_report_abuse`
Expected: FAIL(`write:blocks が無い`)

- [ ] **Step 3: 実装する**

`PERMISSIONS` の `"read:blocks",` の次の行に `"write:blocks",` を、`"write:account",` の次の行に `"write:report-abuse",` を足す。

```rust
    "read:blocks",
    "write:blocks",
```

```rust
    "write:account",
    "write:report-abuse",
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib miauth`
Expected: PASS(既存のテストも通る)

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/src/session/miauth.rs
git commit -m "$(cat <<'EOF'
feat(auth): ブロックと通報の権限をMiAuthに追加 (#546)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `UserProfile` に `isBlocking` / `isRenoteMuted` / `profileUrl` を載せる

**Files:**
- Modify: `src-tauri/src/api/users.rs`(`RawUserShow`、テスト)
- Modify: `src-tauri/src/commands/user.rs`(`UserProfile`、`get_user_profile`、`sanitize_profile_url`、テスト)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Produces(Rust): `RawUserShow { is_blocking: Option<bool>, is_renote_muted: Option<bool>, url: Option<String>, uri: Option<String>, .. }`、`pub(crate) fn sanitize_profile_url(host: Option<&str>, url: Option<&str>, uri: Option<&str>) -> Option<String>`。
- Produces(TS): `UserProfile` に `isBlocking: boolean | null`、`isRenoteMuted: boolean | null`、`profileUrl: string | null`。

- [ ] **Step 1: 失敗するテストを書く(API の読み取り)**

`src-tauri/src/api/users.rs` の `mod tests` の末尾(`raw_user_show_without_is_muted_is_none` の後)に追加する。

```rust
    #[test]
    fn raw_user_show_reads_block_renote_mute_and_profile_urls() {
        let raw: RawUserShow = serde_json::from_value(serde_json::json!({
            "id": "u1", "username": "alice", "host": "remote.example",
            "isBlocking": true, "isRenoteMuted": false,
            "url": "https://remote.example/@alice",
            "uri": "https://remote.example/users/alice"
        }))
        .unwrap();
        assert_eq!(raw.is_blocking, Some(true));
        assert_eq!(raw.is_renote_muted, Some(false));
        assert_eq!(raw.url.as_deref(), Some("https://remote.example/@alice"));
        assert_eq!(raw.uri.as_deref(), Some("https://remote.example/users/alice"));
    }

    #[test]
    fn raw_user_show_without_the_new_fields_leaves_them_none() {
        let raw: RawUserShow = serde_json::from_value(serde_json::json!({
            "id": "u1", "username": "alice", "host": null
        }))
        .unwrap();
        assert_eq!(raw.is_blocking, None);
        assert_eq!(raw.is_renote_muted, None);
        assert_eq!(raw.url, None);
        assert_eq!(raw.uri, None);
    }
```

- [ ] **Step 2: `sanitize_profile_url` のテストを書く**

`src-tauri/src/commands/user.rs` の `mod tests` の末尾(`relation_flag_is_hidden_for_the_own_profile` の後)に追加する。

```rust
    #[test]
    fn profile_url_is_none_for_local_users() {
        assert_eq!(
            sanitize_profile_url(None, Some("https://misskey.example/@alice"), None),
            None
        );
    }

    #[test]
    fn profile_url_prefers_url_over_uri() {
        assert_eq!(
            sanitize_profile_url(
                Some("remote.example"),
                Some("https://remote.example/@alice"),
                Some("https://remote.example/users/alice")
            ),
            Some("https://remote.example/@alice".to_string())
        );
    }

    #[test]
    fn profile_url_falls_back_to_uri_when_url_is_missing_or_unsafe() {
        let uri = Some("https://remote.example/users/alice");
        assert_eq!(
            sanitize_profile_url(Some("remote.example"), None, uri),
            Some("https://remote.example/users/alice".to_string())
        );
        assert_eq!(
            sanitize_profile_url(Some("remote.example"), Some("javascript:alert(1)"), uri),
            Some("https://remote.example/users/alice".to_string())
        );
    }

    #[test]
    fn profile_url_rejects_non_http_schemes_and_garbage() {
        for bad in ["javascript:alert(1)", "data:text/html,<b>x</b>", "file:///etc/passwd", "", "not a url", "https://"] {
            assert_eq!(
                sanitize_profile_url(Some("remote.example"), Some(bad), Some(bad)),
                None,
                "{bad:?} は採用しない"
            );
        }
    }
```

- [ ] **Step 3: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib raw_user_show profile_url`
Expected: コンパイルエラー(`is_blocking` フィールドと `sanitize_profile_url` が無い)

- [ ] **Step 4: `RawUserShow` を実装する**

`src-tauri/src/api/users.rs` の `RawUserShow` の `is_muted` の後に追加する。

```rust
    #[serde(default)]
    pub is_blocking: Option<bool>,
    #[serde(default)]
    pub is_renote_muted: Option<bool>,
    /// リモートユーザーのプロフィールページ。ローカルユーザーでは `null`。
    #[serde(default)]
    pub url: Option<String>,
    /// リモートユーザーの ActivityPub ID。`url` が無いときの代替。
    #[serde(default)]
    pub uri: Option<String>,
```

- [ ] **Step 5: `UserProfile` と `sanitize_profile_url` を実装する**

`src-tauri/src/commands/user.rs` を編集する。

`UserProfile` を次の形にする(コメントも更新)。

```rust
/// プロフィールモーダル用のレスポンス。関係フラグ(`is_following` / `is_muted` / `is_blocking` /
/// `is_renote_muted`)は自分自身の場合 `None`。`profile_url` はリモートユーザーの元ページ
/// (http/https のみ。ローカルユーザーや不正な値では `None`)。
#[derive(Debug, Clone, serde::Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub user: User,
    pub is_following: Option<bool>,
    pub is_muted: Option<bool>,
    pub is_blocking: Option<bool>,
    pub is_renote_muted: Option<bool>,
    pub profile_url: Option<String>,
    pub is_self: bool,
}
```

`relation_flag` の後に追加する。

```rust
/// リモートユーザーの元ページの URL を選ぶ(純粋関数、テスト用に分離)。`url` → `uri` の順で、最初に
/// ホストを持つ http/https として解釈できるものを返す。ローカルユーザー(`host` が `None`)では `None`。
/// 値は外部サーバーが返すので信用せず、`javascript:` / `data:` などは読み飛ばす。
pub(crate) fn sanitize_profile_url(host: Option<&str>, url: Option<&str>, uri: Option<&str>) -> Option<String> {
    host?;
    [url, uri].into_iter().flatten().find_map(|candidate| {
        let parsed = url::Url::parse(candidate).ok()?;
        let is_web = matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some();
        is_web.then(|| parsed.to_string())
    })
}
```

`get_user_profile` の `Ok(UserProfile { .. })` を次にする。

```rust
    Ok(UserProfile {
        is_following: relation_flag(is_self, raw.is_following),
        is_muted: relation_flag(is_self, raw.is_muted),
        is_blocking: relation_flag(is_self, raw.is_blocking),
        is_renote_muted: relation_flag(is_self, raw.is_renote_muted),
        profile_url: sanitize_profile_url(raw.user.host.as_deref(), raw.url.as_deref(), raw.uri.as_deref()),
        user: raw.user.into(),
        is_self,
    })
```

(`raw.user.into()` が `raw.user` をムーブするので、`raw.user.host` を読む `profile_url` を、`user: raw.user.into()` より前に書く。構造体リテラルのフィールドは書いた順に評価される。)

- [ ] **Step 6: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib raw_user_show profile_url relation_flag`
Expected: PASS

- [ ] **Step 7: TS バインディングを再生成して確認する**

Run: `cd src-tauri && cargo test --lib generates_frontend_bindings && grep -n 'isBlocking\|isRenoteMuted\|profileUrl' ../frontend/src/bindings/tauri.gen.ts`
Expected: PASS。`UserProfile` に `isBlocking: boolean | null` / `isRenoteMuted: boolean | null` / `profileUrl: string | null` が出る。

- [ ] **Step 8: フロントの型チェックが通ることを確認する**

Run: `cd frontend && pnpm check`
Expected: エラー 0(`ProfileModal.test.ts` の `profileResponse` は `Record<string, unknown>` を使うので影響しない)。型エラーが出たら、`UserProfile` を直接構築している箇所に新フィールドを足す。

- [ ] **Step 9: コミットする**

```bash
git add src-tauri/src/api/users.rs src-tauri/src/commands/user.rs frontend/src/bindings/tauri.gen.ts
git commit -m "$(cat <<'EOF'
feat(profile): プロフィールにブロック・リノートミュート・元ページURLを載せる (#546)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: ブロックとリノートミュートの API・コマンド

**Files:**
- Modify: `src-tauri/src/api/mutes.rs`(ラッパ4つ + テスト)
- Modify: `src-tauri/src/commands/mute.rs`(コマンド4つ + core 2つ + テスト)
- Modify: `src-tauri/src/lib.rs`(登録)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: `resync_after_mute_change(state: &AppState, account_id: &str, client: &MisskeyClient) -> Option<SyncMuteResult>`(既存、`commands/mute.rs`)。
- Produces(Rust API): `block_user` / `unblock_user` / `renote_mute_user` / `unrenote_mute_user`。いずれも `async fn (client: &MisskeyClient, user_id: &str) -> Result<()>`。
- Produces(コマンド): `block_user(account_id, user_id) -> Result<Option<SyncMuteResult>>`、`unblock_user(同)`、`renote_mute_user(account_id, user_id) -> Result<()>`、`unrenote_mute_user(同)`。TS では `commands.blockUser` / `unblockUser` / `renoteMuteUser` / `unrenoteMuteUser`。

- [ ] **Step 1: API ラッパの失敗するテストを書く**

`src-tauri/src/api/mutes.rs` の `unmute_user_posts_the_user_id_to_mute_delete` の後に追加する。

```rust
    #[tokio::test]
    async fn block_user_posts_the_user_id_to_blocking_create() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/blocking/create"))
            .and(body_partial_json(json!({ "userId": "u1" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "u1" })))
            .expect(1)
            .mount(&mock)
            .await;

        block_user(&test_client(&mock), "u1").await.unwrap();
    }

    #[tokio::test]
    async fn unblock_user_posts_the_user_id_to_blocking_delete() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/blocking/delete"))
            .and(body_partial_json(json!({ "userId": "u1" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "u1" })))
            .expect(1)
            .mount(&mock)
            .await;

        unblock_user(&test_client(&mock), "u1").await.unwrap();
    }

    #[tokio::test]
    async fn renote_mute_user_posts_the_user_id_to_renote_mute_create() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/renote-mute/create"))
            .and(body_partial_json(json!({ "userId": "u1" })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock)
            .await;

        renote_mute_user(&test_client(&mock), "u1").await.unwrap();
    }

    #[tokio::test]
    async fn unrenote_mute_user_posts_the_user_id_to_renote_mute_delete() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/renote-mute/delete"))
            .and(body_partial_json(json!({ "userId": "u1" })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock)
            .await;

        unrenote_mute_user(&test_client(&mock), "u1").await.unwrap();
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib block_user renote_mute_user`
Expected: コンパイルエラー(関数が無い)

- [ ] **Step 3: API ラッパを実装する**

`src-tauri/src/api/mutes.rs` の `unmute_user` の後に追加する。

```rust
/// ユーザーをブロックする。`blocking/create`。サーバーは双方向のフォローも解除する。
/// すでにブロック中なら `ALREADY_BLOCKING` で失敗する。
pub async fn block_user(client: &MisskeyClient, user_id: &str) -> Result<()> {
    let _: Value = client.post("blocking/create", &json!({ "userId": user_id })).await?;
    Ok(())
}

/// ユーザーのブロックを解除する。`blocking/delete`。ブロック中でなければ `NOT_BLOCKING` で失敗する。
pub async fn unblock_user(client: &MisskeyClient, user_id: &str) -> Result<()> {
    let _: Value = client.post("blocking/delete", &json!({ "userId": user_id })).await?;
    Ok(())
}

/// ユーザーのリノートをミュートする。`renote-mute/create`。すでに対象なら `ALREADY_MUTING` で失敗する。
pub async fn renote_mute_user(client: &MisskeyClient, user_id: &str) -> Result<()> {
    let _: Value = client.post("renote-mute/create", &json!({ "userId": user_id })).await?;
    Ok(())
}

/// ユーザーのリノートミュートを解除する。`renote-mute/delete`。対象でなければ `NOT_MUTING` で失敗する。
pub async fn unrenote_mute_user(client: &MisskeyClient, user_id: &str) -> Result<()> {
    let _: Value = client.post("renote-mute/delete", &json!({ "userId": user_id })).await?;
    Ok(())
}
```

- [ ] **Step 4: API ラッパのテストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib block_user renote_mute_user`
Expected: PASS

- [ ] **Step 5: コマンド core の失敗するテストを書く**

`src-tauri/src/commands/mute.rs` の `unmute_user_core_deletes_the_mute_and_resyncs` の後(同じ `mod tests` 内)に追加する。

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn block_user_core_creates_the_block_and_resyncs_the_hidden_set() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        // ブロック後の blocking/list は u1 を含む(mute/list は空、mutedWords も空)
        Mock::given(method("POST"))
            .and(path("/mute/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/blocking/list"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([{ "id": "b1", "blockeeId": "u1" }])),
            )
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/i"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "mutedWords": [] })))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/blocking/create"))
            .and(wiremock::matchers::body_partial_json(serde_json::json!({ "userId": "u1" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .expect(1)
            .mount(&mock)
            .await;

        let result = block_user_core(&state, "acc1", &client, "u1").await.unwrap();

        assert!(result.is_some(), "同期が成功したので、結果を返す");
        assert!(state.is_server_muted("acc1", "u1"), "ブロックしたユーザーは表示を抑制する集合に入る");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn block_user_core_fails_without_syncing_when_the_create_fails() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/blocking/create"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": { "code": "ALREADY_BLOCKING", "message": "You are already blocking that user.", "id": "x" }
            })))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/mute/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .expect(0)
            .mount(&mock)
            .await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());

        let result = block_user_core(&state, "acc1", &client, "u1").await;

        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn block_user_core_succeeds_even_when_the_follow_up_sync_fails() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/blocking/create"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/mute/list"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock)
            .await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());

        let result = block_user_core(&state, "acc1", &client, "u1").await;

        assert!(matches!(result, Ok(None)), "ブロックは成功。同期の失敗は None で伝える: {result:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unblock_user_core_deletes_the_block_and_resyncs() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        mock.reset().await;
        // 解除前: blocking/list に u1
        Mock::given(method("POST"))
            .and(path("/mute/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/i"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "mutedWords": [] })))
            .mount(&mock)
            .await;
        let before = Mock::given(method("POST"))
            .and(path("/blocking/list"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([{ "id": "b1", "blockeeId": "u1" }])),
            )
            .mount_as_scoped(&mock)
            .await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();
        assert!(state.is_server_muted("acc1", "u1"));
        drop(before);

        // 解除後は空
        Mock::given(method("POST"))
            .and(path("/blocking/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/blocking/delete"))
            .and(wiremock::matchers::body_partial_json(serde_json::json!({ "userId": "u1" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .expect(1)
            .mount(&mock)
            .await;

        let result = unblock_user_core(&state, "acc1", &client, "u1").await.unwrap();

        assert!(result.is_some());
        assert!(!state.is_server_muted("acc1", "u1"));
    }
```

(`mount_server_mutes` は `blocking/list` を常に空で返すため、ブロックのテストでは使わず、`mute/list` / `blocking/list` / `/i` を個別に mount する。`unblock_user_core_...` は、解除前の `blocking/list` を `mount_as_scoped` で mount し、同期後に drop して外してから、解除後の空の応答を mount し直す。)

- [ ] **Step 6: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib block_user_core`
Expected: コンパイルエラー(`block_user_core` が無い)

- [ ] **Step 7: コマンドと core を実装する**

`src-tauri/src/commands/mute.rs` の `unmute_user_core` の直後に追加する。

```rust
/// ユーザーをブロックする(Issue #546)。戻り値は `mute_user` と同じ(再同期の結果。失敗時は `None`)。
/// ブロックは `blocking/list` として「表示を抑制する集合」に入るため、ミュートと同じく再同期する。
#[tauri::command]
#[specta::specta]
pub async fn block_user(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
) -> Result<Option<SyncMuteResult>> {
    let client = state.client_for(&account_id)?;
    block_user_core(&state, &account_id, &client, &user_id).await
}

/// ユーザーのブロックを解除する。戻り値は `block_user` と同じ。
#[tauri::command]
#[specta::specta]
pub async fn unblock_user(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
) -> Result<Option<SyncMuteResult>> {
    let client = state.client_for(&account_id)?;
    unblock_user_core(&state, &account_id, &client, &user_id).await
}

/// ユーザーのリノートをミュートする(Issue #546)。サーバー側でタイムラインから除外されるので、
/// クライアントの集合には入れず、再同期もしない。
#[tauri::command]
#[specta::specta]
pub async fn renote_mute_user(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    mutes_api::renote_mute_user(&client, &user_id).await
}

/// ユーザーのリノートミュートを解除する。
#[tauri::command]
#[specta::specta]
pub async fn unrenote_mute_user(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    mutes_api::unrenote_mute_user(&client, &user_id).await
}

async fn block_user_core(
    state: &AppState,
    account_id: &str,
    client: &crate::api::MisskeyClient,
    user_id: &str,
) -> Result<Option<SyncMuteResult>> {
    mutes_api::block_user(client, user_id).await?;
    Ok(resync_after_mute_change(state, account_id, client).await)
}

async fn unblock_user_core(
    state: &AppState,
    account_id: &str,
    client: &crate::api::MisskeyClient,
    user_id: &str,
) -> Result<Option<SyncMuteResult>> {
    mutes_api::unblock_user(client, user_id).await?;
    Ok(resync_after_mute_change(state, account_id, client).await)
}
```

- [ ] **Step 8: コマンドを登録する**

`src-tauri/src/lib.rs` の `commands::mute::unmute_user,` の次の行に追加する。

```rust
            commands::mute::block_user,
            commands::mute::unblock_user,
            commands::mute::renote_mute_user,
            commands::mute::unrenote_mute_user,
```

- [ ] **Step 9: テストが通ることを確認し、バインディングを再生成する**

Run: `cd src-tauri && cargo test --lib block_user renote_mute_user generates_frontend_bindings && grep -n 'blockUser\|unblockUser\|renoteMuteUser\|unrenoteMuteUser' ../frontend/src/bindings/tauri.gen.ts`
Expected: PASS。4つのコマンドが `tauri.gen.ts` に出る。

- [ ] **Step 10: コミットする**

```bash
git add src-tauri/src/api/mutes.rs src-tauri/src/commands/mute.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "$(cat <<'EOF'
feat(mute): ブロックとリノートミュートのコマンドを追加 (#546)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: 通報とリストのコマンド

**Files:**
- Modify: `src-tauri/src/domain/list.rs`(`UserListMembership` を追記)
- Modify: `src-tauri/src/domain/mod.rs`(`pub use list::{SourceItem, UserList, UserListMembership};`)
- Modify: `src-tauri/src/api/users.rs`(`report_abuse`、`add_to_list`、`remove_from_list`)
- Modify: `src-tauri/src/api/meta.rs`(`fetch_user_lists_with_membership`)
- Modify: `src-tauri/src/commands/user.rs`(コマンド4つ、`validate_report_comment`)
- Modify: `src-tauri/src/lib.rs`(登録)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Produces(型): `UserListMembership { id: String, name: String, has_user: bool }`(TS: `{ id, name, hasUser }`)。
- Produces(API): `report_abuse(client, user_id: &str, comment: &str) -> Result<()>`、`add_to_list(client, list_id: &str, user_id: &str) -> Result<()>`、`remove_from_list(同)`、`fetch_user_lists_with_membership(client, user_id: &str) -> Result<Vec<UserListMembership>>`。
- Produces(コマンド): `report_user(account_id, user_id, comment) -> Result<()>`、`list_user_lists_with_membership(account_id, user_id) -> Result<Vec<UserListMembership>>`、`add_user_to_list(account_id, list_id, user_id) -> Result<()>`、`remove_user_from_list(同)`。TS では `commands.reportUser` / `listUserListsWithMembership` / `addUserToList` / `removeUserFromList`。

- [ ] **Step 1: 失敗するテストを書く(API)**

`src-tauri/src/api/users.rs` の `mod tests` の末尾に追加する(ファイル内に wiremock の `use` が無ければ、`mod tests` の先頭の `use` 群に `use wiremock::matchers::{body_partial_json, method, path}; use wiremock::{Mock, MockServer, ResponseTemplate};` を足す)。

```rust
    fn test_client(mock: &wiremock::MockServer) -> crate::api::MisskeyClient {
        crate::api::MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None)
    }

    #[tokio::test]
    async fn report_abuse_posts_the_user_id_and_comment() {
        use wiremock::matchers::{body_partial_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/users/report-abuse"))
            .and(body_partial_json(serde_json::json!({ "userId": "u1", "comment": "spam" })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock)
            .await;

        report_abuse(&test_client(&mock), "u1", "spam").await.unwrap();
    }

    #[tokio::test]
    async fn add_to_list_posts_to_lists_push_and_remove_posts_to_lists_pull() {
        use wiremock::matchers::{body_partial_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let mock = MockServer::start().await;
        for endpoint in ["/users/lists/push", "/users/lists/pull"] {
            Mock::given(method("POST"))
                .and(path(endpoint))
                .and(body_partial_json(serde_json::json!({ "listId": "L1", "userId": "u1" })))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&mock)
                .await;
        }

        add_to_list(&test_client(&mock), "L1", "u1").await.unwrap();
        remove_from_list(&test_client(&mock), "L1", "u1").await.unwrap();
    }
```

`src-tauri/src/api/meta.rs` の既存の `mod tests`(無ければ末尾に `#[cfg(test)] mod tests` を新設)に追加する。

```rust
    #[tokio::test]
    async fn fetch_user_lists_with_membership_marks_the_lists_that_contain_the_user() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/users/lists/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "id": "L1", "name": "友達", "userIds": ["u1", "u2"], "isPublic": false },
                { "id": "L2", "name": "仕事", "userIds": ["u3"], "isPublic": false },
                { "id": "L3", "name": "空", "isPublic": false }
            ])))
            .mount(&mock)
            .await;
        let client = crate::api::MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);

        let lists = fetch_user_lists_with_membership(&client, "u1").await.unwrap();

        assert_eq!(
            lists
                .iter()
                .map(|l| (l.id.as_str(), l.name.as_str(), l.has_user))
                .collect::<Vec<_>>(),
            vec![("L1", "友達", true), ("L2", "仕事", false), ("L3", "空", false)]
        );
    }
```

(`mod tests` を新設する場合、`use super::*;` を先頭に置く。)

- [ ] **Step 2: コマンド側の純粋関数のテストを書く**

`src-tauri/src/commands/user.rs` の `mod tests` の末尾に追加する。

```rust
    #[test]
    fn report_comment_is_trimmed_and_must_not_be_blank() {
        assert_eq!(validate_report_comment("  スパムです \n").unwrap(), "スパムです");
        assert!(validate_report_comment("").is_err());
        assert!(validate_report_comment("  \n\t ").is_err());
    }

    #[test]
    fn report_comment_longer_than_2048_chars_is_rejected() {
        assert!(validate_report_comment(&"あ".repeat(2048)).is_ok());
        assert!(validate_report_comment(&"あ".repeat(2049)).is_err());
    }
```

- [ ] **Step 3: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib report_abuse add_to_list fetch_user_lists_with_membership report_comment`
Expected: コンパイルエラー(関数・型が無い)

- [ ] **Step 4: 型を実装する**

`src-tauri/src/domain/list.rs` の `UserList` の直後に追加する。

```rust
/// 「リストに追加」メニュー用。自分のリスト1件と、対象ユーザーがそのリストに入っているか。
/// `has_user` は Rust 側で `users/lists/list` の `userIds` から計算し、`userIds` 自体は IPC に載せない。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserListMembership {
    pub id: String,
    pub name: String,
    pub has_user: bool,
}
```

`src-tauri/src/domain/mod.rs` の `pub use list::{SourceItem, UserList};` を `pub use list::{SourceItem, UserList, UserListMembership};` にする。

- [ ] **Step 5: API ラッパを実装する**

`src-tauri/src/api/users.rs` の `unfollow` の後に追加する。

```rust
/// ユーザーを通報する。`users/report-abuse`(コメントは1〜2048文字。検証は呼び出し側)。
pub async fn report_abuse(client: &MisskeyClient, user_id: &str, comment: &str) -> Result<()> {
    let _: serde_json::Value = client
        .post("users/report-abuse", &json!({ "userId": user_id, "comment": comment }))
        .await?;
    Ok(())
}

/// 自分のリストにユーザーを追加する。`users/lists/push`。すでに入っていれば `ALREADY_ADDED` で失敗する。
pub async fn add_to_list(client: &MisskeyClient, list_id: &str, user_id: &str) -> Result<()> {
    let _: serde_json::Value = client
        .post("users/lists/push", &json!({ "listId": list_id, "userId": user_id }))
        .await?;
    Ok(())
}

/// 自分のリストからユーザーを外す。`users/lists/pull`。
pub async fn remove_from_list(client: &MisskeyClient, list_id: &str, user_id: &str) -> Result<()> {
    let _: serde_json::Value = client
        .post("users/lists/pull", &json!({ "listId": list_id, "userId": user_id }))
        .await?;
    Ok(())
}
```

`src-tauri/src/api/meta.rs` の `fetch_user_lists` の後に追加し、先頭の `use crate::domain::{EmojiDef, SourceItem, User, UserList};` に `UserListMembership` を足す。

```rust
/// 自分のユーザリスト一覧と、`user_id` が各リストに入っているか(「リストに追加」メニュー用)。
/// `userId` 引数は使わない(ローカルユーザー専用で、自分のリストではなく相手のリストを返すため)。
/// 応答の `userIds` は全メンバーを含む(上流の `UserListEntityService.pack`)。
pub async fn fetch_user_lists_with_membership(
    client: &MisskeyClient,
    user_id: &str,
) -> Result<Vec<UserListMembership>> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RawList {
        id: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        user_ids: Vec<String>,
    }
    let raw: Vec<RawList> = client.post("users/lists/list", &json!({})).await?;
    Ok(raw
        .into_iter()
        .map(|l| UserListMembership {
            has_user: l.user_ids.iter().any(|id| id == user_id),
            id: l.id,
            name: l.name,
        })
        .collect())
}
```

- [ ] **Step 6: コマンドを実装する**

`src-tauri/src/commands/user.rs` の先頭の `use crate::domain::{Account, ColumnKind, Note, User};` に `UserListMembership` を足す。`get_user_following` の後(`#[cfg(test)]` の前)に追加する。

```rust
/// 通報コメントの上限(文字数)。上流 `users/report-abuse` の `maxLength`。
const REPORT_COMMENT_MAX_CHARS: usize = 2048;

/// 通報コメントを検証して前後の空白を削る(純粋関数、テスト用に分離)。空(空白のみを含む)と
/// 2048文字超はサーバーに送らず `Invalid` にする。
pub(crate) fn validate_report_comment(comment: &str) -> Result<String> {
    let trimmed = comment.trim();
    if trimmed.is_empty() {
        return Err(Error::Invalid("通報の内容を入力してください".into()));
    }
    if trimmed.chars().count() > REPORT_COMMENT_MAX_CHARS {
        return Err(Error::Invalid(format!("通報の内容は{REPORT_COMMENT_MAX_CHARS}文字以内にしてください")));
    }
    Ok(trimmed.to_string())
}

/// ユーザーを通報する(Issue #546)。
#[tauri::command]
#[specta::specta]
pub async fn report_user(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
    comment: String,
) -> Result<()> {
    let comment = validate_report_comment(&comment)?;
    let client = state.client_for(&account_id)?;
    api::users::report_abuse(&client, &user_id, &comment).await
}

/// 自分のリスト一覧と、`user_id` が各リストに入っているか。
#[tauri::command]
#[specta::specta]
pub async fn list_user_lists_with_membership(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
) -> Result<Vec<UserListMembership>> {
    let client = state.client_for(&account_id)?;
    api::meta::fetch_user_lists_with_membership(&client, &user_id).await
}

/// リストにユーザーを追加する。
#[tauri::command]
#[specta::specta]
pub async fn add_user_to_list(
    state: State<'_, AppState>,
    account_id: String,
    list_id: String,
    user_id: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    api::users::add_to_list(&client, &list_id, &user_id).await
}

/// リストからユーザーを外す。
#[tauri::command]
#[specta::specta]
pub async fn remove_user_from_list(
    state: State<'_, AppState>,
    account_id: String,
    list_id: String,
    user_id: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    api::users::remove_from_list(&client, &list_id, &user_id).await
}
```

同ファイル先頭の `use crate::error::Result;` を `use crate::error::{Error, Result};` にする。

- [ ] **Step 7: コマンドを登録する**

`src-tauri/src/lib.rs` の `commands::user::get_user_following,` の次の行に追加する。

```rust
            commands::user::report_user,
            commands::user::list_user_lists_with_membership,
            commands::user::add_user_to_list,
            commands::user::remove_user_from_list,
```

- [ ] **Step 8: テストが通ることを確認し、バインディングを再生成する**

Run: `cd src-tauri && cargo test --lib report_abuse add_to_list fetch_user_lists_with_membership report_comment generates_frontend_bindings && grep -n 'reportUser\|listUserListsWithMembership\|addUserToList\|removeUserFromList\|UserListMembership' ../frontend/src/bindings/tauri.gen.ts`
Expected: PASS。4つのコマンドと `UserListMembership` が `tauri.gen.ts` に出る。

- [ ] **Step 9: Rust 全体のテストが通ることを確認する**

Run: `cd src-tauri && cargo test`
Expected: PASS(`#[ignore]` の実接続テストは実行されない)。

- [ ] **Step 10: コミットする**

```bash
git add src-tauri/src/domain/list.rs src-tauri/src/domain/mod.rs src-tauri/src/api/users.rs src-tauri/src/api/meta.rs src-tauri/src/commands/user.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "$(cat <<'EOF'
feat(profile): 通報とリスト追加のコマンドを追加 (#546)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: ストアのメソッド

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`(`unmuteUser` の直後)
- Test: `frontend/src/lib/store.svelte.test.ts`(`時限ミュートの期限切れ再同期(Issue #545)` の `describe` の直後に新しい `describe` を追加)

**Interfaces:**
- Consumes: `commands.blockUser` / `unblockUser` / `renoteMuteUser` / `unrenoteMuteUser` / `reportUser` / `listUserListsWithMembership` / `addUserToList` / `removeUserFromList`(Task 3・4)、`#beginMuteSync` / `#applyMuteSync`(既存)。
- Produces: `app.blockUser(accountId, userId)`、`app.unblockUser(accountId, userId)`、`app.renoteMuteUser(accountId, userId)`、`app.unrenoteMuteUser(accountId, userId)`、`app.reportUser(accountId, userId, comment)`、`app.listUserListsWithMembership(accountId, userId): Promise<UserListMembership[]>`、`app.addUserToList(accountId, listId, userId)`、`app.removeUserFromList(accountId, listId, userId)`。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/store.svelte.test.ts` の、`時限ミュートの期限切れ再同期(Issue #545)` の `describe` を閉じる `});` の直後(同じ `サーバー側ミュート同期` の `describe` の中)に追加する。`callsOf`・`account`・`muteCommandResult`・`syncedAccounts` は、この `describe` のスコープで使える既存のヘルパ。

```ts
  describe("ブロック・リノートミュート・通報・リスト(Issue #546)", () => {
    const callsOf = (cmd: string) => invokeMock.mock.calls.filter(([c]) => c === cmd);

    it("blockUserはblock_userを呼び、再同期の結果の期限でタイマーを張る", async () => {
      vi.useFakeTimers();
      app.accounts = [account("acc1")];
      muteCommandResult = { blockedUsers: 1, wordRules: 0, nextExpiryAt: Date.now() + 60_000 };
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "list_accounts") return [];
        if (cmd === "block_user") return muteCommandResult;
        if (cmd === "sync_server_mutes") return syncResult;
        return null;
      });

      await app.blockUser("acc1", "u1");
      expect(callsOf("block_user")[0][1]).toEqual({ accountId: "acc1", userId: "u1" });
      invokeMock.mockClear();

      await vi.advanceTimersByTimeAsync(63_000);
      expect(syncedAccounts()).toEqual(["acc1"]);
    });

    it("unblockUserはunblock_userを呼ぶ", async () => {
      app.accounts = [account("acc1")];
      await app.unblockUser("acc1", "u1");
      expect(callsOf("unblock_user")[0][1]).toEqual({ accountId: "acc1", userId: "u1" });
    });

    it("renoteMuteUser / unrenoteMuteUserは、対応するコマンドを呼ぶ", async () => {
      app.accounts = [account("acc1")];
      await app.renoteMuteUser("acc1", "u1");
      await app.unrenoteMuteUser("acc1", "u1");
      expect(callsOf("renote_mute_user")[0][1]).toEqual({ accountId: "acc1", userId: "u1" });
      expect(callsOf("unrenote_mute_user")[0][1]).toEqual({ accountId: "acc1", userId: "u1" });
    });

    it("blockUserが失敗したら、エラーを投げ、ログに残す", async () => {
      app.accounts = [account("acc1")];
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "block_user") throw new Error("blocking/create: ALREADY_BLOCKING");
        return null;
      });
      await expect(app.blockUser("acc1", "u1")).rejects.toThrow(/ALREADY_BLOCKING/);
      expect(app.logs.some((l) => l.text.includes("ALREADY_BLOCKING"))).toBe(true);
    });

    it("reportUserはreport_userにコメントを渡す", async () => {
      app.accounts = [account("acc1")];
      await app.reportUser("acc1", "u1", "スパムです");
      expect(callsOf("report_user")[0][1]).toEqual({ accountId: "acc1", userId: "u1", comment: "スパムです" });
    });

    it("listUserListsWithMembershipは、リストの一覧を返す", async () => {
      app.accounts = [account("acc1")];
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "list_user_lists_with_membership") return [{ id: "L1", name: "友達", hasUser: true }];
        return null;
      });
      await expect(app.listUserListsWithMembership("acc1", "u1")).resolves.toEqual([
        { id: "L1", name: "友達", hasUser: true },
      ]);
      expect(callsOf("list_user_lists_with_membership")[0][1]).toEqual({ accountId: "acc1", userId: "u1" });
    });

    it("addUserToList / removeUserFromListは、リストIDとユーザーIDを渡す", async () => {
      app.accounts = [account("acc1")];
      await app.addUserToList("acc1", "L1", "u1");
      await app.removeUserFromList("acc1", "L1", "u1");
      expect(callsOf("add_user_to_list")[0][1]).toEqual({ accountId: "acc1", listId: "L1", userId: "u1" });
      expect(callsOf("remove_user_from_list")[0][1]).toEqual({ accountId: "acc1", listId: "L1", userId: "u1" });
    });
  });
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/lib/store.svelte.test.ts -t "Issue #546"`
Expected: FAIL(`app.blockUser is not a function`)

- [ ] **Step 3: 実装する**

`frontend/src/lib/store.svelte.ts` の `unmuteUser` の直後(`getUserNotes` の前)に追加する。ファイル先頭の `tauri.gen` からの型 import に `UserListMembership` を足す(既に `MutePeriod` などを import している行に追記)。

```ts
  /// ユーザーをブロックする(Issue #546)。ブロックも「表示を抑制する集合」に入るので、`muteUser` と同様に
  /// 再取得の結果でタイマーを張り直す。
  async blockUser(accountId: string, userId: string) {
    const seq = this.#beginMuteSync(accountId);
    try {
      const result = await unwrapAcc(accountId, commands.blockUser(accountId, userId));
      if (result) this.#applyMuteSync(accountId, seq, result.nextExpiryAt);
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }

  /// ユーザーのブロックを解除する。`blockUser` と同様。
  async unblockUser(accountId: string, userId: string) {
    const seq = this.#beginMuteSync(accountId);
    try {
      const result = await unwrapAcc(accountId, commands.unblockUser(accountId, userId));
      if (result) this.#applyMuteSync(accountId, seq, result.nextExpiryAt);
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }

  /// ユーザーのリノートをミュートする(Issue #546)。サーバー側で除外されるので、再同期はしない。
  async renoteMuteUser(accountId: string, userId: string) {
    try {
      await unwrapAcc(accountId, commands.renoteMuteUser(accountId, userId));
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }

  /// ユーザーのリノートミュートを解除する。
  async unrenoteMuteUser(accountId: string, userId: string) {
    try {
      await unwrapAcc(accountId, commands.unrenoteMuteUser(accountId, userId));
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }

  /// ユーザーを通報する。コメントは必須(Rust 側でも検証する)。
  async reportUser(accountId: string, userId: string, comment: string) {
    try {
      await unwrapAcc(accountId, commands.reportUser(accountId, userId, comment));
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }

  /// 自分のリスト一覧と、`userId` が各リストに入っているか。
  async listUserListsWithMembership(accountId: string, userId: string): Promise<UserListMembership[]> {
    try {
      return await unwrapAcc(accountId, commands.listUserListsWithMembership(accountId, userId));
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }

  /// リストにユーザーを追加する。
  async addUserToList(accountId: string, listId: string, userId: string) {
    try {
      await unwrapAcc(accountId, commands.addUserToList(accountId, listId, userId));
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }

  /// リストからユーザーを外す。
  async removeUserFromList(accountId: string, listId: string, userId: string) {
    try {
      await unwrapAcc(accountId, commands.removeUserFromList(accountId, listId, userId));
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/lib/store.svelte.test.ts && pnpm check`
Expected: PASS、型エラー 0。

- [ ] **Step 5: コミットする**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "$(cat <<'EOF'
feat(store): ブロック・リノートミュート・通報・リストのメソッドを追加 (#546)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: `UserListsPanel.svelte` と `ReportUserDialog.svelte`

**Files:**
- Create: `frontend/src/ui/UserListsPanel.svelte`
- Create: `frontend/src/ui/UserListsPanel.test.ts`
- Create: `frontend/src/ui/ReportUserDialog.svelte`
- Create: `frontend/src/ui/ReportUserDialog.test.ts`

**Interfaces:**
- Consumes: `UserListMembership`(`../bindings/tauri.gen`)。
- Produces(`UserListsPanel`): props `{ load: () => Promise<UserListMembership[]>; toggle: (listId: string, add: boolean) => Promise<void>; onback: () => void }`。各リスト行は `role="checkbox"` + `aria-checked`、名前はリスト名。
- Produces(`ReportUserDialog`): props `{ acct: string; onsubmit: (comment: string) => Promise<void>; oncancel: () => void; z?: number }`。`onsubmit` が成功(resolve)したら呼び出し側がダイアログを閉じる。失敗(reject)したらダイアログ内にエラーを出し、開いたままにする。

- [ ] **Step 1: `UserListsPanel` の失敗するテストを書く**

`frontend/src/ui/UserListsPanel.test.ts` を作成する。

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import UserListsPanel from "./UserListsPanel.svelte";

afterEach(() => cleanup());

const LISTS = [
  { id: "L1", name: "友達", hasUser: true },
  { id: "L2", name: "仕事", hasUser: false },
];

function mount(overrides: Partial<{ load: () => Promise<typeof LISTS>; toggle: (id: string, add: boolean) => Promise<void> }> = {}) {
  const load = overrides.load ?? vi.fn().mockResolvedValue(LISTS);
  const toggle = overrides.toggle ?? vi.fn().mockResolvedValue(undefined);
  const onback = vi.fn();
  const r = render(UserListsPanel, { props: { load, toggle, onback } });
  return { ...r, load, toggle, onback };
}

describe("UserListsPanel", () => {
  it("開くとloadを呼び、取得中は「読み込み中…」を出す", async () => {
    let resolve!: (v: typeof LISTS) => void;
    const load = vi.fn(() => new Promise<typeof LISTS>((r) => (resolve = r)));
    const { getByText, findByRole } = mount({ load });
    expect(getByText("読み込み中…")).toBeTruthy();
    resolve(LISTS);
    expect(await findByRole("checkbox", { name: "友達" })).toBeTruthy();
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("入っているリストにはチェックが付き、入っていないリストには付かない", async () => {
    const { findByRole } = mount();
    expect((await findByRole("checkbox", { name: "友達" })).getAttribute("aria-checked")).toBe("true");
    expect((await findByRole("checkbox", { name: "仕事" })).getAttribute("aria-checked")).toBe("false");
  });

  it("入っていないリストを押すとtoggle(id, true)を呼び、完了後にチェックが付く", async () => {
    const { findByRole, toggle } = mount();
    const row = await findByRole("checkbox", { name: "仕事" });

    await fireEvent.click(row);

    expect(toggle).toHaveBeenCalledWith("L2", true);
    await waitFor(() => expect(row.getAttribute("aria-checked")).toBe("true"));
  });

  it("入っているリストを押すとtoggle(id, false)を呼び、完了後にチェックが外れる", async () => {
    const { findByRole, toggle } = mount();
    const row = await findByRole("checkbox", { name: "友達" });

    await fireEvent.click(row);

    expect(toggle).toHaveBeenCalledWith("L1", false);
    await waitFor(() => expect(row.getAttribute("aria-checked")).toBe("false"));
  });

  it("toggleが失敗したら、チェックは変えず、エラーを出し、パネルは開いたまま", async () => {
    const toggle = vi.fn().mockRejectedValue(new Error("users/lists/push: ALREADY_ADDED"));
    const { findByRole, getByText } = mount({ toggle });
    const row = await findByRole("checkbox", { name: "仕事" });

    await fireEvent.click(row);

    await waitFor(() => expect(getByText(/ALREADY_ADDED/)).toBeTruthy());
    expect(row.getAttribute("aria-checked")).toBe("false");
  });

  it("トグルの完了を待つ間は、同じ行を二重に押してもtoggleは1回だけ", async () => {
    let resolve!: () => void;
    const toggle = vi.fn(() => new Promise<void>((r) => (resolve = r)));
    const { findByRole } = mount({ toggle });
    const row = await findByRole("checkbox", { name: "仕事" });

    await fireEvent.click(row);
    await fireEvent.click(row);

    expect(toggle).toHaveBeenCalledTimes(1);
    resolve();
  });

  it("loadが失敗したらエラーと再試行を出し、再試行でもう一度loadを呼ぶ", async () => {
    const load = vi.fn().mockRejectedValueOnce(new Error("boom")).mockResolvedValueOnce(LISTS);
    const { findByText, getByRole, findByRole } = mount({ load });
    expect(await findByText(/boom/)).toBeTruthy();

    await fireEvent.click(getByRole("button", { name: "再試行" }));

    expect(await findByRole("checkbox", { name: "友達" })).toBeTruthy();
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("リストが0件なら「リストがありません」を出す", async () => {
    const { findByText } = mount({ load: vi.fn().mockResolvedValue([]) });
    expect(await findByText("リストがありません")).toBeTruthy();
  });

  it("「戻る」でonbackを呼ぶ", async () => {
    const { getByRole, onback } = mount();
    await fireEvent.click(getByRole("button", { name: "戻る" }));
    expect(onback).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/ui/UserListsPanel.test.ts`
Expected: FAIL(`UserListsPanel.svelte` が無い)

- [ ] **Step 3: `UserListsPanel.svelte` を実装する**

`frontend/src/ui/UserListsPanel.svelte` を作成する。

```svelte
<script lang="ts">
  // UserMenu の「リストに追加」サブメニュー(Issue #546)。開いた時に自分のリストを取得し、対象ユーザーが
  // 入っているものにチェックを付ける。押すと追加/解除をトグルする。トグルは楽観更新せず、完了後に反映する。
  import { untrack } from "svelte";
  import { Check, ChevronLeft } from "@lucide/svelte";
  import type { UserListMembership } from "../bindings/tauri.gen";
  import { Button } from "$lib/components/ui/button";

  let {
    load,
    toggle,
    onback,
  }: {
    load: () => Promise<UserListMembership[]>;
    toggle: (listId: string, add: boolean) => Promise<void>;
    onback: () => void;
  } = $props();

  type PanelState =
    | { status: "loading" }
    | { status: "error"; message: string }
    | { status: "ready"; lists: UserListMembership[] };

  let panel = $state<PanelState>({ status: "loading" });
  let busyIds = $state<string[]>([]);
  let toggleErr = $state<string | null>(null);

  async function reload() {
    panel = { status: "loading" };
    try {
      panel = { status: "ready", lists: await load() };
    } catch (e) {
      panel = { status: "error", message: String(e) };
    }
  }

  // 開いた時に1回だけ取得する(再試行は reload を直接呼ぶ)。`load` の中身(ProfileModal では
  // profileState を読む)を依存として追跡すると、ミュートなどでプロフィールが変わるたびに再取得して
  // 「読み込み中…」に戻ってしまうので、untrack で依存を切る。
  $effect(() => {
    untrack(() => {
      void reload();
    });
  });

  async function onToggle(list: UserListMembership) {
    if (busyIds.includes(list.id)) return;
    busyIds = [...busyIds, list.id];
    toggleErr = null;
    const add = !list.hasUser;
    try {
      await toggle(list.id, add);
      if (panel.status === "ready") {
        panel = {
          status: "ready",
          lists: panel.lists.map((l) => (l.id === list.id ? { ...l, hasUser: add } : l)),
        };
      }
    } catch (e) {
      toggleErr = String(e);
    } finally {
      busyIds = busyIds.filter((id) => id !== list.id);
    }
  }

  const ITEM =
    "box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted disabled:opacity-60";
</script>

<button type="button" class={ITEM} onclick={onback}>
  <ChevronLeft size={16} />
  戻る
</button>
<span class="block px-2 py-1 text-xs text-muted-foreground">リストに追加</span>
{#if panel.status === "loading"}
  <p class="m-0 px-2 py-1.5 text-sm text-muted-foreground">読み込み中…</p>
{:else if panel.status === "error"}
  <p class="m-0 px-2 py-1.5 text-sm text-destructive">{panel.message}</p>
  <div class="px-2 pb-1">
    <Button variant="outline" size="sm" onclick={reload}>再試行</Button>
  </div>
{:else if panel.lists.length === 0}
  <p class="m-0 px-2 py-1.5 text-sm text-muted-foreground">リストがありません</p>
{:else}
  <div class="max-h-[240px] overflow-y-auto">
    {#each panel.lists as list (list.id)}
      <button
        type="button"
        role="checkbox"
        aria-checked={list.hasUser}
        class={ITEM}
        disabled={busyIds.includes(list.id)}
        onclick={() => onToggle(list)}
      >
        <span class="flex h-4 w-4 flex-none items-center justify-center">
          {#if list.hasUser}<Check size={16} />{/if}
        </span>
        <span class="min-w-0 flex-1 truncate">{list.name}</span>
      </button>
    {/each}
  </div>
{/if}
{#if toggleErr}
  <p class="m-0 px-2 py-1.5 text-sm text-destructive">{toggleErr}</p>
{/if}
```

(二重押しのテストは、`disabled` と `busyIds` の二重の防御で1回に抑える。`fireEvent.click` は disabled なボタンにはハンドラを呼ばないが、`onToggle` の先頭の `busyIds.includes` でも防ぐ。)

- [ ] **Step 4: `UserListsPanel` のテストが通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/UserListsPanel.test.ts`
Expected: PASS

- [ ] **Step 5: `ReportUserDialog` の失敗するテストを書く**

`frontend/src/ui/ReportUserDialog.test.ts` を作成する。

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import ReportUserDialog from "./ReportUserDialog.svelte";

afterEach(() => cleanup());

function mount(onsubmit = vi.fn().mockResolvedValue(undefined)) {
  const oncancel = vi.fn();
  const r = render(ReportUserDialog, { props: { acct: "@alice@remote.example", onsubmit, oncancel } });
  return { ...r, onsubmit, oncancel };
}

describe("ReportUserDialog", () => {
  it("対象のacctを表示する", () => {
    const { getByText } = mount();
    expect(getByText(/@alice@remote\.example/)).toBeTruthy();
  });

  it("コメントが空(空白のみを含む)の間は、送信ボタンを押せない", async () => {
    const { getByRole, getByLabelText } = mount();
    const send = getByRole("button", { name: "通報する" }) as HTMLButtonElement;
    expect(send.disabled).toBe(true);

    await fireEvent.input(getByLabelText("通報の内容"), { target: { value: "   \n" } });
    expect(send.disabled).toBe(true);

    await fireEvent.input(getByLabelText("通報の内容"), { target: { value: "スパムです" } });
    expect(send.disabled).toBe(false);
  });

  it("送信するとonsubmitにコメントを渡す", async () => {
    const { getByRole, getByLabelText, onsubmit } = mount();
    await fireEvent.input(getByLabelText("通報の内容"), { target: { value: "スパムです" } });

    await fireEvent.click(getByRole("button", { name: "通報する" }));

    expect(onsubmit).toHaveBeenCalledWith("スパムです");
  });

  it("送信中は二重に送信できない", async () => {
    let resolve!: () => void;
    const onsubmit = vi.fn(() => new Promise<void>((r) => (resolve = r)));
    const { getByRole, getByLabelText } = mount(onsubmit);
    await fireEvent.input(getByLabelText("通報の内容"), { target: { value: "スパムです" } });
    const send = getByRole("button", { name: "通報する" }) as HTMLButtonElement;

    await fireEvent.click(send);
    await fireEvent.click(send);

    expect(onsubmit).toHaveBeenCalledTimes(1);
    expect(send.disabled).toBe(true);
    resolve();
  });

  it("失敗したら、エラーを出し、入力を残し、もう一度送信できる", async () => {
    const onsubmit = vi.fn().mockRejectedValueOnce(new Error("users/report-abuse: CANNOT_REPORT_THE_ADMIN")).mockResolvedValueOnce(undefined);
    const { getByRole, getByLabelText, findByText } = mount(onsubmit);
    const textarea = getByLabelText("通報の内容") as HTMLTextAreaElement;
    await fireEvent.input(textarea, { target: { value: "スパムです" } });

    await fireEvent.click(getByRole("button", { name: "通報する" }));

    expect(await findByText(/CANNOT_REPORT_THE_ADMIN/)).toBeTruthy();
    expect(textarea.value).toBe("スパムです");
    await waitFor(() => expect((getByRole("button", { name: "通報する" }) as HTMLButtonElement).disabled).toBe(false));

    await fireEvent.click(getByRole("button", { name: "通報する" }));
    expect(onsubmit).toHaveBeenCalledTimes(2);
  });

  it("コメントは2048文字までに制限する(maxlength)", () => {
    const { getByLabelText } = mount();
    expect((getByLabelText("通報の内容") as HTMLTextAreaElement).maxLength).toBe(2048);
  });

  it("キャンセルでoncancelを呼ぶ", async () => {
    const { getByRole, oncancel } = mount();
    await fireEvent.click(getByRole("button", { name: "キャンセル" }));
    expect(oncancel).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 6: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/ui/ReportUserDialog.test.ts`
Expected: FAIL(`ReportUserDialog.svelte` が無い)

- [ ] **Step 7: `ReportUserDialog.svelte` を実装する**

`frontend/src/ui/ReportUserDialog.svelte` を作成する。見た目と portal・z の作法は `ConfirmDialog.svelte` に揃える。

```svelte
<script lang="ts">
  // ユーザー通報ダイアログ(Issue #546)。ConfirmDialog と同じ portal + z の作法。送信の成否の扱いは
  // 呼び出し側の onsubmit(成功=resolve で呼び出し側が閉じる、失敗=reject でここにエラーを出して開いたまま)。
  import { Button } from "$lib/components/ui/button";

  let {
    acct,
    onsubmit,
    oncancel,
    // ドロップダウンメニュー(z-1010)から開いても隠れないよう、ConfirmDialog と同じく既定は1020。
    z = 1020,
  }: {
    acct: string;
    onsubmit: (comment: string) => Promise<void>;
    oncancel: () => void;
    z?: number;
  } = $props();

  const MAX_LENGTH = 2048; // 上流 users/report-abuse の comment の maxLength

  let comment = $state("");
  let busy = $state(false);
  let err = $state<string | null>(null);
  const canSend = $derived(!busy && comment.trim().length > 0);

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  async function send() {
    if (!canSend) return;
    busy = true;
    err = null;
    try {
      await onsubmit(comment.trim());
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div
  class="fixed inset-0 grid items-start justify-items-center bg-black/45 pt-[max(8vh,var(--safe-top))]"
  style={`z-index:${z}`}
  use:portal
  onclick={oncancel}
  onkeydown={(e) => e.key === "Escape" && oncancel()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-[min(420px,92vw)] rounded-xl border border-border bg-background p-4"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <header class="mb-2.5 font-semibold">ユーザーを通報</header>
    <p class="mb-2.5 mt-0 text-sm text-foreground">{acct} を、サーバーの管理者に通報します。</p>
    <label class="mb-2.5 flex flex-col gap-1 text-sm">
      <span class="text-muted-foreground">通報の内容</span>
      <textarea
        class="resize-y rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground"
        rows="4"
        maxlength={MAX_LENGTH}
        bind:value={comment}
      ></textarea>
    </label>
    {#if err}<p class="mb-2.5 mt-0 text-sm text-destructive">{err}</p>{/if}
    <div class="flex justify-end gap-2">
      <Button variant="secondary" size="sm" onclick={oncancel}>キャンセル</Button>
      <Button variant="destructive" size="sm" disabled={!canSend} onclick={send}>通報する</Button>
    </div>
  </div>
</div>
```

- [ ] **Step 8: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/ReportUserDialog.test.ts src/ui/UserListsPanel.test.ts && pnpm check`
Expected: PASS、型・a11y 警告 0。

- [ ] **Step 9: コミットする**

```bash
git add frontend/src/ui/UserListsPanel.svelte frontend/src/ui/UserListsPanel.test.ts frontend/src/ui/ReportUserDialog.svelte frontend/src/ui/ReportUserDialog.test.ts
git commit -m "$(cat <<'EOF'
feat(profile): リスト選択パネルと通報ダイアログを追加 (#546)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: `UserMenu.svelte` の拡張

**Files:**
- Modify: `frontend/src/ui/UserMenu.svelte`
- Modify: `frontend/src/ui/UserMenu.test.ts`

**Interfaces:**
- Consumes: `UserListsPanel`(Task 6)。
- Produces(props、既存の `muted` / `onmute` / `onunmute` は変更しない。新しいものは省略可能にして、既存のテストを壊さない):
  - `renoteMuted?: boolean | null`(既定 `null`)、`onrenotemute?: () => void`、`onunrenotemute?: () => void`
  - `blocking?: boolean | null`(既定 `null`)、`onblock?: () => void`(「ブロック」を押した。確認は呼び出し側)、`onunblock?: () => void`
  - `loadLists?: () => Promise<UserListMembership[]>`、`togglelist?: (listId: string, add: boolean) => Promise<void>`(両方あるときだけ「リストに追加」を出す)
  - `profileUrl?: string | null`(既定 `null`)、`onopen?: () => void`
  - `onreport?: () => void`(あるときだけ「通報」を出す)
- 項目の順: ミュート(解除)、リノートをミュート(解除)、ブロック(解除)、リストに追加、ブラウザで開く、区切り線、通報。`renoteMuted` / `blocking` が `null` の項目は出さない。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/UserMenu.test.ts` の `describe("UserMenu", ...)` を閉じる `});` の後に追加する。

```ts
describe("UserMenu のその他の項目(Issue #546)", () => {
  const base = { muted: false, onmute: vi.fn(), onunmute: vi.fn() };

  it("新しいpropsを渡さなければ、ミュートだけを出す(従来どおり)", () => {
    const { queryByRole } = render(UserMenu, { props: base });
    for (const name of ["リノートをミュート", "ブロック", "リストに追加", "ブラウザで開く", "通報"]) {
      expect(queryByRole("button", { name }), name).toBeNull();
    }
  });

  it("renoteMutedがnullなら「リノートをミュート」を出さない", () => {
    const { queryByRole } = render(UserMenu, { props: { ...base, renoteMuted: null, onrenotemute: vi.fn() } });
    expect(queryByRole("button", { name: "リノートをミュート" })).toBeNull();
  });

  it("リノートをミュートしていなければ「リノートをミュート」を出し、押すとonrenotemuteを呼ぶ", async () => {
    const onrenotemute = vi.fn();
    const { getByRole, queryByRole } = render(UserMenu, {
      props: { ...base, renoteMuted: false, onrenotemute, onunrenotemute: vi.fn() },
    });
    expect(queryByRole("button", { name: "リノートのミュートを解除" })).toBeNull();

    await fireEvent.click(getByRole("button", { name: "リノートをミュート" }));

    expect(onrenotemute).toHaveBeenCalledTimes(1);
  });

  it("リノートをミュート中なら「リノートのミュートを解除」を出し、押すとonunrenotemuteを呼ぶ", async () => {
    const onunrenotemute = vi.fn();
    const { getByRole, queryByRole } = render(UserMenu, {
      props: { ...base, renoteMuted: true, onrenotemute: vi.fn(), onunrenotemute },
    });
    expect(queryByRole("button", { name: "リノートをミュート" })).toBeNull();

    await fireEvent.click(getByRole("button", { name: "リノートのミュートを解除" }));

    expect(onunrenotemute).toHaveBeenCalledTimes(1);
  });

  it("ブロックしていなければ「ブロック」を押すとonblockを呼ぶ(確認は呼び出し側)", async () => {
    const onblock = vi.fn();
    const { getByRole } = render(UserMenu, { props: { ...base, blocking: false, onblock, onunblock: vi.fn() } });

    await fireEvent.click(getByRole("button", { name: "ブロック" }));

    expect(onblock).toHaveBeenCalledTimes(1);
  });

  it("ブロック中なら「ブロック解除」を出し、押すとonunblockを呼ぶ", async () => {
    const onunblock = vi.fn();
    const { getByRole, queryByRole } = render(UserMenu, {
      props: { ...base, blocking: true, onblock: vi.fn(), onunblock },
    });
    expect(queryByRole("button", { name: "ブロック" })).toBeNull();

    await fireEvent.click(getByRole("button", { name: "ブロック解除" }));

    expect(onunblock).toHaveBeenCalledTimes(1);
  });

  it("blockingがnullなら「ブロック」を出さない", () => {
    const { queryByRole } = render(UserMenu, { props: { ...base, blocking: null, onblock: vi.fn() } });
    expect(queryByRole("button", { name: "ブロック" })).toBeNull();
  });

  it("「リストに追加」を押すとパネルに切り替わり、loadを呼ぶ。「戻る」で最初の表示に戻る", async () => {
    const loadLists = vi.fn().mockResolvedValue([{ id: "L1", name: "友達", hasUser: false }]);
    const { getByRole, findByRole, queryByRole } = render(UserMenu, {
      props: { ...base, loadLists, togglelist: vi.fn() },
    });
    expect(loadLists).not.toHaveBeenCalled(); // 開くまでは取得しない

    await fireEvent.click(getByRole("button", { name: "リストに追加" }));

    expect(await findByRole("checkbox", { name: "友達" })).toBeTruthy();
    expect(loadLists).toHaveBeenCalledTimes(1);

    await fireEvent.click(getByRole("button", { name: "戻る" }));
    expect(getByRole("button", { name: "リストに追加" })).toBeTruthy();
    expect(queryByRole("checkbox", { name: "友達" })).toBeNull();
  });

  it("loadListsとtogglelistのどちらかが無ければ「リストに追加」を出さない", () => {
    const { queryByRole } = render(UserMenu, { props: { ...base, loadLists: vi.fn() } });
    expect(queryByRole("button", { name: "リストに追加" })).toBeNull();
  });

  it("profileUrlがあれば「ブラウザで開く」を出し、押すとonopenを呼ぶ。無ければ出さない", async () => {
    const onopen = vi.fn();
    const a = render(UserMenu, { props: { ...base, profileUrl: "https://remote.example/@alice", onopen } });
    await fireEvent.click(a.getByRole("button", { name: "ブラウザで開く" }));
    expect(onopen).toHaveBeenCalledTimes(1);
    cleanup();

    const b = render(UserMenu, { props: { ...base, profileUrl: null, onopen } });
    expect(b.queryByRole("button", { name: "ブラウザで開く" })).toBeNull();
  });

  it("「通報」を押すとonreportを呼ぶ", async () => {
    const onreport = vi.fn();
    const { getByRole } = render(UserMenu, { props: { ...base, onreport } });
    await fireEvent.click(getByRole("button", { name: "通報" }));
    expect(onreport).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/ui/UserMenu.test.ts`
Expected: 新しいテストが FAIL(項目が無い)。既存のテストは PASS のまま。

- [ ] **Step 3: 実装する**

`frontend/src/ui/UserMenu.svelte` を次の内容に置き換える(既存のミュートの部分は動作を変えない)。

```svelte
<script lang="ts">
  // ProfileModal の「…」メニュー(Issue #545 / #546)。見た目と項目は NoteMenu に揃える。
  // 非同期処理とエラー表示は呼び出し側(ProfileModal)が持つ。ここは表示と選択だけ
  // (リストの取得とトグルの状態だけは、サブメニューの UserListsPanel が持つ)。
  import {
    Ban,
    ChevronLeft,
    ChevronRight,
    Eye,
    EyeOff,
    ExternalLink,
    Flag,
    ListPlus,
    Repeat2,
  } from "@lucide/svelte";
  import type { MutePeriod, UserListMembership } from "../bindings/tauri.gen";
  import UserListsPanel from "./UserListsPanel.svelte";

  let {
    muted,
    onmute,
    onunmute,
    renoteMuted = null,
    onrenotemute = () => {},
    onunrenotemute = () => {},
    blocking = null,
    onblock = () => {},
    onunblock = () => {},
    loadLists,
    togglelist,
    profileUrl = null,
    onopen = () => {},
    onreport,
  }: {
    muted: boolean;
    onmute: (period: MutePeriod) => void;
    onunmute: () => void;
    /// null は「サーバーが返さなかった」。その項目は出さない。
    renoteMuted?: boolean | null;
    onrenotemute?: () => void;
    onunrenotemute?: () => void;
    blocking?: boolean | null;
    /// 「ブロック」を押したときに呼ぶ。確認ダイアログは呼び出し側が出す。
    onblock?: () => void;
    onunblock?: () => void;
    /// 両方あるときだけ「リストに追加」を出す。
    loadLists?: () => Promise<UserListMembership[]>;
    togglelist?: (listId: string, add: boolean) => Promise<void>;
    /// リモートユーザーの元ページ。null なら「ブラウザで開く」を出さない。
    profileUrl?: string | null;
    onopen?: () => void;
    /// あるときだけ「通報」を出す。ダイアログは呼び出し側が出す。
    onreport?: () => void;
  } = $props();

  // Misskey Web の「ミュートする期限」と同じ選択肢(既定の無期限が先頭)。
  const PERIODS: { value: MutePeriod; label: string }[] = [
    { value: "indefinitely", label: "無期限" },
    { value: "tenMinutes", label: "10分" },
    { value: "oneHour", label: "1時間" },
    { value: "oneDay", label: "1日" },
    { value: "oneWeek", label: "1週間" },
  ];

  // "root": 項目の一覧 / "period": 期間の選択 / "lists": リストへの追加。同じパネルの中で切り替える。
  let view = $state<"root" | "period" | "lists">("root");

  const ITEM =
    "box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted";
  const DANGER_ITEM =
    "box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-destructive hover:bg-muted";
</script>

<div
  class="w-[220px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
  data-testid="user-menu"
>
  {#if view === "root"}
    {#if muted}
      <button type="button" class={ITEM} onclick={onunmute}>
        <Eye size={16} />
        ミュート解除
      </button>
    {:else}
      <button type="button" class={ITEM} onclick={() => (view = "period")}>
        <EyeOff size={16} />
        ミュート
        <ChevronRight size={16} class="ml-auto" />
      </button>
    {/if}
    {#if renoteMuted === true}
      <button type="button" class={ITEM} onclick={onunrenotemute}>
        <Repeat2 size={16} />
        リノートのミュートを解除
      </button>
    {:else if renoteMuted === false}
      <button type="button" class={ITEM} onclick={onrenotemute}>
        <Repeat2 size={16} />
        リノートをミュート
      </button>
    {/if}
    {#if blocking === true}
      <button type="button" class={ITEM} onclick={onunblock}>
        <Ban size={16} />
        ブロック解除
      </button>
    {:else if blocking === false}
      <button type="button" class={ITEM} onclick={onblock}>
        <Ban size={16} />
        ブロック
      </button>
    {/if}
    {#if loadLists && togglelist}
      <button type="button" class={ITEM} onclick={() => (view = "lists")}>
        <ListPlus size={16} />
        リストに追加
        <ChevronRight size={16} class="ml-auto" />
      </button>
    {/if}
    {#if profileUrl}
      <button type="button" class={ITEM} onclick={onopen}>
        <ExternalLink size={16} />
        ブラウザで開く
      </button>
    {/if}
    {#if onreport}
      <div class="my-1 h-px bg-border" role="separator"></div>
      <button type="button" class={DANGER_ITEM} onclick={onreport}>
        <Flag size={16} />
        通報
      </button>
    {/if}
  {:else if view === "period"}
    <button type="button" class={ITEM} onclick={() => (view = "root")}>
      <ChevronLeft size={16} />
      戻る
    </button>
    <span class="block px-2 py-1 text-xs text-muted-foreground">ミュートする期限</span>
    {#each PERIODS as period (period.value)}
      <button type="button" class={ITEM} onclick={() => onmute(period.value)}>{period.label}</button>
    {/each}
  {:else if loadLists && togglelist}
    <UserListsPanel load={loadLists} toggle={togglelist} onback={() => (view = "root")} />
  {/if}
</div>
```

(メニュー幅を 200px → 220px にしたのは、「リノートのミュートを解除」が1行に収まるため。`ProfileModal.svelte` の `USER_MENU_W` も Task 8 で 220 にする。)

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/UserMenu.test.ts && pnpm check`
Expected: PASS(既存の5つ + 新しい11)、型・a11y 警告 0。

- [ ] **Step 5: コミットする**

```bash
git add frontend/src/ui/UserMenu.svelte frontend/src/ui/UserMenu.test.ts
git commit -m "$(cat <<'EOF'
feat(profile): ユーザーメニューにブロック・リスト・通報などの項目を追加 (#546)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: `ProfileModal.svelte` への組み込み

**Files:**
- Modify: `frontend/src/ui/ProfileModal.svelte`
- Modify: `frontend/src/ui/ProfileModal.test.ts`

**Interfaces:**
- Consumes: `app.blockUser` / `unblockUser` / `renoteMuteUser` / `unrenoteMuteUser` / `reportUser` / `listUserListsWithMembership` / `addUserToList` / `removeUserFromList`(Task 5)、`UserMenu` の新 props(Task 7)、`ReportUserDialog`(Task 6)、`ConfirmDialog`(既存)、`openUrl`(`@tauri-apps/plugin-opener`)。
- Produces: 画面の振る舞い(下のテストで規定)。

**変更の要点**
1. `muteBusy` / `muteErr` を `actionBusy` / `actionErr` に改名し、`mute` / `unmute` を共通ヘルパ `runUserAction` に移す(挙動と文言は変えない)。
2. ブロック/解除・リノートミュート/解除のハンドラを `runUserAction` で作る。
3. ブロックは `ConfirmDialog`(`danger`、`z={1020}`)の確認後に実行する。
4. 通報は `ReportUserDialog`。成功したらダイアログを閉じ、「通報しました」を表示する。
5. 「ブラウザで開く」は `openUrl(profile.profileUrl)`。
6. 「ブロック中」バッジ。ブロック成功時は `isFollowing = false`。
7. `USER_MENU_W` を 220 に。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/ProfileModal.test.ts` で、`profileResponse` の既定値に新しいフィールドを足す。

```ts
    isFollowing: false,
    isMuted: false,
    isBlocking: false,
    isRenoteMuted: false,
    profileUrl: null,
    isSelf: false,
```

ファイル先頭付近の `vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));` は、テストから呼び出しを検証できるよう、ホイストした mock に変える。

```ts
const openUrlMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: openUrlMock }));
```

`afterEach` に `openUrlMock.mockClear();` を足す。

`describe("ミュート(Issue #545)", ...)` を閉じる `});` の直後に、次の `describe` を追加する。ミュートの `describe` 内の `mockProfile` / `renderModal` / `menuButton` は、その `describe` のスコープなので、同じ定義を新しい `describe` の先頭に書く。

```ts
  describe("ブロック・リノートミュート・リスト・通報・ブラウザで開く(Issue #546)", () => {
    function mockProfile(overrides: Record<string, unknown> = {}, onCommand?: (cmd: string) => unknown) {
      invokeMock.mockImplementation((cmd: string) => {
        const custom = onCommand?.(cmd);
        if (custom !== undefined) return custom;
        if (cmd === "get_user_profile") return Promise.resolve(profileResponse(overrides));
        if (cmd === "get_user_notes") return Promise.resolve([]);
        return Promise.resolve(null);
      });
    }
    const renderModal = () =>
      render(ProfileModal, { props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} } });
    const menuButton = (r: ReturnType<typeof renderModal>) =>
      r.findByRole("button", { name: "ユーザーメニュー" });
    const called = (cmd: string) => invokeMock.mock.calls.filter(([c]) => c === cmd);

    describe("ブロック", () => {
      it("「ブロック」を押すと確認が出て、キャンセルならblock_userを呼ばない", async () => {
        mockProfile();
        const r = renderModal();
        await fireEvent.click(await menuButton(r));

        await fireEvent.click(r.getByRole("button", { name: "ブロック" }));
        expect(r.getByRole("button", { name: "ブロックする" })).toBeTruthy(); // 確認ダイアログ(Modal も role=dialog なので、ボタンで判定する)
        await fireEvent.click(r.getByRole("button", { name: "キャンセル" }));

        expect(called("block_user")).toHaveLength(0);
        expect(r.queryByText("ブロック中")).toBeNull();
      });

      it("確認でOKするとblock_userを呼び、「ブロック中」バッジが出て、フォロー中ならフォロー解除表示になる", async () => {
        mockProfile({ isFollowing: true });
        const r = renderModal();
        await waitFor(() => expect(r.getByRole("button", { name: "フォロー解除" })).toBeTruthy());
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "ブロック" }));

        await fireEvent.click(r.getByRole("button", { name: "ブロックする" }));

        await waitFor(() =>
          expect(invokeMock).toHaveBeenCalledWith(
            "block_user",
            expect.objectContaining({ accountId: "acc1", userId: "u1" }),
          ),
        );
        await waitFor(() => expect(r.getByText("ブロック中")).toBeTruthy());
        // サーバーはブロック時にフォローを外す
        expect(r.getByRole("button", { name: "フォロー" })).toBeTruthy();
        expect(r.queryByRole("button", { name: "フォロー解除" })).toBeNull();
      });

      it("ブロック中は「ブロック中」バッジが出て、「ブロック解除」は確認なしでunblock_userを呼ぶ", async () => {
        mockProfile({ isBlocking: true });
        const r = renderModal();
        await waitFor(() => expect(r.getByText("ブロック中")).toBeTruthy());
        await fireEvent.click(await menuButton(r));

        await fireEvent.click(r.getByRole("button", { name: "ブロック解除" }));

        await waitFor(() =>
          expect(invokeMock).toHaveBeenCalledWith(
            "unblock_user",
            expect.objectContaining({ accountId: "acc1", userId: "u1" }),
          ),
        );
        await waitFor(() => expect(r.queryByText("ブロック中")).toBeNull());
      });

      it("ALREADY_BLOCKINGで失敗したら、表示を「ブロック中」に直し、補正メッセージを出す", async () => {
        mockProfile({}, (cmd) =>
          cmd === "block_user"
            ? Promise.reject(new Error("blocking/create: ALREADY_BLOCKING You are already blocking that user."))
            : undefined,
        );
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "ブロック" }));

        await fireEvent.click(r.getByRole("button", { name: "ブロックする" }));

        await waitFor(() => expect(r.getByText("ブロック中")).toBeTruthy());
        expect(r.getByText(/すでにブロック中です/)).toBeTruthy();
      });

      it("NOT_BLOCKINGで失敗したら、表示を「ブロック中でない」に直す", async () => {
        mockProfile({ isBlocking: true }, (cmd) =>
          cmd === "unblock_user" ? Promise.reject(new Error("blocking/delete: NOT_BLOCKING")) : undefined,
        );
        const r = renderModal();
        await waitFor(() => expect(r.getByText("ブロック中")).toBeTruthy());
        await fireEvent.click(await menuButton(r));

        await fireEvent.click(r.getByRole("button", { name: "ブロック解除" }));

        await waitFor(() => expect(r.queryByText("ブロック中")).toBeNull());
        expect(r.getByText(/すでにブロック中ではありません/)).toBeTruthy();
      });

      it("block_userが403(権限不足)で失敗したら、エラーを表示し、バッジは出さない", async () => {
        mockProfile({}, (cmd) =>
          cmd === "block_user"
            ? Promise.reject({ kind: "forbidden", message: "blocking/create: PERMISSION_DENIED" })
            : undefined,
        );
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "ブロック" }));

        await fireEvent.click(r.getByRole("button", { name: "ブロックする" }));

        await waitFor(() => expect(r.getByText(/PERMISSION_DENIED|forbidden|権限/)).toBeTruthy());
        expect(r.queryByText("ブロック中")).toBeNull();
      });
    });

    describe("リノートミュート", () => {
      it("「リノートをミュート」でrenote_mute_userを呼び、メニューを開き直すと「リノートのミュートを解除」になる", async () => {
        mockProfile();
        const r = renderModal();
        await fireEvent.click(await menuButton(r));

        await fireEvent.click(r.getByRole("button", { name: "リノートをミュート" }));

        await waitFor(() =>
          expect(invokeMock).toHaveBeenCalledWith(
            "renote_mute_user",
            expect.objectContaining({ accountId: "acc1", userId: "u1" }),
          ),
        );
        await fireEvent.click(await menuButton(r));
        expect(await r.findByRole("button", { name: "リノートのミュートを解除" })).toBeTruthy();
      });

      it("リノートミュート中なら、解除でunrenote_mute_userを呼ぶ", async () => {
        mockProfile({ isRenoteMuted: true });
        const r = renderModal();
        await fireEvent.click(await menuButton(r));

        await fireEvent.click(r.getByRole("button", { name: "リノートのミュートを解除" }));

        await waitFor(() =>
          expect(invokeMock).toHaveBeenCalledWith(
            "unrenote_mute_user",
            expect.objectContaining({ accountId: "acc1", userId: "u1" }),
          ),
        );
      });

      it("ALREADY_MUTINGで失敗したら、表示をリノートミュート中に直す", async () => {
        mockProfile({}, (cmd) =>
          cmd === "renote_mute_user" ? Promise.reject(new Error("renote-mute/create: ALREADY_MUTING")) : undefined,
        );
        const r = renderModal();
        await fireEvent.click(await menuButton(r));

        await fireEvent.click(r.getByRole("button", { name: "リノートをミュート" }));

        await waitFor(() => expect(r.getByText(/すでにリノートをミュート中です/)).toBeTruthy());
        await fireEvent.click(await menuButton(r));
        expect(await r.findByRole("button", { name: "リノートのミュートを解除" })).toBeTruthy();
      });

      it("isRenoteMutedがnullなら「リノートをミュート」を出さない(サーバーが返さない場合)", async () => {
        mockProfile({ isRenoteMuted: null });
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        expect(r.queryByRole("button", { name: "リノートをミュート" })).toBeNull();
      });
    });

    describe("リストに追加", () => {
      it("パネルを開くとlist_user_lists_with_membershipを呼び、押すとadd_user_to_listを呼ぶ", async () => {
        mockProfile({}, (cmd) =>
          cmd === "list_user_lists_with_membership"
            ? Promise.resolve([{ id: "L1", name: "友達", hasUser: false }])
            : undefined,
        );
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "リストに追加" }));

        const row = await r.findByRole("checkbox", { name: "友達" });
        await fireEvent.click(row);

        await waitFor(() =>
          expect(invokeMock).toHaveBeenCalledWith(
            "add_user_to_list",
            expect.objectContaining({ accountId: "acc1", listId: "L1", userId: "u1" }),
          ),
        );
        expect(invokeMock).toHaveBeenCalledWith(
          "list_user_lists_with_membership",
          expect.objectContaining({ accountId: "acc1", userId: "u1" }),
        );
        await waitFor(() => expect(row.getAttribute("aria-checked")).toBe("true"));
      });

      it("入っているリストを押すとremove_user_from_listを呼ぶ", async () => {
        mockProfile({}, (cmd) =>
          cmd === "list_user_lists_with_membership"
            ? Promise.resolve([{ id: "L1", name: "友達", hasUser: true }])
            : undefined,
        );
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "リストに追加" }));

        await fireEvent.click(await r.findByRole("checkbox", { name: "友達" }));

        await waitFor(() =>
          expect(invokeMock).toHaveBeenCalledWith(
            "remove_user_from_list",
            expect.objectContaining({ listId: "L1", userId: "u1" }),
          ),
        );
      });

      it("ALREADY_ADDEDで失敗したら、パネルにエラーを出し、メニューは開いたまま", async () => {
        mockProfile({}, (cmd) => {
          if (cmd === "list_user_lists_with_membership")
            return Promise.resolve([{ id: "L1", name: "友達", hasUser: false }]);
          if (cmd === "add_user_to_list") return Promise.reject(new Error("users/lists/push: ALREADY_ADDED"));
          return undefined;
        });
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "リストに追加" }));

        await fireEvent.click(await r.findByRole("checkbox", { name: "友達" }));

        await waitFor(() => expect(r.getByText(/ALREADY_ADDED/)).toBeTruthy());
        expect(r.getByTestId("user-menu")).toBeTruthy();
      });
    });

    describe("通報", () => {
      it("「通報」でダイアログが開き、コメントを送るとreport_userを呼び、閉じて「通報しました」を出す", async () => {
        mockProfile();
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "通報" }));

        await fireEvent.input(r.getByLabelText("通報の内容"), { target: { value: "スパムです" } });
        await fireEvent.click(r.getByRole("button", { name: "通報する" }));

        await waitFor(() =>
          expect(invokeMock).toHaveBeenCalledWith(
            "report_user",
            expect.objectContaining({ accountId: "acc1", userId: "u1", comment: "スパムです" }),
          ),
        );
        await waitFor(() => expect(r.queryByLabelText("通報の内容")).toBeNull());
        expect(r.getByText("通報しました")).toBeTruthy();
      });

      it("通報に失敗したら、ダイアログは開いたまま入力が残る", async () => {
        mockProfile({}, (cmd) =>
          cmd === "report_user" ? Promise.reject(new Error("users/report-abuse: CANNOT_REPORT_THE_ADMIN")) : undefined,
        );
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "通報" }));
        await fireEvent.input(r.getByLabelText("通報の内容"), { target: { value: "スパムです" } });

        await fireEvent.click(r.getByRole("button", { name: "通報する" }));

        await waitFor(() => expect(r.getByText(/CANNOT_REPORT_THE_ADMIN/)).toBeTruthy());
        expect((r.getByLabelText("通報の内容") as HTMLTextAreaElement).value).toBe("スパムです");
        expect(r.queryByText("通報しました")).toBeNull();
      });
    });

    describe("ブラウザで開く", () => {
      it("profileUrlがあれば「ブラウザで開く」を出し、押すとopenUrlでその元ページを開く", async () => {
        mockProfile({ profileUrl: "https://remote.example/@alice" });
        const r = renderModal();
        await fireEvent.click(await menuButton(r));

        await fireEvent.click(r.getByRole("button", { name: "ブラウザで開く" }));

        expect(openUrlMock).toHaveBeenCalledWith("https://remote.example/@alice");
        expect(r.queryByTestId("user-menu")).toBeNull(); // メニューは閉じる
      });

      it("profileUrlがnullなら「ブラウザで開く」を出さない", async () => {
        mockProfile({ profileUrl: null });
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        expect(r.queryByRole("button", { name: "ブラウザで開く" })).toBeNull();
      });
    });

    it("操作の応答が返る前にプロフィールが切り替わっても、新しいプロフィールの表示とbusyを汚さない", async () => {
      let rejectBlock!: (e: Error) => void;
      mockProfile({}, (cmd) =>
        cmd === "block_user" ? new Promise((_, rej) => (rejectBlock = rej)) : undefined,
      );
      const r = renderModal();
      await fireEvent.click(await menuButton(r));
      await fireEvent.click(r.getByRole("button", { name: "ブロック" }));
      await fireEvent.click(r.getByRole("button", { name: "ブロックする" }));
      await waitFor(() => expect(called("block_user")).toHaveLength(1));

      await r.rerender({ target: { userId: "u2" }, accountId: "acc1", onclose: () => {} });
      rejectBlock(new Error("blocking/create: ALREADY_BLOCKING"));
      await new Promise((resolve) => setTimeout(resolve, 0));

      expect(r.queryByText("ブロック中")).toBeNull();
      expect(r.queryByText(/すでにブロック中です/)).toBeNull();
    });
  });
```

「ブロックする」は確認ダイアログの確定ボタンのラベル。`ConfirmDialog` に `confirmLabel="ブロックする"` を渡す(Step 3)。

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/ui/ProfileModal.test.ts -t "Issue #546"`
Expected: FAIL(項目・ダイアログ・バッジが無い)

- [ ] **Step 3: 実装する**

`frontend/src/ui/ProfileModal.svelte` を編集する。

(a) import を整える。

```ts
  import { openUrl } from "@tauri-apps/plugin-opener";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import ReportUserDialog from "./ReportUserDialog.svelte";
```

(b) 状態を改名・追加する。既存の `let muteBusy = $state(false);` / `let muteErr = $state<string | null>(null);` を次の2行に置き換え(改名)、その下に残りを足す。既存の `const USER_MENU_W = 200;` は `220` に変える(新しい `const` を増やさない)。

```ts
  let actionBusy = $state(false);
  let actionErr = $state<string | null>(null);
  let confirmBlock = $state(false);
  let reportOpen = $state(false);
  let notice = $state<string | null>(null);
```

```ts
  const USER_MENU_W = 220;
```

`load()` の中のリセットも合わせる(`muteErr = null; muteBusy = false;` を次に置き換える)。

```ts
    actionErr = null;
    actionBusy = false;
    confirmBlock = false;
    reportOpen = false;
    notice = null;
```

(c) `mute` / `unmute` を、共通ヘルパと各ハンドラに置き換える。

```ts
  type ReadyProfile = Extract<ProfileState, { status: "ready" }>["profile"];

  // ミュート/ブロック/リノートミュートの共通の流れ。busy 制御、世代番号による古い応答の無視、
  // 「すでにその状態だった」エラーのときの表示補正をまとめる。`stale` は、そのエラーコードが
  // 含まれていたときに、表示を直して補正メッセージを出す。
  async function runUserAction(
    op: (userId: string) => Promise<unknown>,
    onDone: (profile: ReadyProfile) => void,
    stale?: { code: string; message: string; fix: (profile: ReadyProfile) => void },
  ) {
    if (profileState.status !== "ready" || actionBusy) return;
    const gen = requestGen;
    const userId = profileState.profile.user.id;
    userMenuOpen = false;
    actionBusy = true;
    actionErr = null;
    notice = null;
    try {
      await op(userId);
      if (gen === requestGen && profileState.status === "ready") onDone(profileState.profile);
    } catch (e) {
      if (gen === requestGen) {
        if (stale && String(e).includes(stale.code) && profileState.status === "ready") {
          stale.fix(profileState.profile);
          actionErr = stale.message;
        } else {
          actionErr = String(e);
        }
      }
    } finally {
      if (gen === requestGen) actionBusy = false; // 切り替え後の新しいプロフィールの busy を、古い呼び出しが触らない
    }
  }

  const mute = (period: MutePeriod) =>
    runUserAction(
      (userId) => app.muteUser(accountId, userId, period),
      (p) => (p.isMuted = true),
      {
        code: "ALREADY_MUTING",
        message: "すでにミュート中です。表示を更新しました",
        fix: (p) => (p.isMuted = true),
      },
    );

  const unmute = () =>
    runUserAction(
      (userId) => app.unmuteUser(accountId, userId),
      (p) => (p.isMuted = false),
      {
        code: "NOT_MUTING",
        message: "すでにミュート中ではありません。表示を更新しました",
        fix: (p) => (p.isMuted = false),
      },
    );

  const renoteMute = () =>
    runUserAction(
      (userId) => app.renoteMuteUser(accountId, userId),
      (p) => (p.isRenoteMuted = true),
      {
        code: "ALREADY_MUTING",
        message: "すでにリノートをミュート中です。表示を更新しました",
        fix: (p) => (p.isRenoteMuted = true),
      },
    );

  const unrenoteMute = () =>
    runUserAction(
      (userId) => app.unrenoteMuteUser(accountId, userId),
      (p) => (p.isRenoteMuted = false),
      {
        code: "NOT_MUTING",
        message: "すでにリノートをミュート中ではありません。表示を更新しました",
        fix: (p) => (p.isRenoteMuted = false),
      },
    );

  // ブロックの確認後に呼ぶ。サーバーはブロック時にフォローを外すので、表示も外す。
  const block = () => {
    confirmBlock = false;
    return runUserAction(
      (userId) => app.blockUser(accountId, userId),
      (p) => {
        p.isBlocking = true;
        if (p.isFollowing !== null) p.isFollowing = false;
      },
      {
        code: "ALREADY_BLOCKING",
        message: "すでにブロック中です。表示を更新しました",
        fix: (p) => (p.isBlocking = true),
      },
    );
  };

  const unblock = () =>
    runUserAction(
      (userId) => app.unblockUser(accountId, userId),
      (p) => (p.isBlocking = false),
      {
        code: "NOT_BLOCKING",
        message: "すでにブロック中ではありません。表示を更新しました",
        fix: (p) => (p.isBlocking = false),
      },
    );

  // 「リストに追加」パネルへ渡す(状態とエラー表示はパネルが持つ)。
  const loadLists = () => {
    if (profileState.status !== "ready") return Promise.resolve([]);
    return app.listUserListsWithMembership(accountId, profileState.profile.user.id);
  };
  const toggleList = (listId: string, add: boolean) => {
    if (profileState.status !== "ready") return Promise.resolve();
    const userId = profileState.profile.user.id;
    return add ? app.addUserToList(accountId, listId, userId) : app.removeUserFromList(accountId, listId, userId);
  };

  // 通報ダイアログの送信。成功したら閉じて、プロフィール上に通知を出す。失敗は throw してダイアログに出す。
  async function submitReport(comment: string) {
    if (profileState.status !== "ready") return;
    const gen = requestGen;
    await app.reportUser(accountId, profileState.profile.user.id, comment);
    if (gen !== requestGen) return; // 送信中にプロフィールが切り替わったら、新しい側を触らない
    reportOpen = false;
    notice = "通報しました";
  }

  function openProfilePage() {
    if (profileState.status !== "ready" || !profileState.profile.profileUrl) return;
    userMenuOpen = false;
    void openUrl(profileState.profile.profileUrl);
  }
```

(`profile.isMuted = true` などの代入は、`profileState` が `$state` のため元のコードと同じく反映される。`runUserAction` の `onDone` に渡す `profileState.profile` は、同じプロキシ。)

(d) テンプレートを編集する。

- 「ミュート中」バッジの直後に「ブロック中」バッジを足す。

```svelte
          {#if profile.isBlocking}
            <span class="inline-flex w-fit items-center rounded-sm border border-border px-1 text-xs text-muted-foreground">ブロック中</span>
          {/if}
```

- 「…」ボタンの `disabled={muteBusy}` を `disabled={actionBusy}` に。
- `<UserMenu .../>` を次に置き換える。

```svelte
                  <UserMenu
                    muted={profile.isMuted}
                    onmute={mute}
                    onunmute={unmute}
                    renoteMuted={profile.isRenoteMuted}
                    onrenotemute={renoteMute}
                    onunrenotemute={unrenoteMute}
                    blocking={profile.isBlocking}
                    onblock={() => {
                      userMenuOpen = false;
                      confirmBlock = true;
                    }}
                    onunblock={unblock}
                    loadLists={loadLists}
                    togglelist={toggleList}
                    profileUrl={profile.profileUrl}
                    onopen={openProfilePage}
                    onreport={() => {
                      userMenuOpen = false;
                      reportOpen = true;
                    }}
                  />
```

- エラー表示の `{#if muteErr}...{muteErr}...{/if}` を `actionErr` に変え、その下に通知を足す。

```svelte
      {#if actionErr}<p class="my-2 text-sm text-destructive">{actionErr}</p>{/if}
      {#if notice}<p class="my-2 text-sm text-muted-foreground">{notice}</p>{/if}
```

- `{#if followListKind}...{/if}` の後ろ(`{/if}` で `{:else}` ブロックを閉じる前)に、ダイアログを足す。

```svelte
      {#if confirmBlock}
        <ConfirmDialog
          title="ブロック"
          message={`${acct(profile.user)} をブロックしますか?\n相手はあなたをフォローできなくなり、フォロー関係も解除されます。`}
          confirmLabel="ブロックする"
          danger
          z={1020}
          onConfirm={block}
          onCancel={() => (confirmBlock = false)}
        />
      {/if}
      {#if reportOpen}
        <ReportUserDialog acct={acct(profile.user)} onsubmit={submitReport} oncancel={() => (reportOpen = false)} />
      {/if}
```

(e) 「…」ボタンの表示条件 `{#if profile.isMuted !== null}` は変えない(spec どおり)。

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/ProfileModal.test.ts && pnpm check`
Expected: PASS(Issue #545 のミュートのテストを含む既存のすべて + 新しいテスト)、型・a11y 警告 0。

もし既存のミュートのテストが `muteErr` の文言で落ちたら、`runUserAction` に移した文言が元と一字一句同じか確認する(「すでにミュート中です。表示を更新しました」「すでにミュート中ではありません。表示を更新しました」)。

- [ ] **Step 5: フロントエンド全体のテストを実行する**

Run: `cd frontend && pnpm test`
Expected: PASS

- [ ] **Step 6: コミットする**

```bash
git add frontend/src/ui/ProfileModal.svelte frontend/src/ui/ProfileModal.test.ts
git commit -m "$(cat <<'EOF'
feat(profile): プロフィールからブロック・リノートミュート・リスト・通報・ブラウザ表示を行えるようにする (#546)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 9: ドキュメントと実機の確認

**Files:**
- Modify: `docs/guide/user-guide.md`(252行目の「ユーザーのミュート(期間指定)」の後に追記)
- Modify: `docs/superpowers/specs/2026-11-09-user-profile-actions-design.md`(手動確認の結果を追記。必要なら)

- [ ] **Step 1: ユーザーガイドを更新する**

`docs/guide/user-guide.md` の「ユーザーのミュート(期間指定)」の項目の直後に、次の項目を追加する。

```markdown
- **プロフィールの「…」メニューのその他の操作**: ミュートのほかに、次の操作ができます(自分自身のプロフィールでは「…」メニューは出ません)。
  - **リノートをミュート / リノートのミュートを解除**: そのユーザーのリノートだけをタイムラインから隠します。サーバー側の設定で、他のクライアントにも反映されます。
  - **ブロック / ブロック解除**: 確認のあとでブロックします。相手はあなたをフォローできなくなり、あなたと相手のフォローは解除されます。ブロック中のユーザーのプロフィールには「ブロック中」と表示され、「ブロック解除」は確認なしで実行されます。
  - **リストに追加**: 自分のリストの一覧が開き、そのユーザーが入っているリストにはチェックが付きます。押すと追加と解除を切り替えます。
  - **ブラウザで開く**: 他のサーバーのユーザー(リモートユーザー)のプロフィールで、そのユーザーが所属するサーバー上の元のページをブラウザで開きます。ローカルユーザーでは出ません。
  - **通報**: 通報の内容(必須、2048文字まで)を入力して、サーバーの管理者に通報します。送信に失敗しても入力は残ります。
  ブロックと通報には、書き込み権限(ブロックは `write:blocks`、通報は `write:report-abuse`)が必要です。これまでログインしていたアカウントで操作して権限不足の案内が出たら、設定のアカウント一覧にある「再認証」からもう一度認可してください。ブロック・リノートミュートの反映は、次に受信・読み込みするノートからです。
```

- [ ] **Step 2: 全体のテストと型チェックを通す**

Run: `cd src-tauri && cargo test && cd ../frontend && pnpm check && pnpm test`
Expected: すべて PASS、型エラー 0。

- [ ] **Step 3: Rust のフォーマットと lint を確認する**

Run: `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: 問題なし。差分が出たら `cargo fmt` で直して、その変更を直前のタスクのコミットではなく、新しい `style(profile): ...` のコミットにする。

- [ ] **Step 4: 実 UI で確認する(Xvfb 越し。ユーザーの実画面に出さない)**

`e2e/README.md` の手順で使い捨ての Compose Misskey を立て、`cargo build`(`src-tauri/` から。`cargo tauri build` は使わない)で作ったバイナリを Xvfb 越しで動かし、次を実際に操作して確認する。`WAYLAND_DISPLAY` は unset する。起動したプロセスは完了前に正確な PID で止める。

1. リモートでない(同一インスタンスの)別ユーザーのプロフィールを開く → 「…」メニューに、ミュート・リノートをミュート・ブロック・リストに追加・通報が出て、「ブラウザで開く」は出ない。
2. ブロック → 確認 → 「ブロック中」バッジ、フォローが外れる(サーバー側でも `users/show` の `isFollowing` が `false`)。ブロック解除。
3. リノートをミュート → 解除。
4. 「リストに追加」で、リストにチェックが付き、サーバー側のリストのメンバーが変わる。
5. 通報 → サーバー側に通報が作られる(管理画面か `admin/abuse-user-reports/list`)。
6. (リモートユーザーが用意できれば)「ブラウザで開く」が出て、`url` が埋まっている。用意できなければ、その旨を記録する。
7. 既存アカウントの権限不足の経路: `write:blocks` を持たないトークンでブロック → 403 のエラー表示になる。

結果(できたこと・できなかったこと)を、最終報告に事実のまま書く。レビューが通っただけでは完了としない。

- [ ] **Step 5: コミットする**

```bash
git add docs/guide/user-guide.md
git commit -m "$(cat <<'EOF'
docs(guide): プロフィールの「…」メニューの操作を追記 (#546)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 6: PR を作る(ユーザーの指示があったときだけ)**

push と PR 作成は、ユーザーが依頼したときに行う。PR は `gh pr create` で、`.github/pull_request_template.md` の構造に沿った本文を手で書き、`Fixes #546` を含め、末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)` を付ける。マージは `gh pr merge --merge`(squash は使わない)。push 後に CI を待ち続けない。
