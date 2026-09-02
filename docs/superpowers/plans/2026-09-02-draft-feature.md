# 下書き機能 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ComposeBar(投稿入力欄)の内容を手動保存(複数件、一覧から呼び出し)または自動一時保存(1件、再度開いた時に復元)で下書きとして永続化し、投稿成功時には使った下書きを片付ける。

**Architecture:** `store/draft.rs` に `settings.rs` と同型のJSON永続化(`DraftStore`、`app_config_dir/drafts.json`)を新設し、`commands/draft.rs` の6コマンドで公開する。フロントは `ComposeBar.svelte` に下書きポップオーバーUIと自動保存(デバウンス)・自動復元・投稿後クリーンアップのロジックを追加する。復元済み添付ファイルの再取得用に `api/drive.rs::show_file` + `commands::note::get_drive_file` を追加する。

**Tech Stack:** Rust(rusqliteは使わずserde_json + tmp書き込み→rename)、Tauri v2 command、Svelte 5 runes、Vitest + @testing-library/svelte。

## Global Constraints

- 対象Issue: #251「下書き機能」。ブランチは `feature/issue-251-draft-feature`(spec commit済み)で作業を続ける。
- 下書きはアカウントごとに分離する。
- 手動下書きは常に新規追加(既存の上書き編集はしない)。
- 自動下書きはアカウントごとに1件のみ(upsert)。
- 添付ファイルは `AttachmentItem.kind === "drive"`(アップロード済み)のみ下書きに含める。ローカル/クリップボード添付は保存時に除外する。
- 返信/引用先ノートは保存時点のスナップショット(`id`/`username`/`text`)のみ保持し、復元時にMisskeyへ再取得しない。
- コミットメッセージは件名のみ(本文/箇条書き無し)。`git commit`失敗時はリトライせず報告する。
- 参照spec: `docs/superpowers/specs/2026-09-02-draft-feature-design.md`

---

### Task 1: `DraftStore`(JSON永続化) — `store/draft.rs`

**Files:**
- Create: `src-tauri/src/store/draft.rs`
- Modify: `src-tauri/src/store/mod.rs`

**Interfaces:**
- Consumes: なし(新規モジュール)。`crate::api::notes::{VisibilityInput, ReactionAcceptanceInput}` を使う。
- Produces(Task 3が使う):
  - `pub struct DraftStore` — `DraftStore::new(path: PathBuf) -> Result<Self>`、`#[cfg(test)] DraftStore::new_in_memory() -> Self`
  - `pub fn list_manual(&self, account_id: &str) -> Result<Vec<Draft>>`(更新日時降順)
  - `pub fn save_manual(&self, account_id: &str, input: &DraftInput) -> Result<()>`
  - `pub fn delete_manual(&self, account_id: &str, draft_id: &str) -> Result<()>`
  - `pub fn get_auto(&self, account_id: &str) -> Result<Option<Draft>>`
  - `pub fn save_auto(&self, account_id: &str, input: &DraftInput) -> Result<()>`
  - `pub fn clear_auto(&self, account_id: &str) -> Result<()>`
  - `pub enum DraftKind { Manual, Auto }`
  - `pub struct Draft { id, account_id, kind, text, cw, visibility, local_only, reaction_acceptance, channel_id, poll: Option<PollDraftSnapshot>, file_ids: Vec<String>, reply_note: Option<DraftNoteSnapshot>, quote_note: Option<DraftNoteSnapshot>, created_at: i64, updated_at: i64 }`
  - `pub struct DraftInput { text, cw, visibility, local_only, reaction_acceptance, channel_id, poll, file_ids, reply_note, quote_note }`(`Draft`からid/account_id/kind/created_at/updated_atを除いたもの)
  - `pub struct PollDraftSnapshot { choices: Vec<String>, multiple: bool, expires_at: Option<i64> }`
  - `pub struct DraftNoteSnapshot { id: String, username: String, text: Option<String> }`

- [ ] **Step 1: 型定義とJSON永続化の骨格を書く**

`src-tauri/src/store/draft.rs`:

```rust
//! 下書き(未送信の投稿の保存)の永続化。settings.rs と同様、プレーンテキスト(JSON)で
//! 1ファイルに保存する(app_config_dir/drafts.json)。ノートキャッシュ(SQLite, 破棄前提)
//! とは異なり下書きはユーザが書いた再取得不能なデータのため、キャッシュDBには置かない。
//! 自動下書き(2秒デバウンスで書き込まれうる)を手動下書き/設定本体と分けるため、
//! settings.json とは別ファイルに分離する。

use crate::api::notes::{ReactionAcceptanceInput, VisibilityInput};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DraftKind {
    Manual,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PollDraftSnapshot {
    pub choices: Vec<String>,
    pub multiple: bool,
    #[specta(type = Option<specta_typescript::Number>)]
    pub expires_at: Option<i64>,
}

/// 返信/引用先ノートの表示用最小スナップショット(ComposeBarのバナー表示に必要な分のみ)。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DraftNoteSnapshot {
    pub id: String,
    pub username: String,
    pub text: Option<String>,
}

/// 下書き保存/更新の入力(id/account_id/kind/created_at/updated_atはStore側が管理)。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DraftInput {
    pub text: String,
    pub cw: Option<String>,
    pub visibility: VisibilityInput,
    pub local_only: bool,
    pub reaction_acceptance: ReactionAcceptanceInput,
    pub channel_id: Option<String>,
    pub poll: Option<PollDraftSnapshot>,
    pub file_ids: Vec<String>,
    pub reply_note: Option<DraftNoteSnapshot>,
    pub quote_note: Option<DraftNoteSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub id: String,
    pub account_id: String,
    pub kind: DraftKind,
    pub text: String,
    pub cw: Option<String>,
    pub visibility: VisibilityInput,
    pub local_only: bool,
    pub reaction_acceptance: ReactionAcceptanceInput,
    pub channel_id: Option<String>,
    pub poll: Option<PollDraftSnapshot>,
    pub file_ids: Vec<String>,
    pub reply_note: Option<DraftNoteSnapshot>,
    pub quote_note: Option<DraftNoteSnapshot>,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
    #[specta(type = specta_typescript::Number)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DraftData {
    #[serde(default)]
    drafts: Vec<Draft>,
}

enum Backing {
    File(PathBuf),
    #[cfg(test)]
    Memory,
}

pub struct DraftStore {
    backing: Backing,
    data: Mutex<DraftData>,
}

impl DraftStore {
    pub fn new(path: PathBuf) -> Result<Self> {
        let data = load_json_or_default(&path)?;
        Ok(Self { backing: Backing::File(path), data: Mutex::new(data) })
    }

    #[cfg(test)]
    pub(crate) fn new_in_memory() -> Self {
        Self { backing: Backing::Memory, data: Mutex::new(DraftData::default()) }
    }

    #[allow(irrefutable_let_patterns)]
    fn save(&self, data: &DraftData) -> Result<()> {
        if let Backing::File(path) = &self.backing {
            let json = serde_json::to_string_pretty(data)?;
            let tmp_path = path.with_extension("json.tmp");
            std::fs::write(&tmp_path, json)?;
            std::fs::rename(&tmp_path, path)?;
        }
        Ok(())
    }
}

fn load_json_or_default(path: &Path) -> Result<DraftData> {
    if !path.exists() {
        return Ok(DraftData::default());
    }
    let s = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&s)?)
}

fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
```

