# サーバー側ミュートの解除をキャッシュ優先の backfill へ反映する Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** サーバー側ミュート(ユーザー、ワードミュート)の**解除**を同期時に検出して、そのアカウントのカラムの backfill 境界を捨て、キャッシュ優先の backfill が解除したユーザーのノートを欠かないようにする(Issue #454)。

**Architecture:** 前回のミュート集合(ユーザーID、ワードルールの文字列表現)をアカウントごとに設定の JSON へ保存する。`sync_server_mutes_core` が、メモリ上の集合を差し替えた後、前回の集合と比べて「前回あって今回無い要素」があれば、#452 の `ColumnFence::invalidate_boundaries` の中で、そのアカウントの全カラムの境界を `replace_fetch_boundaries(column_id, &[])` で捨てる。追加だけなら捨てない。

**Tech Stack:** Rust(tokio、tauri、serde)、wiremock(テスト)。

**Spec:** `docs/superpowers/specs/2026-10-03-server-mute-boundary-design.md`(承認済み)。実装者は計画と spec の両方を読むこと。

## Global Constraints

- DB スキーマ、Tauri コマンドの署名、TS バインディング(`frontend/src/bindings/tauri.gen.ts`)、フロントエンドは変えない。`cargo test` 後に `git diff main -- frontend/src/bindings/tauri.gen.ts` が空であること。`Error` の variant も足さない。設定の JSON に足すフィールドは `#[serde(default)]` で後方互換にする。
- 保存先は `Account` ではなく、`SettingsData` の別マップ `server_mute_snapshots`(`Account` は TS に出力されるため)。
- 検出するのは**解除だけ**: 前回の `users` / `words` の要素で、今回の集合に無いものがある場合。追加だけ、変化なしでは境界を捨てない。編集された1つのルールは「古いキーが消えた」ので、解除として検出する(安全側)。
- 保存値が**無い**場合(アップグレード直後、新しいアカウント)は、境界を捨てず、保存だけする。
- 捨てる単位は、そのアカウントのカラムだけ(`settings.load_columns()` を `account_id` で絞る)。バックエンドに新しい API は足さない(`replace_fetch_boundaries(column_id, &[])` を使う)。
- ミュート集合の差し替え(`state.set_server_mutes` / `state.set_server_word_mutes`)は、`invalidate_boundaries` より**前**に行う(#452 と同じ不変条件)。`invalidate_boundaries` に渡す `f` の中から、別のフェンス操作を呼ばない。
- 境界の破棄(DB)が失敗したら、保存値を**更新しない**(次回の同期で再試行する)。保存値の書き込みの失敗は、同期を失敗にしない(ログのみ)。取得に失敗したら、何も変えずに `Err`(現状どおり)。
- コミットメッセージは件名のみ・本文なし(末尾の `Co-Authored-By` トレーラーは別)。`--no-verify` は使わない。コミットが失敗したら止まって報告する。
- `main` には直接コミットしない。作業ブランチは `fix/server-mute-boundary-454`(作成済み)。push と PR 作成は、ユーザーの指示を受けてから行う。PR 本文には `Fixes #454` を入れる。
- この環境では、既定のワーカー数でのフロントエンドの全テストが OOM で落ちた。今回はフロントエンドを変えないので、`pnpm test` は走らせない。

## Review Focus

- **ミュートを追加しただけでは、境界を捨てない**: キャッシュ優先の backfill が保たれる(Task 3 のテスト `sync_keeps_boundaries_when_server_mutes_were_only_added`)。
- **他のアカウントのカラムの境界は、捨てない**(Task 3 のテスト `sync_clears_only_the_syncing_accounts_boundaries_when_a_muted_user_was_removed`)。
- **取得に失敗したときは、何も変えない**: 保存値も、境界も、メモリ上のミュート集合も(Task 3 のテスト `sync_changes_nothing_when_the_fetch_fails`)。
- **再認証のように、取得が走っている最中でも、旧い集合の結果に基づく境界を書けない**(Task 3 のテスト `sync_makes_earlier_boundary_epochs_stale_when_it_clears_boundaries`)。
- **古い設定ファイル(`server_mute_snapshots` が無い)を、読める**(Task 2 のテスト `settings_json_without_server_mute_snapshots_still_loads`)。

---

## File Structure

| ファイル | 責務 |
|---|---|
| `src-tauri/src/filter/mute.rs` | `WordMuteRule::key()`(ルールの安定した文字列表現) |
| `src-tauri/src/store/settings.rs` | `ServerMuteSnapshot`(保存する集合と、解除の判定)、`SettingsData.server_mute_snapshots`、読み書きのメソッド |
| `src-tauri/src/commands/mute.rs` | `sync_server_mutes_core` が、解除を検出して、そのアカウントの境界を捨てる。分割した2つの補助関数 |

## Task 1: `WordMuteRule::key()`

**Files:**
- Modify: `src-tauri/src/filter/mute.rs`(`impl WordMuteRule` と、`mod tests` のテスト)

**Interfaces:**
- Consumes: 既存の `WordMuteRule`(`Words(Vec<String>)` と `Regex(regex::Regex)`)。
- Produces(Task 3 が使う): `pub fn key(&self) -> String` — `Words` は `w:` + 小文字にしてソートした語を `\u{1f}` で連結したもの、`Regex` は `r:` + `re.as_str()`。

このタスクの時点では `key` を本体から呼ぶ箇所が無いので、`cargo build` で `dead_code` の警告が出てよい(Task 3 で解消する)。`#[allow(dead_code)]` は付けない。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/filter/mute.rs` の `mod tests` の末尾(最後の `}` の前)に追加する。

```rust
    #[test]
    fn word_rule_key_ignores_word_order_and_case() {
        let a = WordMuteRule::Words(vec!["Foo".into(), "bar".into()]);
        let b = WordMuteRule::Words(vec!["BAR".into(), "foo".into()]);

        assert_eq!(a.key(), b.key());
    }

    #[test]
    fn word_rule_key_distinguishes_different_rules() {
        let one = WordMuteRule::Words(vec!["foo".into()]);
        let two = WordMuteRule::Words(vec!["foo".into(), "bar".into()]);

        assert_ne!(one.key(), two.key());
    }

    #[test]
    fn word_rule_key_does_not_confuse_one_joined_word_with_two_words() {
        let joined = WordMuteRule::Words(vec!["ab".into()]);
        let split = WordMuteRule::Words(vec!["a".into(), "b".into()]);

        assert_ne!(joined.key(), split.key());
    }

    #[test]
    fn word_rule_key_keeps_regex_and_words_apart() {
        let words = WordMuteRule::Words(vec!["foo".into()]);
        let re = WordMuteRule::Regex(regex::Regex::new("foo").unwrap());

        assert_ne!(words.key(), re.key());
        assert!(words.key().starts_with("w:"));
        assert_eq!(re.key(), "r:foo");
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib word_rule_key 2>&1 | grep -E "^error" | sed 's/`[^`]*`/X/g' | sort | uniq -c`
Expected: `no method named key found for enum WordMuteRule` のコンパイルエラー。

- [ ] **Step 3: 最小の実装を書く**

`src-tauri/src/filter/mute.rs` の `pub enum WordMuteRule { ... }` の直後に追加する。

```rust
impl WordMuteRule {
    /// ルールの安定した文字列表現(Issue #454)。サーバー側ミュートの前回の集合を保存して、
    /// 「前回あって今回無いルール」(=解除)を検出するために使う。
    /// - `Words`: 語の順序と大文字小文字に依らない(AND は順序に依らず、照合は大小無視のため)。
    ///   区切り文字(`\\u{1f}`)で連結するので、`"ab"` と `"a","b"` は区別される。
    /// - `Regex`: パターン文字列。`i` フラグは `RegexBuilder::case_insensitive` で適用され、
    ///   `as_str()` には残らないので、`i` フラグだけを外した変更は区別できない(既知の制限)。
    pub fn key(&self) -> String {
        match self {
            WordMuteRule::Words(words) => {
                let mut lowered: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
                lowered.sort();
                format!("w:{}", lowered.join("\u{1f}"))
            }
            WordMuteRule::Regex(re) => format!("r:{}", re.as_str()),
        }
    }
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib word_rule_key 2>&1 | grep -E "^error|FAILED|test result"`
Expected: `test result: ok. 4 passed`。

- [ ] **Step 5: 全体を確認してコミットする**

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^error|FAILED|test result"`
Expected: `test result: ok`、失敗 0(`dead_code` の警告は許容)。

```bash
git add src-tauri/src/filter/mute.rs
git commit -m "feat: ワードミュートのルールの安定した文字列表現key()を追加する"
```

## Task 2: `ServerMuteSnapshot` と設定への保存

**Files:**
- Modify: `src-tauri/src/store/settings.rs`(型、`SettingsData` のフィールド、メソッド、`mod tests`)

**Interfaces:**
- Consumes: 既存の `SettingsStore`(`load_mute` / `save_mute` と同じ書き方)。
- Produces(Task 3 が使う):
  - `pub struct ServerMuteSnapshot { pub users: Vec<String>, pub words: Vec<String> }`(`Debug, Clone, Default, PartialEq, Serialize, Deserialize`)
  - `pub fn new<U, W>(users: U, words: W) -> Self where U: IntoIterator<Item = String>, W: IntoIterator<Item = String>` — ソートして重複を除く
  - `pub fn removed_any_since(&self, previous: &ServerMuteSnapshot) -> bool` — `previous` の `users` / `words` に、`self` に無い要素があれば真
  - `SettingsStore::load_server_mute_snapshot(&self, account_id: &str) -> Result<Option<ServerMuteSnapshot>>`
  - `SettingsStore::save_server_mute_snapshot(&self, account_id: &str, snapshot: &ServerMuteSnapshot) -> Result<()>`

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/store/settings.rs` の `mod tests` の末尾(最後の `}` の前)に追加する。

```rust
    fn snap(users: &[&str], words: &[&str]) -> ServerMuteSnapshot {
        ServerMuteSnapshot::new(
            users.iter().map(|s| s.to_string()),
            words.iter().map(|s| s.to_string()),
        )
    }

    #[test]
    fn server_mute_snapshot_is_sorted_and_deduplicated() {
        let s = snap(&["u2", "u1", "u2"], &["w:b", "w:a"]);

        assert_eq!(s.users, vec!["u1".to_string(), "u2".to_string()]);
        assert_eq!(s.words, vec!["w:a".to_string(), "w:b".to_string()]);
    }

    #[test]
    fn removed_any_since_detects_only_removals() {
        let prev = snap(&["u1", "u2"], &["w:a"]);

        assert!(!snap(&["u1", "u2"], &["w:a"]).removed_any_since(&prev), "変化なし");
        assert!(!snap(&["u1", "u2", "u3"], &["w:a", "w:b"]).removed_any_since(&prev), "増えただけ");
        assert!(snap(&["u1"], &["w:a"]).removed_any_since(&prev), "ユーザーが減った");
        assert!(snap(&["u1", "u2"], &[]).removed_any_since(&prev), "ワードが減った");
        assert!(snap(&["u1", "u3"], &["w:a"]).removed_any_since(&prev), "入れ替わりは、古い方が消えた = 解除");
    }

    #[test]
    fn server_mute_snapshot_roundtrip_per_account() {
        let s = store();
        assert_eq!(s.load_server_mute_snapshot("a1").unwrap(), None);

        s.save_server_mute_snapshot("a1", &snap(&["u1"], &["w:a"])).unwrap();
        s.save_server_mute_snapshot("a2", &snap(&["u9"], &[])).unwrap();

        assert_eq!(s.load_server_mute_snapshot("a1").unwrap(), Some(snap(&["u1"], &["w:a"])));
        assert_eq!(s.load_server_mute_snapshot("a2").unwrap(), Some(snap(&["u9"], &[])));
    }

    #[test]
    fn settings_json_without_server_mute_snapshots_still_loads() {
        let data: SettingsData = serde_json::from_str(r#"{"accounts":[]}"#).unwrap();

        assert!(data.server_mute_snapshots.is_empty());
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib server_mute_snapshot 2>&1 | grep -E "^error" | sed 's/`[^`]*`/X/g' | sort | uniq -c`
Expected: `cannot find type ServerMuteSnapshot` などのコンパイルエラー。

- [ ] **Step 3: 最小の実装を書く**

`src-tauri/src/store/settings.rs` の `use std::path::{Path, PathBuf};` の前後に、`HashMap` と `BTreeSet` の import を足す。

```rust
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
```

`SettingsData` の定義の直前に、型を追加する。

```rust
/// サーバー側ミュートの前回の同期時点の集合(Issue #454)。ミュートの**解除**を検出して、
/// キャッシュ優先の backfill の境界を捨てるために、アカウントごとに保存する。
/// TS には出力しない(`Account` に足すとバインディングが変わるため、`SettingsData` の別マップに置く)。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ServerMuteSnapshot {
    /// ミュート/ブロックしているユーザーID(ソート済み・重複なし)。
    pub users: Vec<String>,
    /// ワードミュートのルールの文字列表現(`WordMuteRule::key()`。ソート済み・重複なし)。
    pub words: Vec<String>,
}

impl ServerMuteSnapshot {
    pub fn new<U, W>(users: U, words: W) -> Self
    where
        U: IntoIterator<Item = String>,
        W: IntoIterator<Item = String>,
    {
        Self {
            users: users.into_iter().collect::<BTreeSet<_>>().into_iter().collect(),
            words: words.into_iter().collect::<BTreeSet<_>>().into_iter().collect(),
        }
    }

    /// `previous` にあって、`self` に無い要素(=ミュートの解除)があるか。追加だけなら偽。
    /// 編集された1つのルールは「古いキーが消えた」ので、真になる(安全側)。
    pub fn removed_any_since(&self, previous: &ServerMuteSnapshot) -> bool {
        previous.users.iter().any(|u| !self.users.contains(u))
            || previous.words.iter().any(|w| !self.words.contains(w))
    }
}

```

`SettingsData` の末尾のフィールド(`cache_backend`)の次に追加する。

```rust
    #[serde(default)]
    cache_backend: CacheBackendConfig,
    /// アカウントID -> サーバー側ミュートの前回の同期時点の集合(Issue #454)。
    #[serde(default)]
    server_mute_snapshots: HashMap<String, ServerMuteSnapshot>,
}
```

`save_mute` の直後(`// ---- note cacheバックエンド設定` の前)にメソッドを追加する。

```rust
    // ---- サーバー側ミュートの前回の集合(Issue #454) ----

    pub fn load_server_mute_snapshot(&self, account_id: &str) -> Result<Option<ServerMuteSnapshot>> {
        Ok(self.data.lock().unwrap().server_mute_snapshots.get(account_id).cloned())
    }

    pub fn save_server_mute_snapshot(&self, account_id: &str, snapshot: &ServerMuteSnapshot) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard.server_mute_snapshots.insert(account_id.to_string(), snapshot.clone());
        self.save(&guard)
    }
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib server_mute_snapshot removed_any_since settings_json_without 2>&1 | grep -E "^error|FAILED|test result"`
Expected: `test result: ok`、失敗 0(4件)。

- [ ] **Step 5: 全体を確認してコミットする**

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^error|FAILED|test result"`
Expected: `test result: ok`、失敗 0(`dead_code` の警告は許容)。

```bash
git add src-tauri/src/store/settings.rs
git commit -m "feat: サーバー側ミュートの前回の集合をアカウントごとに設定へ保存する"
```

## Task 3: 同期での解除の検出と境界の破棄

**Files:**
- Modify: `src-tauri/src/commands/mute.rs`(`sync_server_mutes_core`、補助関数2つ、`mod tests`)

**Interfaces:**
- Consumes: Task 1 の `WordMuteRule::key()`、Task 2 の `ServerMuteSnapshot` と `load_server_mute_snapshot` / `save_server_mute_snapshot`、#452 の `ColumnFence::invalidate_boundaries`、既存の `settings.load_columns()` と `cache.replace_fetch_boundaries`。
- Produces: 外向きの署名は変わらない(`sync_server_mutes_core(state, account_id, client) -> Result<SyncMuteResult>`)。private な補助関数 `reflect_server_mute_change` と `clear_account_boundaries`。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/mute.rs` の最初の `mod tests` の import に、`Column` 関連を足す。

```rust
    use crate::domain::{Column, ColumnKind, FilterQuery, Note, User, Visibility};
```

(既存の `use crate::domain::{Note, User, Visibility};` をこの行に置き換える。)

同じ `mod tests` の末尾(`apply_mute_config_replaces_the_mute_config` の後ろ、最後の `}` の前)に、ヘルパーとテストを追加する。

```rust
    /// `mute/list` / `blocking/list` / `i` を、指定のユーザーIDとワード群で返すモックに組み直す。
    async fn mount_server_mutes(mock: &MockServer, muted_users: &[&str], words: serde_json::Value) {
        mock.reset().await;
        let rows: Vec<serde_json::Value> = muted_users
            .iter()
            .enumerate()
            .map(|(i, u)| serde_json::json!({ "id": format!("r{i}"), "muteeId": u }))
            .collect();
        Mock::given(method("POST"))
            .and(path("/mute/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(rows))
            .mount(mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/blocking/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/i"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "mutedWords": words })))
            .mount(mock)
            .await;
    }

    /// acc1 のカラム c1・c2 と、acc2 のカラム c3 に、それぞれ境界(n500)がある状態の `AppState`。
    async fn state_with_three_columns_and_boundaries() -> AppState {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        for (column_id, account_id) in [("c1", "acc1"), ("c2", "acc1"), ("c3", "acc2")] {
            state
                .settings
                .upsert_column(&Column {
                    id: column_id.into(),
                    account_id: account_id.into(),
                    kind: ColumnKind::Home,
                    order: 0,
                    filter: FilterQuery::Keywords(vec![]),
                    notify_sound: false,
                    notify_desktop: false,
                    notify_sound_choice: String::new(),
                    group_id: "g1".into(),
                    title: None,
                })
                .unwrap();
            state.cache.replace_fetch_boundaries(column_id, &[(0, "n500".to_string())]).await.unwrap();
        }
        state
    }

    async fn boundaries_of(state: &AppState, column_id: &str) -> Vec<(u32, String)> {
        state.cache.get_fetch_boundaries(column_id).await.unwrap()
    }

    fn snap(users: &[&str], words: &[&str]) -> crate::store::settings::ServerMuteSnapshot {
        crate::store::settings::ServerMuteSnapshot::new(
            users.iter().map(|s| s.to_string()),
            words.iter().map(|s| s.to_string()),
        )
    }

    /// 捨てられていない境界(`state_with_three_columns_and_boundaries` が置いたもの)。
    fn kept() -> Vec<(u32, String)> {
        vec![(0, "n500".to_string())]
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_keeps_boundaries_and_saves_a_snapshot_on_the_first_sync() {
        let mock = MockServer::start().await;
        mount_server_mutes(&mock, &["u1"], serde_json::json!([])).await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;

        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        // 保存値が無い(アップグレード直後)ので、捨てずに保存だけする
        for column_id in ["c1", "c2", "c3"] {
            assert_eq!(boundaries_of(&state, column_id).await, kept());
        }
        assert_eq!(state.settings.load_server_mute_snapshot("acc1").unwrap(), Some(snap(&["u1"], &[])));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_keeps_boundaries_when_server_mutes_were_only_added() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;
        mount_server_mutes(&mock, &["u1"], serde_json::json!(["spoiler"])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        mount_server_mutes(&mock, &["u1", "u2"], serde_json::json!(["spoiler", "alpha"])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        for column_id in ["c1", "c2", "c3"] {
            assert_eq!(boundaries_of(&state, column_id).await, kept(), "追加だけでは捨てない");
        }
        let saved = state.settings.load_server_mute_snapshot("acc1").unwrap().unwrap();
        assert_eq!(saved.users, vec!["u1".to_string(), "u2".to_string()], "保存値は更新される");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_clears_only_the_syncing_accounts_boundaries_when_a_muted_user_was_removed() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;
        mount_server_mutes(&mock, &["u1", "u2"], serde_json::json!([])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        mount_server_mutes(&mock, &["u1"], serde_json::json!([])).await; // u2 のミュートを解除
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        assert!(boundaries_of(&state, "c1").await.is_empty());
        assert!(boundaries_of(&state, "c2").await.is_empty());
        assert_eq!(boundaries_of(&state, "c3").await, kept(), "他のアカウントのカラムは、そのまま");
        assert_eq!(state.settings.load_server_mute_snapshot("acc1").unwrap(), Some(snap(&["u1"], &[])));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_clears_the_accounts_boundaries_when_a_muted_word_was_removed() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;
        mount_server_mutes(&mock, &[], serde_json::json!(["spoiler", "alpha"])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        mount_server_mutes(&mock, &[], serde_json::json!(["spoiler"])).await; // "alpha" を解除
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        assert!(boundaries_of(&state, "c1").await.is_empty());
        assert!(boundaries_of(&state, "c2").await.is_empty());
        assert_eq!(boundaries_of(&state, "c3").await, kept());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_changes_nothing_when_the_fetch_fails() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;
        mount_server_mutes(&mock, &["u1", "u2"], serde_json::json!([])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        mock.reset().await;
        Mock::given(method("POST"))
            .and(path("/mute/list"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock)
            .await;
        let result = sync_server_mutes_core(&state, "acc1", &client).await;

        assert!(result.is_err());
        for column_id in ["c1", "c2", "c3"] {
            assert_eq!(boundaries_of(&state, column_id).await, kept());
        }
        assert_eq!(state.settings.load_server_mute_snapshot("acc1").unwrap(), Some(snap(&["u1", "u2"], &[])));
        assert!(state.is_server_muted("acc1", "u2"), "メモリ上のミュート集合も、そのまま");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_makes_earlier_boundary_epochs_stale_when_it_clears_boundaries() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;
        mount_server_mutes(&mock, &["u1", "u2"], serde_json::json!([])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();
        let epoch = state.column_fence.begin("c1"); // 旧い集合で取得を始めた(再認証のときのように)

        mount_server_mutes(&mock, &["u1"], serde_json::json!([])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        let boundaries_ok = state.column_fence.write_if_current("c1", &epoch, |ok| async move { ok }).await;
        assert_eq!(boundaries_ok, Some(false), "旧い集合で控えた境界の世代は、古くなる");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_saves_the_snapshot_even_when_the_account_has_no_columns() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        mount_server_mutes(&mock, &["u1", "u2"], serde_json::json!([])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        mount_server_mutes(&mock, &["u1"], serde_json::json!([])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        assert_eq!(state.settings.load_server_mute_snapshot("acc1").unwrap(), Some(snap(&["u1"], &[])));
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib sync_ 2>&1 | grep -E "^error|^test .*(FAILED|ok)$|test result" | sed 's/commands::mute::tests:://'`
Expected: コンパイルは通る(`ServerMuteSnapshot` と `key` は Task 1・2 で追加済み)。**7件すべてが FAILED** になる。どのテストも、保存値(`load_server_mute_snapshot`)を確認しており、実装前は何も保存されないため。加えて、次の3件は、境界が空になること/世代が古くなることも期待するので、その理由でも失敗する。
- `sync_clears_only_the_syncing_accounts_boundaries_when_a_muted_user_was_removed`
- `sync_clears_the_accounts_boundaries_when_a_muted_word_was_removed`
- `sync_makes_earlier_boundary_epochs_stale_when_it_clears_boundaries`

失敗の理由が、保存値の `None`(期待は `Some(..)`)または、境界が空にならないことであることを、出力で確認する(`ServerMuteSnapshot` が見つからない、といったコンパイルエラーではないこと)。

- [ ] **Step 3: 最小の実装を書く**

`src-tauri/src/commands/mute.rs` の import に追加する(`use crate::state::AppState;` の次の行)。

```rust
use crate::store::settings::ServerMuteSnapshot;
```

`sync_server_mutes_core` の本体を置き換え、補助関数2つを、その直後に追加する。

```rust
async fn sync_server_mutes_core(
    state: &AppState,
    account_id: &str,
    client: &crate::api::MisskeyClient,
) -> Result<SyncMuteResult> {
    let ids = fetch_muted_and_blocked(client).await?;
    let word_rules = fetch_muted_words(client).await?;
    let result = SyncMuteResult {
        blocked_users: ids.len() as u32,
        word_rules: word_rules.len() as u32,
    };
    let snapshot = ServerMuteSnapshot::new(ids.iter().cloned(), word_rules.iter().map(|r| r.key()));
    // メモリ上の集合の差し替えは、境界を捨てる前に行う。書き込み側は、ミュート設定を読む前に世代を
    // 控えるので、境界の世代が進んだ時点で、新しい集合がすでに反映されている(Issue #454, #452)。
    state.set_server_mutes(account_id, ids);
    state.set_server_word_mutes(account_id, word_rules);
    reflect_server_mute_change(state, account_id, snapshot).await;
    Ok(result)
}

/// サーバー側ミュートの**解除**を検出して、そのアカウントのカラムの backfill 境界を捨てる(Issue #454)。
///
/// キャッシュには、取得時のフィルタを通ったノートだけが入る。ミュートを解除しても、除外済みのノートは
/// キャッシュに無く読み直せないので、境界を捨てて、次回の backfill を API 経由に倒す(ローカル NG の
/// `set_mute` と同じ理由、Issue #228)。ミュートの追加は、提供時に再適用されるので、捨てない。
///
/// - 前回の保存値が無い(アップグレード直後、新しいアカウント)場合は、捨てずに保存だけする。
/// - 境界の破棄が失敗したら、保存値を更新しない(次回の同期で、もう一度解除を検出して再試行する)。
/// - 保存値の書き込みの失敗は、同期を失敗にしない(次回、もう一度検出するだけで、捨てるのは安全側)。
async fn reflect_server_mute_change(state: &AppState, account_id: &str, snapshot: ServerMuteSnapshot) {
    let previous = match state.settings.load_server_mute_snapshot(account_id) {
        Ok(previous) => previous,
        Err(e) => {
            log::warn!("failed to load the server mute snapshot for {account_id}: {e}");
            return;
        }
    };
    if let Some(previous) = &previous {
        if snapshot.removed_any_since(previous) {
            if let Err(e) = clear_account_boundaries(state, account_id).await {
                log::warn!("failed to clear backfill boundaries after a server mute was removed ({account_id}): {e}");
                return;
            }
        }
    }
    if previous.as_ref() != Some(&snapshot) {
        if let Err(e) = state.settings.save_server_mute_snapshot(account_id, &snapshot) {
            log::warn!("failed to save the server mute snapshot for {account_id}: {e}");
        }
    }
}

/// そのアカウントの全カラムの境界を捨てる。境界の書きロックの中で行うので、実行中の取得が、
/// 旧い集合の結果に基づく境界を、直後に書き込んで復活させない(`ColumnFence::invalidate_boundaries`)。
/// 1件でも失敗したら、残りのカラムも実行した上で、最初のエラーを返す。
async fn clear_account_boundaries(state: &AppState, account_id: &str) -> Result<()> {
    let column_ids: Vec<String> = state
        .settings
        .load_columns()?
        .into_iter()
        .filter(|c| c.account_id == account_id)
        .map(|c| c.id)
        .collect();
    state
        .column_fence
        .invalidate_boundaries(|| async {
            let mut first_error = None;
            for column_id in &column_ids {
                if let Err(e) = state.cache.replace_fetch_boundaries(column_id, &[]).await {
                    first_error.get_or_insert(e);
                }
            }
            first_error.map_or(Ok(()), Err)
        })
        .await
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib sync_ 2>&1 | grep -E "^error|^test .*FAILED|test result"`
Expected: `test result: ok`、失敗 0。

- [ ] **Step 5: 全体を確認してコミットする**

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^error|^warning: (unused|method|function)|FAILED|test result"`
Expected: `test result: ok`、失敗 0。`dead_code` の警告(`key` など)が消えていること。

```bash
git add src-tauri/src/commands/mute.rs
git commit -m "fix: サーバー側ミュートの解除を検出してそのアカウントの境界を捨てる"
```

## Task 4: 全体の検証

**Files:** 変更なし(検証と、結果の記録のみ)。

- [ ] **Step 1: Rust の全テストと TS バインディングの差分を確認する**

Run: `cd src-tauri && cargo test 2>&1 | grep -E "^test result|FAILED"; cd .. && echo "bindings diff vs main: [$(git diff --stat main -- frontend/src/bindings/tauri.gen.ts)]"; git status --short`
Expected: `test result: ok`(失敗 0)。バインディングの差分は空(`[]`)。作業ツリーはクリーン。

- [ ] **Step 2: 変異確認: 解除の検出を外すと、「減った」系のテストが落ちる**

`src-tauri/src/store/settings.rs` の `removed_any_since` の本体を、一時的に常に偽にする。

```rust
    pub fn removed_any_since(&self, previous: &ServerMuteSnapshot) -> bool {
        let _ = previous; // MUTATION
        false
    }
```

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^test .*FAILED|test result" | sed 's/commands::mute::tests:://;s/store::settings::tests:://'`
Expected: 次のテストが FAILED になる。
- `removed_any_since_detects_only_removals`
- `sync_clears_only_the_syncing_accounts_boundaries_when_a_muted_user_was_removed`
- `sync_clears_the_accounts_boundaries_when_a_muted_word_was_removed`
- `sync_makes_earlier_boundary_epochs_stale_when_it_clears_boundaries`

「追加だけでは捨てない」「初回は捨てない」「取得失敗」は、この変異では落ちない。それでよい。

確認したら、必ず元に戻す。

Run: `git checkout -- src-tauri/src/store/settings.rs && git status --short`
Expected: 作業ツリーはクリーン。`cargo test --lib` が再び全部通ること。

- [ ] **Step 3: 静的確認: 境界を捨てるのは、境界の書きロックの中だけ**

Run: `cd src-tauri && grep -n "replace_fetch_boundaries(column_id, &\[\])\|invalidate_boundaries" src/commands/mute.rs | grep -v "^[0-9]*:\s*///"`
Expected: `clear_account_boundaries` の `invalidate_boundaries` の中の `replace_fetch_boundaries(column_id, &[])`、`apply_mute_config` の `invalidate_boundaries`、テストの中だけ。

Run: `cd src-tauri && sed -n '/^async fn sync_server_mutes_core/,/^}/p' src/commands/mute.rs | grep -n "set_server_mutes\|set_server_word_mutes\|reflect_server_mute_change"`
Expected: `set_server_mutes` と `set_server_word_mutes` が、`reflect_server_mute_change` より**前**に並ぶ(不変条件)。

- [ ] **Step 4: 実機確認の扱いを記録する**

実アプリでの確認は行わない。理由は、#451 / #453 と同じ(既存の E2E は、ミュートも backfill も通らない。本物の Misskey でミュートを解除して再起動する手順は、環境依存で重い)。PR の検証欄に、そのままの事実を書く。`sync_server_mutes`(`#[tauri::command]` の `State` の取り出し)は、薄いラッパーで単体テストを書けない。中核の `sync_server_mutes_core` を、wiremock とメモリ上の設定・キャッシュでテストしている。

- [ ] **Step 5: ブランチの状態を確認して、ユーザーに報告する**

Run: `git status --short; git log --oneline main..HEAD`
Expected: 作業ツリーはクリーン。コミットは spec 1つ、plan 1つ、Task 1〜3 の3つ。

push と PR 作成は、ユーザーの指示を受けてから行う。PR 本文には `Fixes #454` を入れ、`.github/pull_request_template.md` の構造(概要、関連Issue、修正内容、影響範囲、検証)に手で合わせる。検証欄には、実行したコマンドと結果、実機確認を行っていないこと(理由つき)、既知の制限(`i` フラグだけの変更を検出できないこと、保存値が無い場合は捨てないこと、定期同期が無いこと)を、そのまま書く。
