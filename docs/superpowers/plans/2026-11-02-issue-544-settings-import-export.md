# 設定のインポートとエクスポート 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 設定(アカウント一覧・カラム構成・アプリ設定)をJSONファイルに書き出し、別の端末で全置換で読み込めるようにする(デスクトップとAndroid)。

**Architecture:** 純粋ロジック(`build_export` / `parse_export` / `plan_import`)を `store/settings_transfer.rs` に置き、I/Oとアプリ状態の更新を `commands/settings_transfer.rs` に置く。保存は `SettingsStore::replace` で原子的に行い、適用後はWebViewをリロードする。トークンなしで取り込んだアカウントのホストはSSRFガードの除外リストに入れない。

**Tech Stack:** Rust(Tauri v2, serde, specta, tauri-plugin-fs / dialog)、Svelte 5、Vitest。

**Spec:** `docs/superpowers/specs/2026-11-02-issue-544-settings-import-export-design.md`(ブランチ `feat/issue-544-settings-import-export` に作成・承認済み)

## Global Constraints

- ファイル形式: `format` は `"tsumugi-settings"`、`version` は整数の `1`。`version` がこれより大きいファイルは拒否する。
- ファイルサイズの上限は 8MiB(`8 * 1024 * 1024` バイト)。読んだバイト列の長さで判定する。
- 含めない: トークン、キーリングの内容、`cache_backend`、`server_mute_snapshots`、ノートキャッシュ、下書き、予約投稿、背景の実体と `backgroundKind` / `backgroundPath`(エクスポート時に `None` にし、インポート後も `None`)。
- インポートは全置換。`host + userId`(ホストは大文字小文字を区別しない)が一致する端末側アカウントは、端末側のIDとトークンを維持する。ファイルに無い端末側アカウントは残す。
- `ui` のうちRustが直接使う数値だけを丸める: `noteCacheLimit` 0〜100000、`noteCacheMaxAgeDays` 0〜3650、`noteCacheMaxSizeMb` 0〜10000、`gapFillLimit` 0〜1000。それ以外の数値は丸めない。
- `notify.soundChoice` と `column.notifySoundChoice` は `mml::validate_sound_choice` を通す。
- `fetch_allowlist` に入れるのは、トークンを持つアカウントのホストだけ(`refresh_fetch_allowlist` と `AppState::new_with_sound` の両方)。
- 既存のコマンド・型のシグネチャは変えない。追加だけにする(例外: `commands/mute.rs` の `apply_mute_config` を `pub(crate)` にする)。
- `src-tauri/src/lib.rs` の `specta_builder()` に、新しいコマンドを必ず登録する。`frontend/src/bindings/tauri.gen.ts` は手で編集せず、`cd src-tauri && cargo test generates_frontend_bindings` で再生成してコミットする。
- UIは `docs/design/style-guide.md` の規約(角丸・文字サイズ・アイコン・focus-visible)に従い、一度きりの値(`rounded-[Npx]` など)を作らない。ただし、バッジの `text-[0.68rem]` は `AccountsSection.svelte` の既存の例外にならう。
- コミットメッセージは Conventional Commits 形式の件名1行のみ(本文なし)。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` を付ける。`--no-verify` は使わない。コミットが失敗したら止めて報告する(再試行しない)。
- `main` へ直接コミットしない(ブランチ `feat/issue-544-settings-import-export` は作成済み)。
- push後のCI監視はしない。PR本文に `Fixes #544` を入れる。マージは `gh pr merge --merge`(squashしない)。
- 検証用に起動したプロセスは、PID指定でkillする(`pkill` / `killall` は使わない)。実画面の確認はXvfb越しに行い、`WAYLAND_DISPLAY` も unset する。

## Review Focus

仕様が暗黙に求めているが、各タスクの主テストが直接は見ない入力。使う人が踏みやすい順。各行の対応テストは、持ち主のタスクに入れてある。

1. 同じ `host + userId` のアカウントがファイル内に2件ある → 拒否する(Task 4)。
2. ホスト名の大文字小文字が端末側と違う(`Misskey.io` と `misskey.io`) → 同じアカウントとして対応づける(Task 4)。
3. Windowsのエディタが付けるUTF-8のBOM付きファイル → 読み込める(Task 3)。
4. `data` が空(`{}`)のファイル → エラーにせず、プレビューに「カラム0件」と出して、利用者に判断させる(Task 3, 4)。
5. インポート後に再認証したとき、取り込んだカラムが再起動なしで表示される(Task 6)。
6. トークンなしアカウントのホスト(`127.0.0.1` など)が、起動直後も `fetch_allowlist` に入らない(Task 1, 5)。

## ファイル構成

| ファイル | 変更 | 役割 |
|---|---|---|
| `src-tauri/src/state.rs` | 変更 | 許可リストをトークン付きアカウントのホストだけにする。`has_token` を追加 |
| `src-tauri/src/store/settings.rs` | 変更 | `SettingsData` を `pub(crate)` にし、`snapshot` / `replace` を追加 |
| `src-tauri/src/store/settings_transfer.rs` | 新規 | 型、`parse_export`、`build_export`、`plan_import`(I/Oなし) |
| `src-tauri/src/store/mod.rs` | 変更 | モジュール登録 |
| `src-tauri/src/commands/mute.rs` | 変更 | `write_file_bytes` を追加。`apply_mute_config` を `pub(crate)` に |
| `src-tauri/src/commands/settings_transfer.rs` | 新規 | 4コマンドと、テスト可能な `*_bytes` 関数 |
| `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` | 変更 | モジュール登録とコマンド登録 |
| `frontend/src/bindings/tauri.gen.ts` | 再生成 | 型とコマンドの追加 |
| `frontend/src/lib/store.svelte.ts` | 変更 | 再認証でそのアカウントの未表示カラムを開く |
| `frontend/src/ui/settings/SettingsTransfer.svelte` | 新規 | エクスポート/インポートのUI |
| `frontend/src/ui/settings/DataSection.svelte` | 変更 | 上を組み込む |
| `frontend/src/ui/settings/AccountsSection.svelte` | 変更 | 「要ログイン」バッジ |
| `docs/guide/user-guide.md`, `CLAUDE.md` | 変更 | 使い方と構成の追記 |

---

### Task 1: 許可リストをトークン付きアカウントのホストだけにする

**Files:**
- Modify: `src-tauri/src/state.rs`(`use` の追加、`new_with_sound` の許可リスト構築 `:196-197` 付近、`refresh_fetch_allowlist` `:380` 付近、`tests` モジュール)

**Interfaces:**
- Consumes: `SecretStore::get(&self, account_id: &str) -> Result<Option<String>>`、`FetchAllowlist::replace`、`AccountManager::list`
- Produces: `AppState::has_token(&self, account_id: &str) -> bool`(Task 5 が使う)。`refresh_fetch_allowlist()` は引数・戻り値が同じで、挙動だけ変わる

- [ ] **Step 1: 失敗するテストを書く**

`state.rs` の `#[cfg(test)] mod tests` に追加する(`use crate::domain::Account;` は既にある)。

```rust
    fn test_account(id: &str, host: &str) -> Account {
        Account {
            id: id.into(),
            host: host.into(),
            username: "me".into(),
            user_id: format!("u-{id}"),
            display_name: "Me".into(),
            avatar_url: None,
            instance: None,
            is_cat: false,
            avatar_blurhash: None,
        }
    }

    #[test]
    fn refresh_fetch_allowlist_skips_accounts_without_token() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.register_test_account("acc1"); // host=misskey.test、トークンあり
        state.accounts.lock().unwrap().upsert(test_account("acc2", "internal.example")); // トークンなし
        state.refresh_fetch_allowlist();

        assert!(state.fetch_allowlist.contains_host("misskey.test"));
        assert!(!state.fetch_allowlist.contains_host("internal.example"));
    }

    #[test]
    fn startup_allowlist_excludes_accounts_without_token() {
        let settings = SettingsStore::new_in_memory();
        settings.upsert_account(&test_account("with", "misskey.test")).unwrap();
        settings.upsert_account(&test_account("without", "internal.example")).unwrap();
        let secrets = crate::session::MemoryStore::default();
        secrets.set("with", "tok").unwrap();
        let cache = NoteCacheStore::new(crate::store::SqliteBackend::new(
            crate::store::db::open_cache_in_memory().unwrap(),
        ));

        let state = AppState::new_with_sound(
            Box::new(secrets),
            settings,
            DraftStore::new_in_memory(),
            cache,
            std::env::temp_dir(),
            SoundPlayer::new_for_test(),
        );

        assert!(state.fetch_allowlist.contains_host("misskey.test"));
        assert!(!state.fetch_allowlist.contains_host("internal.example"));
    }

    #[test]
    fn has_token_reflects_the_secret_store() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.register_test_account("acc1");
        assert!(state.has_token("acc1"));
        assert!(!state.has_token("nobody"));
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib state::tests::refresh_fetch_allowlist_skips_accounts_without_token state::tests::startup_allowlist_excludes_accounts_without_token state::tests::has_token_reflects_the_secret_store`

(`cargo test` は複数のフィルターを受け付けないので、3回に分けるか `cargo test --lib state::tests::` で状態モジュール全体を流す。)

Expected: コンパイルエラー(`has_token` が無い)か、`internal.example` が許可リストに入っていてアサート失敗。

- [ ] **Step 3: 実装する**

`state.rs` の先頭の `use` に `Account` を足す。

```rust
use crate::domain::{Account, EmojiDef, MuteConfig, Note, Notification};
```

`tests` モジュールの `use crate::domain::Account;` は `use super::*;` 経由で重複するので、重複エラーになる場合は tests 側の行を削除する。

`AppState` の定義の外(ファイル内の自由関数)に追加する:

```rust
/// 許可リストに入れるホスト。**トークンを持つアカウントのものだけ**(Issue #544)。
/// トークンなしのアカウント(設定インポート直後など)のホストを入れると、細工した設定ファイルで
/// `127.0.0.1` などをSSRFガード(Issue #377)の除外対象にできてしまう。キーリングの読み取り失敗は
/// 「トークンなし」として扱う。アカウントIDはログに出さない。
fn hosts_with_token(accounts: &[Account], secrets: &dyn SecretStore) -> Vec<String> {
    accounts
        .iter()
        .filter(|a| token_present(secrets, &a.id))
        .map(|a| a.host.clone())
        .collect()
}

fn token_present(secrets: &dyn SecretStore, account_id: &str) -> bool {
    match secrets.get(account_id) {
        Ok(token) => token.is_some(),
        Err(e) => {
            log::warn!("failed to read a token from the secret store: {e}");
            false
        }
    }
}
```

`new_with_sound` の許可リスト構築(`fetch_allowlist.replace(accounts.iter()...)` の行)を差し替える。`secrets` はこの時点ではまだ `Self` に移動していない。

```rust
        let fetch_allowlist = crate::net_guard::FetchAllowlist::default();
        fetch_allowlist.replace(hosts_with_token(&accounts, secrets.as_ref()));
```

`refresh_fetch_allowlist` と、その直前に `has_token` を追加する:

```rust
    /// `accounts` のうちトークンを持つものの全ホストで、取得先の許可リストを差し替える。
    /// アカウントの追加・削除・再認証の直後に呼ぶ。
    pub fn refresh_fetch_allowlist(&self) {
        let accounts = self.accounts.lock().unwrap().list();
        self.fetch_allowlist.replace(hosts_with_token(&accounts, self.secrets.as_ref()));
    }

    /// そのアカウントのトークンがキーリングにあるか。読み取りに失敗したら `false`。
    pub fn has_token(&self, account_id: &str) -> bool {
        token_present(self.secrets.as_ref(), account_id)
    }
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd src-tauri && cargo test --lib state::tests::`
Expected: すべて PASS(既存の `refresh_fetch_allowlist_follows_registered_accounts` も PASS のまま。`register_test_account` がトークンを入れるため)。