- [ ] **Step 2: コンパイルを確認する**

Run: `cd src-tauri && cargo check`
Expected: エラー無し(未使用importの警告のみ許容)。

- [ ] **Step 3: `save_manual`/`list_manual` の失敗するテストを書く**

`draft.rs` 末尾に追加:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn input(text: &str) -> DraftInput {
        DraftInput {
            text: text.into(),
            cw: None,
            visibility: VisibilityInput::Public,
            local_only: false,
            reaction_acceptance: ReactionAcceptanceInput::All,
            channel_id: None,
            poll: None,
            file_ids: vec![],
            reply_note: None,
            quote_note: None,
        }
    }

    #[test]
    fn save_manual_appends_and_list_manual_returns_newest_first() {
        let s = DraftStore::new_in_memory();
        s.save_manual("acc1", &input("first")).unwrap();
        s.save_manual("acc1", &input("second")).unwrap();
        let list = s.list_manual("acc1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].text, "second"); // 新しい順
        assert_eq!(list[1].text, "first");
        assert!(list.iter().all(|d| matches!(d.kind, DraftKind::Manual)));
    }
}
```

- [ ] **Step 4: テストを実行して失敗を確認する**

Run: `cd src-tauri && cargo test --lib store::draft::tests::save_manual_appends_and_list_manual_returns_newest_first`
Expected: FAIL(`save_manual`/`list_manual` メソッドが存在せずコンパイルエラー)

- [ ] **Step 5: `save_manual`/`list_manual` を実装する**

`impl DraftStore` に追加:

```rust
    pub fn save_manual(&self, account_id: &str, input: &DraftInput) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        let now = now_millis();
        guard.drafts.push(Draft {
            id: uuid::Uuid::new_v4().to_string(),
            account_id: account_id.to_string(),
            kind: DraftKind::Manual,
            text: input.text.clone(),
            cw: input.cw.clone(),
            visibility: input.visibility,
            local_only: input.local_only,
            reaction_acceptance: input.reaction_acceptance,
            channel_id: input.channel_id.clone(),
            poll: input.poll.clone(),
            file_ids: input.file_ids.clone(),
            reply_note: input.reply_note.clone(),
            quote_note: input.quote_note.clone(),
            created_at: now,
            updated_at: now,
        });
        self.save(&guard)
    }

    pub fn list_manual(&self, account_id: &str) -> Result<Vec<Draft>> {
        let guard = self.data.lock().unwrap();
        let mut list: Vec<Draft> = guard
            .drafts
            .iter()
            .filter(|d| d.account_id == account_id && matches!(d.kind, DraftKind::Manual))
            .cloned()
            .collect();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(list)
    }
```

- [ ] **Step 6: テストを実行して成功を確認する**

Run: `cd src-tauri && cargo test --lib store::draft::tests::save_manual_appends_and_list_manual_returns_newest_first`
Expected: PASS

- [ ] **Step 7: `delete_manual` のテストを書いて実行し、失敗を確認する**

テスト追加:

```rust
    #[test]
    fn delete_manual_removes_only_target_draft() {
        let s = DraftStore::new_in_memory();
        s.save_manual("acc1", &input("keep")).unwrap();
        s.save_manual("acc1", &input("drop")).unwrap();
        let id_to_drop = s.list_manual("acc1").unwrap()[0].id.clone(); // "drop"(新しい方)
        s.delete_manual("acc1", &id_to_drop).unwrap();
        let list = s.list_manual("acc1").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].text, "keep");
    }
```

Run: `cd src-tauri && cargo test --lib store::draft::tests::delete_manual_removes_only_target_draft`
Expected: FAIL(コンパイルエラー、`delete_manual`未定義)

- [ ] **Step 8: `delete_manual` を実装しテストを通す**

```rust
    pub fn delete_manual(&self, account_id: &str, draft_id: &str) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard.drafts.retain(|d| {
            !(d.account_id == account_id && d.id == draft_id && matches!(d.kind, DraftKind::Manual))
        });
        self.save(&guard)
    }
```

Run: `cd src-tauri && cargo test --lib store::draft::tests::delete_manual_removes_only_target_draft`
Expected: PASS

- [ ] **Step 9: 自動下書き(`get_auto`/`save_auto`/`clear_auto`)のテストを書いて実行し、失敗を確認する**

テスト追加:

```rust
    #[test]
    fn save_auto_upserts_single_draft_per_account() {
        let s = DraftStore::new_in_memory();
        s.save_auto("acc1", &input("v1")).unwrap();
        s.save_auto("acc1", &input("v2")).unwrap();
        let got = s.get_auto("acc1").unwrap().expect("auto draft should exist");
        assert_eq!(got.text, "v2");
        assert!(matches!(got.kind, DraftKind::Auto));
        // 手動下書きの一覧には出ない
        assert!(s.list_manual("acc1").unwrap().is_empty());
    }

    #[test]
    fn get_auto_returns_none_when_unset() {
        let s = DraftStore::new_in_memory();
        assert!(s.get_auto("acc1").unwrap().is_none());
    }

    #[test]
    fn clear_auto_removes_the_auto_draft() {
        let s = DraftStore::new_in_memory();
        s.save_auto("acc1", &input("v1")).unwrap();
        s.clear_auto("acc1").unwrap();
        assert!(s.get_auto("acc1").unwrap().is_none());
    }

    #[test]
    fn auto_drafts_are_isolated_per_account() {
        let s = DraftStore::new_in_memory();
        s.save_auto("acc1", &input("a1")).unwrap();
        s.save_auto("acc2", &input("a2")).unwrap();
        assert_eq!(s.get_auto("acc1").unwrap().unwrap().text, "a1");
        assert_eq!(s.get_auto("acc2").unwrap().unwrap().text, "a2");
    }
