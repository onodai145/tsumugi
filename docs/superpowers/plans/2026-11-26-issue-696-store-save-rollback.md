# 設定・下書きの保存失敗時の巻き戻し(Issue #696)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 設定と手動下書きの保存に失敗したとき、メモリ上の状態を変えない。

**Architecture:** `SettingsStore` は「複製を書き換え、保存に成功してから差し替える」順序を `commit` / `update` の 2 つに集め、全更新メソッドをそこに通す。`DraftStore` は手動下書きの 2 操作だけを同じ順序にし、自動下書きはメモリを正とする現状を残す。

**Tech Stack:** Rust(`std::sync::Mutex`、`serde_json`)

**Spec:** `docs/superpowers/specs/2026-11-26-issue-696-store-save-rollback-design.md`

## Global Constraints

- 公開メソッドのシグネチャと、成功時の動作は変えない。呼び出し元は変更しない。
- `save_auto` / `clear_auto` の動作は変えない。
- `migrate_from_legacy_sqlite` と `load_json_or_default` は変えない。
- コミットメッセージは件名 1 行(`type(scope): 件名(Issue #696)`)。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` だけを付ける。`--no-verify` は使わない。コミットが失敗したら再試行せず報告する。
- ビルド出力を `/tmp` に置かない。`cargo` は `CARGO_BUILD_JOBS=4` を付ける。実行前に `free -h` で空きとスワップを確認する。

## Review Focus

- 保存先が書けない状態でも、保存しない分岐(`set_column_speech` の対象なし、`trust_summaly_proxy` のインポート由来でない場合)は成功のまま → Task 1
- 保存に失敗した変更が、次の成功した保存でディスクに書かれない → Task 1
- 失敗した `save_ui` / `trust_summaly_proxy` のあと、summaly の取得先が許可リストに入らない → Task 1
- 手動下書きの保存に失敗しても、未保存の自動下書きは消えず、次の成功した保存で書かれる → Task 2

---

### Task 1: `SettingsStore` を保存成功後の差し替えにする

**Files:**
- Modify: `src-tauri/src/store/settings.rs`(`impl SettingsStore` と `mod tests`)

**Interfaces:**
- Consumes: 既存の `fn save(&self, data: &SettingsData) -> Result<()>`、テスト補助の `account(id)` / `column(id, account_id, ord)`(`group_id` は `"g1"`)/ `snap(users, words)`
- Produces: 非公開の `fn commit(&self, guard: &mut SettingsData, next: SettingsData) -> Result<()>` と `fn update<T>(&self, change: impl FnOnce(&mut SettingsData) -> T) -> Result<T>`

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` の末尾(`set_column_speech_updates_only_that_column_and_reports_missing` の後)に追加する。

