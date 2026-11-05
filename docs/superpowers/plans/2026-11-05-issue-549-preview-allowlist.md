# URLプレビュー専用の許可リスト Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 手入力したURLプレビューの取得先のホストが、メディア取得(`fetch_url_bytes` / `save_url_to_file`)の許可リストに入らないようにする。

**Architecture:** `AppState` に `preview_http` と `preview_allowlist` を足す。`fetch_url_preview` だけがこれを使う。`fetch_http` / `fetch_allowlist` は、#549 の前と同じ「トークンを持つアカウントのホストだけ」に戻す。`refresh_fetch_allowlist` が2つを同時に差し替える。

**Tech Stack:** Rust(`reqwest`、`wiremock`)。

**Spec:** `docs/superpowers/specs/2026-11-05-issue-549-external-url-validation-design.md` の「改訂(レビュー指摘への対応)」の「プレビュー専用の許可リスト」(承認済み)。元の計画は `docs/superpowers/plans/2026-11-05-issue-549-external-url-validation.md`。

## Global Constraints

- ブランチは `fix/issue-549-external-url-validation`。`main` では編集しない。
- コミットメッセージは件名1行のみ。末尾にトレーラー `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` を別段落で付ける。形式は `type(scope): 件名 (#549)`。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら止めて報告する(再試行しない)。
- `cargo` は `CARGO_TARGET_DIR=/home/onodai145/.claude/jobs/20a18c17/tmp/target` と `-j2` で、`vitest` と同時に走らせない(OOM対策。あなたの `cargo tauri dev` がビルドロックを持つため、`target` を分ける)。
- 手入力した取得先は宛先を制限しない。インポート由来の取得先は取得時に `net_guard` で制限する(変えない)。
- `fetch_http` / `fetch_allowlist` に入るのは、トークンを持つアカウントのホストだけ(Issue #544、#549 の前の状態)。
- TSバインディングと設定ファイルの形式は変えない。

## Review Focus

- 手入力した取得先のホストが、メディア取得(`fetch_guarded_bytes` が使う `fetch_http` / `fetch_allowlist`)で通らない(Task 1 のテスト)。
- 取得先が空のときの既定経路(`https://{アカウントのホスト}/url`)が、`preview_allowlist` にアカウントのホストが入っているので、今までどおり通る。トークンのないアカウントのホストは入らない(Task 1 のテスト)。
- アカウントの削除・再認証、取得先の変更・信頼・インポートのどれでも、2つの許可リストが同時に更新される(Task 1 のテスト)。
- インポート由来の取得先は、`preview_allowlist` にも入らない(Task 1 のテスト)。
- ポートを書いた取得先は、そのポートだけが `preview_allowlist` で許可される(Task 1 のテスト)。

---

### Task 1: プレビュー専用の許可リストとクライアント

**Files:**
- Modify: `src-tauri/src/state.rs`(フィールド、構築、`refresh_fetch_allowlist`、`allowlist_hosts`、`mod tests`)
- Modify: `src-tauri/src/commands/note.rs`(`fetch_url_preview_core`、`mod tests`)
- Modify: `CLAUDE.md`(`net_guard.rs` の項)

**Interfaces:**
- Consumes: `net_guard::build_fetch_client(user_agent: &str, allow: FetchAllowlist) -> reqwest::Result<reqwest::Client>`、`FetchAllowlist::{default, replace, allows, contains_host}`、`SettingsStore::trusted_summaly_allowlist_entry(&self) -> Option<String>`、`api::url_preview::fetch_url_preview(http, allow, proxy_base, target_url)`。
- Produces:
  - `AppState.preview_http: reqwest::Client`(`pub`)
  - `AppState.preview_allowlist: crate::net_guard::FetchAllowlist`(`pub`)
  - `AppState::refresh_fetch_allowlist(&self)` が `fetch_allowlist` と `preview_allowlist` の両方を差し替える(名前と引数は変えない)。
  - `fetch_url_preview_core` は `preview_http` / `preview_allowlist` を使う(シグネチャは変えない)。

- [ ] **Step 1: 失敗するテストを書く(`state.rs`)**

`src-tauri/src/state.rs` の `mod tests` で、既存の2つのテスト `fetch_allowlist_includes_the_summaly_host_only_when_the_user_saved_it` と `fetch_allowlist_has_no_summaly_host_when_the_url_is_empty` を、次の4つに置き換える。

```rust
    #[test]
    fn preview_allowlist_includes_the_summaly_host_only_when_the_user_saved_it() {
        let settings = SettingsStore::new_in_memory();
        let mut ui = crate::domain::UiPrefs::default();
        ui.summaly_proxy_url = "http://192.168.1.10:3030/url".into();
        settings.save_ui(&ui).unwrap();
        let state = AppState::new_for_test(settings);
        assert!(state.preview_allowlist.allows("192.168.1.10", 3030), "起動時の初期値に入る");
        assert!(!state.preview_allowlist.allows("192.168.1.10", 80), "ポートは明示されたものだけ");

        // インポート直後相当: 許可リストから外れる
        let mut data = state.settings.snapshot();
        data.summaly_proxy_imported = true;
        state.settings.replace(data).unwrap();
        state.refresh_fetch_allowlist();
        assert!(!state.preview_allowlist.contains_host("192.168.1.10"));

        // 設定画面で保存し直す(信頼する)と入る
        state.settings.trust_summaly_proxy("http://192.168.1.10:3030/url").unwrap();
        state.refresh_fetch_allowlist();
        assert!(state.preview_allowlist.allows("192.168.1.10", 3030));
    }

    #[test]
    fn the_summaly_host_never_enters_the_media_fetch_allowlist() {
        let settings = SettingsStore::new_in_memory();
        let mut ui = crate::domain::UiPrefs::default();
        ui.summaly_proxy_url = "http://proxy.example.com:3030/url".into();
        settings.save_ui(&ui).unwrap();
        let state = AppState::new_for_test(settings);
        assert!(state.preview_allowlist.contains_host("proxy.example.com"));
        assert!(!state.fetch_allowlist.contains_host("proxy.example.com"), "起動時");

        state.refresh_fetch_allowlist();
        assert!(!state.fetch_allowlist.contains_host("proxy.example.com"), "更新後");

        state.settings.trust_summaly_proxy("http://proxy.example.com:3030/url").unwrap();
        state.refresh_fetch_allowlist();
        assert!(!state.fetch_allowlist.contains_host("proxy.example.com"), "信頼後");
    }

    #[test]
    fn preview_allowlist_keeps_account_hosts_with_token_and_skips_those_without() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.register_test_account("acc1"); // host=misskey.test、トークンあり
        state.accounts.lock().unwrap().upsert(test_account("acc2", "internal.example")); // トークンなし
        state.refresh_fetch_allowlist();

        // 取得先が空のときの既定経路(`https://{host}/url`)に必要
        assert!(state.preview_allowlist.contains_host("misskey.test"));
        assert!(!state.preview_allowlist.contains_host("internal.example"));

        state.accounts.lock().unwrap().remove("acc1").unwrap();
        state.refresh_fetch_allowlist();
        assert!(!state.preview_allowlist.contains_host("misskey.test"), "削除したら、2つとも外れる");
        assert!(!state.fetch_allowlist.contains_host("misskey.test"));
    }

    #[test]
    fn allowlists_have_no_summaly_host_when_the_url_is_empty() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.refresh_fetch_allowlist();
        assert!(!state.preview_allowlist.contains_host(""));
        assert!(!state.fetch_allowlist.contains_host(""));
    }
