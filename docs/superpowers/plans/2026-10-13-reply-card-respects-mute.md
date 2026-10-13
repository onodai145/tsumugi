# 返信先カードがミュート設定を考慮する Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 返信先ノートがミュート対象(ローカルNG・サーバーミュート・ワードミュート)のとき、カラムと通知カードの返信先カードを出さず、従来の「↩ 返信」だけにする。

**Architecture:** 判定の純粋関数を `filter/mute.rs` に、除去(`note.reply` / `note.renote.reply` を `None` にする)を `AppState` のメソッドに置く。Rust 側は、キャッシュ書き込みの**後**、画面・イベントへ出す**直前**の出口地点で除去を呼ぶ(キャッシュには元データを残す)。フロントは `setMute` でローカルNGだけを表示中のノートと通知へ即時反映する。`NoteCard.svelte` は `inner.replyId && replyTarget` のときだけカードを出すので、UI は変更しない。

**Tech Stack:** Rust (Tauri v2, tokio, wiremock), Svelte 5 + TypeScript (Vitest)

**Spec:** `docs/superpowers/specs/2026-10-13-reply-card-respects-mute-design.md`(Issue #486)

## Global Constraints

- 作業ブランチは `feat/issue-486-reply-card-respects-mute`(作成済み)。`main` では編集しない。
- コミットメッセージは件名1行のみ(本文・箇条書きなし)。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` を付ける。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら止めて報告し、再試行しない。
- `reply_id` / `reply_user_id` は除去しても残す(「↩ 返信」の表示と `reply_to_me` 述語に必要)。
- キャッシュ(`NoteCacheStore`)には元のノート(返信先つき)を書く。除去は書き込みの後、返却・emit の直前に、画面用のコピーへ行う。
- `frontend/src/bindings/tauri.gen.ts` は手編集しない。今回は型の変更がないので再生成も不要。
- `cargo tauri dev` / `./target/debug/tsumugi` を検証目的で直接起動しない。起動が必要な検証は Task 7 の手順に従う。
- 変更はサーバーミュートの同期処理(`api/mutes.rs`)、検索・プロフィール系のノート取得(`get_user_notes` 等)には及ばない。返信先カードを描画するのは `Column.svelte` と `NotificationCard.svelte` だけ。

## Review Focus

- 純粋な Renote の Renote 先に付いた返信先(`note.renote.reply`)がミュート対象のとき、カードが出ないこと(Task 2 のテスト、Task 6 のテスト)。
- 返信先がミュート対象でも、返信ノート本体は残り、`reply_id` が保たれること(Task 2、Task 6)。
- 返信先がミュート対象でないとき、`reply` が変更されない(同一オブジェクト / 値のまま)こと(Task 2、Task 6)。
- ストリーム受信と初期取得で、キャッシュに元の返信先が残ること(Task 3 のテスト。ストリームはユニットテストが書けないため Task 7 の実機確認で担保する)。
- 通知のうち `note` が `None` のもの(フォロー等)で落ちないこと(Task 2、Task 4)。

---

## File Structure

- Modify `src-tauri/src/filter/mute.rs`: `is_reply_target_muted`(純粋関数)と、そのテスト。
- Modify `src-tauri/src/state.rs`: `strip_muted_reply` / `strip_muted_replies` / `strip_muted_reply_in_notification`、そのテスト。
- Modify `src-tauri/src/commands/column.rs`: 出口地点への適用(初期取得・キャッシュ復元・ギャップ埋め・追加取得・検索・`filter_notifications`)、そのテスト。
- Modify `src-tauri/src/stream/connection.rs`: ストリーム受信ノートと通知の emit 直前に適用。
- Modify `frontend/src/lib/store.svelte.ts`: `stripMutedReply` の追加と `setMute` への適用。
- Modify `frontend/src/lib/store.svelte.test.ts`: `stripMutedReply` / `setMute` のテスト。
- Modify `docs/guide/user-guide.md`: 既知の制限の記述を実際の挙動に書き換える。

`NoteCard.svelte` と `NoteCard.test.ts` は変更しない。既存の `NoteCard.test.ts` が、(a) `showReplyCard` なし(展開した引用など)では返信先の本文が出ないこと、(b) `reply: null` なら「返信」だけが出ること、を既に固定している。

---

### Task 1: 返信先のミュート判定(純粋関数)

**Files:**
- Modify: `src-tauri/src/filter/mute.rs`(本体は `is_word_note_muted` の直後、テストは既存 `mod tests` の末尾)

**Interfaces:**
- Produces: `pub fn is_reply_target_muted(reply: &Note, cfg: &MuteConfig, server_muted: bool, word_rules: &[WordMuteRule]) -> bool`

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` の末尾(最後のテストの後)に追加する。既存のヘルパー `note(text, username, host)` を使う。

```rust
    #[test]
    fn reply_target_muted_by_ng_user() {
        let cfg = MuteConfig { ng_users: vec!["@bad".into()], ..Default::default() };
        assert!(is_reply_target_muted(&note("hi", "bad", None), &cfg, false, &[]));
        assert!(!is_reply_target_muted(&note("hi", "good", None), &cfg, false, &[]));
    }

    #[test]
    fn reply_target_muted_by_ng_instance() {
        let cfg = MuteConfig { ng_instances: vec!["bad.example".into()], ..Default::default() };
        assert!(is_reply_target_muted(&note("hi", "a", Some("bad.example")), &cfg, false, &[]));
        assert!(!is_reply_target_muted(&note("hi", "a", Some("ok.example")), &cfg, false, &[]));
    }

    #[test]
    fn reply_target_muted_by_ng_word_in_text_or_cw() {
        let cfg = MuteConfig { ng_words: vec!["spoiler".into()], ..Default::default() };
        assert!(is_reply_target_muted(&note("a SPOILER b", "a", None), &cfg, false, &[]));
        let mut with_cw = note("body", "a", None);
        with_cw.cw = Some("spoiler".into());
        assert!(is_reply_target_muted(&with_cw, &cfg, false, &[]));
        assert!(!is_reply_target_muted(&note("clean", "a", None), &cfg, false, &[]));
    }

    #[test]
    fn reply_target_muted_by_server_mute_flag() {
        let cfg = MuteConfig::default();
        assert!(is_reply_target_muted(&note("hi", "a", None), &cfg, true, &[]));
        assert!(!is_reply_target_muted(&note("hi", "a", None), &cfg, false, &[]));
    }

    #[test]
    fn reply_target_muted_by_server_word_rule() {
        let cfg = MuteConfig::default();
        let rules = vec![WordMuteRule::Words(vec!["secret".into()])];
        assert!(is_reply_target_muted(&note("a secret b", "a", None), &cfg, false, &rules));
        assert!(!is_reply_target_muted(&note("public", "a", None), &cfg, false, &rules));
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib reply_target_muted`
Expected: コンパイルエラー `cannot find function is_reply_target_muted`。

- [ ] **Step 3: 最小実装を書く**

`is_word_note_muted` の直後(`#[cfg(test)]` の前)に追加する。

```rust
/// 返信先ノートがミュート対象か(Issue #486)。表示用に返信先カードを外す判定で、
/// 返信ノート本体の表示・非表示には使わない。
/// `server_muted` は呼び出し側が `reply.user.id` で調べたサーバー側ミュート/ブロックの結果。
pub fn is_reply_target_muted(
    reply: &Note,
    cfg: &MuteConfig,
    server_muted: bool,
    word_rules: &[WordMuteRule],
) -> bool {
    server_muted
        || is_muted_one(reply, cfg)
        || is_word_muted(reply.text.as_deref(), reply.cw.as_deref(), word_rules)
}
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd src-tauri && cargo test --lib reply_target_muted`
Expected: 5 件 PASS。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/filter/mute.rs
git commit -m "feat: 返信先ノートのミュート判定を追加(Issue #486)"
```

(末尾に Co-Authored-By トレーラーを付ける。Global Constraints 参照。)

---

### Task 2: AppState の除去メソッド

**Files:**
- Modify: `src-tauri/src/state.rs`(`set_server_word_mutes` の直後にメソッド、`mod tests` の末尾にテスト)

**Interfaces:**
- Consumes: `crate::filter::mute::is_reply_target_muted`(Task 1)
- Produces:
  - `pub fn strip_muted_reply(&self, account_id: &str, note: &mut Note)`
  - `pub fn strip_muted_replies(&self, account_id: &str, notes: &mut [Note])`
  - `pub fn strip_muted_reply_in_notification(&self, account_id: &str, n: &mut Notification)`

- [ ] **Step 1: 失敗するテストを書く**

`state.rs` の `mod tests` の末尾に追加する。`Note` リテラルが長いので、テスト内ヘルパーを置く。

```rust
    fn reply_test_note(id: &str, user_id: &str, username: &str, text: &str) -> crate::domain::Note {
        use crate::domain::{User, Visibility};
        crate::domain::Note {
            id: id.into(),
            created_at: 0,
            text: Some(text.into()),
            cw: None,
            visibility: Visibility::Public,
            local_only: false,
            user: User {
                id: user_id.into(),
                username: username.into(),
                host: None,
                name: None,
                avatar_url: None,
                is_bot: false,
                is_cat: false,
                followers_count: 0,
                following_count: 0,
                notes_count: 0,
                emojis: std::collections::HashMap::new(),
                bio: None,
                banner_url: None,
                avatar_blurhash: None,
                instance: None,
            },
            reply_id: None,
            reply_user_id: None,
            renote_id: None,
            reply: None,
            renote: None,
            files: vec![],
            poll: None,
            tags: vec![],
            mentions: vec![],
            emojis: std::collections::HashMap::new(),
            channel_id: None,
            via: None,
            lang: None,
            reactions: std::collections::HashMap::new(),
            reaction_count: 0,
            renote_count: 0,
            reply_count: 0,
            my_reaction: None,
            is_renoted_by_me: false,
            is_favorited_by_me: false,
            is_pinned: false,
        }
    }

    /// 返信先 r1 つきの返信ノート n1 を作る。
    fn reply_with_target(target_user_id: &str, target_username: &str, target_text: &str) -> crate::domain::Note {
        let mut n = reply_test_note("n1", "u1", "alice", "my reply");
        n.reply_id = Some("r1".into());
        n.reply_user_id = Some(target_user_id.into());
        n.reply = Some(Box::new(reply_test_note("r1", target_user_id, target_username, target_text)));
        n
    }

    #[test]
    fn strip_muted_reply_removes_target_muted_by_local_ng_user() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        *state.mute.lock().unwrap() = crate::domain::MuteConfig {
            ng_users: vec!["@bob".into()],
            ..Default::default()
        };
        let mut n = reply_with_target("u2", "bob", "hi");

        state.strip_muted_reply("acc1", &mut n);

        assert!(n.reply.is_none());
        assert_eq!(n.reply_id.as_deref(), Some("r1"), "reply_id は残す");
        assert_eq!(n.reply_user_id.as_deref(), Some("u2"), "reply_user_id は残す");
        assert_eq!(n.text.as_deref(), Some("my reply"), "本体は変えない");
    }

    #[test]
    fn strip_muted_reply_removes_target_muted_by_local_ng_word() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        *state.mute.lock().unwrap() = crate::domain::MuteConfig {
            ng_words: vec!["spoiler".into()],
            ..Default::default()
        };
        let mut n = reply_with_target("u2", "bob", "big SPOILER here");

        state.strip_muted_reply("acc1", &mut n);

        assert!(n.reply.is_none());
    }

    #[test]
    fn strip_muted_reply_removes_target_muted_by_server_mute() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.set_server_mutes("acc1", HashSet::from(["u2".to_string()]));
        let mut n = reply_with_target("u2", "bob", "hi");

        state.strip_muted_reply("acc1", &mut n);

        assert!(n.reply.is_none());
    }

    #[test]
    fn strip_muted_reply_removes_target_muted_by_server_word_mute() {
        use crate::filter::mute::WordMuteRule;
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.set_server_word_mutes("acc1", vec![WordMuteRule::Words(vec!["secret".into()])]);
        let mut n = reply_with_target("u2", "bob", "a secret b");

        state.strip_muted_reply("acc1", &mut n);

        assert!(n.reply.is_none());
    }

    #[test]
    fn strip_muted_reply_keeps_target_when_nothing_matches() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.set_server_mutes("acc1", HashSet::from(["someone-else".to_string()]));
        let mut n = reply_with_target("u2", "bob", "hi");
        let before = n.clone();

        state.strip_muted_reply("acc1", &mut n);

        assert_eq!(n, before);
    }

    #[test]
    fn strip_muted_reply_uses_the_given_accounts_server_mutes_only() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.set_server_mutes("acc2", HashSet::from(["u2".to_string()]));
        let mut n = reply_with_target("u2", "bob", "hi");

        state.strip_muted_reply("acc1", &mut n);

        assert!(n.reply.is_some(), "別アカウントのサーバーミュートは効かない");
    }

    #[test]
    fn strip_muted_reply_also_strips_the_reply_of_a_renote_target() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.set_server_mutes("acc1", HashSet::from(["u2".to_string()]));
        let inner = reply_with_target("u2", "bob", "hi");
        let mut wrapper = reply_test_note("rn", "u3", "carol", "");
        wrapper.text = None;
        wrapper.renote_id = Some("n1".into());
        wrapper.renote = Some(Box::new(inner));

        state.strip_muted_reply("acc1", &mut wrapper);

        let inner = wrapper.renote.as_ref().expect("Renote 先は残る");
        assert!(inner.reply.is_none());
        assert_eq!(inner.reply_id.as_deref(), Some("r1"));
    }

    #[test]
    fn strip_muted_replies_handles_a_batch_and_leaves_notes_without_reply_alone() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.set_server_mutes("acc1", HashSet::from(["u2".to_string()]));
        let plain = reply_test_note("p", "u1", "alice", "no reply");
        let mut notes = vec![reply_with_target("u2", "bob", "hi"), plain.clone()];

        state.strip_muted_replies("acc1", &mut notes);

        assert!(notes[0].reply.is_none());
        assert_eq!(notes[1], plain);
    }

    #[test]
    fn strip_muted_reply_in_notification_strips_the_note_and_tolerates_no_note() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.set_server_mutes("acc1", HashSet::from(["u2".to_string()]));
        let mut with_note = crate::domain::Notification {
            id: "x1".into(),
            created_at: 0,
            kind: "reply".into(),
            user: None,
            note: Some(reply_with_target("u2", "bob", "hi")),
            reaction: None,
        };
        let mut without_note = crate::domain::Notification {
            id: "x2".into(),
            created_at: 0,
            kind: "follow".into(),
            user: None,
            note: None,
            reaction: None,
        };

        state.strip_muted_reply_in_notification("acc1", &mut with_note);
        state.strip_muted_reply_in_notification("acc1", &mut without_note);

        assert!(with_note.note.as_ref().unwrap().reply.is_none());
        assert!(without_note.note.is_none());
    }
