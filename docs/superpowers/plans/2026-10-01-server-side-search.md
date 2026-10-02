# サーバーサイド検索 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 検索モーダルに「検索対象: キャッシュ / サーバー」を追加し、カラムを作らずに Misskey の `notes/search` を一回性で検索できるようにする（Issue #430）。

**Architecture:** Rust 側に純粋関数（バージョン判定・ボディ組み立て・ホスト正規化・入力検証・ミュート適用）を切り出して単体テストし、薄い Tauri コマンド2つ（`get_search_capabilities` / `search_server_notes`）でつなぐ。サーバーのバージョンは `AppState` にインメモリでキャッシュし、`SearchCapabilities { dateRange }` としてフロントへ返す。フロントは `SearchModal.svelte` に検索対象の切替を足し、サーバー検索では TQL タブを隠し、日時欄は `dateRange` が true のときだけ出す。

**Tech Stack:** Rust (Tauri v2, tauri-specta `=0.0.12`, serde_json), Svelte 5 (runes), Vitest + @testing-library/svelte, flatpickr。

**Spec:** `docs/superpowers/specs/2026-10-01-server-side-search-design.md`

## Global Constraints

- ブランチは `feat/server-side-search`（作成済み）。`main` へ直接コミットしない。
- コミットメッセージは**件名1行のみ**（本文・箇条書き禁止）。`Co-Authored-By` トレーラーは別途付く。`--no-verify` / `--no-gpg-sign` 禁止。`git commit` が失敗・タイムアウトしたら**リトライせず**ユーザーに報告する。
- `frontend/src/bindings/tauri.gen.ts` は生成物。手編集しない。`cd src-tauri && cargo test generates_frontend_bindings` で再生成する。
- コマンド/型を足したら `src-tauri/src/lib.rs` の `specta_builder()` に登録する（`tauri::Builder` だけでは不足）。
- `specta` は `i64`/`u64` を TS へ出せない。日時引数は秒の `u32`（フロントの TQL `created_at` と同じ単位）で受け、Rust 内でミリ秒（`u64`）へ変換する。
- 日時範囲の対応バージョン: Misskey `2025.7.0` 以降（`sinceDate` / `untilDate`）。`rangeStartAt` / `rangeEndAt`（2026.6.0 以降）は使わない。
- バージョン不明・取得失敗・パース不能は「日時非対応」として扱う（日時欄を隠す側に倒す）。
- `InstanceInfo` / `RawMeta` / `Account` は変更しない。
- ネットワーク呼び出しはモックしない流儀。実接続テストは `#[ignore]` を付ける。
- Rust のコメントは日本語・既存の密度に合わせる（「なぜ」を書く）。UI 文言は日本語。
- コマンド実行は `src-tauri`（Rust）/ `frontend`（pnpm）で行う。`./target/debug/tsumugi` や `cargo run` を直接実行しない。

## Review Focus

実装が仕様の沈黙部分で壊れやすい入力・条件（可能性が高い順）:

1. **空白だけのキーワード**: サーバー検索では検索できない（UI は検索ボタン無効、Rust は `Error::Invalid`）。→ Task 4（`check_search_request`）/ Task 5（Vitest）
2. **フォーク由来・短縮・不正なバージョン文字列**（`2025.4.1-io.12b-...`、`2025.7`、空、`v` 付き）。→ Task 1
3. **インスタンス欄に自分のホスト名（大文字小文字違い・前後空白付き）や `.` を入れた場合**: ローカル指定 `"."` に正規化される。→ Task 4
4. **サーバーから返ったノートがローカル/サーバー/ワードミュートに該当**: キャッシュ検索と同じく除外される。→ Task 4
5. **検索対象・アカウントの切替や能力取得の失敗/遅延で古い結果・古い欄が残る**: 結果は切替でクリア、能力取得失敗時は日時欄も注記も出さない。→ Task 5

---

### Task 1: 検索能力の判定（`domain/search.rs`）

**Files:**
- Create: `src-tauri/src/domain/search.rs`
- Modify: `src-tauri/src/domain/mod.rs`（`mod search;` と `pub use` を追加）

**Interfaces:**
- Consumes: なし
- Produces（Task 2 / 4 が使う）:
  - `pub struct SearchCapabilities { pub date_range: bool }`（`Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type`、`#[serde(rename_all = "camelCase")]`）
  - `pub fn parse_misskey_version(version: &str) -> Option<(u32, u32, u32)>`
  - `pub fn search_capabilities(version: Option<&str>) -> SearchCapabilities`
  - いずれも `crate::domain::` から参照できる

- [ ] **Step 1: 失敗するテストを書く**（ファイルをテストだけで作る）

`src-tauri/src/domain/search.rs`:

```rust
//! サーバーサイド検索(Issue #430)で、接続先サーバーが対応する検索機能の判定。

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_suffixed_versions() {
        assert_eq!(parse_misskey_version("2026.9.1"), Some((2026, 9, 1)));
        assert_eq!(parse_misskey_version("2026.10.0-alpha.0"), Some((2026, 10, 0)));
        // フォーク(misskey.io 等)のサフィックスは無視して先頭の YYYY.M.P だけ読む
        assert_eq!(parse_misskey_version("2025.4.1-io.12b-fb6fbea074"), Some((2025, 4, 1)));
        assert_eq!(parse_misskey_version("2025.7.0+build.5"), Some((2025, 7, 0)));
        assert_eq!(parse_misskey_version(" 2025.7.0 "), Some((2025, 7, 0)));
    }

    #[test]
    fn rejects_unparseable_versions() {
        for v in ["", "abc", "2025", "2025.7", "2025.x.0", "v2025.7.0", "-alpha"] {
            assert_eq!(parse_misskey_version(v), None, "{v:?} should not parse");
        }
    }

    #[test]
    fn date_range_requires_2025_7_0_or_later() {
        assert!(!search_capabilities(Some("2025.6.4")).date_range);
        assert!(search_capabilities(Some("2025.7.0")).date_range);
        assert!(search_capabilities(Some("2026.9.1")).date_range);
        assert!(search_capabilities(Some("2026.10.0-alpha.0")).date_range);
        // フォークの古い番号は非対応扱い
        assert!(!search_capabilities(Some("2025.4.1-io.12b-fb6fbea074")).date_range);
    }

    #[test]
    fn unknown_or_unparseable_version_means_no_date_range() {
        assert!(!search_capabilities(None).date_range);
        assert!(!search_capabilities(Some("garbage")).date_range);
    }

    #[test]
    fn serializes_as_camel_case_for_the_frontend() {
        let v = serde_json::to_value(SearchCapabilities { date_range: true }).unwrap();
        assert_eq!(v, serde_json::json!({ "dateRange": true }));
    }
}
```