```

- [ ] **Step 2: 失敗するテストを書く(`commands/note.rs`)**

`src-tauri/src/commands/note.rs` の `mod tests` の `fetch_url_preview_reaches_a_user_saved_loopback_proxy` の直後に追加する。

```rust
    #[tokio::test]
    async fn a_user_saved_proxy_host_is_not_reachable_by_the_media_fetch() {
        let server = preview_server().await;
        let proxy = format!("http://127.0.0.1:{}/url", server.address().port());
        let state = state_with_summaly(&proxy, false);

        // URLプレビューでは届く
        assert!(fetch_url_preview_core(&state, "acc", "https://example.com/").await.is_ok());

        // メディア取得(`save_url_to_file` / `fetch_url_bytes` が使うクライアントと許可リスト)では、同じホストでも拒否される
        let before = server.received_requests().await.unwrap().len();
        let err = fetch_guarded_bytes(&state.fetch_http, &state.fetch_allowlist, &proxy).await.unwrap_err();
        assert!(matches!(err, Error::Invalid(ref m) if m.contains("許可されていません")), "unexpected: {err:?}");
        assert_eq!(server.received_requests().await.unwrap().len(), before, "メディア取得ではリクエストが出ない");
    }
```

- [ ] **Step 3: 失敗を確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && CARGO_TARGET_DIR=/home/onodai145/.claude/jobs/20a18c17/tmp/target cargo test -j2 --lib state:: commands::note 2>&1 | grep -E "^error|panicked|FAILED|test result"`
Expected: `preview_allowlist` のフィールドが無いためコンパイルエラー(state のテスト)。

- [ ] **Step 4: 実装する(`state.rs`)**

`AppState` の `fetch_allowlist` フィールドの直後に追加する。

```rust
    /// URLプレビュー(`fetch_url_preview`)専用の取得クライアント(Issue #549)。`fetch_http` と同じく
    /// 禁止IPへの接続を `net_guard` が拒否するが、許可リストは `preview_allowlist` を使う。
    pub preview_http: reqwest::Client,
    /// `preview_http` が例外として許可するホスト: 登録アカウントのホスト(取得先が空のときの
    /// `https://{host}/url` に必要)と、利用者が設定した取得先。メディア取得の `fetch_allowlist` には
    /// 取得先を入れない(手入力した取得先のホストで、他の取得の保護まで弱めないため)。
    pub preview_allowlist: crate::net_guard::FetchAllowlist,