```rust
    // ---- 保存の失敗(Issue #696) ----

    /// 返したディレクトリを消すと、以後の保存が失敗するストア。`seeded_data` を保存済み。
    fn seeded_file_store() -> (SettingsStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("tsumugi-save-fail-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let s = SettingsStore::new(dir.join("settings.json"), dir.join("backgrounds")).unwrap();
        s.replace(seeded_data()).unwrap();
        (s, dir)
    }

    /// `update_ops` のすべての操作が、何かを変えられる初期データ。
    fn seeded_data() -> SettingsData {
        let group = |id: &str, order: i32| ColumnGroup { id: id.into(), order, width: 300, auto: false };
        let mut data = SettingsData::default();
        data.accounts = vec![account("a1")];
        data.groups = vec![group("g1", 0), group("g2", 1), group("g_empty", 2)];
        data.columns = vec![column("c1", "a1", 0), Column { group_id: "g2".into(), ..column("c2", "a1", 1) }];
        data.pane_layout = Some(PaneNode::new_leaf("g1"));
        data.ui.summaly_proxy_url = "https://a.example/url".into();
        data.summaly_proxy_imported = true;
        data
    }

    type UpdateOp = (&'static str, fn(&SettingsStore) -> Result<()>);

    /// `SettingsStore` の更新操作すべて。更新メソッドを足したら、ここにも足す。
    fn update_ops() -> Vec<UpdateOp> {
        vec![
            ("upsert_account", |s| s.upsert_account(&account("a2"))),
            ("delete_account", |s| s.delete_account("a1")),
            ("replace", |s| s.replace(SettingsData::default())),
            ("upsert_group", |s| {
                s.upsert_group(&ColumnGroup { id: "g3".into(), order: 3, width: 300, auto: false })
            }),
            ("set_group_width", |s| s.set_group_width("g1", 420)),
            ("set_group_auto", |s| s.set_group_auto("g1", true)),
            ("reorder_groups", |s| s.reorder_groups(&["g2".into(), "g1".into()])),
            ("delete_empty_groups", |s| s.delete_empty_groups()),
            ("set_column_title", |s| s.set_column_title("c1", Some("t"))),
            ("set_column_notify", |s| s.set_column_notify("c1", false, true, "chime")),
            ("upsert_column", |s| s.upsert_column(&column("c3", "a1", 2))),
            ("delete_column", |s| s.delete_column("c1")),
            ("save_mute", |s| {
                s.save_mute(&MuteConfig { ng_words: vec!["x".into()], ..Default::default() })
            }),
            ("save_server_mute_snapshot", |s| s.save_server_mute_snapshot("a1", &snap(&["u1"], &[]))),
            ("save_cache_backend", |s| {
                s.save_cache_backend(&CacheBackendConfig::Postgres {
                    host: "db.example".into(),
                    port: 5432,
                    database: "tsumugi".into(),
                    user: "app".into(),
                })
            }),
            ("update_mcp", |s| s.update_mcp(|m| m.exposed_account_ids = vec!["a1".into()]).map(|_| ())),
            ("update_speech", |s| s.update_speech(|v| v.enabled = true).map(|_| ())),
            ("set_column_speech", |s| {
                let speech = crate::domain::ColumnSpeech { enabled: true, ..Default::default() };
                s.set_column_speech("c1", speech).map(|_| ())
            }),
            ("set_local_model_license_version", |s| s.set_local_model_license_version("v1")),
            ("set_speech_core_terms_version", |s| s.set_speech_core_terms_version("v1")),
            ("save_notify", |s| {
                let mut notify = s.load_notify()?;
                notify.sound_choice = "chime".into();
                s.save_notify(&notify)
            }),
            ("save_ui", |s| {
                let mut ui = s.load_ui()?;
                ui.summaly_proxy_url = "https://b.example/url".into();
                s.save_ui(&ui)
            }),
            ("trust_summaly_proxy", |s| s.trust_summaly_proxy("https://a.example/url")),
            ("save_pane_layout", |s| s.save_pane_layout(&PaneNode::new_leaf("g2"))),
            ("move_tab", |s| s.move_tab("c1", "g2", &["c2".into(), "c1".into()])),
        ]
    }

    fn as_json(s: &SettingsStore) -> serde_json::Value {
        serde_json::to_value(s.snapshot()).unwrap()
    }

    /// 下の「保存に失敗したら変わらない」テストが、何も変えない操作で素通りしないことの確認。
    #[test]
    fn every_update_op_in_the_table_changes_data_when_saving_succeeds() {
        for (name, op) in update_ops() {
            let s = store();
            s.replace(seeded_data()).unwrap();
            let before = as_json(&s);
            op(&s).unwrap();
            assert_ne!(as_json(&s), before, "{name} が何も変えていない");
        }
    }

    #[test]
    fn every_update_op_changes_nothing_when_saving_fails() {
        let mut broken = Vec::new();
        for (name, op) in update_ops() {
            let (s, dir) = seeded_file_store();
            std::fs::remove_dir_all(&dir).unwrap(); // 以後の保存は失敗する
            let before = as_json(&s);
            if op(&s).is_ok() {
                broken.push(format!("{name}: Err にならない"));
            }
            if as_json(&s) != before {
                broken.push(format!("{name}: メモリが変わった"));
            }
        }
        assert!(broken.is_empty(), "{broken:#?}");
    }

    #[test]
    fn a_failed_change_is_not_written_by_the_next_successful_save() {
        let (s, dir) = seeded_file_store();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(s.update_mcp(|m| m.exposed_account_ids = vec!["a1".into()]).is_err());

        std::fs::create_dir_all(&dir).unwrap();
        s.set_group_width("g1", 420).unwrap();

        let reloaded = SettingsStore::new(dir.join("settings.json"), dir.join("backgrounds")).unwrap();
        assert!(reloaded.load_mcp().exposed_account_ids.is_empty());
        assert_eq!(reloaded.load_groups().unwrap()[0].width, 420);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_failed_save_does_not_trust_an_imported_summaly_proxy() {
        let (s, dir) = seeded_file_store();
        std::fs::remove_dir_all(&dir).unwrap();

        let mut ui = s.load_ui().unwrap();
        ui.summaly_proxy_url = "http://192.168.1.10:3030/url".into();
        assert!(s.save_ui(&ui).is_err());
        assert!(s.trust_summaly_proxy("https://a.example/url").is_err());

        assert!(s.summaly_proxy_imported());
        assert_eq!(s.trusted_summaly_allowlist_entry(), None);
    }

    #[test]
    fn branches_that_do_not_save_succeed_even_when_saving_would_fail() {
        let (s, dir) = seeded_file_store();
        s.trust_summaly_proxy("https://a.example/url").unwrap(); // 信頼済みにしておく
        std::fs::remove_dir_all(&dir).unwrap();

        assert!(!s.set_column_speech("nope", Default::default()).unwrap());
        s.trust_summaly_proxy("https://a.example/url").unwrap();
        assert!(matches!(
            s.trust_summaly_proxy("https://other.example/url"),
            Err(crate::error::Error::Invalid(_))
        ));
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib store::settings`