`src-tauri/src/domain/mod.rs` に `mod search;`（`mod reaction;` と `mod share;` の間）を足す:

```rust
mod reaction;
mod search;
mod share;
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib domain::search`
Expected: コンパイルエラー（`parse_misskey_version` / `search_capabilities` / `SearchCapabilities` が未定義）

- [ ] **Step 3: 実装する**

`src-tauri/src/domain/search.rs` の先頭（`//!` コメントの直後、`#[cfg(test)]` の前）に追加:

```rust
use serde::Serialize;
use specta::Type;

/// `notes/search` の日時範囲(`sinceDate`/`untilDate`)が入った Misskey のバージョン。
const DATE_RANGE_MIN_VERSION: (u32, u32, u32) = (2025, 7, 0);

/// 接続先サーバーが対応する検索機能。フロントはこれを見て入力欄の出し分けをする。
/// 将来 `/api.json` から実際の対応パラメータで判定する方式へ変えても、この型を介せば
/// 呼び出し側(コマンド/フロント)は変更不要。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchCapabilities {
    /// 日時範囲の指定に対応しているか。
    pub date_range: bool,
}

/// Misskey のバージョン文字列から先頭の `YYYY.M.P` だけを読む。
/// `-alpha.0` や `-io.12b-...` などのサフィックスは無視する。形式が合わなければ None。
pub fn parse_misskey_version(version: &str) -> Option<(u32, u32, u32)> {
    let core = version.trim().split(|c: char| c == '-' || c == '+').next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// バージョンから対応機能を決める。None・パース不能は安全側（非対応）に倒す。
pub fn search_capabilities(version: Option<&str>) -> SearchCapabilities {
    let date_range = version
        .and_then(parse_misskey_version)
        .is_some_and(|v| v >= DATE_RANGE_MIN_VERSION);
    SearchCapabilities { date_range }
}
```

`src-tauri/src/domain/mod.rs` の `pub use` 群（`pub use reaction::...` の次の行）に追加:

```rust
pub use search::{parse_misskey_version, search_capabilities, SearchCapabilities};
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd src-tauri && cargo test --lib domain::search`
Expected: 5 passed

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/domain/search.rs src-tauri/src/domain/mod.rs
git commit -m "feat: サーバー検索の対応機能(SearchCapabilities)をバージョンから判定する"
```

---

### Task 2: サーバーバージョンの取得と AppState キャッシュ

**Files:**
- Modify: `src-tauri/src/api/meta.rs`（`fetch_server_version` 追加、テスト追加）
- Modify: `src-tauri/src/state.rs`（`server_versions` フィールドとアクセサ追加、テスト追加）
- Modify: `src-tauri/src/commands/account.rs`（`remove_account` でキャッシュを破棄）

**Interfaces:**
- Consumes: Task 1 の型は使わない（このタスクは文字列の取得と保持のみ）
- Produces（Task 4 が使う）:
  - `pub async fn fetch_server_version(client: &MisskeyClient) -> Result<Option<String>>`（`crate::api::meta`）
  - `AppState::server_version(&self, account_id: &str) -> Option<String>`
  - `AppState::set_server_version(&self, account_id: &str, version: String)`
  - `AppState::forget_server_version(&self, account_id: &str)`

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/api/meta.rs` の `mod meta_info_tests` 内（`instance_info_from_meta_without_icon_falls_back_to_host_favicon` の後）に追加:

```rust
    #[test]
    fn raw_version_reads_version_and_ignores_other_fields() {
        let raw: RawVersion =
            serde_json::from_str(r#"{"version":"2026.9.1","name":"Misskey.io"}"#).unwrap();
        assert_eq!(raw.version, Some("2026.9.1".to_string()));
    }

    #[test]
    fn raw_version_defaults_missing_version_to_none() {
        let raw: RawVersion = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(raw.version, None);
    }

    /// 実サーバーの `/api/meta` から version が取れること（認証不要）。
    /// ネットワーク依存のため既定では実行しない: `cargo test --lib real_misskey_meta -- --ignored`
    #[ignore]
    #[tokio::test]
    async fn real_misskey_meta_version_supports_date_range_search() {
        let client = MisskeyClient::new(reqwest::Client::new(), "misskey.omhnc.net", None);
        let version = fetch_server_version(&client).await.unwrap();
        let caps = crate::domain::search_capabilities(version.as_deref());
        assert!(version.is_some(), "version should be present");
        assert!(caps.date_range, "misskey.omhnc.net is 2026.x so date range must be supported");
    }
```

`src-tauri/src/state.rs` の `mod tests` 内（`restores_persisted_accounts_on_construction` の後）に追加:

```rust
    #[test]
    fn server_version_is_remembered_per_account_and_forgettable() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        assert_eq!(state.server_version("a1"), None);

        state.set_server_version("a1", "2026.9.1".into());
        assert_eq!(state.server_version("a1").as_deref(), Some("2026.9.1"));
        assert_eq!(state.server_version("a2"), None);

        state.forget_server_version("a1");
        assert_eq!(state.server_version("a1"), None);
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib -- raw_version server_version_is_remembered`
Expected: コンパイルエラー（`RawVersion` / `fetch_server_version` / `server_version` 等が未定義）

- [ ] **Step 3: 実装する**

`src-tauri/src/api/meta.rs` の `fetch_meta` の直後（`#[derive(Debug, Deserialize)] struct RawMeta` の前）に追加:

```rust
/// `/api/meta` の `version` だけを読む。
#[derive(Debug, Deserialize)]
struct RawVersion {
    #[serde(default)]
    version: Option<String>,
}

/// 接続先サーバーの Misskey バージョン文字列（サーバーサイド検索の対応機能判定用、Issue #430）。
/// `InstanceInfo` はリモートユーザーの `User.instance` やキャッシュDB列と共用のため、
/// そこへは足さずここで独立に取得する。
pub async fn fetch_server_version(client: &MisskeyClient) -> Result<Option<String>> {
    let raw: RawVersion = client.post("meta", &json!({ "detail": false })).await?;
    Ok(raw.version)
}
```

`src-tauri/src/state.rs`: `AppState` の `emoji_cache` フィールドの直後に追加:

```rust
    /// account_id -> 接続先サーバーの Misskey バージョン文字列(`/api/meta`)。サーバーサイド検索
    /// (Issue #430)の対応機能判定に使う。取得に成功した値だけ保存し、アプリ再起動まで再取得しない。
    pub server_versions: Mutex<HashMap<String, String>>,
```

`new_with_sound` の `emoji_cache: Mutex::new(HashMap::new()),` の直後に追加:

```rust
            server_versions: Mutex::new(HashMap::new()),
```

`impl AppState` 内、`set_server_word_mutes` の直後に追加:

```rust
    /// account の接続先サーバーのバージョン（取得済みの場合のみ）。
    pub fn server_version(&self, account_id: &str) -> Option<String> {
        self.server_versions.lock().unwrap().get(account_id).cloned()
    }

    /// account の接続先サーバーのバージョンを保存する。
    pub fn set_server_version(&self, account_id: &str, version: String) {
        self.server_versions
            .lock()
            .unwrap()
            .insert(account_id.to_string(), version);
    }

    /// account のバージョンキャッシュを破棄する（アカウント削除時）。
    pub fn forget_server_version(&self, account_id: &str) {
        self.server_versions.lock().unwrap().remove(account_id);
    }
```

`src-tauri/src/commands/account.rs` の `remove_account` で、`state.secrets.delete(&account_id)?;` の直後に追加:

```rust
    state.forget_server_version(&account_id);
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd src-tauri && cargo test --lib -- raw_version server_version_is_remembered`
Expected: 3 passed

Run（実接続の任意確認。ネットワーク可なら）: `cd src-tauri && cargo test --lib real_misskey_meta -- --ignored`
Expected: 1 passed

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/api/meta.rs src-tauri/src/state.rs src-tauri/src/commands/account.rs
git commit -m "feat: 接続先サーバーのバージョン取得とAppStateキャッシュを追加する"
```

---

### Task 3: `notes/search` リクエストの組み立て（`api/notes.rs`）

**Files:**
- Modify: `src-tauri/src/api/notes.rs`（`SearchParams` / `build_search_body` / `search_notes` 追加、`mod tests` にテスト追加）

**Interfaces:**
- Consumes: 既存の `fetch_notes(client, endpoint, body) -> Result<Vec<Note>>`
- Produces（Task 4 が使う）:
  - `pub struct SearchParams { pub query: String, pub user_id: Option<String>, pub host: Option<String>, pub since_date_ms: Option<u64>, pub until_date_ms: Option<u64>, pub until_id: Option<String>, pub limit: u32 }`（`Debug, Clone, Default, PartialEq`）
  - `pub fn build_search_body(p: &SearchParams) -> serde_json::Value`
  - `pub async fn search_notes(client: &MisskeyClient, p: &SearchParams) -> Result<Vec<Note>>`

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/api/notes.rs` の既存 `mod tests`（208行付近の `mod tests {`）の末尾に追加:

```rust
    #[test]
    fn build_search_body_includes_every_given_condition() {
        let body = build_search_body(&SearchParams {
            query: "rust".into(),
            user_id: Some("u9".into()),
            host: Some("example.com".into()),
            since_date_ms: Some(1_700_000_000_000),
            until_date_ms: Some(1_800_000_000_000),
            until_id: Some("n5".into()),
            limit: 20,
        });
        assert_eq!(
            body,
            json!({
                "query": "rust",
                "limit": 20,
                "userId": "u9",
                "host": "example.com",
                "sinceDate": 1_700_000_000_000u64,
                "untilDate": 1_800_000_000_000u64,
                "untilId": "n5",
            })
        );
    }

    #[test]
    fn build_search_body_omits_conditions_that_are_none() {
        let body = build_search_body(&SearchParams { query: "x".into(), limit: 10, ..Default::default() });
        assert_eq!(body, json!({ "query": "x", "limit": 10 }));
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib build_search_body`
Expected: コンパイルエラー（`SearchParams` / `build_search_body` が未定義）

- [ ] **Step 3: 実装する**

`src-tauri/src/api/notes.rs` の `fetch_notes` の直後に追加:

```rust
/// `notes/search` の検索条件（サーバーサイド検索、Issue #430）。日時はミリ秒の epoch。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchParams {
    pub query: String,
    pub user_id: Option<String>,
    /// ローカルは `"."`（Misskey の仕様）。
    pub host: Option<String>,
    pub since_date_ms: Option<u64>,
    pub until_date_ms: Option<u64>,
    pub until_id: Option<String>,
    pub limit: u32,
}

/// `notes/search` のリクエストボディ。`None` の条件はキーごと出さない
/// （対応していない古いサーバーに余計なパラメータを送らないため）。
pub fn build_search_body(p: &SearchParams) -> serde_json::Value {
    let mut body = json!({ "query": p.query, "limit": p.limit });
    if let Some(v) = &p.user_id {
        body["userId"] = json!(v);
    }
    if let Some(v) = &p.host {
        body["host"] = json!(v);
    }
    if let Some(v) = p.since_date_ms {
        body["sinceDate"] = json!(v);
    }
    if let Some(v) = p.until_date_ms {
        body["untilDate"] = json!(v);
    }
    if let Some(v) = &p.until_id {
        body["untilId"] = json!(v);
    }
    body
}

/// サーバーサイド検索（`notes/search`）。
pub async fn search_notes(client: &MisskeyClient, p: &SearchParams) -> Result<Vec<Note>> {
    fetch_notes(client, "notes/search", &build_search_body(p)).await
}
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd src-tauri && cargo test --lib build_search_body`
Expected: 2 passed

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/api/notes.rs
git commit -m "feat: notes/searchのリクエスト組み立て(SearchParams)を追加する"
```

---

### Task 4: Tauri コマンド `get_search_capabilities` / `search_server_notes`

**Files:**
- Modify: `src-tauri/src/commands/column.rs`（import 追加、ヘルパー・コマンド追加、`mod tests` にテスト追加）
- Modify: `src-tauri/src/lib.rs`（`specta_builder()` 登録、バインディング生成テストに assert 追加）
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: Task 1 の `SearchCapabilities` / `search_capabilities`、Task 2 の `fetch_server_version` / `AppState::{server_version,set_server_version}`、Task 3 の `SearchParams` / `search_notes`、既存の `resolve_user` / `server_muted_note` / `AppState::is_word_muted` / `crate::filter::mute::is_muted`
- Produces（Task 5 が使う。生成される TS）:
  - `commands.getSearchCapabilities(accountId: string)` → `Result<SearchCapabilities, Error>`（`SearchCapabilities = { dateRange: boolean }`）
  - `commands.searchServerNotes(accountId: string, query: string, acct: string | null, host: string | null, sinceDate: number | null, untilDate: number | null, untilId: string | null, limit: number)` → `Result<Note[], Error>`

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/column.rs` の `mod tests`（`use super::*;` がある）の末尾に追加。`note(id, created_at)` ヘルパーは同モジュールに既存:

```rust
    #[test]
    fn normalize_search_host_maps_own_host_and_dot_to_local() {
        assert_eq!(normalize_search_host(None, "misskey.io"), None);
        assert_eq!(normalize_search_host(Some(""), "misskey.io"), None);
        assert_eq!(normalize_search_host(Some("   "), "misskey.io"), None);
        assert_eq!(normalize_search_host(Some("."), "misskey.io"), Some(".".into()));
        assert_eq!(normalize_search_host(Some("misskey.io"), "misskey.io"), Some(".".into()));
        // 大文字小文字・前後空白は無視して自ホストと比較する
        assert_eq!(normalize_search_host(Some("  Misskey.IO "), "misskey.io"), Some(".".into()));
        assert_eq!(
            normalize_search_host(Some(" example.com "), "misskey.io"),
            Some("example.com".into())
        );
    }

    #[test]
    fn check_search_request_requires_keyword_and_capability_for_dates() {
        let no_date = SearchCapabilities { date_range: false };
        let with_date = SearchCapabilities { date_range: true };

        assert!(check_search_request("rust", false, &no_date).is_ok());
        assert!(check_search_request("rust", true, &with_date).is_ok());
        assert!(matches!(check_search_request("", false, &with_date), Err(Error::Invalid(_))));
        assert!(matches!(check_search_request("  \t", false, &with_date), Err(Error::Invalid(_))));
        // 日時が指定されたのにサーバーが非対応なら拒否（UIが隠す前提の二重防御）
        assert!(matches!(check_search_request("rust", true, &no_date), Err(Error::Invalid(_))));
    }

    #[test]
    fn secs_to_ms_converts_without_overflow() {
        assert_eq!(secs_to_ms(0), 0);
        assert_eq!(secs_to_ms(1_700_000_000), 1_700_000_000_000);
        assert_eq!(secs_to_ms(u32::MAX), 4_294_967_295_000);
    }

    #[test]
    fn apply_search_mutes_drops_local_server_and_word_muted_notes_keeping_order() {
        let mut spoiler = note("n1", 100);
        spoiler.text = Some("spoiler content".into());
        let notes = vec![spoiler, note("n2", 200), note("n3", 300), note("n4", 400)];
        let mute = MuteConfig { ng_words: vec!["spoiler".into()], ..Default::default() };

        let got = apply_search_mutes(notes, &mute, |n| n.id == "n2", |n| n.id == "n3");

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n4"]);
    }

    #[tokio::test]
    async fn search_capabilities_for_uses_the_cached_version_without_network() {
        let state = AppState::new_for_test(crate::store::SettingsStore::new_in_memory());
        state.set_server_version("a1", "2026.9.1".into());
        state.set_server_version("a2", "2025.4.1-io.12b-fb6fbea074".into());

        assert!(search_capabilities_for(&state, "a1").await.unwrap().date_range);
        assert!(!search_capabilities_for(&state, "a2").await.unwrap().date_range);
    }
```

`src-tauri/src/lib.rs` の `generates_frontend_bindings` テスト内、`assert!(ts.contains("whoami"));` の直後に追加:

```rust
        // サーバーサイド検索(Issue #430)のコマンドと型
        assert!(ts.contains("searchServerNotes"), "missing searchServerNotes command");
        assert!(ts.contains("getSearchCapabilities"), "missing getSearchCapabilities command");
        assert!(ts.contains("dateRange"), "SearchCapabilities.date_range should be camelCase");
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib -- normalize_search_host check_search_request secs_to_ms apply_search_mutes search_capabilities_for`
Expected: コンパイルエラー（各関数が未定義）

- [ ] **Step 3: 実装する**

`src-tauri/src/commands/column.rs` の import を更新する（既存行を置き換え）:

```rust
use crate::api::meta::{
    fetch_antennas, fetch_followed_channels, fetch_server_version, fetch_user_lists, resolve_user,
};
use crate::api::notes::{fetch_notes, search_notes, SearchParams};
```

```rust
use crate::domain::{
    search_capabilities, Column, ColumnGroup, ColumnKind, Edge, FilterQuery, MuteConfig, Note,
    Notification, PaneNode, SearchCapabilities, SourceItem, SplitDirection, User, UserList,
};
```

`search_cache_notes` コマンドの直後（`/// 解決済みソース群から REST 初期/過去ページを取得し…` の doc コメントの前）に追加:

```rust
/// サーバー検索(Issue #430)の `host` 入力を API 用に正規化する。空は未指定、`.` と
/// アカウント自身のホスト（大文字小文字・前後空白は無視）は Misskey の「ローカル」表記 `"."` にする。
fn normalize_search_host(input: Option<&str>, account_host: &str) -> Option<String> {
    let h = input?.trim();
    if h.is_empty() {
        return None;
    }
    if h == "." || h.eq_ignore_ascii_case(account_host) {
        return Some(".".into());
    }
    Some(h.to_string())
}

/// サーバー検索の入力検証。`notes/search` は `query` 必須（空の挙動は未確認なので送らない）で、
/// 日時範囲はサーバーが対応しているときだけ許可する。
fn check_search_request(query: &str, has_date: bool, caps: &SearchCapabilities) -> Result<()> {
    if query.trim().is_empty() {
        return Err(Error::Invalid("search query is empty".into()));
    }
    if has_date && !caps.date_range {
        return Err(Error::Invalid("date range search is not supported by this server".into()));
    }
    Ok(())
}

/// フロントから秒で受けた日時を API が使うミリ秒へ変換する。
fn secs_to_ms(secs: u32) -> u64 {
    u64::from(secs) * 1000
}

/// サーバー検索の結果にキャッシュ検索(`search_cache_core`)と同じミュートを適用する。
fn apply_search_mutes(
    notes: Vec<Note>,
    mute: &MuteConfig,
    is_server_muted: impl Fn(&Note) -> bool,
    is_word_muted: impl Fn(&Note) -> bool,
) -> Vec<Note> {
    notes
        .into_iter()
        .filter(|n| !crate::filter::mute::is_muted(n, mute) && !is_server_muted(n) && !is_word_muted(n))
        .collect()
}

/// アカウントの接続先サーバーが対応する検索機能。バージョンは `AppState` にキャッシュし、
/// 未取得なら `/api/meta` から取得する。取得失敗は非対応扱い（日時欄を隠す側に倒す）で、
/// 失敗はキャッシュしないため次回また取りに行く。未登録アカウントだけはエラーを返す。
async fn search_capabilities_for(state: &AppState, account_id: &str) -> Result<SearchCapabilities> {
    if let Some(v) = state.server_version(account_id) {
        return Ok(search_capabilities(Some(&v)));
    }
    let client = state.client_for(account_id)?;
    match fetch_server_version(&client).await {
        Ok(Some(v)) => {
            state.set_server_version(account_id, v.clone());
            Ok(search_capabilities(Some(&v)))
        }
        Ok(None) | Err(_) => Ok(search_capabilities(None)),
    }
}

/// 検索モーダル(Issue #430)用: アカウントの接続先サーバーが対応する検索機能を返す。
#[tauri::command]
#[specta::specta]
pub async fn get_search_capabilities(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<SearchCapabilities> {
    search_capabilities_for(&state, &account_id).await
}

/// 検索モーダル(Issue #430)用: Misskey サーバーの `notes/search` による一回性の検索。
/// `acct` は `@user@host` 形式（userId へ解決する）、日時は秒（日時範囲はサーバーが対応する場合のみ）。
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn search_server_notes(
    state: State<'_, AppState>,
    account_id: String,
    query: String,
    acct: Option<String>,
    host: Option<String>,
    since_date: Option<u32>,
    until_date: Option<u32>,
    until_id: Option<String>,
    limit: u32,
) -> Result<Vec<Note>> {
    let has_date = since_date.is_some() || until_date.is_some();
    // 日時指定が無ければ対応判定のためのネットワークアクセスを省く
    let caps = if has_date {
        search_capabilities_for(&state, &account_id).await?
    } else {
        search_capabilities(None)
    };
    check_search_request(&query, has_date, &caps)?;

    let client = state.client_for(&account_id)?;
    let user_id = match acct.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
        Some(a) => Some(resolve_user(&client, a).await?.id),
        None => None,
    };
    let params = SearchParams {
        query: query.trim().to_string(),
        user_id,
        host: normalize_search_host(host.as_deref(), client.host()),
        since_date_ms: since_date.map(secs_to_ms),
        until_date_ms: until_date.map(secs_to_ms),
        until_id,
        limit,
    };
    let raw = search_notes(&client, &params).await?;

    let mute = state.mute.lock().unwrap().clone();
    Ok(apply_search_mutes(
        raw,
        &mute,
        |n| server_muted_note(&state, &account_id, n),
        |n| state.is_word_muted(&account_id, n),
    ))
}
```