- [ ] **Step 5: 影響範囲を確認する**

Run: `cd src-tauri && cargo test --lib`
Expected: すべて PASS。

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/state.rs
git commit -m "fix(net-guard): 取得先の許可リストにトークンを持つアカウントのホストだけを入れる (#544)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: SettingsStore の `snapshot` と `replace`

**Files:**
- Modify: `src-tauri/src/store/settings.rs`(`SettingsData` の可視性 `:47-67`、`impl SettingsStore`、`tests` モジュール)

**Interfaces:**
- Consumes: 既存の `SettingsStore::save(&self, &SettingsData) -> Result<()>`
- Produces:
  - `pub(crate) struct SettingsData`(全フィールドが `pub(crate)`)
  - `SettingsStore::snapshot(&self) -> SettingsData`
  - `SettingsStore::replace(&self, new: SettingsData) -> Result<()>`(保存に成功してからメモリを差し替える)

- [ ] **Step 1: 失敗するテストを書く**

`settings.rs` の `tests` モジュールの末尾に追加する(同モジュールには `account(id: &str) -> Account` がある)。

```rust
    #[test]
    fn replace_swaps_the_whole_settings_and_snapshot_returns_a_copy() {
        let s = SettingsStore::new_in_memory();
        s.upsert_account(&account("old")).unwrap();
        let mut new = s.snapshot();
        new.accounts = vec![account("new")];
        new.mute = MuteConfig { ng_words: vec!["x".into()], ..MuteConfig::default() };

        s.replace(new).unwrap();

        let snap = s.snapshot();
        assert_eq!(snap.accounts, vec![account("new")]);
        assert_eq!(snap.mute.ng_words, vec!["x".to_string()]);
    }

    #[test]
    fn replace_keeps_the_previous_state_when_saving_fails() {
        let dir = std::env::temp_dir().join(format!("tsumugi-replace-fail-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let s = SettingsStore::new(dir.join("settings.json"), dir.join("backgrounds")).unwrap();
        s.upsert_account(&account("old")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap(); // 以後の保存は失敗する

        let mut new = s.snapshot();
        new.accounts = vec![account("new")];

        assert!(s.replace(new).is_err());
        assert_eq!(s.snapshot().accounts, vec![account("old")], "保存に失敗したら、メモリ上の状態は変わらない");
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib store::settings::tests::replace_`
Expected: コンパイルエラー(`snapshot` / `replace` が無い)。

- [ ] **Step 3: 実装する**

`SettingsData` の定義(`:47` 付近)を、構造体とすべてのフィールドが `pub(crate)` になるよう書き換える。フィールドの `#[serde(default)]` とdocコメントは残す。

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct SettingsData {
    #[serde(default)]
    pub(crate) accounts: Vec<Account>,
    #[serde(default)]
    pub(crate) groups: Vec<ColumnGroup>,
    #[serde(default)]
    pub(crate) columns: Vec<Column>,
    #[serde(default)]
    pub(crate) mute: MuteConfig,
    #[serde(default)]
    pub(crate) notify: NotifyConfig,
    #[serde(default)]
    pub(crate) ui: UiPrefs,
    #[serde(default)]
    pub(crate) pane_layout: Option<PaneNode>,
    #[serde(default)]
    pub(crate) cache_backend: CacheBackendConfig,
    /// アカウントID -> サーバー側ミュートの前回の同期時点の集合(Issue #454)。
    #[serde(default)]
    pub(crate) server_mute_snapshots: HashMap<String, ServerMuteSnapshot>,
}
```

`impl SettingsStore` の `save` の直後に追加する:

```rust
    /// 設定全体の複製(設定のエクスポート/インポート用、Issue #544)。
    pub(crate) fn snapshot(&self) -> SettingsData {
        self.data.lock().unwrap().clone()
    }

    /// 設定全体を差し替える(設定のインポート用、Issue #544)。保存に成功してから
    /// メモリ上の状態を差し替えるので、失敗したときは何も変わらない。
    pub(crate) fn replace(&self, new: SettingsData) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        self.save(&new)?;
        *guard = new;
        Ok(())
    }
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd src-tauri && cargo test --lib store::settings::`
Expected: すべて PASS。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/store/settings.rs
git commit -m "feat(settings): 設定全体を複製・差し替えする snapshot/replace を追加 (#544)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: ファイル形式と `parse_export` / `build_export`

**Files:**
- Create: `src-tauri/src/store/settings_transfer.rs`
- Modify: `src-tauri/src/store/mod.rs`(`pub mod settings_transfer;` を `pub mod settings;` の次に追加)

**Interfaces:**
- Consumes: `SettingsData`、`SettingsStore::snapshot`(Task 2)
- Produces(Task 4・5 が使う):
  - `pub const EXPORT_FORMAT: &str`、`EXPORT_VERSION: u32`、`MAX_IMPORT_BYTES: usize`
  - `pub struct ExportFile { format, version, exported_at, app_version, data: ExportData }`
  - `pub struct ExportData { accounts, groups, columns, mute, notify, ui, pane_layout }`
  - `pub struct ImportPreview { column_count: i32, account_count: i32, accounts_needing_login: i32, has_custom_css: bool, has_custom_themes: bool }`(`specta::Type`、`camelCase`)
  - `pub struct ImportResult { column_count: i32, accounts_needing_login: Vec<String> }`(`specta::Type`、`camelCase`)
  - `pub fn parse_export(bytes: &[u8]) -> Result<ExportFile>`
  - `pub fn build_export(data: &SettingsData, app_version: &str, exported_at: &str) -> ExportFile`

- [ ] **Step 1: モジュールの骨組みと失敗するテストを書く**

`store/mod.rs` に `pub mod settings_transfer;` を追加する。

`src-tauri/src/store/settings_transfer.rs` を、まず型の定義とテストだけで作る(関数は次のステップで書く)。

```rust
//! 設定のエクスポート/インポートの純粋ロジック(I/Oなし)。Issue #544。
//! 設計書: docs/superpowers/specs/2026-11-02-issue-544-settings-import-export-design.md
//! トークンはキーリング管轄でここには現れない。入力ファイルは信頼できない外部データとして扱う。

use crate::domain::{Account, Column, ColumnGroup, MuteConfig, NotifyConfig, PaneNode, UiPrefs};
use crate::error::{Error, Result};
use crate::store::settings::SettingsData;
use serde::{Deserialize, Serialize};
use specta::Type;