Expected: `every_update_op_changes_nothing_when_saving_fails` が FAIL し、「メモリが変わった」に `delete_account` と `replace` 以外の 23 操作が並ぶ。`a_failed_change_is_not_written_by_the_next_successful_save` と `a_failed_save_does_not_trust_an_imported_summaly_proxy` も FAIL。残りは PASS。

- [ ] **Step 3: 実装する**

`replace` を置き換え、その上に `commit` と `update` を足す。

```rust
    /// 保存に成功してから、メモリを `next` に差し替える。失敗したときは何も変わらない(Issue #696)。
    fn commit(&self, guard: &mut SettingsData, next: SettingsData) -> Result<()> {
        self.save(&next)?;
        *guard = next;
        Ok(())
    }

    /// 複製を `change` で書き換えて `commit` する。更新メソッドは、メモリを直接書き換えずにこれを使う。
    fn update<T>(&self, change: impl FnOnce(&mut SettingsData) -> T) -> Result<T> {
        let mut guard = self.data.lock().unwrap();
        let mut next = guard.clone();
        let out = change(&mut next);
        self.commit(&mut guard, next)?;
        Ok(out)
    }

    /// 設定全体を差し替える(設定のインポート用、Issue #544)。保存に失敗したときは何も変わらない。
    pub(crate) fn replace(&self, new: SettingsData) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        self.commit(&mut guard, new)
    }
```

各更新メソッドの本体を置き換える(シグネチャと既存のドキュメントコメントは変えない。`delete_account` のコメントもそのまま)。