```

`HashSet` と `SettingsStore` が `mod tests` から見えることを確認する(既存の `is_word_muted_false_before_sync_and_true_after` が `HashSet`/`SettingsStore` を使っていれば `use super::*` で足りる。足りなければテスト内で `use std::collections::HashSet;` を足す)。

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib strip_muted_repl`
Expected: コンパイルエラー `no method named strip_muted_reply`。

- [ ] **Step 3: 実装を書く**

`state.rs` の `set_server_word_mutes` の直後に追加する。冒頭の `use` に `Notification` が無ければ足す(`use crate::domain::{..., Notification, ...}`)。ロックは入れ子にしない(`is_server_muted` は自分で取って離す。ワードルールのロックは判定の間だけ持つ)。

```rust
    /// 返信先ノートがミュート対象か(ローカルNG・サーバーミュート・サーバーワードミュート)。
    fn is_reply_muted(&self, account_id: &str, mute: &MuteConfig, reply: &Note) -> bool {
        let server_muted = self.is_server_muted(account_id, &reply.user.id);
        let word_rules = self.server_word_mutes.lock().unwrap();
        let rules = word_rules.get(account_id).map(Vec::as_slice).unwrap_or(&[]);
        crate::filter::mute::is_reply_target_muted(reply, mute, server_muted, rules)
    }

    /// 返信先(`reply`)がミュート対象なら表示用に `None` にする(Issue #486)。Renote 先の `reply` も同様。
    /// `reply_id` / `reply_user_id` は残すので、UI は従来の「↩ 返信」だけを出す。
    /// キャッシュへ書いた後、画面・イベントへ出す直前のコピーに対して呼ぶこと。
    pub fn strip_muted_reply(&self, account_id: &str, note: &mut Note) {
        let mute = self.mute.lock().unwrap().clone();
        self.strip_muted_reply_with(account_id, &mute, note);
    }

    /// `strip_muted_reply` を複数ノートに適用する(NG 設定のコピーは1回だけ)。
    pub fn strip_muted_replies(&self, account_id: &str, notes: &mut [Note]) {
        let mute = self.mute.lock().unwrap().clone();
        for note in notes {
            self.strip_muted_reply_with(account_id, &mute, note);
        }
    }

    /// 通知の対象ノートの返信先を `strip_muted_reply` と同様に外す。`note` が無い通知は何もしない。
    pub fn strip_muted_reply_in_notification(&self, account_id: &str, n: &mut Notification) {
        if let Some(note) = n.note.as_mut() {
            self.strip_muted_reply(account_id, note);
        }
    }

    fn strip_muted_reply_with(&self, account_id: &str, mute: &MuteConfig, note: &mut Note) {
        if note.reply.as_deref().is_some_and(|r| self.is_reply_muted(account_id, mute, r)) {
            note.reply = None;
        }
        if let Some(inner) = note.renote.as_deref_mut() {
            if inner.reply.as_deref().is_some_and(|r| self.is_reply_muted(account_id, mute, r)) {
                inner.reply = None;
            }
        }
    }
```