```

Run: `cd src-tauri && cargo test --lib store::draft::tests`
Expected: FAIL(コンパイルエラー、`get_auto`/`save_auto`/`clear_auto`未定義)

- [ ] **Step 10: 自動下書きメソッドを実装しテストを通す**

```rust
    pub fn get_auto(&self, account_id: &str) -> Result<Option<Draft>> {
        let guard = self.data.lock().unwrap();
        Ok(guard
            .drafts
            .iter()
            .find(|d| d.account_id == account_id && matches!(d.kind, DraftKind::Auto))
            .cloned())
    }

    pub fn save_auto(&self, account_id: &str, input: &DraftInput) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard
            .drafts
            .retain(|d| !(d.account_id == account_id && matches!(d.kind, DraftKind::Auto)));
        let now = now_millis();
        guard.drafts.push(Draft {
            id: uuid::Uuid::new_v4().to_string(),
            account_id: account_id.to_string(),
            kind: DraftKind::Auto,
            text: input.text.clone(),
            cw: input.cw.clone(),
            visibility: input.visibility,
            local_only: input.local_only,
            reaction_acceptance: input.reaction_acceptance,
            channel_id: input.channel_id.clone(),
            poll: input.poll.clone(),
            file_ids: input.file_ids.clone(),
            reply_note: input.reply_note.clone(),
            quote_note: input.quote_note.clone(),
            created_at: now,
            updated_at: now,
        });
        self.save(&guard)
    }

    pub fn clear_auto(&self, account_id: &str) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard
            .drafts
            .retain(|d| !(d.account_id == account_id && matches!(d.kind, DraftKind::Auto)));
        self.save(&guard)
    }
```

Run: `cd src-tauri && cargo test --lib store::draft::tests`
Expected: PASS(全テスト)

- [ ] **Step 11: JSONファイルへの実書き込み・再読み込みを検証するテストを追加する**

```rust
    #[test]
    fn persists_to_plain_text_json_file_and_reloads() {
        let path = std::env::temp_dir().join(format!("tsumugi-drafts-test-{}.json", uuid::Uuid::new_v4()));
        {
            let s = DraftStore::new(path.clone()).unwrap();
            s.save_manual("acc1", &input("hello")).unwrap();
        }
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"hello\""));
        assert!(serde_json::from_str::<serde_json::Value>(&raw).is_ok());

        let reloaded = DraftStore::new(path.clone()).unwrap();
        assert_eq!(reloaded.list_manual("acc1").unwrap().len(), 1);

        std::fs::remove_file(&path).ok();
    }
```

Run: `cd src-tauri && cargo test --lib store::draft::tests`
Expected: PASS(全テスト)

- [ ] **Step 12: `store/mod.rs` に公開する**

`src-tauri/src/store/mod.rs`:

```rust
pub mod db;
pub mod draft;
pub mod note_cache;
pub mod settings;
pub mod user_ref;

pub use draft::DraftStore;
pub use note_cache::NoteCacheStore;
pub use settings::SettingsStore;
```

- [ ] **Step 13: 全体ビルドを確認する**

Run: `cd src-tauri && cargo check`
Expected: エラー無し

- [ ] **Step 14: コミット**

```bash
git add src-tauri/src/store/draft.rs src-tauri/src/store/mod.rs
git commit -m "feat: 下書きのJSON永続化(DraftStore)を追加"
```

---

### Task 2: ドライブファイル単体取得 — `api/drive.rs::show_file` + `commands::note::get_drive_file`

**Files:**
- Modify: `src-tauri/src/api/drive.rs`
- Modify: `src-tauri/src/commands/note.rs`

**Interfaces:**
- Consumes: `crate::api::normalize::RawFile`、`crate::domain::DriveFile`、`crate::api::MisskeyClient`(いずれも`drive.rs`に既存import済み)。
- Produces(Task 3のlib.rs登録、Task 5のフロントが使う):
  - `pub async fn show_file(client: &MisskeyClient, file_id: &str) -> Result<DriveFile>`(`drive.rs`)
  - `#[tauri::command] pub async fn get_drive_file(state: State<'_, AppState>, account_id: String, file_id: String) -> Result<DriveFile>`(`commands/note.rs`)

- [ ] **Step 1: `show_file` を実装する**

`src-tauri/src/api/drive.rs` の `list_files` の直後に追加:

```rust
/// 単一ファイルのメタ情報を取得する(下書き復元時、保存済み file_id から添付情報を
/// 再構成するために使う)。対象が削除済み等で存在しない場合は Err を返す
/// (呼び出し側で個別に無視できるよう、ここではエラーを握りつぶさない)。
pub async fn show_file(client: &MisskeyClient, file_id: &str) -> Result<DriveFile> {
    let raw: RawFile = client.post("drive/files/show", &json!({ "fileId": file_id })).await?;
    Ok(raw.into())
}
```

- [ ] **Step 2: `get_drive_file` コマンドを追加する**

`src-tauri/src/commands/note.rs` の import に `show_file as api_show_file` を追加し、`list_drive_files` の直後にコマンドを追加:

```rust
use crate::api::drive::{
    list_files as api_list_files, list_folders as api_list_folders, show_file as api_show_file,
    upload_bytes as api_upload_bytes,
};
```

```rust
/// 単一ドライブファイルのメタ情報取得(下書き復元時の添付再構成用)。
#[tauri::command]
#[specta::specta]
pub async fn get_drive_file(
    state: State<'_, AppState>,
    account_id: String,
    file_id: String,
) -> Result<DriveFile> {
    let client = state.client_for(&account_id)?;
    api_show_file(&client, &file_id).await
}
```