```rust
    pub fn upsert_account(&self, a: &Account) -> Result<()> {
        self.update(|data| match data.accounts.iter_mut().find(|x| x.id == a.id) {
            Some(existing) => *existing = a.clone(),
            None => data.accounts.push(a.clone()),
        })
    }

    pub fn delete_account(&self, account_id: &str) -> Result<()> {
        self.update(|data| {
            data.accounts.retain(|a| a.id != account_id);
            data.columns.retain(|c| c.account_id != account_id);
            data.server_mute_snapshots.remove(account_id);
            data.mcp.exposed_account_ids.retain(|id| id != account_id);
        })
    }

    pub fn upsert_group(&self, g: &ColumnGroup) -> Result<()> {
        self.update(|data| match data.groups.iter_mut().find(|x| x.id == g.id) {
            Some(existing) => *existing = g.clone(),
            None => data.groups.push(g.clone()),
        })
    }

    pub fn set_group_width(&self, group_id: &str, width: i32) -> Result<()> {
        self.update(|data| {
            if let Some(g) = data.groups.iter_mut().find(|x| x.id == group_id) {
                g.width = width;
            }
        })
    }

    pub fn set_group_auto(&self, group_id: &str, auto: bool) -> Result<()> {
        self.update(|data| {
            if let Some(g) = data.groups.iter_mut().find(|x| x.id == group_id) {
                g.auto = auto;
            }
        })
    }

    pub fn reorder_groups(&self, ordered_ids: &[String]) -> Result<()> {
        self.update(|data| {
            for (i, id) in ordered_ids.iter().enumerate() {
                if let Some(g) = data.groups.iter_mut().find(|x| &x.id == id) {
                    g.order = i as i32;
                }
            }
        })
    }

    pub fn delete_empty_groups(&self) -> Result<()> {
        self.update(|data| {
            let used: std::collections::HashSet<String> =
                data.columns.iter().map(|c| c.group_id.clone()).collect();
            let removed_ids: Vec<String> =
                data.groups.iter().filter(|g| !used.contains(&g.id)).map(|g| g.id.clone()).collect();
            data.groups.retain(|g| used.contains(&g.id));
            for gid in &removed_ids {
                if let Some(root) = &mut data.pane_layout {
                    let is_root_leaf_target =
                        matches!(root, PaneNode::Leaf { group_id, .. } if group_id == gid);
                    if is_root_leaf_target {
                        data.pane_layout = None;
                    } else {
                        root.remove_group(gid);
                    }
                }
            }
        })
    }

    pub fn set_column_title(&self, column_id: &str, title: Option<&str>) -> Result<()> {
        self.update(|data| {
            if let Some(c) = data.columns.iter_mut().find(|x| x.id == column_id) {
                c.title = title.map(|s| s.to_string());
            }
        })
    }

    pub fn set_column_notify(
        &self,
        column_id: &str,
        notify_desktop: bool,
        notify_sound: bool,
        notify_sound_choice: &str,
    ) -> Result<()> {
        self.update(|data| {
            if let Some(c) = data.columns.iter_mut().find(|x| x.id == column_id) {
                c.notify_desktop = notify_desktop;
                c.notify_sound = notify_sound;
                c.notify_sound_choice = notify_sound_choice.to_string();
            }
        })
    }

    pub fn upsert_column(&self, c: &Column) -> Result<()> {
        self.update(|data| match data.columns.iter_mut().find(|x| x.id == c.id) {
            Some(existing) => *existing = c.clone(),
            None => data.columns.push(c.clone()),
        })
    }

    pub fn delete_column(&self, column_id: &str) -> Result<()> {
        self.update(|data| data.columns.retain(|c| c.id != column_id))
    }

    pub fn save_mute(&self, cfg: &MuteConfig) -> Result<()> {
        self.update(|data| data.mute = cfg.clone())
    }

    pub fn save_server_mute_snapshot(&self, account_id: &str, snapshot: &ServerMuteSnapshot) -> Result<()> {
        self.update(|data| {
            data.server_mute_snapshots.insert(account_id.to_string(), snapshot.clone());
        })
    }

    pub fn save_cache_backend(&self, cfg: &CacheBackendConfig) -> Result<()> {
        self.update(|data| data.cache_backend = cfg.clone())
    }

    pub fn update_mcp(
        &self,
        f: impl FnOnce(&mut crate::mcp::settings::McpSettings),
    ) -> Result<crate::mcp::settings::McpSettings> {
        self.update(|data| {
            f(&mut data.mcp);
            data.mcp.clone()
        })
    }

    pub fn update_speech(
        &self,
        f: impl FnOnce(&mut crate::domain::SpeechSettings),
    ) -> Result<crate::domain::SpeechSettings> {
        self.update(|data| {
            f(&mut data.speech);
            data.speech.clone()
        })
    }

    pub fn set_column_speech(&self, column_id: &str, speech: crate::domain::ColumnSpeech) -> Result<bool> {
        let mut guard = self.data.lock().unwrap();
        let mut next = guard.clone();
        let Some(c) = next.columns.iter_mut().find(|x| x.id == column_id) else {
            return Ok(false);
        };
        c.speech = speech;
        self.commit(&mut guard, next)?;
        Ok(true)
    }

    pub fn set_local_model_license_version(&self, version: &str) -> Result<()> {
        self.update(|data| data.local_model_license_version = Some(version.to_string()))
    }

    pub fn set_speech_core_terms_version(&self, version: &str) -> Result<()> {
        self.update(|data| data.speech_core_terms_version = Some(version.to_string()))
    }

    pub fn save_notify(&self, cfg: &NotifyConfig) -> Result<()> {
        self.update(|data| data.notify = cfg.clone())
    }

    pub fn save_ui(&self, prefs: &UiPrefs) -> Result<()> {
        prefs.validate_external_urls()?;
        self.update(|data| {
            if prefs.summaly_proxy_url != data.ui.summaly_proxy_url {
                data.summaly_proxy_imported = false;
            }
            data.ui = prefs.clone();
        })
    }

    pub fn trust_summaly_proxy(&self, expected_url: &str) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        if guard.ui.summaly_proxy_url != expected_url {
            return Err(crate::error::Error::Invalid(
                "取得先が変更されています。設定画面を開き直してください".to_string(),
            ));
        }
        if !guard.summaly_proxy_imported {
            return Ok(());
        }
        let mut next = guard.clone();
        next.summaly_proxy_imported = false;
        self.commit(&mut guard, next)
    }

    pub fn save_pane_layout(&self, root: &PaneNode) -> Result<()> {
        self.update(|data| data.pane_layout = Some(root.clone()))
    }

    pub fn move_tab(&self, tab_id: &str, group_id: &str, ordered_tab_ids: &[String]) -> Result<()> {
        self.update(|data| {
            if let Some(c) = data.columns.iter_mut().find(|x| x.id == tab_id) {
                c.group_id = group_id.to_string();
            }
            for (i, id) in ordered_tab_ids.iter().enumerate() {
                if let Some(c) = data.columns.iter_mut().find(|x| &x.id == id) {
                    c.order = i as i32;
                }
            }
        })
    }
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib store::settings`