`MuteConfig` / `Note` / `Notification` が `state.rs` の `use` に無ければ足す(既に `Note` と `WordMuteRule` は使われている)。

- [ ] **Step 4: 通ることを確認する**

Run: `cd src-tauri && cargo test --lib strip_muted_repl`
Expected: 9 件 PASS。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/state.rs
git commit -m "feat: ミュート対象の返信先を外すAppStateメソッドを追加(Issue #486)"
```

---

### Task 3: カラムの初期取得・キャッシュ復元・ギャップ埋めに適用

**Files:**
- Modify: `src-tauri/src/commands/column.rs`
  - `open_stream_and_fetch` の通常カラム分の戻り値(`Ok((fetch.notes, vec![]))`、`commit_initial_writes` の後)
  - `open_column` のキャッシュ復元(`let notes = restrict_to_boundary(...)` の後)
  - ギャップ埋めの `ColumnGapFill` を emit する 2 か所(`open_column` 内の `spawn` と、`fill_gap` を呼ぶもう 1 か所)
- Test: 同ファイル末尾の `mod tests`

**Interfaces:**
- Consumes: `AppState::strip_muted_reply` / `strip_muted_replies`(Task 2)

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` の、既存の `open_stream_and_fetch_returns_an_error_and_writes_nothing_when_the_epoch_is_stale` の直前(`mount_one_note_page` の直後)に追加する。