- [ ] **Step 3: ビルドを確認する**

Run: `cd src-tauri && cargo check`
Expected: エラー無し(`get_drive_file`はまだ`specta_builder()`未登録のため未使用警告は出ない — commandは呼ばれる前提でdead_code警告の対象外)

- [ ] **Step 4: コミット**

```bash
git add src-tauri/src/api/drive.rs src-tauri/src/commands/note.rs
git commit -m "feat: ドライブファイル単体取得コマンドを追加"
```

---

### Task 3: `commands/draft.rs` + `AppState`配線 + `specta_builder()`登録

**Files:**
- Create: `src-tauri/src/commands/draft.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Task 1の `DraftStore`/`Draft`/`DraftInput`(`crate::store::draft`)、Task 2の `get_drive_file`。
- Produces(Task 5/6のフロントが`commands.listDrafts`等として呼ぶ、`tauri.gen.ts`に生成される):
  - `list_drafts(account_id: String) -> Vec<Draft>`
  - `save_draft(account_id: String, input: DraftInput) -> ()`
  - `delete_draft(account_id: String, draft_id: String) -> ()`
  - `get_auto_draft(account_id: String) -> Option<Draft>`
  - `save_auto_draft(account_id: String, input: DraftInput) -> ()`
  - `clear_auto_draft(account_id: String) -> ()`
  - `state.drafts: DraftStore`(`AppState`のフィールド)

- [ ] **Step 1: `AppState`に`drafts: DraftStore`を追加する**

`src-tauri/src/state.rs`:

```rust
use crate::store::{DraftStore, NoteCacheStore, SettingsStore};
```

`AppState`構造体に追加:

```rust
    pub settings: SettingsStore,
    pub drafts: DraftStore,
    pub cache: NoteCacheStore,
```

`AppState::new`/`new_with_sound`のシグネチャに`drafts: DraftStore`を追加して`Self { ..., settings, drafts, cache, ... }`に反映。`new_for_test`は`DraftStore::new_in_memory()`を渡すよう更新:

```rust
    pub fn new(secrets: Box<dyn SecretStore>, settings: SettingsStore, drafts: DraftStore, cache: NoteCacheStore) -> Self {
        Self::new_with_sound(secrets, settings, drafts, cache, SoundPlayer::spawn())
    }

    fn new_with_sound(
        secrets: Box<dyn SecretStore>,
        settings: SettingsStore,
        drafts: DraftStore,
        cache: NoteCacheStore,
        sound: SoundPlayer,
    ) -> Self {
        // ...(既存の中身はそのまま)... Self { ..., settings, drafts, cache, ... }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(settings: SettingsStore) -> Self {
        let cache = NoteCacheStore::new(crate::store::db::open_cache_in_memory().unwrap());
        Self::new_with_sound(
            Box::new(crate::session::MemoryStore::default()),
            settings,
            DraftStore::new_in_memory(),
            cache,
            SoundPlayer::new_for_test(),
        )
    }
```

- [ ] **Step 2: `state.rs`の既存テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib state::tests`
Expected: PASS(`new_for_test`のシグネチャは変えていないので既存呼び出し元に影響なし)

- [ ] **Step 3: `commands/draft.rs`を書く**

```rust
//! 下書き(未送信の投稿の保存/復元)系 command。

use crate::state::AppState;
use crate::store::draft::{Draft, DraftInput};
use crate::error::Result;
use tauri::State;

/// アカウントの手動保存下書き一覧(更新日時降順)。
#[tauri::command]
#[specta::specta]
pub async fn list_drafts(state: State<'_, AppState>, account_id: String) -> Result<Vec<Draft>> {
    state.drafts.list_manual(&account_id)
}

/// 手動下書きを新規保存する(既存の上書きはしない)。
#[tauri::command]
#[specta::specta]
pub async fn save_draft(state: State<'_, AppState>, account_id: String, input: DraftInput) -> Result<()> {
    state.drafts.save_manual(&account_id, &input)
}

/// 手動下書きを削除する。
#[tauri::command]
#[specta::specta]
pub async fn delete_draft(state: State<'_, AppState>, account_id: String, draft_id: String) -> Result<()> {
    state.drafts.delete_manual(&account_id, &draft_id)
}

/// アカウントの自動一時下書き(あれば1件)。
#[tauri::command]
#[specta::specta]
pub async fn get_auto_draft(state: State<'_, AppState>, account_id: String) -> Result<Option<Draft>> {
    state.drafts.get_auto(&account_id)
}

/// 自動一時下書きをupsertする(アカウントにつき1件)。
#[tauri::command]
#[specta::specta]
pub async fn save_auto_draft(state: State<'_, AppState>, account_id: String, input: DraftInput) -> Result<()> {
    state.drafts.save_auto(&account_id, &input)
}

/// 自動一時下書きを消す。
#[tauri::command]
#[specta::specta]
pub async fn clear_auto_draft(state: State<'_, AppState>, account_id: String) -> Result<()> {
    state.drafts.clear_auto(&account_id)
}
```

- [ ] **Step 4: `commands/mod.rs`に登録する**

```rust
pub mod account;
pub mod app;
pub mod clip;
pub mod column;
pub mod draft;
pub mod mute;
pub mod note;
pub mod sound;
pub mod user;
```

(既存の`pub use`ブロックは`clip`/`sound`同様、draftも追加しなくてよい — `lib.rs`はフルパス参照のため)

- [ ] **Step 5: `lib.rs`の`specta_builder()`にコマンドを登録する**

`commands::note::fetch_url_preview,`の直後に追加:

```rust
            commands::note::fetch_url_preview,
            commands::note::get_drive_file,
            commands::draft::list_drafts,
            commands::draft::save_draft,
            commands::draft::delete_draft,
            commands::draft::get_auto_draft,
            commands::draft::save_auto_draft,
            commands::draft::clear_auto_draft,
```

- [ ] **Step 6: `lib.rs`のsetup()で`drafts.json`を開き`AppState::new`に渡す**

`use store::{db, NoteCacheStore, SettingsStore};` を `use store::{db, DraftStore, NoteCacheStore, SettingsStore};` に変更。

`let settings_path = config_dir.join("settings.json");` の設定ブロックの後、`let cache_conn = ...` の前に追加:

```rust
            let drafts_path = config_dir.join("drafts.json");
            let drafts = DraftStore::new(drafts_path).expect("failed to open drafts file");
```

`app.manage(AppState::new(Box::new(KeyringStore), settings, cache));` を:

```rust
            app.manage(AppState::new(Box::new(KeyringStore), settings, drafts, cache));
```

- [ ] **Step 7: ビルドとテストを実行し、TSバインディングが再生成されることを確認する**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。加えて `frontend/src/bindings/tauri.gen.ts` に `listDrafts`/`saveDraft`/`deleteDraft`/`getAutoDraft`/`saveAutoDraft`/`clearAutoDraft`/`getDriveFile` と、型 `Draft`/`DraftInput`/`DraftKind`/`PollDraftSnapshot`/`DraftNoteSnapshot` が生成されていることを確認する:

```bash
grep -n "listDrafts\|DraftInput\|DraftNoteSnapshot" frontend/src/bindings/tauri.gen.ts
```

- [ ] **Step 8: Rustテスト全体を実行する**

Run: `cd src-tauri && cargo test`
Expected: PASS(既存テストも含め全て通る)

- [ ] **Step 9: コミット**

```bash
git add -A
git commit -m "feat: 下書きcommandを追加しAppState/specta_builderに配線する"
```

---

### Task 4: フロント下準備 — `replyTo`/`quoteOf`の型を最小化する

下書き復元時、返信/引用先ノートの「保存時点のスナップショット」(`id`/`username`/`text`のみ)から`replyTo`/`quoteOf`相当の状態を作れるようにするための準備。`ComposeBar.svelte`内で`replyTo`/`quoteOf`ローカル状態が実際に使われている箇所は「truthy判定」「`.id`」「`.user.username`」「`.text`」のみ(banner表示・submit・cancelContext・compact判定)であることをspec作成時に確認済み。型を最小化しても他の用途に影響しない。

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`

**Interfaces:**
- Produces(Task 5/6が使う): `type ComposeContextNote = { id: string; text: string | null; user: { username: string } }`。`replyTo`/`quoteOf`はこの型の`$state`になる。

- [ ] **Step 1: 型を定義し、`replyTo`/`quoteOf`の宣言を変更する**

`frontend/src/ui/ComposeBar.svelte`、`type AttachmentItem = ...`の直後あたりに追加:

```ts
  /// 返信/引用コンテキストとして保持する最小限の形。banner表示(user.username/text)と
  /// submit時の.id参照にしか使わないため、下書き復元時にNote全体を持たずに済むよう
  /// フルのNote型ではなくこの最小型で持つ。
  type ComposeContextNote = { id: string; text: string | null; user: { username: string } };
```

`let replyTo = $state<Note | undefined>(undefined);` と `let quoteOf = $state<Note | undefined>(undefined);` を、それぞれ:

```ts
  let replyTo = $state<ComposeContextNote | undefined>(undefined);
  let quoteOf = $state<ComposeContextNote | undefined>(undefined);
```

- [ ] **Step 2: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラー無し(`c.replyTo`/`c.quoteOf`は`Note | undefined`型で`ComposeContextNote`のスーパーセットのため代入互換)

- [ ] **Step 3: コミット**

```bash
git add frontend/src/ui/ComposeBar.svelte
git commit -m "refactor: replyTo/quoteOfを下書き復元に必要な最小の型にする"
```

---

### Task 5: フロント — 下書き一覧ポップオーバー(保存/呼び出し/削除)

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`

**Interfaces:**
- Consumes: Task 3のIPC(`commands.listDrafts`/`saveDraft`/`deleteDraft`/`getDriveFile`、`../lib/ipc`の`unwrapAcc`/`formatError`)、Task 4の`ComposeContextNote`。`bindings/tauri.gen.ts`の`Draft`/`DraftInput`型。
- Produces(Task 6が使う):
  - `function buildDraftInput(): DraftInput`
  - `function snapshotToContextNote(s: DraftNoteSnapshot): ComposeContextNote`
  - `async function loadDraft(d: Draft): Promise<void>`(状態復元。`loadedDraftId`をセット)
  - `let loadedDraftId = $state<string | null>(null);`(投稿成功時のクリーンアップでTask 6が読む)

- [ ] **Step 1: importとアイコン、状態変数を追加する**

`import { ImagePlus, SmilePlus, X } from "@lucide/svelte";` を:

```ts
  import { FileText, ImagePlus, SmilePlus, X } from "@lucide/svelte";
```

`bindings/tauri.gen.ts`の型importに追加:

```ts
  import type {
    NoteDraft_Deserialize as NoteDraft,
    VisibilityInput,
    ReactionAcceptanceInput,
    DriveFile,
    Note,
    SourceItem,
    Draft,
    DraftInput,
    DraftNoteSnapshot,
  } from "../bindings/tauri.gen";
```

`showEmojiPicker`関連の状態宣言の近くに追加:

```ts
  let showDraftMenu = $state(false);
  let draftMenuTrigger = $state<HTMLElement | null>(null);
  let draftMenuPos = $state<{ left: number; top: number } | null>(null);
  let manualDrafts = $state<Draft[]>([]);
  let draftsLoading = $state(false);
  /// 呼び出し中の手動下書きのID(投稿成功時にこれを自動削除する)。手動保存/新規入力/
  /// 自動下書き復元時はnullに戻す。
  let loadedDraftId = $state<string | null>(null);
```

- [ ] **Step 2: `pnpm check`でコンパイルエラーが無いことを確認する(未使用変数は許容)**

Run: `cd frontend && pnpm check`
Expected: 型エラー無し(未使用変数の警告は後続stepで解消)

- [ ] **Step 3: 下書きポップオーバーの開閉・一覧取得・変換ヘルパを実装する**

`toggleAttachMenu`の直後あたりに追加:

```ts
  function snapshotToContextNote(s: DraftNoteSnapshot): ComposeContextNote {
    return { id: s.id, text: s.text, user: { username: s.username } };
  }

  function contextNoteToSnapshot(n: ComposeContextNote): DraftNoteSnapshot {
    return { id: n.id, username: n.user.username, text: n.text };
  }

  async function loadManualDrafts() {
    if (!accountId) {
      manualDrafts = [];
      return;
    }
    draftsLoading = true;
    try {
      manualDrafts = await unwrapAcc(accountId, commands.listDrafts(accountId));
    } catch {
      manualDrafts = [];
    } finally {
      draftsLoading = false;
    }
  }

  function toggleDraftMenu() {
    if (showDraftMenu) {
      showDraftMenu = false;
      return;
    }
    const r = draftMenuTrigger?.getBoundingClientRect();
    if (r) draftMenuPos = { left: r.left, top: r.bottom + 4 };
    showDraftMenu = true;
    void loadManualDrafts();
  }
```

- [ ] **Step 4: 投票期限計算を`submit`から切り出して`buildDraftInput`と共有する**

`submit`内の以下を:

```ts
    const choices = pollChoices.map((s) => s.trim()).filter(Boolean);
    if (!text.trim() && !quoteOf && choices.length === 0 && attachments.length === 0) return;
    let expiresAt: number | null = null;
    if (pollExpiryMode === "at" && pollExpiresAt) {
      expiresAt = new Date(pollExpiresAt).getTime();
    } else if (pollExpiryMode === "after") {
      expiresAt = Date.now() + pollAfterAmount * POLL_AFTER_UNIT_MS[pollAfterUnit];
    }
```

以下のように、期限計算部分を関数化して置き換える:

```ts
  function computePollExpiresAt(): number | null {
    if (pollExpiryMode === "at" && pollExpiresAt) return new Date(pollExpiresAt).getTime();
    if (pollExpiryMode === "after") return Date.now() + pollAfterAmount * POLL_AFTER_UNIT_MS[pollAfterUnit];
    return null;
  }
```

`submit`内は:

```ts
    const choices = pollChoices.map((s) => s.trim()).filter(Boolean);
    if (!text.trim() && !quoteOf && choices.length === 0 && attachments.length === 0) return;
    const expiresAt = computePollExpiresAt();
```

- [ ] **Step 5: `buildDraftInput`を実装する**

`computePollExpiresAt`の直後に追加:

```ts
  function buildDraftInput(): DraftInput {
    const choices = pollChoices.map((s) => s.trim()).filter(Boolean);
    return {
      text,
      cw: useCw && cw.trim() ? cw : null,
      visibility,
      localOnly: useChannel || localOnly,
      reactionAcceptance,
      channelId: useChannel && channelId ? channelId : null,
      poll: usePoll && choices.length >= 2 ? { choices, multiple: pollMultiple, expiresAt: computePollExpiresAt() } : null,
      fileIds: attachments.flatMap((a) => (a.kind === "drive" ? [a.file.id] : [])),
      replyNote: replyTo ? contextNoteToSnapshot(replyTo) : null,
      quoteNote: quoteOf ? contextNoteToSnapshot(quoteOf) : null,
    };
  }
```

- [ ] **Step 6: 「下書き保存」「呼び出し」「削除」アクションを実装する**

`buildDraftInput`の直後に追加:

```ts
  async function saveCurrentAsDraft() {
    if (!accountId) return;
    try {
      await unwrapAcc(accountId, commands.saveDraft(accountId, buildDraftInput()));
      await loadManualDrafts();
    } catch (e) {
      err = String(e);
    }
  }

  async function deleteManualDraft(id: string) {
    if (!accountId) return;
    try {
      await unwrapAcc(accountId, commands.deleteDraft(accountId, id));
      manualDrafts = manualDrafts.filter((d) => d.id !== id);
      if (loadedDraftId === id) loadedDraftId = null;
    } catch (e) {
      err = String(e);
    }
  }

  async function loadDraft(d: Draft) {
    text = d.text;
    cw = d.cw ?? "";
    useCw = d.cw != null;
    visibility = d.visibility;
    localOnly = d.localOnly;
    reactionAcceptance = d.reactionAcceptance;
    if (d.channelId) {
      useChannel = true;
      channelId = d.channelId;
    } else {
      useChannel = false;
      channelId = "";
    }
    if (d.poll) {
      usePoll = true;
      const padded = [...d.poll.choices];
      while (padded.length < 2) padded.push("");
      pollChoices = padded;
      pollMultiple = d.poll.multiple;
      if (d.poll.expiresAt != null) {
        pollExpiryMode = "at";
        pollExpiresAt = new Date(d.poll.expiresAt).toISOString().slice(0, 16);
      } else {
        pollExpiryMode = "none";
        pollExpiresAt = "";
      }
    } else {
      usePoll = false;
      pollChoices = ["", ""];
      pollMultiple = false;
      pollExpiryMode = "none";
      pollExpiresAt = "";
    }
    replyTo = d.replyNote ? snapshotToContextNote(d.replyNote) : undefined;
    quoteOf = d.quoteNote ? snapshotToContextNote(d.quoteNote) : undefined;
    attachments = [];
    if (d.fileIds.length > 0 && accountId) {
      const acc = accountId;
      const results = await Promise.allSettled(
        d.fileIds.map((id) => unwrapAcc(acc, commands.getDriveFile(acc, id))),
      );
      attachments = results.flatMap((r) =>
        r.status === "fulfilled" ? [{ kind: "drive" as const, id: r.value.id, file: r.value }] : [],
      );
    }
    loadedDraftId = d.kind === "manual" ? d.id : null;
    showDraftMenu = false;
  }
```

- [ ] **Step 7: ツールバーにボタンとポップオーバーを追加する**

CWボタンの直前(`{#if useCw}`より前、`<div class="relative">`の`textarea`より前でもよいが、他ツールバーボタンと並べるため`投票`ボタンの直後)に配置する。`投稿`ボタンの並びの直前、ツールバー行に追加:

```svelte
      <Button
        type="button"
        variant="outline"
        size="icon-sm"
        title="下書き"
        bind:ref={draftMenuTrigger}
        onclick={toggleDraftMenu}
        disabled={busy || !accountId}
      ><FileText size={16} class="size-4" /></Button>
```

(`ReactionAcceptanceSelect`の直前あたり、他の`icon-sm`ボタンと並べる)

ポップオーバー本体は、既存の`{#if showAttachMenu && attachMenuPos}`ブロックの直後に追加:

```svelte
{#if showDraftMenu && draftMenuPos}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (showDraftMenu = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="fixed w-[280px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
      style={`left:${draftMenuPos.left}px;top:${draftMenuPos.top}px`}
      onclick={(e) => e.stopPropagation()}
      role="menu"
      tabindex="-1"
    >
      <button
        class="block w-full rounded-md px-2.5 py-[7px] text-left font-[inherit] text-sm text-foreground hover:bg-muted"
        type="button"
        onclick={saveCurrentAsDraft}
      >現在の内容を下書き保存</button>
      <div class="my-1 border-t border-border"></div>
      {#if draftsLoading}
        <div class="px-2.5 py-[7px] text-sm text-muted-foreground">読み込み中…</div>
      {:else if manualDrafts.length === 0}
        <div class="px-2.5 py-[7px] text-sm text-muted-foreground">保存済みの下書きはありません</div>
      {:else}
        <div class="max-h-[280px] overflow-y-auto">
          {#each manualDrafts as d (d.id)}
            <div class="flex items-center gap-1">
              <button
                class="min-w-0 flex-1 truncate rounded-md px-2.5 py-[7px] text-left font-[inherit] text-sm text-foreground hover:bg-muted"
                type="button"
                title={d.text}
                onclick={() => loadDraft(d)}
              >{d.text.trim() || "(本文なし)"}</button>
              <Button type="button" variant="ghost" size="icon-xs" class="flex-none text-muted-foreground" title="削除" onclick={() => deleteManualDraft(d.id)}><X size={12} /></Button>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}
```

- [ ] **Step 8: 型チェックとlintを実行する**

Run: `cd frontend && pnpm check`
Expected: エラー無し

- [ ] **Step 9: コミット**

```bash
git add frontend/src/ui/ComposeBar.svelte
git commit -m "feat: ComposeBarに下書き保存/呼び出し/削除のポップオーバーを追加"
```

---

### Task 6: フロント — 自動一時保存・自動復元・投稿後クリーンアップ

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`

**Interfaces:**
- Consumes: Task 5の`buildDraftInput`/`loadDraft`/`snapshotToContextNote`/`loadedDraftId`、Task 3の`commands.getAutoDraft`/`saveAutoDraft`/`clearAutoDraft`/`deleteDraft`。
- Produces: なし(ComposeBar内で完結)。

- [ ] **Step 1: 自動保存(デバウンス)の`$effect`を追加する**

`app.compose`を消費する`$effect`(`const c = app.compose; ...`)より**後**に追加する(復元判定がこの効果の実行後の`replyTo`/`quoteOf`を見られるようにするため)。既存の「アカウント切替時にchannelIdをリセットする」`$effect`の直後に追加:

```ts
  /// 自動一時保存: text/cw/添付/投票のいずれかが非空の間、入力変更を2秒デバウンスして
  /// save_auto_draftを呼ぶ。全て空になったらclear_auto_draftで消す(空の下書きを残さない)。
  let autoSaveTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    // 依存関係として拾うため、使う値をすべて先に読む
    const snapshot = { text, cw, useCw, attachmentsLen: attachments.length, usePoll, hasAccount: !!accountId };
    clearTimeout(autoSaveTimer);
    if (!snapshot.hasAccount) return;
    const acc = accountId!;
    const nonEmpty =
      snapshot.text.trim() !== "" ||
      (snapshot.useCw && snapshot.cw.trim() !== "") ||
      snapshot.attachmentsLen > 0 ||
      snapshot.usePoll;
    if (!nonEmpty) {
      void unwrapAcc(acc, commands.clearAutoDraft(acc)).catch(() => {});
      return;
    }
    autoSaveTimer = setTimeout(() => {
      void unwrapAcc(acc, commands.saveAutoDraft(acc, buildDraftInput())).catch(() => {});
    }, 2000);
    return () => clearTimeout(autoSaveTimer);
  });
```

- [ ] **Step 2: マウント時の自動復元を追加する**

同じ場所の直後に追加(`import { tick } from "svelte";`は既にimport済み。復元は初回マウント時1回のみでよいため`onMount`を使う):

```ts
  import { onMount } from "svelte";
```

(既存の`import { tick } from "svelte";`を`import { onMount, tick } from "svelte";`に変更する)

```ts
  onMount(() => {
    if (!accountId || text.trim() || replyTo || quoteOf) return;
    const acc = accountId;
    unwrapAcc(acc, commands.getAutoDraft(acc))
      .then((d) => {
        if (!d) return;
        // マウント後、他の初期化(app.compose消費など)で既に何か入力/文脈が付いていたら
        // 上書きしない
        if (text.trim() || replyTo || quoteOf) return;
        void loadDraft(d);
      })
      .catch(() => {});
  });
```

- [ ] **Step 3: 投稿成功時のクリーンアップを追加する**

`submit`内、`await app.postNote(accountId, draft);`の直後(既存のフォームリセット処理の前)に追加:

```ts
      await app.postNote(accountId, draft);
      const draftToDelete = loadedDraftId;
      void unwrapAcc(accountId, commands.clearAutoDraft(accountId)).catch(() => {});
      if (draftToDelete) {
        void unwrapAcc(accountId, commands.deleteDraft(accountId, draftToDelete)).catch(() => {});
      }
      loadedDraftId = null;