Expected: すべて PASS。

Run: `grep -n 'self\.save(' src-tauri/src/store/settings.rs`

Expected: `commit` の中の 1 件だけ(`migrate_from_legacy_sqlite` は `store.save`)。

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/src/store/settings.rs
git commit -m "fix(settings): 保存に失敗した設定の変更をメモリに残さない(Issue #696)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: 手動下書きを保存成功後の差し替えにし、規則を記録する

**Files:**
- Modify: `src-tauri/src/store/draft.rs`
- Modify: `CLAUDE.md`(`### src-tauri/src layout` の `store/` の項)

**Interfaces:**
- Consumes: 既存の `fn save(&self, data: &DraftData) -> Result<()>`、テスト補助の `input(text)`
- Produces: 非公開の `fn update_if_saved(&self, change: impl FnOnce(&mut Vec<Draft>)) -> Result<()>`

- [ ] **Step 1: テストを書く**

`draft.rs` の `mod tests` の末尾に追加する。

```rust
    // ---- 保存の失敗(Issue #696) ----

    /// 返したディレクトリを消すと、以後の保存が失敗するストア。
    fn file_store() -> (DraftStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("tsumugi-draft-fail-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        (DraftStore::new(dir.join("drafts.json")).unwrap(), dir)
    }

    fn manual_texts(s: &DraftStore) -> Vec<String> {
        s.list_manual("acc1").unwrap().into_iter().map(|d| d.text).collect()
    }

    fn auto_text(s: &DraftStore) -> Option<String> {
        s.get_auto("acc1").unwrap().map(|d| d.text)
    }

    #[test]
    fn manual_draft_operations_change_nothing_when_saving_fails() {
        let (s, dir) = file_store();
        s.save_manual("acc1", &input("kept")).unwrap();
        let id = s.list_manual("acc1").unwrap()[0].id.clone();
        std::fs::remove_dir_all(&dir).unwrap(); // 以後の保存は失敗する

        assert!(s.save_manual("acc1", &input("failed")).is_err());
        assert_eq!(manual_texts(&s), ["kept"]);

        assert!(s.delete_manual("acc1", &id).is_err());
        assert_eq!(manual_texts(&s), ["kept"]);
    }

    /// 自動下書きは失敗を画面に出さないので、メモリを正とする。巻き戻すと、投稿画面を開き直したときに
    /// 古い本文が復元され、投稿後の掃除の失敗では投稿済みの本文が復元される。
    #[test]
    fn auto_draft_operations_stay_in_memory_when_saving_fails() {
        let (s, dir) = file_store();
        s.save_auto("acc1", &input("old")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        assert!(s.save_auto("acc1", &input("latest")).is_err());
        assert_eq!(auto_text(&s).as_deref(), Some("latest"));

        assert!(s.clear_auto("acc1").is_err());
        assert_eq!(auto_text(&s), None);
    }

    #[test]
    fn an_unsaved_auto_draft_survives_a_failed_manual_save_and_is_written_later() {
        let (s, dir) = file_store();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(s.save_auto("acc1", &input("unsaved")).is_err());
        assert!(s.save_manual("acc1", &input("failed")).is_err());
        assert_eq!(auto_text(&s).as_deref(), Some("unsaved"));

        std::fs::create_dir_all(&dir).unwrap();
        s.save_manual("acc1", &input("manual")).unwrap();

        let reloaded = DraftStore::new(dir.join("drafts.json")).unwrap();
        assert_eq!(auto_text(&reloaded).as_deref(), Some("unsaved"));
        assert_eq!(manual_texts(&reloaded), ["manual"]);
        std::fs::remove_dir_all(&dir).ok();
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib store::draft`

Expected: `manual_draft_operations_change_nothing_when_saving_fails` が FAIL(`["kept"]` に対して 2 件)。`an_unsaved_auto_draft_survives_a_failed_manual_save_and_is_written_later` も FAIL(再読み込み後の手動下書きに `failed` が混じる)。`auto_draft_operations_stay_in_memory_when_saving_fails` は現状の動作を固定するテストなので PASS。

- [ ] **Step 3: 実装する**

`save` の後に追加する。

```rust
    /// 複製を `change` で書き換え、保存に成功してからメモリを差し替える。失敗が画面に出る
    /// 手動下書きの操作に使う(Issue #696)。自動下書き(`save_auto` / `clear_auto`)は使わない。
    fn update_if_saved(&self, change: impl FnOnce(&mut Vec<Draft>)) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        let mut next = guard.clone();
        change(&mut next.drafts);
        self.save(&next)?;
        *guard = next;
        Ok(())
    }
```

