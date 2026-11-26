# アカウント削除の部分失敗を防ぐ Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** アカウント削除がエラーを返すなら何も変わらず、確定した後はエラーを返さないようにする(Issue #694)。

**Architecture:** 確定点を `SettingsStore::delete_account` の1か所にする。トークンの削除はその前に置き(失敗時は書き戻す)、ストリーム停止などの後始末はその後に置く。`delete_account` は、保存に成功してからメモリを差し替える。

**Tech Stack:** Rust(Tauri v2、tokio、`cargo test`)

**Spec:** `docs/superpowers/specs/2026-11-25-issue-694-account-removal-partial-failure-design.md`

## Global Constraints

- ブランチは `fix/issue-694-account-removal-partial-failure`(`origin/main` の `6db690e` から分岐済み)。
- 公開シグネチャは変えない: `remove_account`、`logout`、`SettingsStore::delete_account`。
- `#[tauri::command]` の `///` コメントは変えない(生成される `frontend/src/bindings/tauri.gen.ts` に出るため)。
- `store/scheduled_post.rs` は変えない(PR #695 との衝突を避ける)。
- フロントエンドは変えない。
- ログに、トークンとアカウントIDを含めない。
- コミットメッセージは件名1行のみ。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` を付ける。`--no-verify` は使わない。コミットが失敗したら、再試行せずに報告する。
- ビルド前に `free -h` で空きとスワップを確認する。`cargo` は `src-tauri/` で、次の形で実行する(llama.cpp のビルドが OOM を起こすため)。出力先を `/tmp` にしない。

```bash
CARGO_BUILD_JOBS=4 systemd-run --user --scope -p MemoryMax=10G -p MemoryHigh=8G cargo test --lib <filter>
```

## Review Focus

- トークンの無いアカウント(設定インポート直後): 削除できる。設定の保存に失敗しても、空のトークンを作らない(Task 2)。
- トークンの書き戻しにも失敗: keyring のエラーではなく、元の設定保存のエラーを返す(Task 2)。
- keyring がロックされていて読めない: 何も変えずにエラーを返す(Task 2)。
- アカウントが複数: 別のアカウントのカラム、トークン、予約、ストリームは変わらない(Task 2)。
- 削除したアカウントだけが使っていたグループ: 成功時に消える(Task 2)。

## File Structure

| ファイル | 役割 |
|---|---|
| `src-tauri/src/store/testutil.rs`(新規) | 保存の失敗を起こすテスト用の `BrokenDisk` |
| `src-tauri/src/store/mod.rs` | `testutil` の登録 |
| `src-tauri/src/store/settings.rs` | `delete_account` を「保存してから差し替え」にする |
| `src-tauri/src/commands/account.rs` | `remove_account_core` と削除の順序 |
| `CLAUDE.md` | 削除の順序の決まり |

---

### Task 1: `delete_account` は保存に失敗したらメモリを変えない

**Files:**
- Create: `src-tauri/src/store/testutil.rs`
- Modify: `src-tauri/src/store/mod.rs`
- Modify: `src-tauri/src/store/settings.rs`(`delete_account` と、その単体テスト)

**Interfaces:**
- Produces: `crate::store::testutil::BrokenDisk`(`new() -> Self`、`path(&self, name: &str) -> PathBuf`、`break_dir(&self)`、`repair_dir(&self)`)。Task 2 が使う。
- Produces: `SettingsStore::delete_account(&self, account_id: &str) -> Result<()>` は、`Err` のときメモリを変えない。

- [ ] **Step 1: テスト用の `BrokenDisk` を作る**

`src-tauri/src/store/testutil.rs`:

```rust
//! 永続化のテスト用の補助。

use std::path::PathBuf;

/// 保存先のディレクトリを消して、保存を失敗させる(`repair_dir` で直る)。
pub(crate) struct BrokenDisk {
    dir: PathBuf,
}

impl BrokenDisk {
    pub(crate) fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("tsumugi-broken-disk-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    pub(crate) fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub(crate) fn break_dir(&self) {
        std::fs::remove_dir_all(&self.dir).unwrap();
    }

    pub(crate) fn repair_dir(&self) {
        std::fs::create_dir_all(&self.dir).unwrap();
    }
}

impl Drop for BrokenDisk {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
```

`src-tauri/src/store/mod.rs` の `pub(crate) mod search_contract;` の直後に追加する。

```rust
#[cfg(test)]
pub(crate) mod testutil;
```

- [ ] **Step 2: 失敗するテストを書く**

`store/settings.rs` の、`delete_account_cascades_columns` があるテストモジュールの、そのテストの直後に追加する。

```rust
    /// Issue #694: 保存に失敗した削除は、メモリも変えない。
    #[test]
    fn delete_account_keeps_memory_when_saving_fails() {
        let disk = crate::store::testutil::BrokenDisk::new();
        let s = SettingsStore::new(disk.path("settings.json"), disk.path("backgrounds")).unwrap();
        s.upsert_account(&account("a1")).unwrap();
        s.upsert_column(&column("c1", "a1", 0)).unwrap();
        s.save_server_mute_snapshot("a1", &snap(&["u1"], &[])).unwrap();
        s.update_mcp(|m| m.exposed_account_ids = vec!["a1".into()]).unwrap();

        disk.break_dir();
        assert!(s.delete_account("a1").is_err());

        assert_eq!(s.load_accounts().unwrap().len(), 1);
        assert_eq!(s.load_columns().unwrap().len(), 1);
        assert!(s.load_server_mute_snapshot("a1").unwrap().is_some());
        assert_eq!(s.load_mcp().exposed_account_ids, vec!["a1".to_string()]);

        disk.repair_dir();
        s.delete_account("a1").unwrap();
        assert!(s.load_accounts().unwrap().is_empty());
        assert!(s.load_columns().unwrap().is_empty());
    }
```

- [ ] **Step 3: 失敗を確認する**

Run: `cargo test --lib delete_account`(Global Constraints の形で)
Expected: `delete_account_keeps_memory_when_saving_fails` が、`load_accounts().unwrap().len()` の `left: 0, right: 1` で FAIL。ほかの `delete_account_*` は PASS。

- [ ] **Step 4: 実装する**

`delete_account` を次に置き換える。

```rust
    /// アカウント削除。紐づくカラムも消す。保存に成功してからメモリを差し替えるので、
    /// 失敗したときは何も変わらない(アカウント削除の確定点。Issue #694)。
    pub fn delete_account(&self, account_id: &str) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        let mut next = guard.clone();
        next.accounts.retain(|a| a.id != account_id);
        next.columns.retain(|c| c.account_id != account_id);
        next.server_mute_snapshots.remove(account_id);
        next.mcp.exposed_account_ids.retain(|id| id != account_id);
        self.save(&next)?;
        *guard = next;
        Ok(())
    }
```

- [ ] **Step 5: テストが通ることを確認する**

Run: `cargo test --lib delete_account`
Expected: すべて PASS。

- [ ] **Step 6: コミットする**

```bash
git add src-tauri/src/store/testutil.rs src-tauri/src/store/mod.rs src-tauri/src/store/settings.rs
git commit -m "fix(store): アカウント削除の保存に失敗したらメモリを変えない(Issue #694)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `remove_account` の順序を組み替える

**Files:**
- Modify: `src-tauri/src/commands/account.rs`(`remove_account` と、新しいテストモジュール `removal_tests`)
- Modify: `CLAUDE.md`(`commands/` の項目)

**Interfaces:**
- Consumes: `crate::store::testutil::BrokenDisk`、保存失敗でメモリを変えない `SettingsStore::delete_account`(Task 1)。
- Produces: `async fn remove_account_core(state: &AppState, account_id: &str) -> Result<()>`(`account.rs` 内の private 関数)。

- [ ] **Step 1: 本体を切り出す(挙動は変えない)**

`remove_account` を次の2つに置き換える。`remove_account_core` は、現在の本体をそのまま移したもの(引数が `&str` になる分だけ、`&account_id` が `account_id` になる)。

```rust
/// アカウント削除（token も keyring から消す）。
#[tauri::command]
#[specta::specta]
pub async fn remove_account(state: State<'_, AppState>, account_id: String) -> Result<()> {
    remove_account_core(&state, &account_id).await
}

/// `remove_account` の本体。`State` を引数に取らない形にして、テストできるようにしている。
async fn remove_account_core(state: &AppState, account_id: &str) -> Result<()> {
    // 関連カラムのストリームを止める
    for col in state.settings.load_columns()?.into_iter().filter(|c| c.account_id == account_id) {
        state.connections.close(&col.id);
    }
    // そのアカウントのクライアント側予約(Issue #60 B)も消す。送信先が無くなるため。
    state.scheduled_posts.remove_account(account_id)?;
    state.accounts.lock().unwrap().remove(account_id)?;
    state.refresh_fetch_allowlist();
    state.secrets.delete(account_id)?;
    state.forget_server_version(account_id);
    state.forget_translator_available(account_id);
    state.forget_server_mute_sync_lock(account_id);
    // そのアカウントのノート帰属も消す(Issue #557)。
    crate::attribution::forget(&state.cache, account_id).await;
    state.settings.delete_account(account_id)?; // アカウント＋カラムを永続層から削除
    // 実行中の取得が上の `forget` の後に完了して再記録した分も消す(競合の窓を狭める。完全には塞げない)。
    crate::attribution::forget(&state.cache, account_id).await;
    // 上記でそのアカウントのカラムが全て消えたことで空になったグループが有り得るため、
    // 空グループとペイン分割ツリー上の対応するLeafも掃除する(Issue #31)。
    state.settings.delete_empty_groups()?;
    Ok(())
}
```

- [ ] **Step 2: テストを書く**

`account.rs` の末尾に追加する。

```rust
#[cfg(test)]
mod removal_tests {
    use super::*;
    use crate::domain::{Column, ColumnGroup, ColumnKind, FilterQuery};
    use crate::session::{MemoryStore, SecretStore};
    use crate::store::scheduled_post::test_input;
    use crate::store::testutil::BrokenDisk;
    use crate::store::{ScheduledPostStore, SettingsStore};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// 立てた操作だけが失敗する。
    #[derive(Default)]
    struct SecretFaults {
        get: AtomicBool,
        set: AtomicBool,
        delete: AtomicBool,
    }

    struct FaultySecrets {
        inner: MemoryStore,
        faults: Arc<SecretFaults>,
    }

    fn fault(flag: &AtomicBool) -> Result<()> {
        if flag.load(Ordering::SeqCst) {
            return Err(Error::Secret("injected failure".into()));
        }
        Ok(())
    }

    impl SecretStore for FaultySecrets {
        fn set(&self, account_id: &str, token: &str) -> Result<()> {
            fault(&self.faults.set)?;
            self.inner.set(account_id, token)
        }

        fn get(&self, account_id: &str) -> Result<Option<String>> {
            fault(&self.faults.get)?;
            self.inner.get(account_id)
        }

        fn delete(&self, account_id: &str) -> Result<()> {
            fault(&self.faults.delete)?;
            self.inner.delete(account_id)
        }
    }

    /// アカウント `a1` と `a2`。それぞれ、グループ1つ、通知カラム1つ、予約1件、ストリーム1本、トークンを持つ。
    struct Fixture {
        state: AppState,
        faults: Arc<SecretFaults>,
        settings_disk: BrokenDisk,
        scheduled_disk: BrokenDisk,
        _app: tauri::App<tauri::test::MockRuntime>,
    }

    fn account(id: &str, host: &str) -> Account {
        Account {
            id: id.into(),
            host: host.into(),
            username: id.into(),
            user_id: format!("u-{id}"),
            display_name: id.into(),
            avatar_url: None,
            instance: None,
            is_cat: false,
            avatar_blurhash: None,
        }
    }

    fn column(id: &str, account_id: &str, group_id: &str) -> Column {
        Column {
            id: id.into(),
            account_id: account_id.into(),
            kind: ColumnKind::Notifications,
            order: 0,
            filter: FilterQuery::Keywords(vec![]),
            notify_sound: false,
            notify_desktop: false,
            notify_sound_choice: String::new(),
            speech: Default::default(),
            group_id: group_id.into(),
            title: None,
        }
    }

    fn fixture() -> Fixture {
        // 接続先は、すぐに拒否されるローカルのポート。
        const ACCOUNTS: [(&str, &str, &str, &str); 2] = [
            ("a1", "127.0.0.1:1", "c1", "g1"),
            ("a2", "127.0.0.1:2", "c2", "g2"),
        ];
        let settings_disk = BrokenDisk::new();
        let scheduled_disk = BrokenDisk::new();
        let settings = SettingsStore::new(
            settings_disk.path("settings.json"),
            settings_disk.path("backgrounds"),
        )
        .unwrap();
        for (order, (id, host, column_id, group_id)) in ACCOUNTS.iter().enumerate() {
            settings.upsert_account(&account(id, host)).unwrap();
            settings
                .upsert_group(&ColumnGroup { id: (*group_id).into(), order: order as i32, width: 320, auto: false })
                .unwrap();
            settings.upsert_column(&column(column_id, id, group_id)).unwrap();
        }

        let scheduled =
            ScheduledPostStore::new(scheduled_disk.path("scheduled_posts.json")).unwrap();
        let mut state = AppState::new_for_test(settings).with_scheduled_posts(scheduled);
        let faults = Arc::new(SecretFaults::default());
        state.secrets = Box::new(FaultySecrets { inner: MemoryStore::default(), faults: faults.clone() });

        let app = tauri::test::mock_app();
        for (id, host, column_id, _) in ACCOUNTS {
            state.secrets.set(id, "token").unwrap();
            state.scheduled_posts.add(id, 4_000_000_000, test_input("later"), 0).unwrap();
            state.connections.open_notifications(
                app.handle().clone(),
                column_id.into(),
                id.into(),
                host.into(),
                "token".into(),
                None,
            );
        }
        assert_eq!(state.connections.open_count(), 2);

        Fixture { state, faults, settings_disk, scheduled_disk, _app: app }
    }

    fn has_account(f: &Fixture, id: &str) -> (bool, bool) {
        (
            f.state.accounts.lock().unwrap().get(id).is_some(),
            f.state.settings.load_accounts().unwrap().iter().any(|a| a.id == id),
        )
    }

    fn column_ids(f: &Fixture) -> Vec<String> {
        let mut ids: Vec<String> =
            f.state.settings.load_columns().unwrap().into_iter().map(|c| c.id).collect();
        ids.sort();
        ids
    }

    /// トークン以外が、削除前のまま。
    fn assert_untouched(f: &Fixture) {
        assert_eq!(has_account(f, "a1"), (true, true), "(メモリ, 設定)");
        assert_eq!(column_ids(f), ["c1", "c2"]);
        assert_eq!(f.state.scheduled_posts.list_for_account("a1").len(), 1);
        assert_eq!(f.state.connections.open_count(), 2);
    }

    fn token(f: &Fixture, id: &str) -> Option<String> {
        f.state.secrets.get(id).unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn removes_everything_of_that_account_only() {
        let f = fixture();

        remove_account_core(&f.state, "a1").await.unwrap();

        assert_eq!(has_account(&f, "a1"), (false, false));
        assert_eq!(column_ids(&f), ["c2"]);
        let groups: Vec<String> =
            f.state.settings.load_groups().unwrap().into_iter().map(|g| g.id).collect();
        assert_eq!(groups, ["g2"]);
        assert_eq!(token(&f, "a1"), None);
        assert!(f.state.scheduled_posts.list_for_account("a1").is_empty());
        assert_eq!(f.state.connections.open_count(), 1);

        assert_eq!(has_account(&f, "a2"), (true, true));
        assert_eq!(token(&f, "a2").as_deref(), Some("token"));
        assert_eq!(f.state.scheduled_posts.list_for_account("a2").len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn keyring_delete_failure_changes_nothing() {
        let f = fixture();
        f.faults.delete.store(true, Ordering::SeqCst);

        let err = remove_account_core(&f.state, "a1").await.unwrap_err();

        assert!(matches!(err, Error::Secret(_)), "{err}");
        assert_untouched(&f);
        assert_eq!(token(&f, "a1").as_deref(), Some("token"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn keyring_read_failure_changes_nothing() {
        let f = fixture();
        f.faults.get.store(true, Ordering::SeqCst);

        let err = remove_account_core(&f.state, "a1").await.unwrap_err();

        assert!(matches!(err, Error::Secret(_)), "{err}");
        assert_untouched(&f);
        f.faults.get.store(false, Ordering::SeqCst);
        assert_eq!(token(&f, "a1").as_deref(), Some("token"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn settings_save_failure_restores_the_token_and_a_retry_succeeds() {
        let f = fixture();
        f.settings_disk.break_dir();

        assert!(remove_account_core(&f.state, "a1").await.is_err());

        assert_untouched(&f);
        assert_eq!(token(&f, "a1").as_deref(), Some("token"));

        f.settings_disk.repair_dir();
        remove_account_core(&f.state, "a1").await.unwrap();
        assert_eq!(has_account(&f, "a1"), (false, false));
        assert_eq!(token(&f, "a1"), None);
        assert_eq!(f.state.connections.open_count(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_token_restore_still_reports_the_settings_error() {
        let f = fixture();
        f.settings_disk.break_dir();
        f.faults.set.store(true, Ordering::SeqCst);

        let err = remove_account_core(&f.state, "a1").await.unwrap_err();

        assert!(!matches!(err, Error::Secret(_)), "{err}");
        assert_untouched(&f);
        assert_eq!(token(&f, "a1"), None, "書き戻せないので、要ログインの状態で残る");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_account_without_a_token_gets_none_on_failure_and_can_be_removed() {
        let f = fixture();
        f.state.secrets.delete("a1").unwrap();
        f.settings_disk.break_dir();

        assert!(remove_account_core(&f.state, "a1").await.is_err());
        assert_untouched(&f);
        assert_eq!(token(&f, "a1"), None);

        f.settings_disk.repair_dir();
        remove_account_core(&f.state, "a1").await.unwrap();
        assert_eq!(has_account(&f, "a1"), (false, false));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_scheduled_post_save_failure_after_the_commit_still_succeeds() {
        let f = fixture();
        f.scheduled_disk.break_dir();

        remove_account_core(&f.state, "a1").await.unwrap();

        assert_eq!(has_account(&f, "a1"), (false, false));
        assert_eq!(column_ids(&f), ["c2"]);
        assert_eq!(token(&f, "a1"), None);
        assert_eq!(f.state.connections.open_count(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_unknown_account_is_rejected() {
        let f = fixture();

        let err = remove_account_core(&f.state, "nope").await.unwrap_err();

        assert!(matches!(err, Error::Invalid(_)), "{err}");
        assert_untouched(&f);
    }
}
```

- [ ] **Step 3: 失敗を確認する**

Run: `cargo test --lib removal_tests`
Expected: 8件中6件が FAIL(`keyring_delete_failure_changes_nothing`、`keyring_read_failure_changes_nothing`、`settings_save_failure_restores_the_token_and_a_retry_succeeds`、`a_failed_token_restore_still_reports_the_settings_error`、`an_account_without_a_token_gets_none_on_failure_and_can_be_removed`、`a_scheduled_post_save_failure_after_the_commit_still_succeeds`)。`removes_everything_of_that_account_only` と `an_unknown_account_is_rejected` は PASS。違う結果になったら、実装へ進まずに原因を調べる。

- [ ] **Step 4: 実装する**

`remove_account_core`(コメントを含む)を次に置き換える。

```rust
/// `remove_account` の本体。`State` を引数に取らない形にして、テストできるようにしている。
///
/// エラーを返すなら何も変えない。確定点(`SettingsStore::delete_account`)の後は、`?` で抜けずに
/// 後始末を最後まで行う(Issue #694)。
async fn remove_account_core(state: &AppState, account_id: &str) -> Result<()> {
    if state.accounts.lock().unwrap().get(account_id).is_none() {
        return Err(Error::Invalid(format!("unknown account: {account_id}")));
    }
    // 確定後は設定から引けないので、先に控える。
    let column_ids: Vec<String> = state
        .settings
        .load_columns()?
        .into_iter()
        .filter(|c| c.account_id == account_id)
        .map(|c| c.id)
        .collect();

    // トークンは確定点より前に消す。後だと、失敗したときに、画面から消し直せないトークンが残る。
    let token = state.secrets.get(account_id)?;
    state.secrets.delete(account_id)?;

    // 確定点: アカウントとカラムを永続層から削除する。
    if let Err(e) = state.settings.delete_account(account_id) {
        if let Some(token) = &token {
            if let Err(restore) = state.secrets.set(account_id, token) {
                log::warn!("failed to restore the token after the account removal failed: {restore}");
            }
        }
        return Err(e);
    }

    for column_id in &column_ids {
        state.connections.close(column_id);
    }
    // 存在は先頭で確かめた。同時に走った削除が先に消していても、失敗にはしない。
    let _ = state.accounts.lock().unwrap().remove(account_id);
    state.refresh_fetch_allowlist();
    state.forget_server_version(account_id);
    state.forget_translator_available(account_id);
    state.forget_server_mute_sync_lock(account_id);
    // そのアカウントのノート帰属も消す(Issue #557)。
    crate::attribution::forget(&state.cache, account_id).await;
    // そのアカウントのクライアント側予約(Issue #60 B)も消す。送信先が無くなるため。
    if let Err(e) = state.scheduled_posts.remove_account(account_id) {
        log::warn!("failed to remove the scheduled posts of the removed account: {e}");
    }
    // そのアカウントのカラムが全て消えたことで空になったグループと、ペイン分割ツリー上の
    // 対応するLeafを掃除する(Issue #31)。
    if let Err(e) = state.settings.delete_empty_groups() {
        log::warn!("failed to remove the empty groups after removing an account: {e}");
    }
    // 実行中の取得が上の `forget` の後に完了して再記録した分も消す(競合の窓を狭める。完全には塞げない)。
    crate::attribution::forget(&state.cache, account_id).await;
    Ok(())
}
```

- [ ] **Step 5: テストが通ることを確認する**

Run: `cargo test --lib removal_tests`
Expected: 8件すべて PASS。

- [ ] **Step 6: `CLAUDE.md` に決まりを足す**

`### src-tauri/src layout` の、`` - `commands/` — `` で始まる行の末尾に、次の文を足す。

```markdown
 アカウント削除(`account.rs` の `remove_account_core`、Issue #694)は、エラーを返すなら何も変えない順にする: トークン削除 → 確定点の `SettingsStore::delete_account`(失敗したらトークンを書き戻す)→ 後始末。確定点の後は `?` で抜けず、警告ログにする。アカウントに紐づく保存先を増やすときは、確定点の後に足す。
```

- [ ] **Step 7: 全体のテストを実行する**

Run: `cargo test`(Global Constraints の形で、`--lib <filter>` を付けない)
Expected: FAIL が0件(`#[ignore]` は除く)。

Run: `git status --short`
Expected: 変更は `src-tauri/src/commands/account.rs` と `CLAUDE.md` だけ。`frontend/src/bindings/tauri.gen.ts` に差分が出たら、コミットせずに原因を調べる。

- [ ] **Step 8: コミットする**

```bash
git add src-tauri/src/commands/account.rs CLAUDE.md
git commit -m "fix(account): アカウント削除が途中で失敗しても中途半端な状態を残さない(Issue #694)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 削除の途中に、スケジューラがトークンを読まないようにする(最終レビュー後の追加)

Global Constraints は同じ。ただし、スワップがほぼ埋まっている場合は `CARGO_BUILD_JOBS=2`、`MemoryMax=6G`、`MemoryHigh=5G` に下げる。

**Files:**
- Modify: `src-tauri/src/state.rs`(`AppState` のフィールドと `new_with_sound`)
- Modify: `src-tauri/src/commands/account.rs`(`remove_account_core` と `removal_tests`)
- Modify: `src-tauri/src/scheduler.rs`(`attempt`)
- Modify: `CLAUDE.md`

**Interfaces:**
- Produces: `AppState.token_removal: std::sync::RwLock<()>`(`pub(crate)`)

- [ ] **Step 1: テストを書く(`removal_tests`)**

`SecretFaults` に `after_delete: Mutex<Option<Hook>>` を足し、フックの呼び出しを関数にまとめる。

```rust
    fn run_hook(slot: &Mutex<Option<Hook>>, account_id: &str) {
        let hook = slot.lock().unwrap().take_if(|(id, _)| *id == account_id);
        if let Some((_, run)) = hook {
            run();
        }
    }
```

`get` の中のフック呼び出しは `run_hook(&self.faults.on_get, account_id);` に置き換え、`delete` は次にする。

```rust
        fn delete(&self, account_id: &str) -> Result<()> {
            fault(&self.faults.delete)?;
            self.inner.delete(account_id)?;
            run_hook(&self.faults.after_delete, account_id);
            Ok(())
        }
```

`use` に `crate::domain::LocalScheduleStatus`、`std::sync::mpsc`、`std::time::Duration` を足し、テストを追加する。

```rust
    /// 削除の途中(トークンを消してから、確定に失敗して書き戻すまで)に予約の時刻が来ても、`Failed` にしない。
    #[tokio::test(flavor = "multi_thread")]
    async fn a_post_due_during_a_failing_removal_is_not_marked_failed() {
        let f = fixture();
        let entry = f.state.scheduled_posts.add("a1", 1_000, test_input("due"), 0).unwrap();
        f.settings_disk.break_dir();
        let (deleted_tx, deleted_rx) = mpsc::channel();
        let (scheduled_tx, scheduled_rx) = mpsc::channel::<()>();
        *f.faults.after_delete.lock().unwrap() = Some((
            "a1",
            Box::new(move || {
                deleted_tx.send(()).unwrap();
                // 排他が無ければ、この間にスケジューラの処理が終わる。
                let _ = scheduled_rx.recv_timeout(Duration::from_millis(500));
            }),
        ));

        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| {
            std::thread::scope(|s| {
                let removal = s.spawn(|| handle.block_on(remove_account_core(&f.state, "a1")));
                deleted_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                handle.block_on(crate::scheduler::process_due(&f.state, 1_000));
                let _ = scheduled_tx.send(());
                assert!(removal.join().unwrap().is_err());
            })
        });

        // 戻ったトークンで送り、接続できないので再試行待ちになる。
        let after = f.state.scheduled_posts.get(&entry.id).unwrap();
        assert_eq!((after.status, after.attempts), (LocalScheduleStatus::Pending, 1), "{:?}", after.error);
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cargo test --lib removal_tests`
Expected: 追加した1件だけが、`left: (Failed, 0)` で FAIL。ほかの9件は PASS。

- [ ] **Step 3: 実装する**

`state.rs` の `AppState` の `column_fence` の後に足す。

```rust
    /// アカウント削除が、トークンを消してから確定する(または書き戻す)までの間、書きロックを持つ。
    /// トークンの読み取り失敗を状態として記録する処理(予約投稿のスケジューラ)は、読みロックを取ってから読む(Issue #694)。
    pub(crate) token_removal: std::sync::RwLock<()>,
```

`new_with_sound` の `Self { … }` の `column_fence` の次に `token_removal: Default::default(),` を足す。

`account.rs` の `remove_account_core` の、「トークンは確定点より前に消す」から確定点の `if let Err` の終わりまでを、次のブロックにする。

```rust
    {
        // トークンが一時的に消えている間は、スケジューラに読ませない。この中で `await` しない。
        let _removing = state.token_removal.write().unwrap();
        // トークンは確定点より前に消す。後だと、失敗したときに、画面から消し直せないトークンが残る。
        let token = state.secrets.get(account_id)?;
        state.secrets.delete(account_id)?;

        // 確定点: アカウントとカラムを永続層から削除する。
        if let Err(e) = state.settings.delete_account(account_id) {
            if let Some(token) = &token {
                if let Err(restore) = state.secrets.set(account_id, token) {
                    log::warn!("failed to restore the token after the account removal failed: {restore}");
                }
            }
            return Err(e);
        }
    }
```

`scheduler.rs` の `attempt` の `let result = match state.client_for(&account_id) {` を次にする。

```rust
    // アカウント削除の途中で一時的に消えているトークンを読まない(Issue #694)。通信の前に放す。
    let client = {
        let _reading = state.token_removal.read().unwrap();
        state.client_for(&account_id)
    };
    let result = match client {
```

`CLAUDE.md` の `commands/` の行の末尾に足す。

```markdown
 トークンを消してから確定するまでは `AppState.token_removal` の書きロックを持つ。トークンが読めないことを状態として記録する処理(予約投稿のスケジューラ)は、読みロックを取ってから `client_for` する。
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test --lib removal_tests`、続けて `cargo test`
Expected: `removal_tests` は10件すべて PASS。全体で FAIL が0件。

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/src/state.rs src-tauri/src/commands/account.rs src-tauri/src/scheduler.rs CLAUDE.md
git commit -m "fix(scheduler): アカウント削除の途中に予約の時刻が来ても失敗扱いにしない(Issue #694)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