```rust
    /// ノート n9(u1)が、返信先 r1(u_muted)への返信になっているページ。
    async fn mount_reply_page(mock: &MockServer) {
        let page = serde_json::json!([{
            "id": "n9",
            "createdAt": "2026-07-05T12:00:00.000Z",
            "text": "hello",
            "user": { "id": "u1", "username": "alice", "host": null },
            "visibility": "home",
            "replyId": "r1",
            "reply": {
                "id": "r1",
                "createdAt": "2026-07-05T11:00:00.000Z",
                "text": "secret",
                "user": { "id": "u_muted", "username": "bob", "host": null },
                "visibility": "home"
            }
        }]);
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_json(page)).mount(mock).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn open_stream_and_fetch_hides_a_muted_reply_target_but_caches_the_original() {
        let mock = MockServer::start().await;
        mount_reply_page(&mock).await;
        let state = command_state(&mock);
        state.set_server_mutes("acc1", std::collections::HashSet::from(["u_muted".to_string()]));
        let column = command_column("c1", ColumnKind::Local);
        state.settings.upsert_column(&column).unwrap();
        let app = tauri::test::mock_app();
        let resolved = resolve_sources(&state, "acc1", &column.kind, &column.filter).await.unwrap();
        let (host, token) = state.host_token("acc1").unwrap();
        let epoch = state.column_fence.begin("c1");

        let (notes, _) = open_stream_and_fetch(app.handle(), &state, &column, Some(resolved), host, token, &epoch)
            .await
            .unwrap();

        assert_eq!(notes.len(), 1, "返信ノート本体は残る");
        assert!(notes[0].reply.is_none(), "ミュート対象の返信先は外す");
        assert_eq!(notes[0].reply_id.as_deref(), Some("r1"), "reply_id は残す");
        let cached = state.cache.load_cached("c1", 10).await.unwrap();
        assert_eq!(
            cached[0].reply.as_ref().map(|r| r.id.as_str()),
            Some("r1"),
            "キャッシュには元の返信先を残す"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn open_stream_and_fetch_keeps_a_reply_target_that_is_not_muted() {
        let mock = MockServer::start().await;
        mount_reply_page(&mock).await;
        let state = command_state(&mock);
        let column = command_column("c1", ColumnKind::Local);
        state.settings.upsert_column(&column).unwrap();
        let app = tauri::test::mock_app();
        let resolved = resolve_sources(&state, "acc1", &column.kind, &column.filter).await.unwrap();
        let (host, token) = state.host_token("acc1").unwrap();
        let epoch = state.column_fence.begin("c1");

        let (notes, _) = open_stream_and_fetch(app.handle(), &state, &column, Some(resolved), host, token, &epoch)
            .await
            .unwrap();

        assert_eq!(notes[0].reply.as_ref().map(|r| r.id.as_str()), Some("r1"));
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib open_stream_and_fetch_hides_a_muted_reply_target open_stream_and_fetch_keeps_a_reply_target`
Expected: 1 本目が FAIL(`notes[0].reply.is_none()` が偽)。2 本目は現状でも PASS する(回帰防止)。