pub const EXPORT_FORMAT: &str = "tsumugi-settings";
pub const EXPORT_VERSION: u32 = 1;
/// 読み込むファイルのサイズ上限。カスタムCSS(256KiB)より十分大きい値にしてある。
pub const MAX_IMPORT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportFile {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub app_version: String,
    pub data: ExportData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportData {
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub groups: Vec<ColumnGroup>,
    #[serde(default)]
    pub columns: Vec<Column>,
    #[serde(default)]
    pub mute: MuteConfig,
    #[serde(default)]
    pub notify: NotifyConfig,
    #[serde(default)]
    pub ui: UiPrefs,
    #[serde(default)]
    pub pane_layout: Option<PaneNode>,
}

/// インポート前に利用者へ見せる概要。
#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub column_count: i32,
    pub account_count: i32,
    /// ファイルにだけあり、トークンなしで追加されるアカウントの数。
    pub accounts_needing_login: i32,
    pub has_custom_css: bool,
    pub has_custom_themes: bool,
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub column_count: i32,
    /// トークンなしで追加したアカウントのID。再認証が必要。
    pub accounts_needing_login: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{BackgroundKind, ColumnKind, FilterQuery};

    pub(super) fn account(id: &str, host: &str, user_id: &str) -> Account {
        Account {
            id: id.into(),
            host: host.into(),
            username: format!("name-{user_id}"),
            user_id: user_id.into(),
            display_name: "Me".into(),
            avatar_url: None,
            instance: None,
            is_cat: false,
            avatar_blurhash: None,
        }
    }

    pub(super) fn group(id: &str, order: i32) -> ColumnGroup {
        ColumnGroup { id: id.into(), order, width: 300, auto: false }
    }

    pub(super) fn column(id: &str, account_id: &str, group_id: &str) -> Column {
        Column {
            id: id.into(),
            account_id: account_id.into(),
            kind: ColumnKind::Home,
            order: 0,
            filter: FilterQuery::Keywords(vec![]),
            notify_sound: false,
            notify_desktop: false,
            notify_sound_choice: String::new(),
            group_id: group_id.into(),
            title: None,
        }
    }

    pub(super) fn empty_data() -> ExportData {
        ExportData {
            accounts: vec![],
            groups: vec![],
            columns: vec![],
            mute: MuteConfig::default(),
            notify: NotifyConfig::default(),
            ui: UiPrefs::default(),
            pane_layout: None,
        }
    }

    pub(super) fn file_with(data: ExportData) -> ExportFile {
        ExportFile {
            format: EXPORT_FORMAT.into(),
            version: EXPORT_VERSION,
            exported_at: "2026-11-02T00:00:00Z".into(),
            app_version: "0.0.0".into(),
            data,
        }
    }

    fn settings_data() -> SettingsData {
        let mut d = SettingsData::default();
        d.accounts = vec![account("a1", "misskey.io", "u1")];
        d.groups = vec![group("g1", 0)];
        d.columns = vec![column("c1", "a1", "g1")];
        d.ui.background_kind = Some(BackgroundKind::Image);
        d.ui.background_path = Some("/home/me/bg.png".into());
        d.ui.custom_css = "body { color: red }".into();
        d.cache_backend = crate::domain::CacheBackendConfig::default();
        d
    }

    #[test]
    fn build_export_drops_background_paths_and_device_specific_data() {
        let mut d = settings_data();
        d.server_mute_snapshots.insert("a1".into(), Default::default());

        let file = build_export(&d, "1.2.3", "2026-11-02T00:00:00Z");

        assert_eq!(file.format, EXPORT_FORMAT);
        assert_eq!(file.version, EXPORT_VERSION);
        assert_eq!(file.app_version, "1.2.3");
        assert_eq!(file.exported_at, "2026-11-02T00:00:00Z");
        assert_eq!(file.data.ui.background_kind, None);
        assert_eq!(file.data.ui.background_path, None);
        assert_eq!(file.data.ui.custom_css, "body { color: red }", "それ以外のUI設定は残る");
        assert_eq!(file.data.accounts, d.accounts);
        let json = serde_json::to_string(&file).unwrap();
        assert!(!json.contains("cacheBackend"), "キャッシュDBの接続設定は含めない");
        assert!(!json.contains("serverMuteSnapshots"), "同期スナップショットは含めない");
        assert!(!json.contains("/home/me/bg.png"), "端末ローカルのパスは含めない");
    }

    #[test]
    fn parse_export_reads_what_build_export_wrote() {
        let file = build_export(&settings_data(), "1.2.3", "2026-11-02T00:00:00Z");
        let bytes = serde_json::to_vec_pretty(&file).unwrap();

        assert_eq!(parse_export(&bytes).unwrap(), file);
    }

    #[test]
    fn parse_export_accepts_a_utf8_bom() {
        let file = build_export(&settings_data(), "1.2.3", "2026-11-02T00:00:00Z");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend(serde_json::to_vec(&file).unwrap());

        assert_eq!(parse_export(&bytes).unwrap(), file);
    }

    #[test]
    fn parse_export_accepts_an_empty_data_section() {
        let bytes = br#"{"format":"tsumugi-settings","version":1,"exportedAt":"x","appVersion":"y","data":{}}"#;

        let file = parse_export(bytes).unwrap();

        assert!(file.data.accounts.is_empty());
        assert!(file.data.columns.is_empty());
        assert_eq!(file.data.ui, UiPrefs::default());
    }

    #[test]
    fn parse_export_rejects_files_that_are_not_tsumugi_settings() {
        let err = parse_export(br#"{"hello":"world"}"#).unwrap_err().to_string();
        assert!(err.contains("tsumugiの設定ファイルではありません"), "{err}");

        let err = parse_export(br#"{"format":"other","version":1,"data":{}}"#).unwrap_err().to_string();
        assert!(err.contains("tsumugiの設定ファイルではありません"), "{err}");
    }

    #[test]
    fn parse_export_rejects_newer_versions_before_reading_the_body() {
        // 将来の形式は本体の構造が違うかもしれないので、本体を読む前にバージョンで弾く。
        let bytes = br#"{"format":"tsumugi-settings","version":2,"data":"not an object"}"#;

        let err = parse_export(bytes).unwrap_err().to_string();

        assert!(err.contains("より新しいバージョン(v2)"), "{err}");
    }

    #[test]
    fn parse_export_rejects_invalid_json_and_oversized_files() {
        let err = parse_export(b"{not json").unwrap_err().to_string();
        assert!(err.contains("JSONとして読み込めません"), "{err}");

        let big = vec![b' '; MAX_IMPORT_BYTES + 1];
        let err = parse_export(&big).unwrap_err().to_string();
        assert!(err.contains("ファイルが大きすぎます"), "{err}");
    }
}
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib store::settings_transfer::`
Expected: コンパイルエラー(`build_export` / `parse_export` が無い)。

- [ ] **Step 3: 実装する**

`ImportResult` の定義の後、`#[cfg(test)]` の前に追加する。

```rust
/// 書き出す内容を組み立てる。端末固有の項目(`cache_backend`、同期スナップショット、背景のパス)は含めない。
pub fn build_export(data: &SettingsData, app_version: &str, exported_at: &str) -> ExportFile {
    let mut ui = data.ui.clone();
    ui.background_kind = None;
    ui.background_path = None;
    ExportFile {
        format: EXPORT_FORMAT.into(),
        version: EXPORT_VERSION,
        exported_at: exported_at.into(),
        app_version: app_version.into(),
        data: ExportData {
            accounts: data.accounts.clone(),
            groups: data.groups.clone(),
            columns: data.columns.clone(),
            mute: data.mute.clone(),
            notify: data.notify.clone(),
            ui,
            pane_layout: data.pane_layout.clone(),
        },
    }
}

/// ファイルの中身を読む。サイズ・形式・バージョンを、本体を読む前に確認する。
pub fn parse_export(bytes: &[u8]) -> Result<ExportFile> {
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err(Error::Invalid("ファイルが大きすぎます(上限8MiB)".into()));
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes); // UTF-8 BOM
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| Error::Invalid(format!("JSONとして読み込めません: {e}")))?;
    if value.get("format").and_then(|v| v.as_str()) != Some(EXPORT_FORMAT) {
        return Err(Error::Invalid("tsumugiの設定ファイルではありません".into()));
    }
    if let Some(v) = value.get("version").and_then(|v| v.as_u64()) {
        if v > u64::from(EXPORT_VERSION) {
            return Err(Error::Invalid(format!("より新しいバージョン(v{v})で書き出されたファイルです")));
        }
    }
    serde_json::from_value(value).map_err(|e| Error::Invalid(format!("設定ファイルの内容が不正です: {e}")))
}
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd src-tauri && cargo test --lib store::settings_transfer::`
Expected: すべて PASS。

`ImportPreview` / `ImportResult` が未使用の警告になる場合は、Task 4・5 で使うので、このタスクでは `#[allow(dead_code)]` を付けず、警告のままコミットしてよい(Task 5 で解消する)。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/store/settings_transfer.rs src-tauri/src/store/mod.rs
git commit -m "feat(settings): 設定ファイルの形式と読み書きの純粋ロジックを追加 (#544)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: `plan_import`(検証・ID付け替え・丸め)

**Files:**
- Modify: `src-tauri/src/store/settings_transfer.rs`(関数の追加、`tests` モジュールへの追記)

**Interfaces:**
- Consumes: Task 3 の型と `parse_export`、`SettingsData`(全フィールド `pub(crate)`)、`crate::mml::validate_sound_choice(&str) -> Result<()>`
- Produces(Task 5 が使う):
  - `pub struct ImportPlan { pub data: SettingsData, pub preview: ImportPreview, pub accounts_needing_login: Vec<String> }`
  - `pub fn plan_import(current: &SettingsData, file: ExportFile) -> Result<ImportPlan>`

- [ ] **Step 1: 失敗するテストを書く**

`tests` モジュールに追加する(`use super::*;` と、Task 3 のヘルパー `account` / `group` / `column` / `empty_data` / `file_with` を使う)。モジュール先頭の `use crate::domain::{...}` に `PaneChild, PaneNode, SplitDirection` を足す。

```rust
    fn current_with_local_account() -> SettingsData {
        let mut d = SettingsData::default();
        d.accounts = vec![account("local1", "misskey.io", "u1")];
        d.groups = vec![group("old-g", 0)];
        d.columns = vec![column("old-c", "local1", "old-g")];
        d
    }

    fn plan_err(current: &SettingsData, file: ExportFile) -> String {
        plan_import(current, file).err().expect("拒否されるはず").to_string()
    }

    #[test]
    fn matching_accounts_keep_the_local_id_and_columns_follow_it() {
        let current = current_with_local_account();
        let mut data = empty_data();
        data.accounts = vec![account("file1", "misskey.io", "u1")];
        data.groups = vec![group("g1", 0)];
        data.columns = vec![column("c1", "file1", "g1")];
        data.ui.default_account_id = "file1".into();

        let plan = plan_import(&current, file_with(data)).unwrap();

        assert_eq!(plan.data.accounts.len(), 1, "端末側のアカウントがそのまま残る");
        assert_eq!(plan.data.accounts[0].id, "local1");
        assert_eq!(plan.data.columns[0].account_id, "local1");
        assert_eq!(plan.data.ui.default_account_id, "local1");
        assert!(plan.accounts_needing_login.is_empty());
        assert_eq!(plan.data.columns.len(), 1, "端末側のカラムはファイルの内容で置き換わる");
        assert_eq!(plan.data.columns[0].id, "c1");
    }

    #[test]
    fn host_comparison_ignores_ascii_case() {
        let current = current_with_local_account();
        let mut data = empty_data();
        data.accounts = vec![account("file1", "Misskey.IO", "u1")];

        let plan = plan_import(&current, file_with(data)).unwrap();

        assert_eq!(plan.data.accounts.len(), 1, "同じアカウントとして対応づく");
        assert!(plan.accounts_needing_login.is_empty());
    }

    #[test]
    fn new_accounts_are_added_without_a_token_and_local_only_accounts_remain() {
        let current = current_with_local_account();
        let mut data = empty_data();
        data.accounts = vec![account("file2", "other.example", "u9")];

        let plan = plan_import(&current, file_with(data)).unwrap();

        let ids: Vec<&str> = plan.data.accounts.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["local1", "file2"], "端末側のアカウントは残り、ファイル側が追加される");
        assert_eq!(plan.accounts_needing_login, vec!["file2".to_string()]);
        assert_eq!(plan.preview.accounts_needing_login, 1);
    }

    #[test]
    fn device_specific_settings_are_carried_over_from_the_current_state() {
        let mut current = current_with_local_account();
        current.server_mute_snapshots.insert("local1".into(), Default::default());

        let plan = plan_import(&current, file_with(empty_data())).unwrap();

        assert!(plan.data.server_mute_snapshots.contains_key("local1"));
    }

    #[test]
    fn background_is_reset_and_the_four_rust_side_numbers_are_clamped() {
        let current = SettingsData::default();
        let mut data = empty_data();
        data.ui.background_kind = Some(BackgroundKind::Video);
        data.ui.background_path = Some("/elsewhere/bg.mp4".into());
        data.ui.note_cache_limit = 9_999_999;
        data.ui.note_cache_max_age_days = -5;
        data.ui.note_cache_max_size_mb = 99_999;
        data.ui.gap_fill_limit = 5_000;
        data.ui.ui_scale = 9_999; // フロントが適用時に丸めるので、ここでは触らない

        let ui = plan_import(&current, file_with(data)).unwrap().data.ui;

        assert_eq!(ui.background_kind, None);
        assert_eq!(ui.background_path, None);
        assert_eq!(ui.note_cache_limit, 100_000);
        assert_eq!(ui.note_cache_max_age_days, 0);
        assert_eq!(ui.note_cache_max_size_mb, 10_000);
        assert_eq!(ui.gap_fill_limit, 1_000);
        assert_eq!(ui.ui_scale, 9_999);
    }

    #[test]
    fn default_account_id_pointing_nowhere_is_cleared() {
        let mut data = empty_data();
        data.ui.default_account_id = "ghost".into();

        let plan = plan_import(&SettingsData::default(), file_with(data)).unwrap();

        assert_eq!(plan.data.ui.default_account_id, "");
    }

    #[test]
    fn preview_counts_and_flags_custom_content() {
        let mut data = empty_data();
        data.accounts = vec![account("f1", "a.example", "u1"), account("f2", "b.example", "u2")];
        data.groups = vec![group("g1", 0)];
        data.columns = vec![column("c1", "f1", "g1"), column("c2", "f2", "g1"), column("c3", "f2", "g1")];
        data.ui.custom_css = "  body{}  ".into();

        let preview = plan_import(&SettingsData::default(), file_with(data)).unwrap().preview;

        assert_eq!(
            preview,
            ImportPreview {
                column_count: 3,
                account_count: 2,
                accounts_needing_login: 2,
                has_custom_css: true,
                has_custom_themes: false,
            }
        );
    }

    #[test]
    fn an_empty_data_section_is_planned_as_zero_columns() {
        let preview = plan_import(&current_with_local_account(), file_with(empty_data())).unwrap().preview;

        assert_eq!(preview.column_count, 0);
        assert_eq!(preview.account_count, 0);
    }

    #[test]
    fn rejects_wrong_format_and_unsupported_version() {
        let mut f = file_with(empty_data());
        f.format = "other".into();
        assert!(plan_err(&SettingsData::default(), f).contains("tsumugiの設定ファイルではありません"));

        let mut f = file_with(empty_data());
        f.version = 0;
        assert!(plan_err(&SettingsData::default(), f).contains("未対応のバージョン"));
    }

    #[test]
    fn rejects_duplicate_ids() {
        let mut data = empty_data();
        data.accounts = vec![account("x", "a.example", "u1"), account("x", "b.example", "u2")];
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("アカウントのIDが重複"));

        let mut data = empty_data();
        data.groups = vec![group("g", 0), group("g", 1)];
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("グループのIDが重複"));

        let mut data = empty_data();
        data.accounts = vec![account("a", "a.example", "u1")];
        data.groups = vec![group("g", 0)];
        data.columns = vec![column("c", "a", "g"), column("c", "a", "g")];
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("カラムのIDが重複"));
    }

    #[test]
    fn rejects_the_same_host_and_user_listed_twice() {
        let mut data = empty_data();
        data.accounts = vec![account("f1", "misskey.io", "u1"), account("f2", "MISSKEY.io", "u1")];

        let err = plan_err(&SettingsData::default(), file_with(data));

        assert!(err.contains("同じアカウントが複数含まれています"), "{err}");
    }

    #[test]
    fn rejects_a_new_account_whose_id_collides_with_another_local_account() {
        let current = current_with_local_account(); // local1 = misskey.io / u1
        let mut data = empty_data();
        data.accounts = vec![account("local1", "other.example", "u9")]; // 同じIDだが別のアカウント

        let err = plan_err(&current, file_with(data));

        assert!(err.contains("この端末の別のアカウントと衝突"), "{err}");
    }

    #[test]
    fn rejects_hosts_that_are_not_bare_host_names() {
        for bad in [
            "https://evil.example",
            "evil.example/path",
            "evil.example:8080",
            "user@evil.example",
            "",
            ".evil.example",
            "evil..example",
            "evil example",
        ] {
            let mut data = empty_data();
            data.accounts = vec![account("f1", bad, "u1")];
            let err = plan_err(&SettingsData::default(), file_with(data));
            assert!(err.contains("ホスト名が不正"), "{bad:?}: {err}");
        }
        // 素のホスト名とIPアドレスは通る(通すが、トークンがないので許可リストには入らない)
        for ok in ["misskey.io", "127.0.0.1", "xn--abc.example"] {
            let mut data = empty_data();
            data.accounts = vec![account("f1", ok, "u1")];
            assert!(plan_import(&SettingsData::default(), file_with(data)).is_ok(), "{ok}");
        }
    }

    #[test]
    fn rejects_dangling_references() {
        let mut data = empty_data();
        data.groups = vec![group("g", 0)];
        data.columns = vec![column("c", "ghost-account", "g")];
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("存在しないアカウントを参照"));

        let mut data = empty_data();
        data.accounts = vec![account("a", "a.example", "u1")];
        data.columns = vec![column("c", "a", "ghost-group")];
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("存在しないグループを参照"));

        let mut data = empty_data();
        data.groups = vec![group("g", 0)];
        data.pane_layout = Some(PaneNode::Split {
            id: "s".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild { node: PaneNode::new_leaf("g"), size: 1.0, auto: false },
                PaneChild { node: PaneNode::new_leaf("ghost-group"), size: 1.0, auto: false },
            ],
        });
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("ペイン配置が存在しないグループを参照"));
    }

    #[test]
    fn rejects_invalid_mml_sound_choices() {
        let mut data = empty_data();
        data.notify.sound_choice = "mml:j".into();
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("MML が不正です"));

        let mut data = empty_data();
        data.accounts = vec![account("a", "a.example", "u1")];
        data.groups = vec![group("g", 0)];
        let mut c = column("c", "a", "g");
        c.notify_sound_choice = "mml:j".into();
        data.columns = vec![c];
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("MML が不正です"));
    }

    #[test]
    fn a_valid_pane_layout_is_kept_as_is() {
        let mut data = empty_data();
        data.groups = vec![group("g", 0)];
        data.pane_layout = Some(PaneNode::new_leaf("g"));

        let plan = plan_import(&SettingsData::default(), file_with(data)).unwrap();

        assert_eq!(plan.data.pane_layout, Some(PaneNode::new_leaf("g")));
    }

    #[test]
    fn export_then_import_on_the_same_state_changes_nothing_except_the_background() {
        let mut current = current_with_local_account();
        current.ui.custom_css = "a{}".into();
        current.ui.background_path = Some("/x/bg.png".into());
        current.ui.background_kind = Some(BackgroundKind::Image);
        current.ui.default_account_id = "local1".into();
        let file = build_export(&current, "1.0.0", "2026-11-02T00:00:00Z");

        let plan = plan_import(&current, file).unwrap();

        assert_eq!(plan.data.accounts, current.accounts);
        assert_eq!(plan.data.columns, current.columns);
        assert_eq!(plan.data.groups, current.groups);
        assert_eq!(plan.data.ui.custom_css, "a{}");
        assert_eq!(plan.data.ui.default_account_id, "local1");
        assert_eq!(plan.data.ui.background_path, None);
        assert!(plan.accounts_needing_login.is_empty());
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib store::settings_transfer::`
Expected: コンパイルエラー(`plan_import` / `ImportPlan` が無い)。

- [ ] **Step 3: 実装する**

`settings_transfer.rs` の先頭の `use` に `std::collections::{HashMap, HashSet}` を足す。`build_export` / `parse_export` の後に追加する。

```rust
/// インポートの計画。`data` を `SettingsStore::replace` に渡す。
#[derive(Debug)]
pub struct ImportPlan {
    pub data: SettingsData,
    pub preview: ImportPreview,
    /// ファイルにだけあり、トークンなしで追加するアカウントのID。
    pub accounts_needing_login: Vec<String>,
}

fn invalid(msg: impl Into<String>) -> Error {
    Error::Invalid(msg.into())
}

/// 素のホスト名か。スキーム・パス・ポート・認証情報を含まず、空でないこと。
fn is_bare_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.starts_with('.')
        && !host.ends_with('.')
        && !host.contains("..")
        && host.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-')
}

fn ensure_unique<'a>(ids: impl Iterator<Item = &'a str>, what: &str) -> Result<()> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(invalid(format!("{what}のIDが重複しています")));
        }
    }
    Ok(())
}

fn collect_leaf_group_ids<'a>(node: &'a PaneNode, out: &mut Vec<&'a str>) {
    match node {
        PaneNode::Leaf { group_id, .. } => out.push(group_id),
        PaneNode::Split { children, .. } => {
            for child in children {
                collect_leaf_group_ids(&child.node, out);
            }
        }
    }
}

/// ファイルの内容と現在の設定から、インポート後の設定を組み立てる。ファイルは信頼しない。
/// 1つでも検証に落ちたらファイル全体を拒否する(部分的に取り込まない)。
pub fn plan_import(current: &SettingsData, file: ExportFile) -> Result<ImportPlan> {
    if file.format != EXPORT_FORMAT {
        return Err(invalid("tsumugiの設定ファイルではありません"));
    }
    if file.version != EXPORT_VERSION {
        return Err(invalid(format!("未対応のバージョン(v{})です", file.version)));
    }
    let d = file.data;

    ensure_unique(d.accounts.iter().map(|a| a.id.as_str()), "アカウント")?;
    ensure_unique(d.groups.iter().map(|g| g.id.as_str()), "グループ")?;
    ensure_unique(d.columns.iter().map(|c| c.id.as_str()), "カラム")?;

    let mut identities = HashSet::new();
    for a in &d.accounts {
        if !is_bare_host(&a.host) {
            let shown: String = a.host.chars().take(64).collect();
            return Err(invalid(format!("アカウントのホスト名が不正です: {shown}")));
        }
        if !identities.insert((a.host.to_ascii_lowercase(), a.user_id.clone())) {
            return Err(invalid("同じアカウントが複数含まれています"));
        }
    }

    // ファイル側のアカウントID -> 取り込み後のアカウントID
    let mut id_map: HashMap<String, String> = HashMap::new();
    let mut accounts = current.accounts.clone();
    let mut accounts_needing_login = Vec::new();
    for fa in &d.accounts {
        let local = current
            .accounts
            .iter()
            .find(|la| la.host.eq_ignore_ascii_case(&fa.host) && la.user_id == fa.user_id);
        match local {
            Some(la) => {
                id_map.insert(fa.id.clone(), la.id.clone());
            }
            None => {
                if current.accounts.iter().any(|la| la.id == fa.id) {
                    return Err(invalid("アカウントのIDがこの端末の別のアカウントと衝突しています"));
                }
                id_map.insert(fa.id.clone(), fa.id.clone());
                accounts.push(fa.clone());
                accounts_needing_login.push(fa.id.clone());
            }
        }
    }

    let group_ids: HashSet<&str> = d.groups.iter().map(|g| g.id.as_str()).collect();
    let mut columns = Vec::with_capacity(d.columns.len());
    for c in &d.columns {
        let Some(local_id) = id_map.get(&c.account_id) else {
            return Err(invalid("カラムが存在しないアカウントを参照しています"));
        };
        if !group_ids.contains(c.group_id.as_str()) {
            return Err(invalid("カラムが存在しないグループを参照しています"));
        }
        crate::mml::validate_sound_choice(&c.notify_sound_choice)?;
        let mut c = c.clone();
        c.account_id = local_id.clone();
        columns.push(c);
    }

    if let Some(root) = &d.pane_layout {
        let mut leaves = Vec::new();
        collect_leaf_group_ids(root, &mut leaves);
        if leaves.iter().any(|g| !group_ids.contains(g)) {
            return Err(invalid("ペイン配置が存在しないグループを参照しています"));
        }
    }
    crate::mml::validate_sound_choice(&d.notify.sound_choice)?;

    let mut ui = d.ui;
    ui.background_kind = None;
    ui.background_path = None;
    ui.default_account_id = id_map.get(&ui.default_account_id).cloned().unwrap_or_default();
    ui.note_cache_limit = ui.note_cache_limit.clamp(0, 100_000);
    ui.note_cache_max_age_days = ui.note_cache_max_age_days.clamp(0, 3_650);
    ui.note_cache_max_size_mb = ui.note_cache_max_size_mb.clamp(0, 10_000);
    ui.gap_fill_limit = ui.gap_fill_limit.clamp(0, 1_000);

    let preview = ImportPreview {
        column_count: columns.len() as i32,
        account_count: d.accounts.len() as i32,
        accounts_needing_login: accounts_needing_login.len() as i32,
        has_custom_css: !ui.custom_css.trim().is_empty(),
        has_custom_themes: !ui.custom_themes.is_empty() || !ui.custom_syntax_themes.is_empty(),
    };
    let data = SettingsData {
        accounts,
        groups: d.groups,
        columns,
        mute: d.mute,
        notify: d.notify,
        ui,
        pane_layout: d.pane_layout,
        cache_backend: current.cache_backend.clone(),
        server_mute_snapshots: current.server_mute_snapshots.clone(),
    };
    Ok(ImportPlan { data, preview, accounts_needing_login })
}
```

`ui.note_cache_limit` などの型が `i32` でない場合は、`clamp` の定数の型を合わせる(フィールドの型は `src-tauri/src/domain/ui.rs` を見る)。

- [ ] **Step 4: 通ることを確認する**

Run: `cd src-tauri && cargo test --lib store::settings_transfer::`
Expected: すべて PASS。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/store/settings_transfer.rs
git commit -m "feat(settings): インポートの検証とアカウントIDの付け替えを追加 (#544)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: コマンド・ファイルI/O・登録・バインディング

**Files:**
- Modify: `src-tauri/src/commands/mute.rs`(`write_file_bytes` の追加、`apply_mute_config` を `pub(crate)` に)
- Create: `src-tauri/src/commands/settings_transfer.rs`
- Modify: `src-tauri/src/commands/mod.rs`(`pub mod settings_transfer;`)
- Modify: `src-tauri/src/lib.rs`(`collect_commands!` に4つ追加)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: Task 1 `AppState::has_token`、Task 2 `snapshot` / `replace`、Task 3・4 の `build_export` / `parse_export` / `plan_import` / 型、`commands::mute::{read_file_bytes, apply_mute_config}`、`ConnectionManager::close(&self, column_id: &str)`、`AccountManager::{with_accounts, set_active, active_id, list}`
- Produces(フロントが使う。生成される名前):
  - `commands.exportSettings(path: string): Promise<Result<null, Error>>`
  - `commands.inspectSettingsFile(path: string): Promise<Result<ImportPreview, Error>>`
  - `commands.importSettings(path: string): Promise<Result<ImportResult, Error>>`
  - `commands.accountsWithoutToken(): Promise<Result<string[], Error>>`
  - 型 `ImportPreview`、`ImportResult`

- [ ] **Step 1: `commands/mute.rs` を変更する**

`apply_mute_config` の可視性を変える:

```rust
pub(crate) async fn apply_mute_config(state: &AppState, config: MuteConfig) -> Result<()> {
```

`read_file_bytes` の直後に `write_file_bytes` を追加する。`FsExt` は Android でだけ `use` されているので、Android 側では既存の `use tauri_plugin_fs::FsExt;` で足りる。

```rust
/// ファイルへ書く共通処理(Issue #544)。
///
/// Android は SAF の保存ダイアログが `content://` URI を返し、`tokio::fs::write` では書けない
/// (ENOENT になる)ため、`tauri-plugin-fs` 経由で書く。`write` + `truncate` は Android 側で
/// `"wt"` モードになり、上書き時に古い末尾が残らない。
pub(crate) async fn write_file_bytes(
    #[cfg_attr(not(target_os = "android"), allow(unused_variables))] app: &AppHandle,
    path: &str,
    bytes: &[u8],
) -> Result<()> {
    #[cfg(target_os = "android")]
    {
        use std::io::Write;
        let app = app.clone();
        let path_owned = path.to_string();
        let bytes = bytes.to_vec();
        let file_path: tauri_plugin_fs::FilePath = path.parse().unwrap();
        tauri::async_runtime::spawn_blocking(move || -> std::io::Result<()> {
            let mut opts = tauri_plugin_fs::OpenOptions::new();
            opts.write(true).truncate(true);
            let mut file = app.fs().open(file_path, opts)?;
            file.write_all(&bytes)?;
            file.flush()
        })
        .await
        .map_err(|e| Error::Invalid(format!("cannot write file {path_owned}: {e}")))?
        .map_err(|e| Error::Invalid(format!("cannot write file {path_owned}: {e}")))
    }
    #[cfg(not(target_os = "android"))]
    {
        tokio::fs::write(path, bytes)
            .await
            .map_err(|e| Error::Invalid(format!("cannot write file {path}: {e}")))
    }
}
```

- [ ] **Step 2: 失敗するテストを書く(`commands/settings_transfer.rs` を新規作成)**

`commands/mod.rs` に `pub mod settings_transfer;` を追加する(アルファベット順で `scheduled` の次)。

```rust
//! 設定のエクスポート/インポート(Issue #544)。純粋ロジックは `store::settings_transfer`。
//! ファイルの読み書きはコマンドだけが行い、中身の処理は `*_bytes` 関数に分けてテストする。

use crate::commands::mute::{apply_mute_config, read_file_bytes, write_file_bytes};
use crate::error::Result;
use crate::session::AccountManager;
use crate::state::AppState;
use crate::store::settings_transfer::{build_export, parse_export, plan_import, ImportPreview, ImportResult};
use tauri::{AppHandle, State};

fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

/// 書き出すJSON(整形済み)。
pub(crate) fn export_settings_bytes(state: &AppState) -> Result<Vec<u8>> {
    let file = build_export(&state.settings.snapshot(), env!("CARGO_PKG_VERSION"), &now_rfc3339());
    Ok(serde_json::to_vec_pretty(&file)?)
}

/// 検証だけ行い、概要を返す(何も書き込まない)。
pub(crate) fn inspect_settings_bytes(state: &AppState, bytes: &[u8]) -> Result<ImportPreview> {
    let file = parse_export(bytes)?;
    Ok(plan_import(&state.settings.snapshot(), file)?.preview)
}

/// 検証し、保存し、実行中の状態を更新する。保存に失敗したら何も変わらない。
pub(crate) async fn import_settings_bytes(state: &AppState, bytes: &[u8]) -> Result<ImportResult> {
    let file = parse_export(bytes)?;
    let current = state.settings.snapshot();
    let plan = plan_import(&current, file)?;

    // 置き換え前のカラムのストリームは、保存に成功してから止める(失敗時に何も変えないため)。
    state.settings.replace(plan.data.clone())?;
    for column in &current.columns {
        state.connections.close(&column.id);
    }

    // 実行中のアカウント一覧を差し替える。選択中のアカウントは、残っていれば維持する。
    {
        let mut accounts = state.accounts.lock().unwrap();
        let previous_active = accounts.active_id().map(str::to_owned);
        let mut next = AccountManager::with_accounts(plan.data.accounts.clone());
        if let Some(id) = previous_active {
            let _ = next.set_active(&id);
        }
        *accounts = next;
    }
    state.refresh_fetch_allowlist(); // トークンなしのアカウントのホストは入らない
    // ミュート設定を反映し、カラムごとのbackfill境界を捨てる(カラムが置き換わったため)。
    apply_mute_config(state, plan.data.mute.clone()).await?;

    Ok(ImportResult {
        column_count: plan.data.columns.len() as i32,
        accounts_needing_login: plan.accounts_needing_login,
    })
}

/// トークンを持たないアカウントのID。
pub(crate) fn accounts_without_token_ids(state: &AppState) -> Vec<String> {
    state
        .accounts
        .lock()
        .unwrap()
        .list()
        .into_iter()
        .filter(|a| !state.has_token(&a.id))
        .map(|a| a.id)
        .collect()
}

/// 設定を `path` に書き出す。Androidでは `path` は `content://` URI でもよい。
#[tauri::command]
#[specta::specta]
pub async fn export_settings(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<()> {
    let bytes = export_settings_bytes(&state)?;
    write_file_bytes(&app, &path, &bytes).await
}

/// 設定ファイルを検証して概要を返す。何も書き込まない。
#[tauri::command]
#[specta::specta]
pub async fn inspect_settings_file(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<ImportPreview> {
    let bytes = read_file_bytes(&app, &path).await?;
    inspect_settings_bytes(&state, &bytes)
}

/// 設定ファイルを読み込み、現在の設定を置き換える。呼び出し元はこのあとWebViewをリロードする。
#[tauri::command]
#[specta::specta]
pub async fn import_settings(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<ImportResult> {
    let bytes = read_file_bytes(&app, &path).await?;
    import_settings_bytes(&state, &bytes).await
}

/// トークンを持たないアカウントのID一覧。UIの「要ログイン」表示に使う。
#[tauri::command]
#[specta::specta]
pub async fn accounts_without_token(state: State<'_, AppState>) -> Result<Vec<String>> {
    Ok(accounts_without_token_ids(&state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Account, Column, ColumnGroup, ColumnKind, FilterQuery};
    use crate::store::settings_transfer::{parse_export, ExportData, ExportFile, EXPORT_FORMAT, EXPORT_VERSION};
    use crate::store::SettingsStore;

    fn account(id: &str, host: &str, user_id: &str) -> Account {
        Account {
            id: id.into(),
            host: host.into(),
            username: "me".into(),
            user_id: user_id.into(),
            display_name: "Me".into(),
            avatar_url: None,
            instance: None,
            is_cat: false,
            avatar_blurhash: None,
        }
    }

    fn column(id: &str, account_id: &str) -> Column {
        Column {
            id: id.into(),
            account_id: account_id.into(),
            kind: ColumnKind::Home,
            order: 0,
            filter: FilterQuery::Keywords(vec![]),
            notify_sound: false,
            notify_desktop: false,
            notify_sound_choice: String::new(),
            group_id: "g1".into(),
            title: None,
        }
    }

    /// `register_test_account("local1")` と同じアカウント(host=misskey.test, user=u1)を、
    /// 設定ファイル側にも保存した状態にする。
    fn state_with_local_account() -> AppState {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.register_test_account("local1");
        state.settings.upsert_account(&account("local1", "misskey.test", "u1")).unwrap();
        state
            .settings
            .upsert_group(&ColumnGroup { id: "old-g".into(), order: 0, width: 300, auto: false })
            .unwrap();
        state.settings.upsert_column(&Column { group_id: "old-g".into(), ..column("old-c", "local1") }).unwrap();
        state
    }

    fn file_bytes(data: ExportData) -> Vec<u8> {
        serde_json::to_vec(&ExportFile {
            format: EXPORT_FORMAT.into(),
            version: EXPORT_VERSION,
            exported_at: "2026-11-02T00:00:00Z".into(),
            app_version: "0.0.0".into(),
            data,
        })
        .unwrap()
    }

    fn data_with(accounts: Vec<Account>, columns: Vec<Column>) -> ExportData {
        ExportData {
            accounts,
            groups: vec![ColumnGroup { id: "g1".into(), order: 0, width: 300, auto: false }],
            columns,
            mute: Default::default(),
            notify: Default::default(),
            ui: Default::default(),
            pane_layout: None,
        }
    }

    #[test]
    fn export_bytes_parse_back_and_contain_no_token() {
        let state = state_with_local_account();

        let bytes = export_settings_bytes(&state).unwrap();

        let file = parse_export(&bytes).unwrap();
        assert_eq!(file.data.accounts.len(), 1);
        assert_eq!(file.data.columns.len(), 1);
        assert!(!String::from_utf8(bytes).unwrap().contains("\"token\""), "トークンは含めない");
    }

    #[test]
    fn inspect_does_not_change_anything() {
        let state = state_with_local_account();
        let bytes = file_bytes(data_with(vec![account("f1", "other.example", "u9")], vec![column("c1", "f1")]));

        let preview = inspect_settings_bytes(&state, &bytes).unwrap();

        assert_eq!(preview.column_count, 1);
        assert_eq!(preview.accounts_needing_login, 1);
        assert_eq!(state.settings.load_columns().unwrap()[0].id, "old-c", "検証だけでは何も置き換わらない");
    }

    #[tokio::test]
    async fn import_replaces_settings_keeps_the_local_account_and_reports_new_ones() {
        let state = state_with_local_account();
        let bytes = file_bytes(data_with(
            vec![account("file1", "misskey.test", "u1"), account("file2", "other.example", "u9")],
            vec![column("c1", "file1"), column("c2", "file2")],
        ));

        let result = import_settings_bytes(&state, &bytes).await.unwrap();

        assert_eq!(result.column_count, 2);
        assert_eq!(result.accounts_needing_login, vec!["file2".to_string()]);
        let columns = state.settings.load_columns().unwrap();
        let by_id = |id: &str| columns.iter().find(|c| c.id == id).unwrap().account_id.clone();
        assert_eq!(by_id("c1"), "local1", "端末側のアカウントIDに付け替わる");
        assert_eq!(by_id("c2"), "file2");
        assert!(columns.iter().all(|c| c.id != "old-c"), "端末側のカラムは置き換わる");
        assert_eq!(state.accounts.lock().unwrap().list().len(), 2, "実行中のアカウント一覧も更新される");
        assert_eq!(accounts_without_token_ids(&state), vec!["file2".to_string()]);
    }

    #[tokio::test]
    async fn import_never_adds_hosts_of_token_less_accounts_to_the_fetch_allowlist() {
        let state = state_with_local_account();
        let bytes = file_bytes(data_with(
            vec![account("evil1", "127.0.0.1", "u8"), account("evil2", "internal.example", "u9")],
            vec![],
        ));

        import_settings_bytes(&state, &bytes).await.unwrap();

        assert!(!state.fetch_allowlist.contains_host("127.0.0.1"));
        assert!(!state.fetch_allowlist.contains_host("internal.example"));
        assert!(state.fetch_allowlist.contains_host("misskey.test"), "トークンのあるアカウントは残る");
    }

    #[tokio::test]
    async fn a_rejected_file_changes_nothing() {
        let state = state_with_local_account();
        let bytes = file_bytes(data_with(vec![], vec![column("c1", "ghost-account")]));

        assert!(import_settings_bytes(&state, &bytes).await.is_err());

        assert_eq!(state.settings.load_columns().unwrap()[0].id, "old-c");
        assert_eq!(state.accounts.lock().unwrap().list().len(), 1);
    }
}
```

- [ ] **Step 3: 失敗を確認する**

`lib.rs` への登録はまだしていないので、先にコンパイルと単体テストだけ見る。

Run: `cd src-tauri && cargo test --lib commands::settings_transfer::`
Expected: すべて PASS。コンパイルエラーが出た場合は次を確認する。
- `chrono::DateTime::<Utc>::from_timestamp` が無い → `Cargo.toml` の chrono のバージョンと features(`std`, `alloc`)を確認。
- `AccountManager::active_id` の `#[allow(dead_code)]` は、使うようになったので外してよい(外さなくても警告にならない)。

- [ ] **Step 4: コマンドを登録する**

`src-tauri/src/lib.rs` の `collect_commands!` の `commands::mute::sync_server_mutes,` の次に追加する。

```rust
            commands::settings_transfer::export_settings,
            commands::settings_transfer::inspect_settings_file,
            commands::settings_transfer::import_settings,
            commands::settings_transfer::accounts_without_token,
```

- [ ] **Step 5: バインディングを再生成して確認する**

Run: `cd src-tauri && cargo test --lib generates_frontend_bindings`
Expected: PASS。

Run: `git diff --stat frontend/src/bindings/tauri.gen.ts && grep -n "exportSettings\|inspectSettingsFile\|importSettings\|accountsWithoutToken\|export type ImportPreview\|export type ImportResult" frontend/src/bindings/tauri.gen.ts`
Expected: 4つのコマンドと2つの型が出力される。

Run: `cd src-tauri && cargo test`
Expected: すべて PASS。

Android のコンパイルは、ローカルにターゲットがあれば `cd src-tauri && cargo check --target aarch64-linux-android` で確認する(NDKが無くて通らない場合は、CI の `android-build` ジョブに任せる。その旨を最終報告に書く)。

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/commands/mute.rs src-tauri/src/commands/settings_transfer.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat(settings): 設定のエクスポート/インポートのコマンドを追加 (#544)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: 再認証でそのアカウントの未表示カラムを開く

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`(`completeAccount` `:1186` 付近)
- Create: `frontend/src/lib/store.svelte.resumeColumns.test.ts`

**Interfaces:**
- Consumes: 既存の `commands.listColumns()` / `commands.resumeColumn(id)`、`#insertTab(opened)`、`#captureInitial(id, notes)`、`#logFailure(e)`
- Produces: `completeAccount(sessionId)` の挙動の追加(シグネチャは同じ)

背景: `boot()` は、カラムの復元に失敗したタブ(トークンなしアカウントのカラムなど)を挿入せずにログだけ出して続ける。いまは再認証してもそのカラムが開き直されず、再起動まで表示されない。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/store.svelte.resumeColumns.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Account, Column, OpenedColumn } from "../bindings/tauri.gen";
import type { TabView } from "./store.svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue({ status: "ok", data: null }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { app } = await import("./store.svelte");
const { commands } = await import("./ipc");

const account: Account = {
  id: "acc1",
  host: "misskey.test",
  username: "me",
  userId: "u1",
  displayName: "Me",
  avatarUrl: null,
  instance: null,
  isCat: false,
  avatarBlurhash: null,
};

function column(id: string, accountId: string): Column {
  return {
    id,
    accountId,
    kind: { type: "home" },
    order: 0,
    filter: { kind: "keywords", value: [] },
    notifySound: false,
    notifyDesktop: false,
    notifySoundChoice: "",
    groupId: "g1",
    title: null,
  };
}

function opened(c: Column): OpenedColumn {
  return { column: c, group: { id: "g1", order: 0, width: 300, auto: false }, notes: [], notifications: [] };
}

function shownTab(id: string): TabView {
  return {
    id,
    accountId: "acc1",
    kind: { type: "home" },
    title: "ホーム",
    customTitle: null,
    filter: { kind: "keywords", value: [] },
    notifyDesktop: false,
    notifySound: false,
    notifySoundChoice: "",
    notes: [],
    notifications: [],
    state: "connected",
    loadingMore: false,
    gapMarker: null,
    fillingGap: false,
    selectedNoteId: null,
    selectionMoveSeq: 0,
    epoch: 0,
  };
}

beforeEach(() => {
  app.accounts = [account];
  app.groups = [{ id: "g1", width: 300, auto: false, tabs: [shownTab("c1")], activeTabId: "c1" }];
  vi.spyOn(commands, "completeMiauth").mockResolvedValue({ status: "ok", data: account } as never);
  vi.spyOn(commands, "syncServerMutes").mockResolvedValue({
    status: "ok",
    data: { blockedUsers: 0, wordRules: 0 },
  } as never);
});

afterEach(() => {
  vi.restoreAllMocks();
  app.groups = [];
  app.accounts = [];
});

describe("再認証後のカラム復元(Issue #544)", () => {
  it("再認証したアカウントのカラムのうち、まだ表示されていないものだけを開く", async () => {
    vi.spyOn(commands, "listColumns").mockResolvedValue({
      status: "ok",
      data: [column("c1", "acc1"), column("c2", "acc1"), column("c3", "other")],
    } as never);
    const resume = vi
      .spyOn(commands, "resumeColumn")
      .mockResolvedValue({ status: "ok", data: opened(column("c2", "acc1")) } as never);

    await app.completeAccount("session1");

    expect(resume).toHaveBeenCalledTimes(1);
    expect(resume).toHaveBeenCalledWith("c2");
    expect(app.groups[0].tabs.map((t) => t.id)).toEqual(["c1", "c2"]);
  });

  it("開き直しに失敗しても、再認証自体は成功のままにする", async () => {
    vi.spyOn(commands, "listColumns").mockResolvedValue({
      status: "ok",
      data: [column("c2", "acc1")],
    } as never);
    vi.spyOn(commands, "resumeColumn").mockResolvedValue({
      status: "error",
      error: { kind: "unauthorized", message: "no token" },
    } as never);

    await expect(app.completeAccount("session1")).resolves.toBeUndefined();

    expect(app.groups[0].tabs.map((t) => t.id)).toEqual(["c1"]);
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && npx vitest run src/lib/store.svelte.resumeColumns.test.ts`
Expected: 1つ目が FAIL(`resumeColumn` が呼ばれない)。

- [ ] **Step 3: 実装する**

`store.svelte.ts` の `completeAccount` を変更し、その直前に private メソッドを追加する。

```ts
  /// 再認証・追加の直後に、そのアカウントのカラムのうち画面に出ていないものを開き直す。
  /// 設定のインポート(Issue #544)でトークンなしのまま取り込んだアカウントのカラムは、
  /// 起動時の resumeColumn が認証エラーで失敗してタブが挿入されない。認証が通った時点で、
  /// 再起動なしに表示する。失敗はログのみ(再認証自体の成否には影響させない)。
  async #resumeAccountColumns(accountId: string) {
    try {
      const shown = new Set(this.groups.flatMap((g) => g.tabs.map((t) => t.id)));
      const defs = await unwrap(commands.listColumns());
      for (const def of defs) {
        if (def.accountId !== accountId || shown.has(def.id)) continue;
        try {
          const opened = await unwrap(commands.resumeColumn(def.id));
          this.#insertTab(opened);
          this.#captureInitial(opened.column.id, opened.notes);
        } catch (e) {
          this.#logFailure(e);
        }
      }
    } catch (e) {
      this.#logFailure(e);
    }
  }

  async completeAccount(sessionId: string) {
    const account = await unwrap(commands.completeMiauth(sessionId));
    const existingIdx = this.accounts.findIndex((a) => a.id === account.id);
    if (existingIdx === -1) {
      this.accounts = [...this.accounts, account];
      this.#log("success", `アカウントを追加: @${account.username}@${account.host}`);
    } else {
      this.accounts = this.accounts.map((a, i) => (i === existingIdx ? account : a));
      this.#log("success", `再認証しました: @${account.username}@${account.host}`);
    }
    await this.#syncServerMutes(account.id);
    await this.#resumeAccountColumns(account.id);
  }
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd frontend && npx vitest run src/lib/store.svelte.resumeColumns.test.ts && pnpm check`
Expected: PASS。`pnpm check` はエラー0(テスト内の型は `as never` で通す。通らない箇所があれば、バインディングの型に合わせて直す)。

Run: `cd frontend && pnpm test`
Expected: すべて PASS。

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.resumeColumns.test.ts
git commit -m "feat(store): 再認証したアカウントの未表示カラムを再起動なしで開く (#544)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: 設定画面のエクスポート/インポートUI

**Files:**
- Create: `frontend/src/ui/settings/SettingsTransfer.svelte`
- Create: `frontend/src/ui/settings/SettingsTransfer.test.ts`
- Modify: `frontend/src/ui/settings/DataSection.svelte`(`<SaveBar ...>` の直前に組み込む)

**Interfaces:**
- Consumes: Task 5 の `commands.exportSettings` / `inspectSettingsFile` / `importSettings`、型 `ImportPreview`、`unwrap`(`lib/ipc`)、`ConfirmDialog`(`ui/ConfirmDialog.svelte`、props: `title` `message` `confirmLabel` `cancelLabel` `danger` `onConfirm` `onCancel`)、`@tauri-apps/plugin-dialog` の `save` / `open`
- Produces: `SettingsTransfer.svelte`(props: `reload?: () => void`、既定は `window.location.reload()`)

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/settings/SettingsTransfer.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import type { ImportPreview } from "../../bindings/tauri.gen";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const dialog = await import("@tauri-apps/plugin-dialog");
const { commands } = await import("../../lib/ipc");
const { default: SettingsTransfer } = await import("./SettingsTransfer.svelte");

const preview: ImportPreview = {
  columnCount: 3,
  accountCount: 2,
  accountsNeedingLogin: 1,
  hasCustomCss: true,
  hasCustomThemes: false,
};

let reload: ReturnType<typeof vi.fn>;

beforeEach(() => {
  reload = vi.fn();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.mocked(dialog.open).mockReset();
  vi.mocked(dialog.save).mockReset();
});

describe("SettingsTransfer: エクスポート", () => {
  it("保存先を選ぶと書き出し、完了を表示する", async () => {
    vi.mocked(dialog.save).mockResolvedValue("/tmp/out.json");
    const spy = vi.spyOn(commands, "exportSettings").mockResolvedValue({ status: "ok", data: null } as never);
    render(SettingsTransfer, { props: { reload } });

    await fireEvent.click(screen.getByText("エクスポート…"));

    await vi.waitFor(() => expect(screen.getByText("書き出しました")).toBeTruthy());
    expect(spy).toHaveBeenCalledWith("/tmp/out.json");
    const options = vi.mocked(dialog.save).mock.calls[0][0]!;
    expect(options.defaultPath).toMatch(/^tsumugi-settings-\d{8}\.json$/);
  });

  it("保存先を選ばずに閉じたら、何もしない", async () => {
    vi.mocked(dialog.save).mockResolvedValue(null);
    const spy = vi.spyOn(commands, "exportSettings");
    render(SettingsTransfer, { props: { reload } });

    await fireEvent.click(screen.getByText("エクスポート…"));

    await vi.waitFor(() => expect(dialog.save).toHaveBeenCalled());
    expect(spy).not.toHaveBeenCalled();
  });

  it("書き出しに失敗したらエラーを表示する", async () => {
    vi.mocked(dialog.save).mockResolvedValue("/tmp/out.json");
    vi.spyOn(commands, "exportSettings").mockResolvedValue({
      status: "error",
      error: { kind: "invalid", message: "cannot write file" },
    } as never);
    render(SettingsTransfer, { props: { reload } });

    await fireEvent.click(screen.getByText("エクスポート…"));

    await vi.waitFor(() => expect(screen.getByText(/cannot write file/)).toBeTruthy());
  });
});

describe("SettingsTransfer: インポート", () => {
  it("検証の概要を確認ダイアログに出し、承認すると取り込んでリロードする", async () => {
    vi.mocked(dialog.open).mockResolvedValue("/tmp/in.json");
    vi.spyOn(commands, "inspectSettingsFile").mockResolvedValue({ status: "ok", data: preview } as never);
    const importSpy = vi
      .spyOn(commands, "importSettings")
      .mockResolvedValue({ status: "ok", data: { columnCount: 3, accountsNeedingLogin: ["a2"] } } as never);
    render(SettingsTransfer, { props: { reload } });

    await fireEvent.click(screen.getByText("インポート…"));

    await vi.waitFor(() => expect(screen.getByText(/カラム3件・アカウント2件/)).toBeTruthy());
    expect(screen.getByText(/1件のアカウントは、取り込み後に再認証が必要/)).toBeTruthy();
    expect(screen.getByText(/カスタムCSSを含みます/)).toBeTruthy();
    expect(screen.getByText(/現在の設定とカラムは置き換えられます/)).toBeTruthy();
    expect(importSpy).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByText("インポート"));

    await vi.waitFor(() => expect(reload).toHaveBeenCalledTimes(1));
    expect(importSpy).toHaveBeenCalledWith("/tmp/in.json");
  });

  it("確認でキャンセルしたら、取り込まない", async () => {
    vi.mocked(dialog.open).mockResolvedValue("/tmp/in.json");
    vi.spyOn(commands, "inspectSettingsFile").mockResolvedValue({ status: "ok", data: preview } as never);
    const importSpy = vi.spyOn(commands, "importSettings");
    render(SettingsTransfer, { props: { reload } });
    await fireEvent.click(screen.getByText("インポート…"));
    await vi.waitFor(() => expect(screen.getByText(/カラム3件/)).toBeTruthy());

    await fireEvent.click(screen.getByText("キャンセル"));

    expect(importSpy).not.toHaveBeenCalled();
    expect(reload).not.toHaveBeenCalled();
    expect(screen.queryByText(/カラム3件/)).toBeNull();
  });

  it("ファイルを選ばずに閉じたら、検証しない", async () => {
    vi.mocked(dialog.open).mockResolvedValue(null);
    const inspect = vi.spyOn(commands, "inspectSettingsFile");
    render(SettingsTransfer, { props: { reload } });

    await fireEvent.click(screen.getByText("インポート…"));

    await vi.waitFor(() => expect(dialog.open).toHaveBeenCalled());
    expect(inspect).not.toHaveBeenCalled();
  });

  it("検証に落ちたらエラーを表示し、確認ダイアログは出さない", async () => {
    vi.mocked(dialog.open).mockResolvedValue("/tmp/bad.json");
    vi.spyOn(commands, "inspectSettingsFile").mockResolvedValue({
      status: "error",
      error: { kind: "invalid", message: "tsumugiの設定ファイルではありません" },
    } as never);
    render(SettingsTransfer, { props: { reload } });

    await fireEvent.click(screen.getByText("インポート…"));

    await vi.waitFor(() => expect(screen.getByText(/tsumugiの設定ファイルではありません/)).toBeTruthy());
    expect(screen.queryByText(/現在の設定とカラムは置き換えられます/)).toBeNull();
  });

  it("取り込みに失敗したらリロードせず、エラーを表示する", async () => {
    vi.mocked(dialog.open).mockResolvedValue("/tmp/in.json");
    vi.spyOn(commands, "inspectSettingsFile").mockResolvedValue({ status: "ok", data: preview } as never);
    vi.spyOn(commands, "importSettings").mockResolvedValue({
      status: "error",
      error: { kind: "invalid", message: "disk full" },
    } as never);
    render(SettingsTransfer, { props: { reload } });
    await fireEvent.click(screen.getByText("インポート…"));
    await vi.waitFor(() => expect(screen.getByText(/カラム3件/)).toBeTruthy());

    await fireEvent.click(screen.getByText("インポート"));

    await vi.waitFor(() => expect(screen.getByText(/disk full/)).toBeTruthy());
    expect(reload).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && npx vitest run src/ui/settings/SettingsTransfer.test.ts`
Expected: FAIL(`SettingsTransfer.svelte` が無い)。

- [ ] **Step 3: コンポーネントを書く**

`frontend/src/ui/settings/SettingsTransfer.svelte`:

```svelte
<script lang="ts">
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "../ConfirmDialog.svelte";
  import { commands, unwrap, type ImportPreview } from "../../lib/ipc";

  // インポート後にWebViewを読み込み直す。テストでは差し替える。
  let { reload = () => window.location.reload() }: { reload?: () => void } = $props();

  const filters = [{ name: "JSON", extensions: ["json"] }];

  let busy = $state(false);
  let err = $state<string | null>(null);
  let message = $state<string | null>(null);
  let pendingPath = $state<string | null>(null);
  let preview = $state<ImportPreview | null>(null);

  function defaultFileName(): string {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    return `tsumugi-settings-${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}.json`;
  }

  function previewMessage(p: ImportPreview): string {
    const lines = [`カラム${p.columnCount}件・アカウント${p.accountCount}件を読み込みます。`];
    if (p.accountsNeedingLogin > 0) {
      lines.push(`うち${p.accountsNeedingLogin}件のアカウントは、取り込み後に再認証が必要です。`);
    }
    if (p.hasCustomCss) lines.push("カスタムCSSを含みます。");
    if (p.hasCustomThemes) lines.push("カスタムテーマを含みます。");
    lines.push("現在の設定とカラムは置き換えられます。");
    return lines.join("\n");
  }

  async function exportSettings() {
    err = null;
    message = null;
    busy = true;
    try {
      const path = await saveDialog({ defaultPath: defaultFileName(), filters });
      if (!path) return;
      await unwrap(commands.exportSettings(path));
      message = "書き出しました";
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  async function pickImportFile() {
    err = null;
    message = null;
    busy = true;
    try {
      const picked = await openDialog({ multiple: false, directory: false, filters });
      if (!picked || Array.isArray(picked)) return;
      preview = await unwrap(commands.inspectSettingsFile(picked));
      pendingPath = picked;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  function cancelImport() {
    pendingPath = null;
    preview = null;
  }

  async function confirmImport() {
    const path = pendingPath;
    cancelImport();
    if (!path) return;
    err = null;
    busy = true;
    try {
      await unwrap(commands.importSettings(path));
      reload();
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h4 class="mb-2 mt-4 text-sm font-semibold text-muted-foreground">設定のバックアップ</h4>

<div class="mb-2 flex flex-wrap gap-2">
  <Button type="button" variant="outline" disabled={busy} onclick={exportSettings}>エクスポート…</Button>
  <Button type="button" variant="outline" disabled={busy} onclick={pickImportFile}>インポート…</Button>
</div>
<p class="mb-2 mt-0 text-xs text-muted-foreground">
  アカウント一覧・カラム構成・各種設定(カスタムCSSを含む)をファイルに書き出します。トークンは含みません。
  インポートすると現在の設定とカラムが置き換わり、取り込んだアカウントは再認証が必要になります。
  背景画像・動画とノートキャッシュは対象外です。
</p>
{#if message}
  <p class="mb-2 mt-0 text-sm text-muted-foreground" data-testid="settings-transfer-message">{message}</p>
{/if}
{#if err}
  <p class="mb-2 mt-0 text-sm text-destructive" data-testid="settings-transfer-error">{err}</p>
{/if}

{#if preview && pendingPath}
  <ConfirmDialog
    title="設定をインポート"
    message={previewMessage(preview)}
    confirmLabel="インポート"
    danger
    onConfirm={confirmImport}
    onCancel={cancelImport}
  />
{/if}
```

`DataSection.svelte` に組み込む。`import` を足し、`<SaveBar ...>` の直前に置く。

```svelte
  import SettingsTransfer from "./SettingsTransfer.svelte";
```

```svelte
<SettingsTransfer />

<SaveBar {busy} {saved} {err} onsave={save} />
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/settings/SettingsTransfer.test.ts && pnpm check`
Expected: PASS、`pnpm check` はエラー0。`text-destructive` などのクラスが `docs/design/style-guide.md` の規約と食い違わないか確認する(`AccountsSection.svelte` / `SaveBar.svelte` のエラー表示と合わせる)。

Run: `cd frontend && pnpm test`
Expected: すべて PASS(`DataSection` を描画する既存テストがあれば、`@tauri-apps/plugin-dialog` のモックに `save` が無くて落ちる場合がある。その場合は、そのテストのモックに `save: vi.fn()` を足す)。

- [ ] **Step 5: コミット**

```bash
git add frontend/src/ui/settings/SettingsTransfer.svelte frontend/src/ui/settings/SettingsTransfer.test.ts frontend/src/ui/settings/DataSection.svelte
git commit -m "feat(settings): 設定のエクスポート/インポートのUIを追加 (#544)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: 「要ログイン」バッジ

**Files:**
- Modify: `frontend/src/ui/settings/AccountsSection.svelte`
- Create: `frontend/src/ui/settings/AccountsSection.test.ts`

**Interfaces:**
- Consumes: Task 5 の `commands.accountsWithoutToken(): Promise<Result<string[], Error>>`、`app.accounts`
- Produces: アカウント行に「要ログイン」バッジ、そのアカウントの「再認証」ボタンを強調(`variant="default"`)

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/settings/AccountsSection.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/svelte";
import type { Account } from "../../bindings/tauri.gen";
import { app } from "../../lib/store.svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { commands } = await import("../../lib/ipc");
const { default: AccountsSection } = await import("./AccountsSection.svelte");

function account(id: string, username: string): Account {
  return {
    id,
    host: "misskey.test",
    username,
    userId: `u-${id}`,
    displayName: username,
    avatarUrl: null,
    instance: null,
    isCat: false,
    avatarBlurhash: null,
  };
}

beforeEach(() => {
  app.accounts = [account("a1", "alice"), account("a2", "bob")];
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  app.accounts = [];
});

describe("AccountsSection: 要ログイン(Issue #544)", () => {
  it("トークンを持たないアカウントの行にだけ「要ログイン」を出す", async () => {
    vi.spyOn(commands, "accountsWithoutToken").mockResolvedValue({ status: "ok", data: ["a2"] } as never);
    const { container } = render(AccountsSection, { props: { onAddAccount: vi.fn(), onReauth: vi.fn() } });

    await vi.waitFor(() => expect(screen.getAllByText("要ログイン")).toHaveLength(1));
    const row = (id: string) => container.querySelector(`[data-account-row-id="${id}"]`)!;
    expect(row("a2").textContent).toContain("要ログイン");
    expect(row("a1").textContent).not.toContain("要ログイン");
  });

  it("全員がトークンを持っていれば、バッジは出ない", async () => {
    vi.spyOn(commands, "accountsWithoutToken").mockResolvedValue({ status: "ok", data: [] } as never);
    render(AccountsSection, { props: { onAddAccount: vi.fn(), onReauth: vi.fn() } });

    await vi.waitFor(() => expect(commands.accountsWithoutToken).toHaveBeenCalled());
    expect(screen.queryByText("要ログイン")).toBeNull();
  });

  it("取得に失敗してもアカウント一覧は表示する", async () => {
    vi.spyOn(commands, "accountsWithoutToken").mockResolvedValue({
      status: "error",
      error: { kind: "invalid", message: "boom" },
    } as never);
    render(AccountsSection, { props: { onAddAccount: vi.fn(), onReauth: vi.fn() } });

    await vi.waitFor(() => expect(commands.accountsWithoutToken).toHaveBeenCalled());
    expect(screen.getByText("@alice@misskey.test")).toBeTruthy();
    expect(screen.queryByText("要ログイン")).toBeNull();
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && npx vitest run src/ui/settings/AccountsSection.test.ts`
Expected: 1つ目が FAIL(バッジが無い)。

- [ ] **Step 3: 実装する**

`AccountsSection.svelte` の `<script>` に追加する(`import` に `unwrap` と `commands` を足す)。

```ts
  import { commands, unwrap } from "../../lib/ipc";

  // トークンを持たないアカウントのID(設定のインポート直後など。Issue #544)。
  // app.accounts が変わる(再認証・追加・削除)たびに取り直す。取得に失敗してもバッジが出ないだけ。
  let needsLogin = $state<Set<string>>(new Set());
  $effect(() => {
    void app.accounts;
    void (async () => {
      try {
        needsLogin = new Set(await unwrap(commands.accountsWithoutToken()));
      } catch {
        needsLogin = new Set();
      }
    })();
  });
```

行の表示部分を変更する。「既定」バッジの次に、同じ書式のバッジを追加する:

```svelte
          <div class="overflow-hidden text-ellipsis whitespace-nowrap text-sm font-semibold">{a.displayName || a.username}{#if a.id === app.defaultAccountId()}<span class="default-badge ml-1.5 rounded px-1.5 py-px text-[0.68rem] font-semibold text-primary">既定</span>{/if}{#if needsLogin.has(a.id)}<span class="ml-1.5 rounded px-1.5 py-px text-[0.68rem] font-semibold text-destructive" data-testid="needs-login-badge">要ログイン</span>{/if}</div>
```

「再認証」ボタンを、要ログインのときだけ強調する:

```svelte
          <Button type="button" variant={needsLogin.has(a.id) ? "default" : "outline"} size="xs" onclick={() => onReauth(a)}>再認証</Button>
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/settings/AccountsSection.test.ts && pnpm check && pnpm test`
Expected: すべて PASS、`pnpm check` はエラー0。

- [ ] **Step 5: コミット**

```bash
git add frontend/src/ui/settings/AccountsSection.svelte frontend/src/ui/settings/AccountsSection.test.ts
git commit -m "feat(settings): トークンを持たないアカウントに「要ログイン」を表示する (#544)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 9: ドキュメント

**Files:**
- Modify: `docs/guide/user-guide.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: ユーザーガイドを書く**

Run: `grep -n "データ\|ノートキャッシュ" docs/guide/user-guide.md | head`
設定の「データ」タブの説明(`ノートキャッシュ` の保持上限などを書いている箇所)を探し、その直後に次の節を足す。見出しの階層と文体は周囲に合わせる。

```markdown
### 設定のバックアップ(エクスポート/インポート)

設定の「データ」タブの「設定のバックアップ」で、設定をファイルに書き出したり、読み込んだりできます。別のPCへの引っ越しやバックアップに使えます。

- **エクスポート**: アカウント一覧・カラム構成・各種設定(カスタムCSS・カスタムテーマ・キーマップを含む)をJSONファイルに書き出します。**トークンは含まれません。**背景画像・動画、ノートキャッシュ、下書き、予約投稿も含まれません。
- **インポート**: ファイルを選ぶと、読み込む内容(カラム数・アカウント数など)が表示されます。承認すると、**現在の設定とカラムがファイルの内容で置き換わり**、アプリが読み込み直されます。背景は「なし」に戻ります。
  - すでにログインしているアカウントと同じアカウント(ホストとユーザーが同じ)は、そのままのログイン状態で引き継がれます。
  - ファイルにだけあるアカウントは「要ログイン」として追加されます。設定の「アカウント」で「再認証」を押してログインすると、そのアカウントのカラムが表示されます。
  - ファイルに無い、この端末のアカウントは残ります(そのアカウントのカラムは置き換えで消えます)。
- 共有されたファイルには、カスタムCSSなど画面の見た目を変える内容が含まれることがあります。出どころの分からないファイルはインポートしないでください。
- Androidでも使えます。保存先・読み込み元は、端末のファイル選択画面で選びます。
```

- [ ] **Step 2: CLAUDE.md の構成の説明に1行足す**

`CLAUDE.md` の「`store/` — persistence.」の項の末尾に、次の文を足す。

```
設定のエクスポート/インポート(Issue #544)の純粋ロジックは `store/settings_transfer.rs`(`parse_export` / `build_export` / `plan_import`)、I/Oとアプリ状態の更新は `commands/settings_transfer.rs`。インポートしたトークンなしアカウントのホストは `fetch_allowlist` に入れない(`AppState::refresh_fetch_allowlist` はトークンを持つアカウントだけを見る)。
```

- [ ] **Step 3: コミット**

```bash
git add docs/guide/user-guide.md CLAUDE.md
git commit -m "docs: 設定のエクスポート/インポートの使い方と構成を追記 (#544)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 10: 実機確認とPR

**Files:** なし(確認と、必要なら修正コミット)

実機確認は、記憶の方針どおりレビュー通過だけで済ませない。**設計書の「実装前の検証項目」のうち、実機でしか確かめられない1〜4と6がここで解消される。** どれかが期待どおりでなければ、実装を直す前に設計書へ戻って利用者に報告する。

- [ ] **Step 1: 自動テストを全部流す**

Run: `cd src-tauri && cargo test && cd ../frontend && pnpm check && pnpm test`
Expected: すべて PASS。

- [ ] **Step 2: デスクトップの実機確認(隔離インスタンス)**

`e2e/README.md` と記憶(`reference-isolated-instance-needs-short-xdg-path`、`feedback-dev-server-verification-must-use-virtual-display`、`reference-real-screen-check-via-e2e-compose`)の手順に従う。Xvfb + `dbus-run-session` + `/tmp` 直下の短いXDGパスで隔離インスタンスを2つ(端末A・B)起動し、`WAYLAND_DISPLAY` は unset する。ローカルのMisskey(`e2e/` のCompose)にログインする。

確認項目:
1. 端末Aでカラムを複数作り、カスタムCSS・キーマップ・背景を設定する。「データ」タブで**エクスポート**し、ファイルにトークンと背景パスが含まれないことを `grep -c token` などで確認する(**検証項目1の前提**)。
2. 端末Bで**インポート**する。確認ダイアログの内容(カラム数・アカウント数・要ログイン数)が正しい。
3. 承認後、アプリが読み込み直され、設定(カスタムCSS・キーマップ)が反映される。背景は「なし」。**カラムはまだ出ない**(トークンなしのため)。アカウント一覧に「要ログイン」が出る(**検証項目6**)。
4. 「再認証」でログインすると、**再起動なしで**そのアカウントのカラムが出て、ストリームが流れる(**検証項目1**: リロード後の再接続)。
5. 同じアカウントで既にログイン済みの端末Bに、端末Aのファイルをインポートすると、ログイン状態を保ったままカラムだけが置き換わる。
6. 壊れたファイル(中身が `{}`、拡張子だけJSON、バージョン違い)を選ぶと、現在の設定が変わらず、エラーが表示される。
7. 後始末: 起動したプロセスは `ps aux` で確認した**PID指定**で kill する(`pkill` / `killall` は使わない)。起動した Docker コンテナは `docker rm -fv <id>`。

- [ ] **Step 3: Androidの実機確認(エミュレーター)**

`CLAUDE.md` の Android の節に従い、`cd src-tauri && cargo tauri android build --debug --target aarch64` でビルドし、エミュレーターで確認する。操作は `adb shell input tap` の本物のタップで行う。

確認項目:
1. 保存ダイアログが開き、選んだ場所に**書き込める**(**検証項目2**)。書き込み後のファイルの中身が正しいJSONで、古い末尾が残っていない(同じファイルへ上書きして確認)。
2. 保存ダイアログの初期ファイル名が効くか(**検証項目3**。効かなくても機能に影響はないので、結果を記録するだけ)。
3. 開くダイアログで、JSON以外のファイルも選べてしまう場合に、内容検証で弾かれる(**検証項目4**)。
4. 読み込み → 確認 → リロード → 再認証までデスクトップと同じ流れで通る。
5. 設定画面のボタンがモバイル幅で折り返し、はみ出さない。

書き込みに失敗する場合: 実装を足す前に止めて報告する(設計書の「実装前の検証項目」2のフォールバックを、利用者に選んでもらう)。

- [ ] **Step 4: 確認結果で見つかった問題を直す**

見つかった問題は、再現するテストを先に書いてから直し、1問題1コミットにする。

- [ ] **Step 5: プッシュしてPRを作る**

```bash
git push -u origin feat/issue-544-settings-import-export
```

`.github/pull_request_template.md` を読み、その構成に沿って本文を書く(`--body` を渡すとテンプレートが適用されないため、構成は手で合わせる)。本文に次を含める。
- 概要(エクスポート/インポート、全置換、トークンなしアカウントの扱い、Android対応)
- `Fixes #544`
- 実機確認の結果(デスクトップとAndroidで、Step 2・3 のどこまで確認したか。確認できなかった項目は明記する)
- セキュリティ上の変更点(`fetch_allowlist` をトークン付きアカウントのホストだけにした。信頼できないファイルの検証)
- 末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)`

```bash
gh pr create --title "feat: 設定のインポートとエクスポート" --body-file <作成した本文ファイル>
```

push後、CIの結果をMonitorや待機ループで追わない(利用者が自分で確認する)。マージは行わない。

---

## Self-Review(計画を書いたあとの確認)

**1. 設計書のカバー:**

| 設計書の項目 | 担当タスク |
|---|---|
| ファイル形式(`format` / `version` / BOM / サイズ上限) | Task 3 |
| 除外項目(`cache_backend`、スナップショット、背景) | Task 3(エクスポート)、Task 4(インポートで引き継ぎ・リセット) |
| アカウントの対応づけとID付け替え | Task 4 |
| 検証(ID重複、参照切れ、ホスト名、MML、丸め) | Task 4 |
| `SettingsStore::replace` の原子性 | Task 2 |
| 適用(アカウント一覧、ミュート、接続、許可リスト) | Task 5 |
| SSRF対策(再構築と起動時の両方) | Task 1、Task 5 のテスト |
| 4つのコマンドと型、`specta` 登録、バインディング | Task 5 |
| Android(`write_file_bytes`、`read_file_bytes` の再利用) | Task 5、Task 10 |
| UI(エクスポート/インポート、確認ダイアログ) | Task 7 |
| 要ログインの表示 | Task 8 |
| 再認証後のカラム復元(設計書に無かった抜け) | Task 6 |
| ユーザーガイド | Task 9 |
| 実装前の検証項目 1〜7 | 5(コード確認)→Task 1・5、7(コード確認)→Task 1、1〜4・6→Task 10 |

設計書に無かったもの: **Task 6(再認証でカラムを開き直す)**。設計書の「実装前の検証項目」6 の延長で、読み取りの結果、再認証だけではカラムが出ないことが分かったため足した。`boot()` が失敗したタブを挿入しない挙動は、既存コードのままで、変更しない。

設計書の記述とずれた点を2つ、この計画の前に設計書を直してコミット済み: `set_ui_prefs` は範囲チェックをしない(丸めはRustが使う4項目だけにした)、起動時の許可リスト構築も対象にした。

**2. プレースホルダーの確認:** 「TBD」「後で」「適切に処理」の類は無い。条件分岐として書いた「通らなければ〜」は、Task 4 のフィールド型、Task 5 のコンパイルエラー、Task 7 の既存テストのモックの3か所。いずれも該当ファイルを見れば決まる内容で、確認先を書いてある。

**3. 型と名前の整合:** `SettingsData`(`pub(crate)`、Task 2)→ `build_export` / `plan_import`(Task 3・4)→ `import_settings_bytes`(Task 5)。`ImportPreview { column_count, account_count, accounts_needing_login, has_custom_css, has_custom_themes }` は Task 3 で定義し、Task 4 の期待値・Task 5 のテスト・Task 7 の `preview.columnCount` などで同じ名前。`ImportResult { column_count, accounts_needing_login }` は Task 3・5・7 で一致。`has_token`(Task 1)→ `accounts_without_token_ids`(Task 5)。コマンド名 `export_settings` → `exportSettings`、`inspect_settings_file` → `inspectSettingsFile`、`import_settings` → `importSettings`、`accounts_without_token` → `accountsWithoutToken`(Task 5・7・8 で一致)。

**4. Review Focus の対応:** 1・2・4 → Task 4 のテスト、3・4 → Task 3 のテスト、5 → Task 6 のテスト、6 → Task 1 と Task 5 のテスト。

**5. 範囲外として、テストしていないもの(計画の限界):**
- 置き換え前のカラムのストリーム接続が閉じること(`connections.close`)。Tauriのランタイムが無いと接続を開けないため、単体テストにできない。Task 10 の実機確認(リロード後に二重接続しない)で見る。
- Android の `write_file_bytes` の分岐。デスクトップではコンパイルされないため、CIの `android-build` ジョブと Task 10 のエミュレーター確認が検証になる。