`src-tauri/src/lib.rs` の `specta_builder()` で `commands::column::search_cache_notes,` の直後に追加:

```rust
            commands::column::get_search_capabilities,
            commands::column::search_server_notes,
```

- [ ] **Step 4: 通ることを確認し、バインディングを再生成する**

Run: `cd src-tauri && cargo test --lib -- normalize_search_host check_search_request secs_to_ms apply_search_mutes search_capabilities_for`
Expected: 5 passed

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: 1 passed（`frontend/src/bindings/tauri.gen.ts` が更新される）

Run: `git diff --stat ../frontend/src/bindings/tauri.gen.ts && grep -n "searchServerNotes\|getSearchCapabilities\|SearchCapabilities" ../frontend/src/bindings/tauri.gen.ts`
Expected: 差分あり。`searchServerNotes: (accountId: string, query: string, acct: string | null, host: string | null, sinceDate: number | null, untilDate: number | null, untilId: string | null, limit: number)` と `getSearchCapabilities: (accountId: string)` と `SearchCapabilities = { dateRange: boolean }` が含まれる

Run: `cd src-tauri && cargo test`
Expected: 全体 PASS（`#[ignore]` は実行されない）。`cargo clippy` が使えるなら `cargo clippy --all-targets` で新規警告が無いこと。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/commands/column.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: サーバー検索コマンド(search_server_notes / get_search_capabilities)を追加する"
```

---

### Task 5: 検索モーダルのサーバー検索 UI

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`（`searchCacheNotes` の直後にラッパー2つ追加）
- Modify: `frontend/src/ui/SearchModal.svelte`
- Test: `frontend/src/ui/SearchModal.test.ts`（末尾に `describe` を追加）