- [ ] **Step 3: 出口地点に適用する**

(a) `open_stream_and_fetch` の通常カラム分(`commit_initial_writes(...).await?;` の直後)。`fetch.notes` を取り出して除去する。

```rust
    commit_initial_writes(
        // ...既存の引数のまま...
    )
    .await?;
    let mut notes = fetch.notes;
    state.strip_muted_replies(&column.account_id, &mut notes);
    Ok((notes, vec![]))
```

(`Ok((fetch.notes, vec![]))` を上の 3 行に置き換える。`cacheable` は `commit_initial_writes` が書き終えているので、ここで `fetch.notes` を動かしてよい。)

(b) `open_column` のキャッシュ復元。`let notes = restrict_to_boundary(cached, restore_e.as_deref());` の次の行以降を次のようにする。

```rust
        let mut notes = restrict_to_boundary(cached, restore_e.as_deref());
        state.cache_metrics.record_resume(!notes.is_empty());
        state.strip_muted_replies(&column.account_id, &mut notes);
        notes
```

(`notes[0].id` を後で使うが、除去は `reply` しか触らないので id は変わらない。)

(c) ギャップ埋めの emit 2 か所。`open_column` 内の `spawn` の中(`account_id` 変数がある)と、`fill_gap(&state, &column.account_id, ...)` を呼ぶ関数の中。どちらも `ColumnGapFill { notes: gap_result.notes, ... }` の直前で、`commit_gap_fill_writes` と空判定を通った後に除去する。

`spawn` 内:

```rust
                let mut gap_notes = gap_result.notes;
                state.strip_muted_replies(&account_id, &mut gap_notes);
                let _ = crate::events::ColumnGapFill {
                    column_id,
                    notes: gap_notes,
                    truncated: gap_result.truncated,
                    // ...以下は既存のまま...
```

もう一方(`column.id` / `column.account_id` を使う側):

```rust
    let mut gap_notes = gap_result.notes;
    state.strip_muted_replies(&column.account_id, &mut gap_notes);
    let _ = crate::events::ColumnGapFill {
        column_id: column.id.clone(),
        notes: gap_notes,
        truncated: gap_result.truncated,
        // ...以下は既存のまま...
```

(`state` は前者では `app2.try_state::<AppState>()` で得た値、後者では引数。型が `State` でも `&AppState` でも `strip_muted_replies` は自動参照で呼べる。コンパイラの指示に従って `&*state` 等に直す。)

呼び出し元の確認: `grep -n "open_stream_and_fetch(\|ColumnGapFill {" src-tauri/src/commands/column.rs` で、`open_stream_and_fetch` の呼び出しは `open_column` / `update_column` 系 3 か所(テスト除く)のみ、`ColumnGapFill` の emit は上記 2 か所のみであること。他にあれば同様に適用する。

- [ ] **Step 4: 通ることを確認する**

Run: `cd src-tauri && cargo test --lib open_stream_and_fetch`
Expected: 新規 2 件を含め、既存の `open_stream_and_fetch_*` も PASS。

Run: `cd src-tauri && cargo test --lib commit_`
Expected: PASS(キャッシュ書き込みの既存テストが壊れていないこと)。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "feat: カラムの初期取得・復元・ギャップ埋めでミュート対象の返信先を外す(Issue #486)"
```

---

### Task 4: 追加取得・検索・通知一覧に適用

**Files:**
- Modify: `src-tauri/src/commands/column.rs`
  - `fetch_backfill`(キャッシュヒットの返却と、末尾の `Ok(fetch.notes)`)
  - `search_cache_notes`(`search_cache_core` の結果)
  - サーバー検索の `apply_search_mutes` を呼ぶクロージャ
  - `filter_notifications`
- Test: 同ファイルの `mod tests`

**Interfaces:**
- Consumes: `AppState::strip_muted_replies` / `strip_muted_reply_in_notification`(Task 2)

- [ ] **Step 1: 失敗するテストを書く**

`filter_notifications` のテスト。既存テストのヘルパー `note(id, created_at)`、`command_state(&mock)` を使う。`mod tests` の末尾に追加する。

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn filter_notifications_strips_a_muted_reply_target_and_keeps_the_notification() {
        let mock = MockServer::start().await;
        let state = command_state(&mock);
        state.set_server_mutes("acc1", std::collections::HashSet::from(["u_muted".to_string()]));
        let mut reply_note = note("n1", 1);
        reply_note.reply_id = Some("r1".into());
        let mut target = note("r1", 0);
        target.user.id = "u_muted".into();
        reply_note.reply = Some(Box::new(target));
        let with_note = Notification {
            id: "x1".into(),
            created_at: 1,
            kind: "reply".into(),
            user: None,
            note: Some(reply_note),
            reaction: None,
        };
        let without_note = Notification {
            id: "x2".into(),
            created_at: 2,
            kind: "follow".into(),
            user: None,
            note: None,
            reaction: None,
        };

        let got = filter_notifications(&state, "acc1", vec![with_note, without_note]);

        assert_eq!(got.len(), 2, "通知自体は残る");
        let n = got[0].note.as_ref().unwrap();
        assert!(n.reply.is_none());
        assert_eq!(n.reply_id.as_deref(), Some("r1"));
        assert!(got[1].note.is_none());
    }
```

