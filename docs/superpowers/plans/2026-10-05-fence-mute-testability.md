# 世代フェンス・ミュート境界のテスト可能性とテスト追加 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** #456 の P3 として、世代フェンス・ミュート境界の「テストで固定できていない」5項目を、本番の挙動を変えずにテストで固定する。

**Architecture:** private 関数の切り出し(`apply_mute_config_with`、`update_column_core`、`close_column_core`、`open_stream_and_fetch` のジェネリック化)と、`client_for` の `#[cfg(test)]` の継ぎ目(`wiremock` 向けの API ベース)だけを足し、そのうえにテストを積む。実 DB のテストは、既存の `TestBackend`(Docker)を使い、`#[ignore]` にする。

**Tech Stack:** Rust(Tauri v2)、`tauri::test::mock_app`、`wiremock`、`tokio`、`testcontainers-modules`(Postgres / MySQL)。

**Spec:** `docs/superpowers/specs/2026-10-05-fence-mute-testability-design.md`(実行者は spec も読むこと)

## Global Constraints

- ブランチは `test/p3-testability-456`(作成済み)。`main` に直接コミットしない。
- コミットメッセージは**件名のみ**(本文なし)。末尾の `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` は別段落として付ける(`-m` を2回)。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら、リトライせず報告する(GPG 署名のタイムアウトが起きたことがある)。
- 本番の挙動を変えない。Tauri コマンドの署名、TS バインディング、`lib.rs` の `specta_builder()`、DB スキーマ、フロントエンドは変えない。`#[cfg(test)]` の分岐は `client_for` の1か所だけ。
- Rust のテストは `cd src-tauri && cargo test --lib <フィルタ>`。コンパイルが数分かかる。長いコマンドはバックグラウンドで流して出力をファイルに落とし、末尾だけ読む。ディスクの空きが少ない環境なので、`No space left on device` が出たら止めてユーザーに報告する(`target/` の削除は、ユーザーの承認を取ってから)。
- `pkill` / `killall` は使わない。ユーザーの実アプリ(`target/debug/tsumugi`、`cargo tauri dev`)のプロセスには触らない。`cargo tauri dev` は起動しない。
- 実 DB のテストは Docker が要る。実行後、`docker ps -a` で、自分が起動したコンテナが残っていないことを確認する(残っていれば ID を指定して `docker rm -fv <id>`。`-v` が必須)。
- clippy の警告数は変更前と同じ(基準: `cargo clippy --all-targets 2>&1 | grep -c "^warning"` が `23`)。増やさない。

## Review Focus

- 古い `Epoch` で `open_stream_and_fetch` を呼んだとき、`Err` を返すだけでなく、**ストリームを開かない・キャッシュにも境界にも書かない**こと(Task 3 のテスト)。
- 未知のカラムIDを `close_column_core` に渡しても、フェンスのエントリが残らず、`Err` にもならないこと(Task 3 のテスト)。
- `apply_mute_config_with` に渡したクロージャが失敗しても、設定の保存と `state.mute` の差し替えは済んでいて、`Ok` を返すこと(現状の挙動の固定。Task 1 のテスト)。
- 境界を捨てる DB が失敗したとき、同期自体は `Ok` で、メモリ上のミュート集合は新しい集合に差し替わっていること(Task 2 のテスト)。
- 実 DB で、書きロックが、読みロックを持つ実行中の書き込みを待ってから `clear_all_fetch_boundaries` を実行し、デッドロックしないこと(Task 4 のテスト)。

---

## File Structure

| ファイル | 責任 |
|---|---|
| `src-tauri/src/commands/mute.rs` | `apply_mute_config_with` の切り出しと、順序・DB 失敗・`sync_server_mutes` ラッパのテスト |
| `src-tauri/src/state.rs` | `#[cfg(test)]` の `test_api_base` と、`client_for` の上書き、`register_test_account` / `set_test_api_base` |
| `src-tauri/src/commands/column.rs` | `open_stream_and_fetch` / `open_streams_only` のジェネリック化、`update_column_core` / `close_column_core` の切り出し、配線のテスト |
| `src-tauri/src/store/postgres_backend.rs` | 実 DB の境界ロックのテスト(テストのみ) |
| `src-tauri/src/store/mysql_backend.rs` | 同上(テストのみ) |

## Task 1: `apply_mute_config` の順序を固定する

**Files:**
- Modify: `src-tauri/src/commands/mute.rs`(`apply_mute_config`、テスト)

**Interfaces:**
- Consumes: 既存の `SettingsStore::save_mute`、`AppState::mute`、`ColumnFence::invalidate_boundaries`、`NoteCacheStore::clear_all_fetch_boundaries`。
- Produces: `async fn apply_mute_config_with<F, Fut>(state: &AppState, config: MuteConfig, clear_boundaries: F) -> Result<()>`(`F: FnOnce() -> Fut`、`Fut: std::future::Future<Output = Result<()>>`)。`apply_mute_config(state, config)` の署名は変わらない。

- [ ] **Step 1: 失敗するテストを書く(`commands/mute.rs` の `mod tests` の末尾、`sync_of_another_account_is_not_blocked_by_a_held_sync_lock` の後ろ)**