**Interfaces:**
- Consumes: Task 4 の生成 TS（`commands.searchServerNotes` / `commands.getSearchCapabilities`）
- Produces: `app.searchServerNotes(accountId, params, untilId?, limit?)` と `app.getSearchCapabilities(accountId)`（`SearchModal` 内でのみ使用）。`params` は `{ query: string; acct?: string; host?: string; sinceDate?: number; untilDate?: number }`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/SearchModal.test.ts` の末尾（既存の最後の `});` の後）に追加。モックは既存と同じく invoke の生の戻り値（`typedError` が `{status:"ok"}` に包む）を返す:

```ts
describe("SearchModal サーバー検索", () => {
  const ADV_BASE = "詳細条件を指定（ユーザー・インスタンス）";
  const ADV_WITH_DATE = "詳細条件を指定（ユーザー・インスタンス・日時）";
  const NO_DATE_NOTE = "日時範囲の指定は Misskey 2025.7.0 以降のサーバーで利用できます";

  function mockServer(opts: { dateRange?: boolean; capsFail?: boolean; notes?: Note[] } = {}) {
    app.accounts = [makeAccount()];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_search_capabilities") {
        return opts.capsFail
          ? Promise.reject(new Error("boom"))
          : Promise.resolve({ dateRange: opts.dateRange ?? false });
      }
      if (cmd === "search_server_notes") return Promise.resolve(opts.notes ?? []);
      if (cmd === "search_cache_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
  }

  const calledCommands = () => invokeMock.mock.calls.map((c) => c[0]);

  it("サーバーを選ぶとTQLタブが隠れ、キーワードが空白だけの間は検索できない", async () => {
    mockServer();
    const { getByText, queryByText, getByTestId, getByPlaceholderText } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    const submit = () => getByTestId("search-submit") as HTMLButtonElement;
    const keyword = getByPlaceholderText("本文に含まれる語");

    expect(queryByText("エキスパート(TQL)")).toBeTruthy();
    expect(submit().disabled).toBe(false); // キャッシュ検索は条件なしでも検索できる

    await fireEvent.click(getByText("サーバー"));
    expect(queryByText("エキスパート(TQL)")).toBeNull();
    expect(submit().disabled).toBe(true);

    await fireEvent.input(keyword, { target: { value: "  " } });
    expect(submit().disabled).toBe(true);

    await fireEvent.input(keyword, { target: { value: "rust" } });
    expect(submit().disabled).toBe(false);
  });

  it("サーバー検索はsearch_server_notesを条件付きで呼び、キャッシュ検索は呼ばない", async () => {
    mockServer({ notes: [makeNote("n1", 100, "from server")] });
    const { getByText, getByTestId, getByPlaceholderText } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    await fireEvent.click(getByText("サーバー"));
    await fireEvent.input(getByPlaceholderText("本文に含まれる語"), { target: { value: " rust " } });
    await fireEvent.click(getByText(ADV_BASE));
    await fireEvent.input(getByPlaceholderText(/^@user@host/), { target: { value: "@bob@example.com" } });
    await fireEvent.input(getByPlaceholderText(/^misskey\.example/), { target: { value: "example.com" } });
    await fireEvent.click(getByTestId("search-submit"));

    await waitFor(() => expect(getByText("from server")).toBeTruthy());
    expect(invokeMock).toHaveBeenCalledWith("search_server_notes", {
      accountId: "acc1",
      query: "rust",
      acct: "@bob@example.com",
      host: "example.com",
      sinceDate: null,
      untilDate: null,
      untilId: null,
      limit: 20,
    });
    expect(calledCommands()).not.toContain("search_cache_notes");
  });

  it("日時対応のサーバーでは日時欄が出て、選んだ日時が秒でsearch_server_notesへ渡る", async () => {
    mockServer({ dateRange: true });
    const { container, getByText, findByText, getByTestId, getByPlaceholderText } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    await fireEvent.click(getByText("サーバー"));
    await fireEvent.click(await findByText(ADV_WITH_DATE));
    await findByText("日時（開始）");

    // flatpickrは素のinputにインスタンス(_flatpickr)を載せる。setDate(.., true)でonChangeが走る。
    const [fromInput, toInput] = Array.from(
      container.querySelectorAll<HTMLInputElement>("input[placeholder='未指定']"),
    );
    const from = new Date(2026, 0, 2, 3, 4, 0);
    const to = new Date(2026, 0, 3, 23, 59, 0);
    (fromInput as unknown as { _flatpickr: { setDate(d: Date, t: boolean): void } })._flatpickr.setDate(from, true);
    (toInput as unknown as { _flatpickr: { setDate(d: Date, t: boolean): void } })._flatpickr.setDate(to, true);

    await fireEvent.input(getByPlaceholderText("本文に含まれる語"), { target: { value: "rust" } });
    await fireEvent.click(getByTestId("search-submit"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "search_server_notes",
        expect.objectContaining({
          sinceDate: Math.floor(from.getTime() / 1000),
          untilDate: Math.floor(to.getTime() / 1000),
        }),
      ),
    );
  });

  it("日時非対応のサーバーでは日時欄を出さず、非対応の注記を出す", async () => {
    mockServer({ dateRange: false });
    const { getByText, findByText, queryByText } = render(SearchModal, { props: { onclose: () => {} } });
    await fireEvent.click(getByText("サーバー"));
    await fireEvent.click(getByText(ADV_BASE));

    await findByText(NO_DATE_NOTE);
    expect(queryByText("日時（開始）")).toBeNull();
  });

  it("能力の取得に失敗したら日時欄も注記も出さない", async () => {
    mockServer({ capsFail: true });
    const { getByText, queryByText } = render(SearchModal, { props: { onclose: () => {} } });
    await fireEvent.click(getByText("サーバー"));
    await fireEvent.click(getByText(ADV_BASE));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("get_search_capabilities", { accountId: "acc1" }),
    );
    await new Promise((r) => setTimeout(r, 0)); // 失敗したPromiseの処理を流す
    expect(queryByText("日時（開始）")).toBeNull();
    expect(queryByText(NO_DATE_NOTE)).toBeNull();
  });

  it("検索対象を切り替えると結果がクリアされる", async () => {
    mockServer({ notes: [makeNote("n1", 100, "from server")] });
    const { getByText, queryByText, findByText, getByTestId, getByPlaceholderText } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    await fireEvent.click(getByText("サーバー"));
    await fireEvent.input(getByPlaceholderText("本文に含まれる語"), { target: { value: "rust" } });
    await fireEvent.click(getByTestId("search-submit"));
    await findByText("from server");

    await fireEvent.click(getByText("キャッシュ"));
    expect(queryByText("from server")).toBeNull();
  });

  it("サーバー検索中にアカウントを変えると結果がクリアされる", async () => {
    mockServer({ notes: [makeNote("n1", 100, "from server")] });
    const second: Account = { ...makeAccount(), id: "acc2", username: "carol" };
    app.accounts = [makeAccount(), second];
    const { getByText, queryByText, findByText, getByTestId, getByPlaceholderText } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    await fireEvent.click(getByText("サーバー"));
    await fireEvent.input(getByPlaceholderText("本文に含まれる語"), { target: { value: "rust" } });
    await fireEvent.click(getByTestId("search-submit"));
    await findByText("from server");

    await fireEvent.click(getByTestId("account-select-trigger"));
    await fireEvent.click(getByTestId("account-select-option-acc2"));
    await waitFor(() => expect(queryByText("from server")).toBeNull());
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && pnpm vitest run src/ui/SearchModal.test.ts`
Expected: 新規7件が FAIL（「サーバー」ボタンが無い）。既存テストは PASS のまま。

- [ ] **Step 3: 実装する**

**3-1. `frontend/src/lib/store.svelte.ts`** — `searchCacheNotes` メソッドの直後に追加:

```ts
  /// 検索モーダル(Issue #430)用: サーバー(Misskey notes/search)検索。日時は秒(unix epoch)で渡す。
  /// 呼び出し元(SearchModal)が自前のエラー表示を持つため this.#fail()（バナー表示）は呼ばない。
  async searchServerNotes(
    accountId: string,
    params: { query: string; acct?: string; host?: string; sinceDate?: number; untilDate?: number },
    untilId?: string,
    limit = 20,
  ) {
    try {
      return await unwrapAcc(
        accountId,
        commands.searchServerNotes(
          accountId,
          params.query,
          params.acct ?? null,
          params.host ?? null,
          params.sinceDate ?? null,
          params.untilDate ?? null,
          untilId ?? null,
          limit,
        ),
      );
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }

  /// 検索モーダル(Issue #430)用: アカウントの接続先サーバーが対応する検索機能。
  async getSearchCapabilities(accountId: string) {
    try {
      return await unwrapAcc(accountId, commands.getSearchCapabilities(accountId));
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }
```

**3-2. `frontend/src/ui/SearchModal.svelte` の `<script>`**

(a) `let tqlErr = $state<string | null>(null);` の直後に追加:

```ts
  // 検索対象。サーバー検索(Issue #430)は Misskey の notes/search に問い合わせる。
  let scope = $state<"cache" | "server">("cache");
  // サーバーが対応する検索機能。取得前・取得失敗は null（日時欄は出さない）。
  let caps = $state<{ dateRange: boolean } | null>(null);
  let capsGen = 0;
  // サーバー検索は簡単モード固定（notes/search はTQLを受け付けない）。
  const showGuided = $derived(scope === "server" || uiMode === "guided");
  const dateVisible = $derived(scope === "cache" || caps?.dateRange === true);
```

(b) `loadMore` の直前に追加:

```ts
  // サーバー検索の条件。日時は秒（Rust側でミリ秒へ変換する）。日時欄が出ていないときは
  // 下の $effect が dateFrom/dateTo を null に戻すので、そのまま使える。
  function serverParams() {
    return {
      query: keyword.trim(),
      acct: userAcct.trim() || undefined,
      host: host.trim() || undefined,
      sinceDate: dateFrom ? Math.floor(dateFrom.getTime() / 1000) : undefined,
      untilDate: dateTo ? Math.floor(dateTo.getTime() / 1000) : undefined,
    };
  }
```

(c) `loadMore` 内の次の2行を置き換える:

```ts
      const filter: FilterQuery = { kind: "tql", value: currentPredicate() };
      const page = await app.searchCacheNotes(accountId, filter, untilId, 20);
```
↓
```ts
      const page =
        scope === "server"
          ? await app.searchServerNotes(accountId, serverParams(), untilId, 20)
          : await app.searchCacheNotes(
              accountId,
              { kind: "tql", value: currentPredicate() } satisfies FilterQuery,
              untilId,
              20,
            );
```

(d) `runSearch` を次の4関数に置き換える（`runSearch` 本体は旧 `runSearch` の置換）:

```ts
  function resetResults() {
    requestGen++;
    notes = [];
    busy = false;
    done = false;
    err = null;
    searched = false;
  }

  // サーバー検索はキーワード必須（notes/search の query は必須）。キャッシュ検索は従来どおり
  // エキスパートモードでTQLエラーがある間だけ不可。
  function canSearch(): boolean {
    if (scope === "server") return keyword.trim() !== "";
    return !(uiMode === "expert" && tqlErr);
  }

  function setScope(next: "cache" | "server") {
    if (scope === next) return;
    scope = next;
    resetResults();
  }

  function runSearch(e: Event) {
    e.preventDefault();
    if (!canSearch()) return;
    resetResults();
    searched = true;
    void loadMore();
  }
```

(e) `onScroll` の後（`</script>` の前）に追加:

```ts
  // サーバー検索のときだけ、問い合わせ先アカウントのサーバーが対応する検索機能を取得する。
  // 取得失敗は握りつぶして caps=null のまま（日時欄も注記も出さない）。
  $effect(() => {
    const id = accountId;
    const gen = ++capsGen;
    caps = null;
    if (scope !== "server") return;
    app.getSearchCapabilities(id).then(
      (c) => {
        if (gen === capsGen) caps = c;
      },
      () => {},
    );
  });

  // 日時欄が出ていない間は、残っている日時の値を捨てる（古い値を送らないため）。
  $effect(() => {
    if (!dateVisible) {
      dateFrom = null;
      dateTo = null;
    }
  });

  // サーバー検索は問い合わせ先がアカウントのサーバーなので、アカウントを変えたら結果を作り直す。
  // キャッシュ検索の結果はアカウントに依存しないため従来どおり保持する。
  let prevAccountId = accountId;
  $effect(() => {
    const id = accountId;
    if (scope === "server" && id !== prevAccountId) resetResults();
    prevAccountId = id;
  });
```

**3-3. `SearchModal.svelte` のテンプレート**

(a) アカウント注記の `<span>` を置き換え:

```svelte
        <span class="text-muted-foreground">アカウント（検索結果の操作に使用。検索条件には影響しません）</span>
```
↓
```svelte
        <span class="text-muted-foreground"
          >{scope === "server"
            ? "アカウント（このアカウントのサーバーに検索を問い合わせます）"
            : "アカウント（検索結果の操作に使用。検索条件には影響しません）"}</span
        >
```

(b) `<AccountSelect ... />` を含む `<div class="flex flex-col gap-1 text-sm">…</div>` の直後（モード切替の `<div class="flex items-center gap-0 ...">` の前）に追加し、**続くモード切替の `<div>`（簡単/エキスパートのボタン2つを含むもの）全体を `{#if scope === "cache"}` … `{/if}` で囲む**:

```svelte
      <div
        class="flex items-center gap-0 self-start overflow-hidden rounded-lg border border-border text-sm"
        role="group"
        aria-label="検索対象"
      >
        <button
          type="button"
          class={scope === "cache"
            ? "border-r border-border bg-primary px-3.5 py-1.5 text-primary-foreground"
            : "border-r border-border bg-muted px-3.5 py-1.5 text-foreground"}
          onclick={() => setScope("cache")}
        >キャッシュ</button>
        <button
          type="button"
          class={scope === "server"
            ? "bg-primary px-3.5 py-1.5 text-primary-foreground"
            : "bg-muted px-3.5 py-1.5 text-foreground"}
          onclick={() => setScope("server")}
        >サーバー</button>
      </div>

      {#if scope === "cache"}
        <!-- 既存の 簡単 / エキスパート(TQL) 切替 div（内容は変更しない） -->
      {/if}
```

(c) `{#if uiMode === "guided"}` を `{#if showGuided}` に置き換える。

(d) キーワードのラベル `<span class="text-muted-foreground">キーワード</span>` を置き換え:

```svelte
          <span class="text-muted-foreground">{scope === "server" ? "キーワード（必須）" : "キーワード"}</span>
```

(e) 「詳細条件」トグルの文言を置き換え:

```svelte
        >{showAdvanced ? "詳細条件を隠す" : "詳細条件を指定（ユーザー・インスタンス・日時）"}</button>
```
↓
```svelte
        >{showAdvanced
          ? "詳細条件を隠す"
          : `詳細条件を指定（ユーザー・インスタンス${dateVisible ? "・日時" : ""}）`}</button>
```

(f) インスタンス欄の `placeholder="misskey.example（空欄で全インスタンス対象）"` を置き換え:

```svelte
              placeholder={scope === "server"
                ? "misskey.example（空欄で全インスタンス。自インスタンスは . かホスト名）"
                : "misskey.example（空欄で全インスタンス対象）"}
```

(g) 日時の2欄を含む `<div class="flex gap-2.5">…</div>` を `{#if dateVisible}` … `{/if}` で囲み、続けて非対応の注記を足す。`<div class="flex gap-2.5">` の直前に `{#if dateVisible}` を、対応する `</div>` と `{/if}`（`showAdvanced` を閉じるもの）の間に次を挿入する（内側の再インデントはしない）:

```svelte
          {/if}
          {#if scope === "server" && caps && !caps.dateRange}
            <p class="mb-0 mt-0 text-xs text-muted-foreground">
              日時範囲の指定は Misskey 2025.7.0 以降のサーバーで利用できます
            </p>
          {/if}
```

結果の構造:

```svelte
        {#if showAdvanced}
          <!-- ユーザー / インスタンス の label 2つ -->
          {#if dateVisible}
          <div class="flex gap-2.5"> … 日時（開始）/（終了）… </div>
          {/if}
          {#if scope === "server" && caps && !caps.dateRange}
            <p …>日時範囲の指定は Misskey 2025.7.0 以降のサーバーで利用できます</p>
          {/if}
        {/if}
```

(h) 検索ボタンの disabled を置き換え:

```svelte
      <Button type="submit" disabled={busy || (uiMode === "expert" && !!tqlErr)} data-testid="search-submit"
```
↓
```svelte
      <Button type="submit" disabled={busy || !canSearch()} data-testid="search-submit"
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd frontend && pnpm vitest run src/ui/SearchModal.test.ts`
Expected: 既存 + 新規7件すべて PASS。

もし flatpickr の `setDate` が jsdom で `onChange` を発火せず日時テストだけ落ちる場合は、`setDate` の第2引数・`_flatpickr` の有無をログで確認し、原因を直してから進める（テストを弱めて通さない）。

Run: `cd frontend && pnpm check && pnpm test`
Expected: 型チェック・全 Vitest が PASS。

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/ui/SearchModal.svelte frontend/src/ui/SearchModal.test.ts
git commit -m "feat: 検索モーダルにサーバー検索を追加する"
```

---

### Task 6: ユーザーガイド更新と最終検証

**Files:**
- Modify: `docs/guide/user-guide.md`（「検索」節）
- Verify only: Rust / フロント全テスト、実機確認

**Interfaces:**
- Consumes: Task 1〜5 の成果物
- Produces: なし

- [ ] **Step 1: ユーザーガイドを更新する**

`docs/guide/user-guide.md` の「検索」節で、冒頭の説明文（「画面下部のメニュー…ローカルにキャッシュ済みのノートを検索するモーダルが開きます。カラムを追加せず、その場限りで検索したいときに使います。」）を次に置き換える:

```markdown
画面下部のメニュー（ハンバーガーアイコン。「＋カラム」「設定」と並びます）から「検索」を選ぶと、検索モーダルが開きます。カラムを追加せず、その場限りで検索したいときに使います。「検索対象」で次の2つを切り替えられます。

- **キャッシュ**: ローカルにキャッシュ済みのノートを検索します。
- **サーバー**: 選択中のアカウントのMisskeyサーバーの検索機能（`notes/search`）に問い合わせます。キャッシュに無いノートも探せます。
```

同節の箇条書きを次のとおり更新する（既存の「アカウント」「簡単モード」「エキスパート」「検索対象はローカルキャッシュのみ」の4項目を置き換え、最後の「結果はスクロールで…」は残す）:

```markdown
- **アカウント**: キャッシュ検索では、検索結果のノートに対して返信・Renote・リアクション等の操作を行うアカウントを選びます（検索条件そのものには影響しません。キャッシュは全アカウント共通です）。サーバー検索では、そのアカウントのサーバーに検索を問い合わせます。
- **簡単モード**: 既定ではキーワード欄のみ表示されます。「詳細条件を指定」を押すと、ユーザー（`@user@host`形式。自インスタンスのユーザーは`@user`のみ）・インスタンス（空欄で全インスタンスが対象）・日時範囲（開始/終了）の各条件を追加できます。
- **エキスパート(TQL)モード**（キャッシュ検索のみ）: `cache`ソースのwhere句を直接書けます（例: `has_files && user.acct == "@alice@misskey.example"`）。
- **サーバー検索の条件**: キーワードは必須です。インスタンス欄に自分のサーバーを指定したいときは、ホスト名か `.` を入力します。サーバー検索ではエキスパート(TQL)モードは使えません。
- **日時範囲（サーバー検索）**: Misskey 2025.7.0以降のサーバーでのみ指定できます。バージョンを取得できないサーバーや古いサーバーでは日時欄は表示されません。Misskeyのフォーク（Sharkey等）はバージョン番号が本家と対応しない場合があり、番号が新しくても日時の指定が効かないことがあります。
- キャッシュ検索の対象は**この端末にローカルキャッシュ済みのノートのみ**です。まだ一度も受信・取得していないノートはヒットしません。サーバー検索はサーバー側の検索機能に依存するため、サーバーが検索を無効にしている場合などは結果が返りません。
```

Run: `git diff docs/guide/user-guide.md` で意図した箇所だけが変わっていることを確認する。

- [ ] **Step 2: 全テストを通す**

Run: `cd src-tauri && cargo test`
Expected: PASS（`#[ignore]` 除く）

Run: `cd frontend && pnpm check && pnpm test`
Expected: PASS

- [ ] **Step 3: 実機確認（実UI・実サーバー）**

ユーザーの `cargo tauri dev` が動いていれば、ホットリビルドで今回の Rust/フロント変更が反映される（DBマイグレーションは含まないので既存データに影響しない）。実在アカウント（例: `misskey.omhnc.net`）で次を確認し、結果を記録する:

1. 検索モーダルで「サーバー」を選ぶと TQL タブが消え、キーワード空では検索ボタンが無効。
2. キーワードで検索すると、キャッシュに無いノートも出る。スクロールで追加読み込みされる。
3. 「詳細条件」にユーザー `@user@host`・インスタンス・日時が出る（`misskey.omhnc.net` は 2026.x なので日時欄が出る）。日時範囲を指定すると範囲内のノートだけが返る。
4. インスタンス欄に自ホスト名を入れるとローカルのノートだけになる。
5. 「キャッシュ」に戻すと結果がクリアされ、従来どおりキャッシュ検索できる。

検証用に自分で起動した `cargo tauri dev` / Xvfb 等は、完了前に正確な PID で止める（`pkill` 禁止）。ユーザーの実画面を使わない確認が必要なときは Xvfb + `dbus-run-session` + `WAYLAND_DISPLAY` unset の隔離環境で行う。実接続の確認が取れない項目は、行っていない旨と理由を PR に書く。

- [ ] **Step 4: コミット**

```bash
git add docs/guide/user-guide.md
git commit -m "docs: ユーザーガイドにサーバー検索を追記する"
```

- [ ] **Step 5: PR を作る（ユーザーの指示があってから）**

`git push -u origin feat/server-side-search` のあと、`.github/pull_request_template.md` の構造に沿った本文で `gh pr create`（`--body` を使うのでテンプレートを手で再現する）。本文に `Fixes #430`、影響範囲の「TSバインディング生成に影響あり」にチェック、検証欄に上記コマンドと実機確認の結果を書く。末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)`。push 後に CI を Monitor で待たない。マージは `gh pr merge --merge`（squash 不可）。