```

- [ ] **Step 4: 手動保存/新規入力時に`loadedDraftId`をリセットする**

`saveCurrentAsDraft`は新規下書きとして保存するだけで「呼び出し中」の扱いにはしない(既に正しい、変更不要)。ただし、ユーザが下書きを呼び出した後にさらに手で編集して「別の」下書きとして保存した場合、元の`loadedDraftId`の追跡が残ると意図せず削除されてしまうため、`saveCurrentAsDraft`成功時は`loadedDraftId`をリセットしない(呼び出し中の下書きを保存し直したいユースケースは本Issueの対象外・YAGNI節に明記済み)。この挙動を明示するコメントを`loadedDraftId`宣言の横に追加する(Task 5 Step 1で追加済みのコメントで足りているため、追加コード変更は無し)。

- [ ] **Step 5: `pnpm check`を実行する**

Run: `cd frontend && pnpm check`
Expected: エラー無し

- [ ] **Step 6: 手動で動作確認する(自動化テストはTask 7で追加)**

Run: `cargo tauri dev`(リポジトリルートから)
Expected: ComposeBarに文字を入力→2秒待つ→「下書き」ボタンを開いても一覧には出ない(自動下書きのため)。「現在の内容を下書き保存」→一覧に反映される。ComposeBarを空にする→再度開いても自動復元されない。テキストを入力したままウィンドウを再起動 → 次回起動時に自動復元される。確認できたら`cargo tauri dev`のプロセスを終了する。

- [ ] **Step 7: コミット**

```bash
git add frontend/src/ui/ComposeBar.svelte
git commit -m "feat: ComposeBarの自動一時保存・自動復元・投稿後クリーンアップを追加"
```

---

### Task 7: フロントテスト — `ComposeBar.test.ts`

**Files:**
- Create: `frontend/src/ui/ComposeBar.test.ts`

**Interfaces:**
- Consumes: `ComposeBar.svelte`(Task 4-6で完成したもの)、`../lib/store.svelte`の`app`、`../bindings/tauri.gen`の`commands`(モック対象)。

- [ ] **Step 1: テストファイルの雛形とモックを書く(`NoteCard.test.ts`と同じパターン)**

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, fireEvent, waitFor } from "@testing-library/svelte";
import { app } from "../lib/store.svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

const { default: ComposeBar } = await import("./ComposeBar.svelte");

function setupAccount() {
  app.accounts = [
    {
      id: "acc1",
      host: "misskey.io",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
      instance: null,
    },
  ];
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_drafts") return Promise.resolve({ status: "ok", data: [] });
    if (cmd === "get_auto_draft") return Promise.resolve({ status: "ok", data: null });
    return Promise.resolve({ status: "ok", data: null });
  });
  setupAccount();
});

afterEach(() => {
  cleanup();
  app.accounts = [];
});

describe("ComposeBar 下書き", () => {
  it("マウント時にget_auto_draftを呼ぶ", async () => {
    render(ComposeBar);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("get_auto_draft", expect.objectContaining({ accountId: "acc1" }));
    });
  });
});
```

- [ ] **Step 2: テストを実行して現状を確認する**

Run: `cd frontend && pnpm vitest run src/ui/ComposeBar.test.ts`
Expected: PASS(Task 6実装済みのため、この時点で素直に通る想定。通らない場合はコマンド名のsnake_case変換や`accountId`引数名を`tauri.gen.ts`の実際の生成結果に合わせて修正する)

- [ ] **Step 3: 自動保存デバウンスのテストを追加する**

```ts
  it("入力後2秒でsave_auto_draftを呼ぶ", async () => {
    vi.useFakeTimers();
    const { getByTestId } = render(ComposeBar);
    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "書きかけ" } });
    vi.advanceTimersByTime(2000);
    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "save_auto_draft",
        expect.objectContaining({ accountId: "acc1" }),
      );
    });
    vi.useRealTimers();
  });

  it("空に戻すとclear_auto_draftを呼ぶ", async () => {
    const { getByTestId } = render(ComposeBar);
    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "a" } });
    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "" } });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("clear_auto_draft", expect.objectContaining({ accountId: "acc1" }));
    });
  });
```

(`vi.advanceTimersByTime`は`$effect`内の`setTimeout`に対して有効。既存コードベースで`fake timers`を使うテストが無い場合は、`vi.useFakeTimers({ shouldAdvanceTime: true })`等の調整が必要になることがある — 実行結果を見て調整する)

- [ ] **Step 4: テストを実行し、通ることを確認する**

Run: `cd frontend && pnpm vitest run src/ui/ComposeBar.test.ts`
Expected: PASS

- [ ] **Step 5: 手動下書きの呼び出し復元と投稿後自動削除のテストを追加する**

```ts
  it("手動下書きを呼び出すとtextが復元され、投稿成功後にdelete_draftが呼ばれる", async () => {
    const draft = {
      id: "d1",
      accountId: "acc1",
      kind: "manual",
      text: "保存済み本文",
      cw: null,
      visibility: "public",
      localOnly: false,
      reactionAcceptance: "all",
      channelId: null,
      poll: null,
      fileIds: [],
      replyNote: null,
      quoteNote: null,
      createdAt: 0,
      updatedAt: 0,
    };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drafts") return Promise.resolve({ status: "ok", data: [draft] });
      if (cmd === "get_auto_draft") return Promise.resolve({ status: "ok", data: null });
      if (cmd === "post_note") return Promise.resolve({ status: "ok", data: { id: "n1" } });
      if (cmd === "delete_draft") return Promise.resolve({ status: "ok", data: null });
      if (cmd === "clear_auto_draft") return Promise.resolve({ status: "ok", data: null });
      return Promise.resolve({ status: "ok", data: null });
    });
    const { getByTitle, getByText, getByTestId } = render(ComposeBar);
    await fireEvent.click(getByTitle("下書き"));
    await fireEvent.click(getByText("保存済み本文"));
    expect((getByTestId("compose-textarea") as HTMLTextAreaElement).value).toBe("保存済み本文");

    await fireEvent.click(getByTestId("compose-submit"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("delete_draft", expect.objectContaining({ draftId: "d1" }));
    });
  });
```

- [ ] **Step 6: テストを実行し、通ることを確認する。落ちる場合は原因を特定して実装(Task 5/6)側を直す**

Run: `cd frontend && pnpm vitest run src/ui/ComposeBar.test.ts`
Expected: PASS

- [ ] **Step 7: フロント全体のテスト・型チェックを実行する**

Run: `cd frontend && pnpm check && pnpm test`
Expected: PASS

- [ ] **Step 8: コミット**

```bash
git add frontend/src/ui/ComposeBar.test.ts
git commit -m "test: ComposeBarの下書き機能のユニットテストを追加"
```

---

## 最終確認

- [ ] `cd src-tauri && cargo test` が全て通る
- [ ] `cd frontend && pnpm check && pnpm test` が全て通る
- [ ] `cargo tauri dev` で手動下書き保存→一覧→呼び出し→投稿→一覧から消えることを目視確認する
- [ ] `git log --oneline feature/issue-251-draft-feature` で各タスクのコミットが積まれていることを確認する
- [ ] `gh pr create` でPRを作成する(本文に `Fixes #251` を含める)。マージは通常のマージコミット(`--merge`)。