`Notification` が `mod tests` から見えなければ `use crate::domain::Notification;` を足す。

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib filter_notifications_strips`
Expected: FAIL(`n.reply.is_none()` が偽)。

- [ ] **Step 3: 実装を書く**

(a) `filter_notifications` を次のようにする(既存の `.filter(...)` の後ろに `.map(...)` を足す)。

```rust
fn filter_notifications(
    state: &AppState,
    account_id: &str,
    raw: Vec<Notification>,
) -> Vec<Notification> {
    let mute = state.mute.lock().unwrap().clone();
    raw.into_iter()
        .filter(|n| match &n.user {
            Some(u) => {
                !state.is_server_muted(account_id, &u.id)
                    && !crate::filter::mute::is_user_muted(u, &mute)
            }
            None => true,
        })
        .map(|mut n| {
            state.strip_muted_reply_in_notification(account_id, &mut n);
            n
        })
        .collect()
}
```

doc コメントの末尾に「通知ノートの返信先がミュート対象なら外す(Issue #486)。」を足す。

(b) `fetch_backfill` のキャッシュヒット。`if let Some(notes) = cache_backfill_page(...) {` を `if let Some(mut notes) = ...` にして、`return Ok(notes)` の前に除去する。

```rust
        if let Some(mut notes) = cache_backfill_page(effective.as_deref(), &until_id, cached, raw_loaded, INITIAL_LIMIT) {
            state.cache_metrics.record_backfill(BackfillOutcome::Hit);
            state.strip_muted_replies(&column.account_id, &mut notes);
            return Ok(notes);
        }
```

(c) `fetch_backfill` の末尾(`commit_backfill_writes` の `match` の後)の `Ok(fetch.notes)` を置き換える。

```rust
    let mut notes = fetch.notes;
    state.strip_muted_replies(&column.account_id, &mut notes);
    Ok(notes)
```

(d) `search_cache_notes`(`search_cache_core(...).await` を返している関数)。結果を受けて除去する。

```rust
    let mut notes = search_cache_core(
        &state.cache,
        &filter,
        &eval_ctx,
        &mute,
        until_id.as_deref(),
        limit,
        |n| server_muted_note(&state, &account_id, n),
        |n| state.is_word_muted(&account_id, n),
    )
    .await?;
    state.strip_muted_replies(&account_id, &mut notes);
    Ok(notes)
```

(`search_cache_core` の引数は既存の呼び出しのまま。シグネチャは変えない。)

(e) サーバー検索の `collect_unmuted_page` に渡している 2 つ目のクロージャを置き換える。

```rust
        |raw| {
            let mut kept = apply_search_mutes(
                raw,
                &mute,
                |n| server_muted_note(&state, &account_id, n),
                |n| state.is_word_muted(&account_id, n),
            );
            state.strip_muted_replies(&account_id, &mut kept);
            kept
        },
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd src-tauri && cargo test --lib filter_notifications`
Expected: PASS。

Run: `cd src-tauri && cargo test --lib column`
Expected: `commands::column` の既存テストがすべて PASS(`search_cache_core_*`、`apply_search_mutes_*`、`backfill` 系を含む)。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "feat: 追加取得・検索・通知一覧でミュート対象の返信先を外す(Issue #486)"
```

---

### Task 5: ストリーム受信に適用

**Files:**
- Modify: `src-tauri/src/stream/connection.rs`(`handle_text` の `Incoming::ChannelNote` 分岐と `Incoming::ChannelNotification` 分岐)

**Interfaces:**
- Consumes: `AppState::strip_muted_reply` / `strip_muted_reply_in_notification`(Task 2)

`handle_text` には既存のユニットテストが無く、`AppHandle` と WebSocket 受信の足場が要るため、ユニットテストは書かない。ここの正しさは、Task 2(除去ロジック)、Task 3(キャッシュに元データが残る順序の考え方)、Task 7(実機確認)で担保する。順序(キャッシュ保存が先、除去が後)をコードレビューで必ず確認すること。

- [ ] **Step 1: ノート受信を変更する**

`handle_text` の `Incoming::ChannelNote` 分岐で、`let normalized: Note = (*note).into();` を `let mut normalized: Note = (*note).into();` にする。そのうえで、ミュート判定の後の `cache_note` の直後に 1 行足す。`cache_note(...).await` は書き込みが終わってから戻るので、その後に `normalized` を直接書き換えても、キャッシュには元の返信先が残る。

```rust
                let _ = state.cache.cache_note(&column_id, &normalized).await;
                // キャッシュには元の返信先を残し、画面へ出す分だけ外す(Issue #486)。cache_note の後に呼ぶこと。
                state.strip_muted_reply(account_id, &mut normalized);
            }
            let _ = ColumnNote {
                column_id: column_id.clone(),
                note: normalized,
            }
            .emit(app);
```

(`account_id` は同じ分岐で `StreamMode::Notes { account_id, .. }` から束縛済みの `&String`。コンパイラが型の不一致を示したら `account_id.as_str()` にする。)

- [ ] **Step 2: 通知受信を変更する**

`Incoming::ChannelNotification` 分岐で、`let n: Notification = (*notification).into();` を `let mut n` にする。`ColumnNotification { ... notification: n }` を emit する直前(reaction 通知のフォールバック処理の後)に、次を足す。

```rust
            if let Some(state) = app.try_state::<AppState>() {
                state.strip_muted_reply_in_notification(&account_id, &mut n);
            }
```

reaction 通知のフォールバック処理(`n.note` から `note.id` を読む)は、除去より前に走る位置のまま触らない(除去は `reply` だけを触るので影響しないが、順序を保つ)。

- [ ] **Step 3: ビルドと既存テスト**

Run: `cd src-tauri && cargo build 2>&1 | tail -20`
Expected: エラーなしでビルドできる。

Run: `cd src-tauri && cargo test --lib connection`
Expected: `stream::connection` の既存テストがすべて PASS。

- [ ] **Step 4: コミット**

```bash
git add src-tauri/src/stream/connection.rs
git commit -m "feat: ストリーム受信でミュート対象の返信先を外す(Issue #486)"
```

---

### Task 6: フロント(ローカルNGの即時反映)

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`(`isMuted` の直後に `stripMutedReply`、`setMute` を変更)
- Test: `frontend/src/lib/store.svelte.test.ts`

**Interfaces:**
- Produces: `export function stripMutedReply(n: Note, cfg: MuteConfig): Note`

- [ ] **Step 1: 失敗するテストを書く**

`store.svelte.test.ts` の先頭近くの動的 import を変更する。

```ts
const { app, applyOwnReactionEvent, stripMutedReply } = await import("./store.svelte");
```

ファイル末尾に追加する(`makeNote` / `makeUser` / `makeNotification` / `makeNoteTab` / `makeGroup` は既存)。

```ts
describe("stripMutedReply (Issue #486)", () => {
  const cfg = (over: Partial<{ ngWords: string[]; ngUsers: string[]; ngInstances: string[] }> = {}) => ({
    ngWords: [],
    ngUsers: [],
    ngInstances: [],
    ...over,
  });
  const bob = () => makeUser({ id: "u2", username: "bob", name: "Bob" });
  const replyNote = (target: Note) => makeNote({ id: "n1", replyId: "r1", replyUserId: "u2", reply: target });

  it("NGユーザーの返信先は reply を null にし、replyId は残す", () => {
    const n = replyNote(makeNote({ id: "r1", user: bob() }));
    const got = stripMutedReply(n, cfg({ ngUsers: ["@bob"] }));
    expect(got.reply).toBeNull();
    expect(got.replyId).toBe("r1");
    expect(got.text).toBe("hello");
  });

  it("NGインスタンスの返信先を外す", () => {
    const n = replyNote(makeNote({ id: "r1", user: makeUser({ id: "u2", username: "bob", host: "bad.example" }) }));
    expect(stripMutedReply(n, cfg({ ngInstances: ["bad.example"] })).reply).toBeNull();
  });

  it("NGワードを含む返信先を外す(本文とCW)", () => {
    const byText = replyNote(makeNote({ id: "r1", user: bob(), text: "big SPOILER" }));
    expect(stripMutedReply(byText, cfg({ ngWords: ["spoiler"] })).reply).toBeNull();
    const byCw = replyNote(makeNote({ id: "r1", user: bob(), text: "body", cw: "spoiler" }));
    expect(stripMutedReply(byCw, cfg({ ngWords: ["spoiler"] })).reply).toBeNull();
  });

  it("Renote先の返信先も外し、Renote先は残す", () => {
    const inner = replyNote(makeNote({ id: "r1", user: bob() }));
    const wrapper = makeNote({ id: "rn", text: null, renoteId: "n1", renote: inner });
    const got = stripMutedReply(wrapper, cfg({ ngUsers: ["@bob"] }));
    expect(got.renote).not.toBeNull();
    expect(got.renote!.reply).toBeNull();
    expect(got.renote!.replyId).toBe("r1");
  });

  it("該当しなければ同じオブジェクトを返す", () => {
    const n = replyNote(makeNote({ id: "r1", user: bob() }));
    expect(stripMutedReply(n, cfg({ ngUsers: ["@someone"] }))).toBe(n);
    const plain = makeNote({ id: "p" });
    expect(stripMutedReply(plain, cfg({ ngUsers: ["@bob"] }))).toBe(plain);
  });
});

describe("app.setMute strips muted reply targets (Issue #486)", () => {
  it("表示中のノートと通知の返信先を外し、本体は残す", async () => {
    const target = () => makeNote({ id: "r1", user: makeUser({ id: "u2", username: "bob" }) });
    const inTimeline = makeNote({ id: "n1", replyId: "r1", reply: target() });
    const inNotification = makeNote({ id: "n2", replyId: "r1", reply: target() });
    const tab = makeNoteTab([inTimeline], {
      notifications: [makeNotification({ id: "x1", type: "reply", note: inNotification })],
    });
    app.groups = [makeGroup([tab])];

    await app.setMute({ ngWords: [], ngUsers: ["@bob"], ngInstances: [] });

    const live = app.groups[0].tabs[0];
    expect(live.notes).toHaveLength(1);
    expect(live.notes[0].reply).toBeNull();
    expect(live.notes[0].replyId).toBe("r1");
    expect(live.notifications).toHaveLength(1);
    expect(live.notifications[0].note?.reply).toBeNull();
  });

  it("note を持たない通知はそのまま残る", async () => {
    const tab = makeNoteTab([], { notifications: [makeNotification({ id: "x1", type: "follow", note: null })] });
    app.groups = [makeGroup([tab])];

    await app.setMute({ ngWords: [], ngUsers: ["@bob"], ngInstances: [] });

    expect(app.groups[0].tabs[0].notifications).toHaveLength(1);
    expect(app.groups[0].tabs[0].notifications[0].note).toBeNull();
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && npx vitest run src/lib/store.svelte.test.ts -t "Issue #486"`
Expected: FAIL(`stripMutedReply is not a function` など)。

- [ ] **Step 3: 実装を書く**

`store.svelte.ts` の `isMuted` の直後に追加する。

```ts
/// 返信先(`reply` / Renote先の `reply`)が NG 設定に該当するとき、`reply` を null にしたノートを返す(Issue #486)。
/// 該当しなければ同じオブジェクトを返す。`replyId` は残すので、表示は従来の「↩ 返信」になる。
/// サーバーミュート/ワードミュートはフロントから見えないため扱わない(Rust 側が配信時に外す)。
export function stripMutedReply(n: Note, cfg: MuteConfig): Note {
  const reply = n.reply && noteMutedOne(n.reply, cfg) ? null : n.reply;
  const renote = n.renote ? stripMutedReply(n.renote, cfg) : n.renote;
  if (reply === n.reply && renote === n.renote) return n;
  return { ...n, reply, renote };
}
```

`setMute` のループを置き換える。

```ts
    for (const t of this.#allTabs()) {
      t.notes = t.notes
        .filter((n) => !isMuted(n, config))
        .map((n) => stripMutedReply(n, config));
      t.notifications = t.notifications.map((x) =>
        x.note ? { ...x, note: stripMutedReply(x.note, config) } : x,
      );
    }
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd frontend && npx vitest run src/lib/store.svelte.test.ts`
Expected: 新規を含めすべて PASS。

Run: `cd frontend && pnpm check`
Expected: エラーなし。

Run: `cd frontend && pnpm test`
Expected: 全体 PASS(`NoteCard.test.ts` の既存の返信先カードのテストを含む)。

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "feat: NG設定の保存時に表示中のノートと通知の返信先カードを外す(Issue #486)"
```

---

### Task 7: ドキュメントと全体検証

**Files:**
- Modify: `docs/guide/user-guide.md`(133 行目の「返信先の表示」の項)

- [ ] **Step 1: ユーザーガイドを書き換える**

133 行目の末尾の次の文を置き換える。

置換前:

```
既知の制限として、ミュート設定(ローカルNG・サーバーミュート・ワードミュート)は返信先のカードには適用されません。ミュート対象のユーザーや単語を含む投稿への返信でも、返信先の投稿者名と本文は表示されます。
```

置換後:

```
返信先がミュート設定(ローカルNG・サーバーミュート・ワードミュート)に該当する場合は、カードを出さず、従来どおり「↩ 返信」とだけ表示されます。返信ノート自体の表示・非表示は、これまでのミュート判定のままです。NGの設定を保存すると、表示中のノートと通知のカードにもすぐ反映されます。サーバー側のミュートやワードミュートの変更は、次に受信・読み込みするノートから反映されます。一度「↩ 返信」に戻ったカードは、ミュートを解除してもカラムを開き直すまで元に戻りません。
```

- [ ] **Step 2: 全テストと静的検査**

Run: `cd src-tauri && cargo test`
Expected: 全体 PASS(実接続テストは `#[ignore]`)。`generates_frontend_bindings` が PASS し、`git status` で `frontend/src/bindings/tauri.gen.ts` に差分が出ていないこと(型は変えていない)。

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | tail -20`
Expected: この変更に起因する警告なし。

Run: `cd frontend && pnpm check && pnpm test`
Expected: PASS。

- [ ] **Step 3: 実機確認(仮想ディスプレイ越し)**

メモリの運用に従う: 実 UI の確認は Xvfb 越し、`WAYLAND_DISPLAY` を unset、`dbus-run-session` を使い、`/tmp` 直下の短い XDG パスに隔離したインスタンスで行う。ユーザーの実データのキャッシュ DB(`~/.cache/com.onodai.tsumugi/cache.db`)は触らない。`cargo tauri dev` はリポジトリルートから起動し、自分で起動したプロセスはタスク完了前に正確な PID で停止する(`pkill` 系は使わない)。

確認項目(`e2e/` の Compose Misskey を使い、実描画のスクリーンショットを自分で見る):

1. ミュート対象のユーザーの投稿に返信するノートを、ホームカラムのストリームで受信する → 返信先カードではなく「↩ 返信」だけが出る。ミュートしていないユーザーへの返信は従来どおりカードが出る。
2. 同じ返信を通知カラム(reply / mention 通知)で受信する → 同様。
3. カラムを閉じて開き直す(キャッシュ復元)→ 「↩ 返信」のまま。キャッシュ DB の読み取り専用コピー(`sqlite3 -readonly <db> ".backup '<copy>'"`)で、`reply` が元のまま保存されていること。
4. 設定でローカルNGにユーザーを追加して保存 → 表示中の返信先カードがその場で「↩ 返信」に戻る。

実機確認が環境の都合でできない場合は、できなかった項目と理由をユーザーに明示して報告する(成功と言わない)。

- [ ] **Step 4: コミット**

```bash
git add docs/guide/user-guide.md
git commit -m "docs: 返信先カードがミュート設定を考慮する旨をユーザーガイドに反映(Issue #486)"
```

- [ ] **Step 5: PR**

`git push -u origin feat/issue-486-reply-card-respects-mute` の後、`gh pr create`。PR 本文は `.github/pull_request_template.md` の構成に手で合わせ、`Fixes #486` を含める。末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)` を付ける。push 後に CI を Monitor で待たない(ユーザーが自分で見る)。マージは `gh pr merge --merge`(squash しない)で、ユーザーの指示があるまで行わない。