```

`fetch_allowlist` の `///` コメント(`/// \`fetch_http\` が例外として許可するホスト(登録アカウントのホスト)。…`)はそのまま残す。

`new_with_sound` の構築を次に置き換える。

```rust
        let fetch_allowlist = crate::net_guard::FetchAllowlist::default();
        fetch_allowlist.replace(hosts_with_token(&accounts, secrets.as_ref()));
        let preview_allowlist = crate::net_guard::FetchAllowlist::default();
        preview_allowlist.replace(preview_allowlist_hosts(&accounts, secrets.as_ref(), &settings));
        Self {
            http: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .expect("failed to build reqwest client"),
            fetch_http: crate::net_guard::build_fetch_client(USER_AGENT, fetch_allowlist.clone())
                .expect("failed to build fetch client"),
            fetch_allowlist,
            preview_http: crate::net_guard::build_fetch_client(USER_AGENT, preview_allowlist.clone())
                .expect("failed to build preview client"),
            preview_allowlist,
```

`refresh_fetch_allowlist` とそのコメントを次に置き換える。

```rust
    /// 取得先の許可リストを差し替える。`fetch_allowlist` は、トークンを持つアカウントの全ホスト。
    /// `preview_allowlist` は、それに加えて利用者が設定したURLプレビューの取得先(Issue #549)。
    /// アカウントの追加・削除・再認証の直後と、取得先を変えた・信頼した・インポートした直後に呼ぶ。
    pub fn refresh_fetch_allowlist(&self) {
        let accounts = self.accounts.lock().unwrap().list();
        self.fetch_allowlist.replace(hosts_with_token(&accounts, self.secrets.as_ref()));
        self.preview_allowlist
            .replace(preview_allowlist_hosts(&accounts, self.secrets.as_ref(), &self.settings));
    }
```

`allowlist_hosts` を次に置き換える(関数名も変える)。

```rust
/// URLプレビュー専用の許可リストに入れるホストの全体: トークンを持つアカウントのホストと、利用者が設定した
/// 取得先(Issue #549。インポート由来の取得先は入れない)。
fn preview_allowlist_hosts(accounts: &[Account], secrets: &dyn SecretStore, settings: &SettingsStore) -> Vec<String> {
    let mut hosts = hosts_with_token(accounts, secrets);
    hosts.extend(settings.trusted_summaly_allowlist_entry());
    hosts
}
```

- [ ] **Step 5: 実装する(`commands/note.rs`)**

`fetch_url_preview_core` の取得の行を置き換える。

```rust
    let result = fetch_url_preview_api(&state.preview_http, &state.preview_allowlist, &proxy_base, url).await;
```

`fetch_url_preview` コマンドのドキュメントコメントの「取得は `fetch_http`(`net_guard` 付き)で行い、…許可リストに入る(Issue #549)。」を次に置き換える。

```rust
/// 取得は `preview_http`(`net_guard` 付き)で行い、利用者が設定画面で保存した取得先のホストだけが
/// `preview_allowlist` に入る。メディア取得の許可リスト(`fetch_allowlist`)には入らない(Issue #549)。
```

- [ ] **Step 6: 通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && CARGO_TARGET_DIR=/home/onodai145/.claude/jobs/20a18c17/tmp/target cargo test -j2 --lib 2>&1 | grep -E "^error|panicked|FAILED|test result"`
Expected: すべて PASS(失敗0)。`generates_frontend_bindings` の差分(`tauri.gen.ts`)が出ないことも `git status --short` で確認する。

- [ ] **Step 7: `CLAUDE.md` を更新する**

`net_guard.rs` の項の、`fetch_url_preview` の文(`` `fetch_url_preview` (URL previews, Issue #549) also goes through `fetch_http`: … ``)を、次に置き換える。

```
`fetch_url_preview` (URL previews, Issue #549) uses its own `AppState.preview_http` / `preview_allowlist` (same guard, separate allowlist): `preview_allowlist` = hosts of accounts with a token + the user-saved summaly proxy host (via `SettingsStore::trusted_summaly_allowlist_entry`), but only while `SettingsData.summaly_proxy_imported` is false — a proxy that arrived via settings import stays blocked from private addresses until the user re-saves it in Settings → External integration (`trust_summaly_proxy(expected_url)`). The summaly host never enters `fetch_allowlist`, so it can't weaken the media-fetch guard; `refresh_fetch_allowlist()` updates both lists.
```

- [ ] **Step 8: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/state.rs src-tauri/src/commands/note.rs CLAUDE.md
git commit -m "fix(net_guard): URLプレビューの取得先をメディア取得の許可リストから分ける (#549)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