```rust
    #[tokio::test]
    async fn apply_mute_config_replaces_state_mute_before_clearing_boundaries() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        let config = MuteConfig { ng_words: vec!["spoiler".into()], ..Default::default() };
        let seen_when_clearing = std::sync::Mutex::new(None);

        apply_mute_config_with(&state, config.clone(), || async {
            // 境界を捨てる時点で、新しい設定がすでに反映されていること(Issue #452)
            *seen_when_clearing.lock().unwrap() = Some(state.mute.lock().unwrap().clone());
            Ok(())
        })
        .await
        .unwrap();

        assert_eq!(seen_when_clearing.lock().unwrap().as_ref(), Some(&config));
        assert_eq!(state.settings.load_mute().unwrap(), config, "保存もされる");
    }

    #[tokio::test]
    async fn apply_mute_config_still_applies_and_saves_when_clearing_boundaries_fails() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        let config = MuteConfig { ng_words: vec!["spoiler".into()], ..Default::default() };

        let result = apply_mute_config_with(&state, config.clone(), || async { Err(Error::Invalid("db down".into())) }).await;

        assert!(result.is_ok(), "境界を捨てる処理の失敗は無視する(現状の挙動)");
        assert_eq!(*state.mute.lock().unwrap(), config);
        assert_eq!(state.settings.load_mute().unwrap(), config);
    }
```

- [ ] **Step 2: テストが、関数が無いために失敗することを確認する**

Run: `cd src-tauri && cargo test --lib apply_mute_config_ 2>&1 | tail -15`
Expected: コンパイルエラー `cannot find function \`apply_mute_config_with\` in this scope`。

- [ ] **Step 3: `apply_mute_config` を切り出す**

`commands/mute.rs` の `apply_mute_config`(doc コメントを含む。`async fn apply_mute_config(state: &AppState, config: MuteConfig) -> Result<()> { ... }`)を、次の2つの関数に置き換える。doc コメントは `apply_mute_config_with` に移し、`apply_mute_config` には1行の説明だけを付ける。

```rust
/// ミュート設定を保存して差し替え、全カラムの backfill 境界を捨てる(`set_mute` の本体)。
async fn apply_mute_config(state: &AppState, config: MuteConfig) -> Result<()> {
    apply_mute_config_with(state, config, || state.cache.clear_all_fetch_boundaries()).await
}

/// `apply_mute_config` の中核。境界を捨てる処理を `clear_boundaries` として受け取り、順序をテストで
/// 固定できるようにしている(Issue #456)。
///
/// `state.mute` の差し替えは、境界を捨てる**前**に行う。書き込み側は、ミュート設定を読む前に
/// 世代を控えるので、境界の世代が進んだ時点で、新しい設定がすでに反映されている(Issue #452)。
/// 境界を捨てる処理は、境界の書きロックの中で行う。実行中の取得が、旧ミュートの結果に基づく境界を
/// 直後に書き込んで復活させないため(`ColumnFence::invalidate_boundaries`)。
/// `clear_boundaries` の失敗は無視する(現状の挙動)。
async fn apply_mute_config_with<F, Fut>(state: &AppState, config: MuteConfig, clear_boundaries: F) -> Result<()>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    state.settings.save_mute(&config)?;
    *state.mute.lock().unwrap() = config;
    // ミュート解除方向の変更は、除外済み(=キャッシュされていない)ノートを読み直せないため
    // キャッシュ提供パスでは反映できない。境界を捨てて次回backfillをAPI経由に倒す(Issue #228)。
    state
        .column_fence
        .invalidate_boundaries(|| async {
            let _ = clear_boundaries().await;
        })
        .await;
    Ok(())
}
```

- [ ] **Step 4: テストを流して、通ることを確認する**

Run: `cd src-tauri && cargo test --lib commands::mute 2>&1 | tail -8`
Expected: `test result: ok.`(既存の `apply_mute_config_clears_boundaries_and_stales_earlier_boundary_epochs` と `apply_mute_config_replaces_the_mute_config` も通る)。

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/src/commands/mute.rs
git commit -m "test: apply_mute_configの、設定の差し替えが境界の破棄より先であることを固定する" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 6: 変異確認(コミット後。確認したら必ず元に戻す)**

`apply_mute_config_with` の `*state.mute.lock().unwrap() = config;` の1行を、`invalidate_boundaries(...).await;` の**後ろ**(`Ok(())` の直前)へ動かす。

Run: `cd src-tauri && cargo test --lib apply_mute_config_replaces_state_mute_before 2>&1 | tail -12`
Expected: `FAILED`(クロージャの中で、`state.mute` が古いまま)。

元に戻す: `git checkout -- src-tauri/src/commands/mute.rs`、`git status --short` が空であること。

## Task 2: DB 失敗時の保存値と、`sync_server_mutes` ラッパのテスト

**Files:**
- Modify: `src-tauri/src/state.rs`(`#[cfg(test)]` のフィールドとヘルパー、`client_for`)
- Modify: `src-tauri/src/commands/mute.rs`(テスト)