`save_manual` と `delete_manual` を置き換える。

```rust
    pub fn save_manual(&self, account_id: &str, input: &DraftInput) -> Result<()> {
        self.update_if_saved(|drafts| {
            let now = now_millis();
            drafts.push(Draft {
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
        })
    }
```

```rust
    pub fn delete_manual(&self, account_id: &str, draft_id: &str) -> Result<()> {
        self.update_if_saved(|drafts| {
            drafts.retain(|d| {
                !(d.account_id == account_id && d.id == draft_id && matches!(d.kind, DraftKind::Manual))
            })
        })
    }
```

`save_auto` の直前にドキュメントコメントを足す(本体と `clear_auto` は変えない)。

```rust
    /// 自動下書きの保存と削除(`clear_auto`)は、保存に失敗してもメモリを巻き戻さない。呼び出し側は
    /// 失敗を画面に出さず、巻き戻すと古い本文や投稿済みの本文が復元されるため(Issue #696)。
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib store::draft`

Expected: すべて PASS。

- [ ] **Step 5: CLAUDE.md に規則を足す**

`store/` の項の「アカウント削除(`account.rs` の …)」とは別に、`store/` の項の末尾へ次の文を足す。

```markdown
 `SettingsStore` の更新メソッドは、メモリを直接書き換えず `update` / `commit` を通す(保存に成功してから差し替える。失敗したら何も変えない。Issue #696)。足したら、テストの `update_ops` にも足す。`DraftStore` は手動下書きだけ同じ順序で、自動下書き(`save_auto` / `clear_auto`)は保存に失敗してもメモリを正とする(巻き戻すと、古い本文や投稿済みの本文が復元される)。
```

- [ ] **Step 6: 全体のテストを実行する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test`

Expected: すべて PASS(`#[ignore]` は除く)。

Run: `git status --short`

Expected: `src-tauri/src/store/draft.rs` と `CLAUDE.md` だけ(`frontend/src/bindings/tauri.gen.ts` に差分が出ない)。

- [ ] **Step 7: コミットする**

```bash
git add src-tauri/src/store/draft.rs CLAUDE.md
git commit -m "fix(draft): 保存に失敗した手動下書きの変更をメモリに残さない(Issue #696)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## 追加タスク(ブランチのレビューで追加。spec の 3)

Global Constraints の「呼び出し元は変更しない」は、spec の 3 の 2 か所(`commands/account.rs`、`commands/local_model.rs`)を除く。

追加の Review Focus:

- 再認証の保存に失敗しても、前のトークンが残り、アカウントは使い続けられる → Task 3
- 控えのトークンが読めないときは、何も書かない → Task 3
- 後始末も失敗したら、返すのは設定の保存のエラー → Task 3

### Task 3: ログインの保存に失敗したら keyring を元に戻す

> 実装後に spec の 3 を改定した(控えが読めなくても続ける、`token_removal` の読みロックを持つ)。
> 現在の動作は spec と `persist_login` が正で、以下の「読めなければ何も書かない」は古い。

**Files:**
- Modify: `src-tauri/src/commands/account.rs`(`complete_miauth`、`mod removal_tests`)

**Interfaces:**
- Produces: 非公開の `fn persist_login(state: &AppState, account: &Account, token: &str) -> Result<()>`
- Consumes: `mod removal_tests` の `fixture()` / `account(id, host)` / `token(&f, id)` / `has_account(&f, id)`、`f.settings_disk.break_dir()`、`f.faults.get` / `f.faults.delete`

- [ ] **Step 1: 動作を変えずに `persist_login` を切り出す**

`complete_miauth` の次の 3 行を

```rust
    // token は keyring のみに保存（Account/戻り値には含めない）
    state.secrets.set(&account.id, &token)?;
    state.settings.upsert_account(&account)?; // 再起動で復元できるよう永続化
```

次の 1 行に置き換える。

```rust
    persist_login(&state, &account, &token)?;
```

`complete_miauth` の後に追加する。

```rust
fn persist_login(state: &AppState, account: &Account, token: &str) -> Result<()> {
    // token は keyring のみに保存（Account/戻り値には含めない）
    state.secrets.set(&account.id, token)?;
    state.settings.upsert_account(account) // 再起動で復元できるよう永続化
}
```

- [ ] **Step 2: テストを書く**

`mod removal_tests` の末尾に追加する。

```rust
    // ---- ログインの保存の失敗(Issue #696) ----

    #[tokio::test(flavor = "multi_thread")]
    async fn a_saved_login_writes_the_token_and_the_account() {
        let f = fixture();

        persist_login(&f.state, &account("a3", "127.0.0.1:3"), "new-token").unwrap();

        assert_eq!(token(&f, "a3").as_deref(), Some("new-token"));
        assert!(f.state.settings.load_accounts().unwrap().iter().any(|a| a.id == "a3"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_login_that_cannot_be_saved_leaves_no_token_for_a_new_account() {
        let f = fixture();
        f.settings_disk.break_dir();

        assert!(persist_login(&f.state, &account("a3", "127.0.0.1:3"), "new-token").is_err());

        assert_eq!(token(&f, "a3"), None);
        assert_eq!(has_account(&f, "a3"), (false, false));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_reauthentication_that_cannot_be_saved_keeps_the_previous_token() {
        let f = fixture();
        f.settings_disk.break_dir();

        assert!(persist_login(&f.state, &account("a1", "127.0.0.1:1"), "new-token").is_err());

        assert_eq!(token(&f, "a1").as_deref(), Some("token"));
        assert_eq!(has_account(&f, "a1"), (true, true));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_login_writes_nothing_when_the_previous_token_cannot_be_read() {
        let f = fixture();
        f.faults.get.store(true, Ordering::SeqCst);

        let result = persist_login(&f.state, &account("a1", "127.0.0.1:1"), "new-token");

        f.faults.get.store(false, Ordering::SeqCst);
        assert!(matches!(result, Err(Error::Secret(_))), "{result:?}");
        assert_eq!(token(&f, "a1").as_deref(), Some("token"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_cleanup_still_reports_the_save_error() {
        let f = fixture();
        f.settings_disk.break_dir();
        f.faults.delete.store(true, Ordering::SeqCst);

        let result = persist_login(&f.state, &account("a3", "127.0.0.1:3"), "new-token");

        assert!(result.is_err() && !matches!(result, Err(Error::Secret(_))), "{result:?}");
    }
```

- [ ] **Step 3: テストが失敗することを確認する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib commands::account`

Expected: `a_login_that_cannot_be_saved_leaves_no_token_for_a_new_account`、`a_reauthentication_that_cannot_be_saved_keeps_the_previous_token`(`new-token` が残る)、`a_login_writes_nothing_when_the_previous_token_cannot_be_read`(`Ok` になる)が FAIL。残りは PASS。

- [ ] **Step 4: 実装する**

`persist_login` を置き換える。

```rust
/// token を keyring に、アカウントを設定に書く。設定の保存に失敗したら、keyring を書く前の状態に戻す
/// (残すと、どのアカウントにも結びつかない token ができる。Issue #696)。
fn persist_login(state: &AppState, account: &Account, token: &str) -> Result<()> {
    // 戻せるように、書く前に控える。読めなければ何も書かない。
    let previous = state.secrets.get(&account.id)?;
    // token は keyring のみに保存（Account/戻り値には含めない）
    state.secrets.set(&account.id, token)?;
    // 再起動で復元できるよう永続化
    state.settings.upsert_account(account).inspect_err(|_| {
        let restored = match &previous {
            Some(previous) => state.secrets.set(&account.id, previous),
            None => state.secrets.delete(&account.id),
        };
        if let Err(e) = restored {
            log::warn!("failed to restore the token after the login failed: {e}");
        }
    })
}
```

- [ ] **Step 5: テストが通ることを確認してコミットする**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib commands::account`

Expected: すべて PASS。

```bash
git add src-tauri/src/commands/account.rs
git commit -m "fix(account): ログインの保存に失敗したら token を元に戻す(Issue #696)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 同意の保存の失敗で、開始済みのダウンロードをエラーにしない

**Files:**
- Modify: `src-tauri/src/commands/local_model.rs`(`start_core` と `mod tests`)

**Interfaces:**
- Consumes: `BrokenDisk`(`crate::store::testutil::BrokenDisk`)の `new` / `path` / `break_dir`、テスト補助の `noop_notify()`

- [ ] **Step 1: テストを書く**

`the_right_version_starts_and_records_the_acceptance` の後に追加する。

```rust
    #[cfg(llama_target)]
    #[tokio::test]
    async fn a_failed_consent_save_does_not_fail_a_download_that_already_started() {
        use crate::store::testutil::BrokenDisk;
        use sha2::{Digest, Sha256};
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let data = vec![7u8; 10_000];
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200).set_body_bytes(data.clone())).mount(&server).await;
        let spec = ModelSpec {
            url: format!("{}/m.gguf", server.uri()),
            size: data.len() as u64,
            sha256: format!("{:x}", Sha256::digest(&data)),
        };
        let models_disk = BrokenDisk::new();
        let models = Arc::new(LocalModels::with_spec(
            models_disk.path("models"),
            reqwest::Client::new(),
            Arc::new(LocalTranslate::unconfigured()),
            spec,
        ));
        let settings_disk = BrokenDisk::new();
        let settings =
            SettingsStore::new(settings_disk.path("settings.json"), settings_disk.path("backgrounds")).unwrap();
        let s = AppState::new_for_test(settings).with_local_models(models);
        settings_disk.break_dir(); // 同意の保存は失敗する

        start_core(&s, LICENSE_VERSION, noop_notify()).await.unwrap();

        tokio::time::timeout(Duration::from_secs(20), async {
            while info(&s).status != LocalModelStatus::Ready {
                tokio::time::sleep(Duration::from_millis(10)).await
            }
        })
        .await
        .expect("モデルが Ready にならない");
        assert_eq!(s.settings.local_model_license_version(), None);
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib commands::local_model`

Expected: 追加したテストが、`start_core(...).unwrap()` の I/O エラーで FAIL。残りは PASS。

- [ ] **Step 3: 実装する**

`start_core` の末尾の 2 行(`s.local_models.start(notify)?;` と `s.settings.set_local_model_license_version(LICENSE_VERSION)`)を置き換える。

```rust
    s.local_models.start(notify)?;
    // 取得はもう始まっている。同意の保存に失敗しても、取得は止めずに成功として返す(次回の取得の前に、また確認が出るだけ)。
    if let Err(e) = s.settings.set_local_model_license_version(LICENSE_VERSION) {
        log::warn!("local model license consent could not be saved: {e}");
    }
    Ok(())
```

- [ ] **Step 4: テストが通ることを確認してコミットする**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib commands::local_model`

Expected: すべて PASS。

```bash
git add src-tauri/src/commands/local_model.rs
git commit -m "fix(translate): 同意の保存に失敗しても開始済みの取得をエラーにしない(Issue #696)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: 保存失敗のテストを `BrokenDisk` にそろえる

**Files:**
- Modify: `src-tauri/src/store/settings.rs`(`mod tests`)
- Modify: `src-tauri/src/store/draft.rs`(`mod tests`)

**Interfaces:**
- Consumes: `crate::store::testutil::BrokenDisk` の `new` / `path` / `break_dir` / `repair_dir`(`Drop` でディレクトリを消す)

テストだけの整理で、検証する内容は変えない。

- [ ] **Step 1: 補助関数を置き換える**

`settings.rs` の `mod tests` に `use crate::store::testutil::BrokenDisk;` を足し、`seeded_file_store` を置き換える。

```rust
    /// `break_dir` すると、以後の保存が失敗するストア。`seeded_data` を保存済み。
    fn seeded_file_store() -> (SettingsStore, BrokenDisk) {
        let disk = BrokenDisk::new();
        let s = SettingsStore::new(disk.path("settings.json"), disk.path("backgrounds")).unwrap();
        s.replace(seeded_data()).unwrap();
        (s, disk)
    }
```

`draft.rs` の `mod tests` に `use crate::store::testutil::BrokenDisk;` を足し、`file_store` を置き換える。

```rust
    /// `break_dir` すると、以後の保存が失敗するストア。
    fn file_store() -> (DraftStore, BrokenDisk) {
        let disk = BrokenDisk::new();
        (DraftStore::new(disk.path("drafts.json")).unwrap(), disk)
    }
```

- [ ] **Step 2: Issue #696 で足したテストの呼び出しを置き換える**

両ファイルの「保存の失敗(Issue #696)」より後のテストで、次のとおり置き換える。

| 変更前 | 変更後 |
|---|---|
| `let (s, dir) = ...` | `let (s, disk) = ...` |
| `std::fs::remove_dir_all(&dir).unwrap();` | `disk.break_dir();` |
| `std::fs::create_dir_all(&dir).unwrap();` | `disk.repair_dir();` |
| `dir.join("...")` | `disk.path("...")` |
| `std::fs::remove_dir_all(&dir).ok();` | 行ごと削除(`Drop` が消す) |

- [ ] **Step 3: 全体のテストを実行してコミットする**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test`

Expected: すべて PASS(`#[ignore]` は除く)。

Run: `git status --short`

Expected: `store/settings.rs` と `store/draft.rs` だけ。

```bash
git add src-tauri/src/store/settings.rs src-tauri/src/store/draft.rs
git commit -m "test(store): 保存失敗のテストを BrokenDisk にそろえる(Issue #696)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