**Interfaces:**
- Consumes: Task 1 は使わない。既存の `sync_server_mutes_core`、`sync_server_mutes`、`state_with_three_columns_and_boundaries()`、`mount_server_mutes()`、`snap()`、`kept()`、`boundaries_of()`(`commands/mute.rs` のテストヘルパー)。
- Produces(Task 3 が使う):
  - `AppState` の `#[cfg(test)] pub(crate) test_api_base: Mutex<Option<String>>`
  - `#[cfg(test)] AppState::set_test_api_base(&self, base: String)`: 以降の `client_for` が、このベース(`wiremock` の `uri()`)へ向かうクライアントを返す
  - `#[cfg(test)] AppState::register_test_account(&self, account_id: &str)`: アカウント(host `misskey.test`)とトークンを登録し、`host_token` / `client_for` が通るようにする

- [ ] **Step 1: 失敗するテストを書く(`commands/mute.rs` の `mod tests` の末尾)**

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn sync_keeps_the_snapshot_when_discarding_boundaries_fails() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let mut state = state_with_three_columns_and_boundaries().await;
        // 境界のテーブルを失った DB に差し替える。`replace_fetch_boundaries` が実際の SQL エラーになる
        let conn = crate::store::db::open_cache_in_memory().unwrap();
        conn.execute("DROP TABLE column_source_boundary", []).unwrap();
        state.cache = crate::store::NoteCacheStore::new(crate::store::SqliteBackend::new(conn));
        state.settings.save_server_mute_snapshot("acc1", &snap(&["u1", "u2"], &[])).unwrap();
        mount_server_mutes(&mock, &["u1"], serde_json::json!([])).await; // u2 のミュートを解除

        let result = sync_server_mutes_core(&state, "acc1", &client).await;

        assert!(result.is_ok(), "境界を捨てられなくても、同期自体は成功する");
        assert_eq!(
            state.settings.load_server_mute_snapshot("acc1").unwrap(),
            Some(snap(&["u1", "u2"], &[])),
            "境界を捨てられなかったので、保存値は前回のまま(次回の同期で、解除を再検出する)"
        );
        assert!(state.is_server_muted("acc1", "u1"));
        assert!(!state.is_server_muted("acc1", "u2"), "メモリ上の集合は、新しい集合に差し替わっている");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_server_mutes_command_returns_an_error_for_an_unknown_account() {
        let app = tauri::test::mock_app();
        app.manage(AppState::new_for_test(SettingsStore::new_in_memory()));

        let result = sync_server_mutes(app.state::<AppState>(), "ghost".into()).await;

        assert!(matches!(result, Err(Error::Invalid(_))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_server_mutes_command_applies_the_server_mutes_through_the_state_extractor() {
        let mock = MockServer::start().await;
        mount_server_mutes(&mock, &["u1"], serde_json::json!(["spoiler"])).await;
        let app = tauri::test::mock_app();
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.register_test_account("acc1");
        state.set_test_api_base(mock.uri());
        app.manage(state);

        let result = sync_server_mutes(app.state::<AppState>(), "acc1".into()).await.unwrap();

        assert_eq!(result.blocked_users, 1);
        assert_eq!(result.word_rules, 1);
        assert!(app.state::<AppState>().is_server_muted("acc1", "u1"));
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib commands::mute::tests::sync_ 2>&1 | grep -E "^error" | sort | uniq -c`
Expected: コンパイルエラー `no method named \`register_test_account\`` と `no method named \`set_test_api_base\``(ラッパのハッピーパスのテストが使う)。
なお、`sync_keeps_the_snapshot_when_discarding_boundaries_fails` と、未知のアカウントのラッパのテストは、`reflect_server_mute_change` / `client_for` が元から正しく動くので、**コンパイルが通れば最初から通る**(既存の挙動を固定する回帰テスト)。これらが実際に回帰を捕まえることは、Step 7 の変異確認で確かめる。

- [ ] **Step 3: `state.rs` に継ぎ目を足す**

`AppState` の `server_mute_sync_locks` フィールドの直後に足す:

```rust
    /// テスト専用: `client_for` が返すクライアントの API ベースの上書き(`wiremock` の `uri()`)。
    /// 本番のクライアントは `https://{host}/api` 固定で、モック HTTP に向けられないため。
    #[cfg(test)]
    pub(crate) test_api_base: Mutex<Option<String>>,
```

`new_with_sound` の `Self { ... }` の `server_mute_sync_locks: Mutex::new(HashMap::new()),` の直後に足す:

```rust
            #[cfg(test)]
            test_api_base: Mutex::new(None),
```

`client_for` を次に置き換える:

```rust
    pub fn client_for(&self, account_id: &str) -> crate::error::Result<crate::api::MisskeyClient> {
        let (host, token) = self.host_token(account_id)?;
        #[cfg(test)]
        if let Some(base) = self.test_api_base.lock().unwrap().clone() {
            return Ok(crate::api::MisskeyClient::new_with_api_base(self.http.clone(), base, Some(token)));
        }
        Ok(crate::api::MisskeyClient::new(
            self.http.clone(),
            host,
            Some(token),
        ))
    }
```

`impl AppState` の中、`client_for` の直後に足す(`#[cfg(test)]` のメソッド):

```rust
    /// テスト用: 以降の `client_for` を、指定の API ベース(`wiremock` の `uri()`)へ向ける。
    #[cfg(test)]
    pub(crate) fn set_test_api_base(&self, base: String) {
        *self.test_api_base.lock().unwrap() = Some(base);
    }

    /// テスト用: アカウント(host `misskey.test`)とトークンを登録し、`host_token` / `client_for` が
    /// 通る状態にする。
    #[cfg(test)]
    pub(crate) fn register_test_account(&self, account_id: &str) {
        self.accounts.lock().unwrap().upsert(crate::domain::Account {
            id: account_id.into(),
            host: "misskey.test".into(),
            username: "me".into(),
            user_id: "u1".into(),
            display_name: "Me".into(),
            avatar_url: None,
            instance: None,
            is_cat: false,
            avatar_blurhash: None,
        });
        self.secrets.set(account_id, "token").unwrap();
    }
```

(`token` は `cfg(test)` の分岐で `return` するときだけムーブされるので、借用エラーにならない。もし出たら、分岐の中だけ `token.clone()` にする。)

- [ ] **Step 4: テストを流して、全件が通ることを確認する**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -8`
Expected: `test result: ok.`(追加3件を含む)。

- [ ] **Step 5: clippy の警告数を確認する**

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | grep -c "^warning"`
Expected: `23`。

- [ ] **Step 6: コミットする**

```bash
git add src-tauri/src/state.rs src-tauri/src/commands/mute.rs
git commit -m "test: 境界の破棄が失敗したときの保存値と、sync_server_mutesのラッパを固定する" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 7: 変異確認(コミット後。確認したら必ず元に戻す)**

`commands/mute.rs` の `reflect_server_mute_change` の `clear_account_boundaries` 失敗時の `return;` を、一時的に削除する(失敗しても保存値を更新してしまう)。

Run: `cd src-tauri && cargo test --lib sync_keeps_the_snapshot_when_discarding_boundaries_fails 2>&1 | tail -12`
Expected: `FAILED`(`保存値は前回のまま` のアサーション)。

元に戻す: `git checkout -- src-tauri/src/commands/mute.rs`、`git status --short` が空であること。

## Task 3: `update_column` / `close_column` / `open_stream_and_fetch` の配線を固定する

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(ジェネリック化、`*_core` の切り出し、テスト)

**Interfaces:**
- Consumes(Task 2): `AppState::set_test_api_base(&self, base: String)`、`AppState::register_test_account(&self, account_id: &str)`。既存: `ColumnFence::{begin, invalidate, write_if_current, tracks}`、`resolve_sources(state, account_id, &kind, &filter) -> Result<ResolvedSources>`、`NoteCacheStore::{replace_fetch_boundaries, get_fetch_boundaries}`、`ConnectionManager::open_count()`。
- Produces:
  - `async fn update_column_core<R: Runtime>(app: &AppHandle<R>, state: &AppState, column_id: String, kind: ColumnKind, filter: FilterQuery, title: Option<String>) -> Result<OpenedColumn>`
  - `async fn close_column_core(state: &AppState, column_id: &str) -> Result<()>`
  - `async fn open_stream_and_fetch<R: Runtime>(app: &AppHandle<R>, state: &AppState, column: &Column, resolved: Option<ResolvedSources>, host: String, token: String, epoch: &Epoch) -> Result<(Vec<Note>, Vec<Notification>)>`(`R` が追加。引数の並びは同じ)
  - `fn open_streams_only<R: Runtime>(app: &AppHandle<R>, ...)`(同上)

- [ ] **Step 1: 失敗するテストを書く(`commands/column.rs` の `mod tests` の末尾)**

`mod tests` の先頭の `use` に次を足す(重複しないように):

```rust
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};
```

テストを `mod tests` の末尾に足す:

```rust
    /// `acc1` を登録し、API を `mock` へ向け、グループ `g1` を持つ `AppState`。
    fn command_state(mock: &MockServer) -> AppState {
        let state = AppState::new_for_test(crate::store::SettingsStore::new_in_memory());
        state.register_test_account("acc1");
        state.set_test_api_base(mock.uri());
        state
            .settings
            .upsert_group(&ColumnGroup { id: "g1".into(), order: 0, width: 400, auto: false })
            .unwrap();
        state
    }

    fn command_column(id: &str, kind: ColumnKind) -> Column {
        Column {
            id: id.into(),
            account_id: "acc1".into(),
            kind,
            order: 0,
            filter: FilterQuery::Keywords(vec![]),
            notify_sound: false,
            notify_desktop: false,
            notify_sound_choice: String::new(),
            group_id: "g1".into(),
            title: None,
        }
    }

    /// すべての POST に、空のページを返す。
    async fn mount_empty_pages(mock: &MockServer) {
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(mock)
            .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn open_stream_and_fetch_returns_an_error_and_writes_nothing_when_the_epoch_is_stale() {
        let mock = MockServer::start().await;
        mount_empty_pages(&mock).await;
        let state = command_state(&mock);
        let column = command_column("c1", ColumnKind::Local);
        state.settings.upsert_column(&column).unwrap();
        let app = tauri::test::mock_app();
        let resolved = resolve_sources(&state, "acc1", &column.kind, &column.filter).await.unwrap();
        let (host, token) = state.host_token("acc1").unwrap();
        let stale = state.column_fence.begin("c1");
        state.column_fence.invalidate("c1", || async {}).await; // 取得中に update_column / close_column が走った

        let result = open_stream_and_fetch(app.handle(), &state, &column, Some(resolved), host, token, &stale).await;

        assert!(matches!(result, Err(Error::Invalid(_))), "古ければ、空の成功ではなく Err を返す");
        assert!(state.cache.get_fetch_boundaries("c1").await.unwrap().is_empty(), "境界を書かない");
        assert!(state.cache.load_cached("c1", 10).await.unwrap().is_empty(), "ノートを書かない");
        assert_eq!(state.connections.open_count(), 0, "ストリームを開かない");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn open_stream_and_fetch_succeeds_for_a_current_epoch() {
        let mock = MockServer::start().await;
        mount_empty_pages(&mock).await;
        let state = command_state(&mock);
        let column = command_column("c1", ColumnKind::Tag { tag: "foo".into() }); // ストリームを開かないソース
        state.settings.upsert_column(&column).unwrap();
        let app = tauri::test::mock_app();
        let resolved = resolve_sources(&state, "acc1", &column.kind, &column.filter).await.unwrap();
        let (host, token) = state.host_token("acc1").unwrap();
        let current = state.column_fence.begin("c1");

        let result = open_stream_and_fetch(app.handle(), &state, &column, Some(resolved), host, token, &current).await;

        let (notes, notifications) = result.unwrap();
        assert!(notes.is_empty() && notifications.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn update_column_core_saves_the_new_definition_clears_the_cache_and_stales_earlier_epochs() {
        let mock = MockServer::start().await;
        mount_empty_pages(&mock).await;
        let state = command_state(&mock);
        state.settings.upsert_column(&command_column("c1", ColumnKind::Home)).unwrap();
        state.cache.replace_fetch_boundaries("c1", &[(0, "n100".to_string())]).await.unwrap(); // 旧定義で貯めた境界
        let in_flight = state.column_fence.begin("c1"); // 旧定義で取得を始めた
        let app = tauri::test::mock_app();

        let opened = update_column_core(
            app.handle(),
            &state,
            "c1".into(),
            ColumnKind::Tag { tag: "foo".into() },
            FilterQuery::Keywords(vec![]),
            Some("新しい名前".into()),
        )
        .await
        .unwrap();

        assert_eq!(opened.column.kind, ColumnKind::Tag { tag: "foo".into() });
        let saved = state.settings.load_columns().unwrap();
        assert_eq!(saved[0].kind, ColumnKind::Tag { tag: "foo".into() });
        assert_eq!(saved[0].title.as_deref(), Some("新しい名前"));
        assert!(state.cache.get_fetch_boundaries("c1").await.unwrap().is_empty(), "旧定義の境界は消える");
        let late = state.column_fence.write_if_current("c1", &in_flight, |_| async {}).await;
        assert!(late.is_none(), "旧定義の取得の書き込みは捨てられる");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn update_column_core_returns_an_error_for_an_unknown_column() {
        let mock = MockServer::start().await;
        let state = command_state(&mock);
        let app = tauri::test::mock_app();

        let result = update_column_core(
            app.handle(),
            &state,
            "ghost".into(),
            ColumnKind::Home,
            FilterQuery::Keywords(vec![]),
            None,
        )
        .await;

        assert!(matches!(result, Err(Error::Invalid(_))));
        assert!(!state.column_fence.tracks("ghost"), "未知のカラムのエントリを作らない");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn close_column_core_removes_the_column_clears_the_cache_and_stales_earlier_epochs() {
        let mock = MockServer::start().await;
        let state = command_state(&mock);
        state.settings.upsert_column(&command_column("c1", ColumnKind::Home)).unwrap();
        state.cache.replace_fetch_boundaries("c1", &[(0, "n100".to_string())]).await.unwrap();
        let in_flight = state.column_fence.begin("c1");

        close_column_core(&state, "c1").await.unwrap();

        assert!(state.settings.load_columns().unwrap().is_empty());
        assert!(state.settings.load_groups().unwrap().is_empty(), "空になったグループも消える");
        assert!(state.cache.get_fetch_boundaries("c1").await.unwrap().is_empty());
        assert!(!state.column_fence.tracks("c1"), "フェンスのエントリが残らない");
        let late = state.column_fence.write_if_current("c1", &in_flight, |_| async {}).await;
        assert!(late.is_none(), "閉じる前の取得の書き込みは捨てられる");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn close_column_core_leaves_no_fence_entry_for_an_unknown_column() {
        let mock = MockServer::start().await;
        let state = command_state(&mock);

        close_column_core(&state, "ghost").await.unwrap();

        assert!(!state.column_fence.tracks("ghost"));
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib commands::column 2>&1 | grep -E "^error" | sort | uniq -c`
Expected: コンパイルエラー: `cannot find function \`update_column_core\``、`cannot find function \`close_column_core\``、`open_stream_and_fetch` に `&AppHandle<MockRuntime>` を渡せない(型の不一致 `expected \`&AppHandle\`, found \`&AppHandle<MockRuntime>\``)。

- [ ] **Step 3: `open_stream_and_fetch` / `open_streams_only` をジェネリックにする**

`commands/column.rs`:

- `async fn open_stream_and_fetch(` を `async fn open_stream_and_fetch<R: Runtime>(` に、引数 `app: &AppHandle,` を `app: &AppHandle<R>,` に変える。
- `fn open_streams_only(` を `fn open_streams_only<R: Runtime>(` に、引数 `app: &AppHandle,` を `app: &AppHandle<R>,` に変える。

(本体は変えない。`state.connections.open_channel` / `open_notifications` は、すでに `R: Runtime` でジェネリック。)

- [ ] **Step 4: `update_column` / `close_column` を切り出す**

`update_column` コマンド(`#[tauri::command] #[specta::specta] pub async fn update_column(...)`)の本体を、次の2つに置き換える。doc コメントは `update_column` に残す。

```rust
#[tauri::command]
#[specta::specta]
pub async fn update_column(
    app: AppHandle,
    state: State<'_, AppState>,
    column_id: String,
    kind: ColumnKind,
    filter: FilterQuery,
    title: Option<String>,
) -> Result<OpenedColumn> {
    update_column_core(&app, &state, column_id, kind, filter, title).await
}

/// `update_column` の本体。`AppHandle` / `State` を引数に取らない形(`R: Runtime` ジェネリック)にして、
/// `mock_app()` からテストできるようにしている(`sync_server_mutes_core` と同じ狙い。Issue #456)。
async fn update_column_core<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    column_id: String,
    kind: ColumnKind,
    filter: FilterQuery,
    title: Option<String>,
) -> Result<OpenedColumn> {
    let mut column = load_column(state, &column_id)?;
    let is_notif = matches!(kind, ColumnKind::Notifications);
    let resolved = if is_notif {
        None
    } else {
        Some(resolve_sources(state, &column.account_id, &kind, &filter).await?)
    };

    column.kind = kind;
    column.filter = filter;
    column.title = title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    // 新しい定義の保存・既存ストリームのクローズ・旧フィルタで貯めたキャッシュの破棄を、世代を進める
    // ロックの中で行う。実行中の取得は、世代が古くなって書き込みを捨てる(Issue #446)。
    let (epoch, cleared) = state
        .column_fence
        .invalidate(&column_id, || async {
            state.settings.upsert_column(&column)?;
            state.connections.close(&column_id);
            state.cache.clear_column_notes(&column_id).await
        })
        .await;
    cleared?;

    let group = state
        .settings
        .load_groups()?
        .into_iter()
        .find(|g| g.id == column.group_id)
        .ok_or_else(|| Error::Invalid(format!("unknown group: {}", column.group_id)))?;
    let (host, token) = state.host_token(&column.account_id)?;
    let (notes, notifications) =
        open_stream_and_fetch(app, state, &column, resolved, host, token, &epoch).await?;

    Ok(OpenedColumn {
        column,
        group,
        notes,
        notifications,
    })
}
```

`close_column` コマンドを、次の2つに置き換える。

```rust
#[tauri::command]
#[specta::specta]
pub async fn close_column(state: State<'_, AppState>, column_id: String) -> Result<()> {
    close_column_core(&state, &column_id).await
}

/// `close_column` の本体。`State` を引数に取らない形にして、テストできるようにしている(Issue #456)。
async fn close_column_core(state: &AppState, column_id: &str) -> Result<()> {
    state.settings.delete_column(column_id)?;
    // ストリームを閉じる処理と clear を、世代を進めるロックの中で行う。進行中の open_stream_and_fetch は、
    // ストリームを開く処理も同じロックの中なので、閉じた後に開き直すことも、書き込みを残すことも無い
    // (孤児データとリークしたストリームを作らない, Issue #446)。
    let (_, cleared) = state
        .column_fence
        .invalidate(column_id, || async {
            state.connections.close(column_id);
            state.cache.clear_column_notes(column_id).await
        })
        .await;
    state.column_fence.remove(column_id);
    cleared?;
    state.settings.delete_empty_groups()?;
    Ok(())
}
```

(切り出し前のコードと本体が一致していること: `git diff` で、`&state` → `state`、`&column_id` → `column_id` の機械的な置換だけであることを確認する。`update_column_core` の `load_column` が `Err` を返すとき、`update_column` は `begin` を呼ばず、フェンスのエントリを作らない。)

- [ ] **Step 5: テストを流して、全件が通ることを確認する**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -8`
Expected: `test result: ok.`(追加6件を含む)。失敗した場合は、失敗の理由が、テストの前提の誤り(例: `Tag` で `resolve_sources` が `Err`、`wiremock` のレスポンスの形)か、本番コードの誤りかを `superpowers:systematic-debugging` で切り分ける。`Tag` ソースの成功経路が組めない場合は、`open_stream_and_fetch_succeeds_for_a_current_epoch` と、`update_column_core_saves_the_new_definition_...` の `Tag` を使う部分を外し、失敗・古い場合のテストだけにする。その判断は台帳に `Ruling:` として残す(spec の「リスク」参照)。

- [ ] **Step 6: clippy の警告数を確認する**

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | grep -c "^warning"`
Expected: `23`。

- [ ] **Step 7: コミットする**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "test: update_column・close_column・open_stream_and_fetchの古い場合の分岐を固定する" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 8: 変異確認(コミット後。各変異のあと必ず元に戻す)**

1. `open_stream_and_fetch` の中の `commit_initial_writes(... epoch, ...)` の `epoch` を、`&state.column_fence.begin(&column.id)` に一時的に置き換える(古さを無視する)。Run: `cd src-tauri && cargo test --lib open_stream_and_fetch_returns_an_error 2>&1 | tail -12`。Expected: `FAILED`(`Err` ではなく `Ok`)。`git checkout -- src-tauri/src/commands/column.rs` で戻す。
2. `close_column_core` の `state.column_fence.remove(column_id);` を一時的に削除する。Run: `cd src-tauri && cargo test --lib close_column_core 2>&1 | tail -12`。Expected: `FAILED`(`フェンスのエントリが残らない`)。`git checkout -- src-tauri/src/commands/column.rs` で戻す。

`git status --short` が空であること。

## Task 4: 実 DB(Postgres / MySQL)での、境界の書きロックの中の `clear_all_fetch_boundaries`

**Files:**
- Modify: `src-tauri/src/store/postgres_backend.rs`(`mod tests` の末尾にテストを足す)
- Modify: `src-tauri/src/store/mysql_backend.rs`(同上)

**Interfaces:**
- Consumes: 各ファイルの `mod tests` にある `backend() -> TestBackend`、`b(idx, id)`、`set0(&Backend, column_id, id)`(`TestBackend` は `Deref<Target = PostgresBackend / MySqlBackend>`)。`crate::fence::ColumnFence`。
- Produces: なし(テストのみ)。

- [ ] **Step 1: Postgres のテストを書く(`postgres_backend.rs` の `mod tests` の末尾)**

```rust
    /// 境界の書きロックの中で `clear_all_fetch_boundaries` を実行しても、実DBで詰まらず、全カラムの境界が
    /// 空になり、古い世代の書き込みが境界を復活させない(Issue #456。#453 の最終レビュー指摘)。
    #[tokio::test]
    #[ignore]
    async fn clear_all_fetch_boundaries_inside_the_boundary_write_lock_completes_and_blocks_stale_writes() {
        use crate::fence::ColumnFence;
        use std::sync::Arc;
        use std::time::Duration;

        let s = Arc::new(backend().await);
        let fence = Arc::new(ColumnFence::default());
        let columns = ["c1", "c2", "c3", "c4"];
        for c in columns {
            set0(&s, c, "n500").await;
        }
        // 読みロックを持ったまま DB に書く、実行中の取得。書きロックを待たせるために、書く前に少し待つ
        let mut writers = Vec::new();
        for c in columns {
            let epoch = fence.begin(c);
            let (s, fence) = (Arc::clone(&s), Arc::clone(&fence));
            writers.push(tokio::spawn(async move {
                fence
                    .write_if_current(c, &epoch, |boundaries_ok| async move {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        if boundaries_ok {
                            s.extend_fetch_boundaries(c, &[b(0, "n100")]).await.unwrap();
                        }
                    })
                    .await
            }));
        }
        let late_epoch = fence.begin("c1"); // invalidate_boundaries より前に控える

        let cleared = tokio::time::timeout(
            Duration::from_secs(30), // デッドロック検出の上限。正常なら、ほぼ即座に終わる
            fence.invalidate_boundaries(|| async { s.clear_all_fetch_boundaries().await }),
        )
        .await;

        assert!(cleared.expect("書きロックの中の clear_all_fetch_boundaries が詰まらない").is_ok());
        for writer in writers {
            writer.await.unwrap();
        }
        for c in columns {
            assert!(s.get_fetch_boundaries(c).await.unwrap().is_empty(), "{c} の境界が空になる");
        }
        let late = fence.write_if_current("c1", &late_epoch, |boundaries_ok| async move { boundaries_ok }).await;
        assert_eq!(late, Some(false), "invalidate_boundaries より前に控えた世代の境界の書き込みは、古い扱い");
    }
```

- [ ] **Step 2: Postgres のテストを実行して、通ることを確認する**

Run: `cd src-tauri && cargo test --lib clear_all_fetch_boundaries_inside_the_boundary_write_lock -- --ignored 2>&1 | tail -12`(`postgres_backend` と `mysql_backend` の両方に同名のテストが入るのは Step 3 の後。この時点では Postgres のみ)
Expected: `test result: ok. 1 passed`。Docker が起動していて、`postgres:11-alpine`(`testcontainers-modules` の既定のタグ)がローカルにあること。pull が走る場合は、ディスクの空きを確認してから続ける(`df -h /`)。
実行後: `docker ps -a --format '{{.ID}} {{.Image}} {{.Status}}'` で、`postgres` のコンテナが残っていないことを確認する。

- [ ] **Step 3: MySQL のテストを書く(`mysql_backend.rs` の `mod tests` の末尾)**

Step 1 のテストと同じコードを足す(別ファイルの別モジュールなので、関数名も同じでよい。`backend()` / `set0` / `b` は、そのファイルの `mod tests` のもので、型は `Deref` で `MySqlBackend` に解決される)。

```rust
    /// 境界の書きロックの中で `clear_all_fetch_boundaries` を実行しても、実DBで詰まらず、全カラムの境界が
    /// 空になり、古い世代の書き込みが境界を復活させない(Issue #456。#453 の最終レビュー指摘)。
    #[tokio::test]
    #[ignore]
    async fn clear_all_fetch_boundaries_inside_the_boundary_write_lock_completes_and_blocks_stale_writes() {
        use crate::fence::ColumnFence;
        use std::sync::Arc;
        use std::time::Duration;

        let s = Arc::new(backend().await);
        let fence = Arc::new(ColumnFence::default());
        let columns = ["c1", "c2", "c3", "c4"];
        for c in columns {
            set0(&s, c, "n500").await;
        }
        let mut writers = Vec::new();
        for c in columns {
            let epoch = fence.begin(c);
            let (s, fence) = (Arc::clone(&s), Arc::clone(&fence));
            writers.push(tokio::spawn(async move {
                fence
                    .write_if_current(c, &epoch, |boundaries_ok| async move {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        if boundaries_ok {
                            s.extend_fetch_boundaries(c, &[b(0, "n100")]).await.unwrap();
                        }
                    })
                    .await
            }));
        }
        let late_epoch = fence.begin("c1");

        let cleared = tokio::time::timeout(
            Duration::from_secs(30),
            fence.invalidate_boundaries(|| async { s.clear_all_fetch_boundaries().await }),
        )
        .await;

        assert!(cleared.expect("書きロックの中の clear_all_fetch_boundaries が詰まらない").is_ok());
        for writer in writers {
            writer.await.unwrap();
        }
        for c in columns {
            assert!(s.get_fetch_boundaries(c).await.unwrap().is_empty(), "{c} の境界が空になる");
        }
        let late = fence.write_if_current("c1", &late_epoch, |boundaries_ok| async move { boundaries_ok }).await;
        assert_eq!(late, Some(false), "invalidate_boundaries より前に控えた世代の境界の書き込みは、古い扱い");
    }
```

- [ ] **Step 4: MySQL のテストを実行して、通ることを確認する**

Run: `cd src-tauri && cargo test --lib mysql_backend::tests::clear_all_fetch_boundaries_inside_the_boundary_write_lock -- --ignored --test-threads=2 2>&1 | tail -12`
Expected: `test result: ok. 1 passed`(`mysql:8.1` がローカルにあること)。
実行後: `docker ps -a --format '{{.ID}} {{.Image}} {{.Status}}'` で、`mysql` のコンテナが残っていないことを確認する。残っていれば `docker rm -fv <id>`。`docker volume ls -f dangling=true` に、このテストが作った匿名ボリュームが残っていないことも確認する。

- [ ] **Step 5: 通常の `cargo test --lib` に影響が無いことを確認する**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -4`
Expected: `test result: ok.`、`ignored` が増える(2件)以外は変わらない。

- [ ] **Step 6: コミットする**

```bash
git add src-tauri/src/store/postgres_backend.rs src-tauri/src/store/mysql_backend.rs
git commit -m "test: 実DBで境界の書きロックの中のclear_all_fetch_boundariesが詰まらないことを確認する" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 7: 変異確認(コミット後。確認したら必ず元に戻す)**

`src-tauri/src/fence.rs` の `invalidate_boundaries` の `let _write = self.boundary_lock.write().await;` の行を、一時的に削除する(書きロックを取らない)。

Run: `cd src-tauri && cargo test --lib postgres_backend::tests::clear_all_fetch_boundaries_inside_the_boundary_write_lock -- --ignored 2>&1 | tail -12`
Expected: `FAILED`(書きロックが無いと、読みロックを持つ書き込みの完了を待たずに clear が走り、後から入った `extend_fetch_boundaries` が境界を復活させて、`の境界が空になる` が落ちる。タイミング次第で通る場合は、`sleep` を長くして再実行する。それでも通る場合は、変異確認が成立しないので、台帳に `Ruling:` として残し、テストの待ち時間を調整する)。

元に戻す: `git checkout -- src-tauri/src/fence.rs`、`git status --short` が空であること。実行後、`docker ps -a` でコンテナが残っていないことを確認する。

## 完了後(実行者が行う。ユーザーの承認を取ってから)

- 全体の確認: `cd src-tauri && cargo test 2>&1 | grep -E "^test result|FAILED"` と、`git status --short` で `frontend/src/bindings/tauri.gen.ts` に差分が出ていないこと。
- push して PR を作る。本文は `.github/pull_request_template.md` の構成に沿い、関連 Issue は `Refs #456`(自動クローズさせない)。検証欄には、実機・実 UI の確認をしていない旨、実 DB のテストを手元の Docker で実行した結果を書く。
- マージ後、#456 の対応済みの4項目(`update_column` / `close_column` / `open_stream_and_fetch` の分岐テスト、`apply_mute_config` の順序、実 DB の `clear_all_fetch_boundaries`、境界の破棄失敗時の保存値と `sync_server_mutes` のラッパ)にチェックを入れる。
