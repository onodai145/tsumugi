# ノート翻訳 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** NoteMenu の「翻訳」から、接続先 Misskey サーバーの `notes/translate` でノート本文を翻訳し、NoteCard の本文直下に展開表示する（Issue #440）。

**Architecture:** Rust は `api/notes.rs` に `translate_note`、`api/meta.rs` に `fetch_translator_available` を足し、`commands/note.rs` の 2 コマンド（`get_translator_available` / `translate_note`）で公開する。翻訳先言語は `UiPrefs.translate_target_lang`（既定 `ja`）を Rust 側で読む。フロントは store に 2 メソッドを足し、NoteMenu が項目を出し、NoteCard が翻訳ブロックの状態（読み込み中/成功/結果なし/エラー）を持つ。翻訳結果は保持しない（キャッシュなし）。

**Tech Stack:** Rust (tauri v2, tauri-specta, reqwest, wiremock), Svelte 5 (runes), Vitest + @testing-library/svelte, Tailwind。

**Spec:** `docs/superpowers/specs/2026-10-02-note-translate-design.md`

## Global Constraints

- Issue #440 は feature ブランチ `feat/note-translate` 上で作業する（`main` へ直接コミットしない）。
- コミットメッセージは件名のみ（本文・箇条書きなし）。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` を付ける。`git commit` が失敗/タイムアウトしたら止めて報告し、再試行しない。`--no-verify` は使わない。
- `frontend/src/bindings/tauri.gen.ts` は生成物。手で編集せず `cd src-tauri && cargo test generates_frontend_bindings` で再生成する。
- 新しいコマンドは `lib.rs` の `specta_builder()` に登録する。
- 翻訳先言語の既定値は `"ja"`。保存済み JSON に項目がなくても読めること（`#[serde(default = ...)]`）。
- 翻訳結果はキャッシュしない（メモリ・永続とも）。MFM としては描画せずプレーンテキストで描画する。
- `Error` enum に variant を足さない。`UNAVAILABLE` はフロントが `Error::Api` の message に含まれる `UNAVAILABLE` で判定する。
- 視覚値は `docs/design/style-guide.md` のスケールに従い、`rounded-[Npx]` のような一回限りの値を使わない。
- 実 UI の確認は必ず Xvfb 越しに行い、`WAYLAND_DISPLAY` も unset する。`cargo tauri dev` は**リポジトリルート**から実行し、起動したものは完了前に自分で（正確な PID で）kill する。`pkill`/`killall` は使わない。
- 説明・コメントは日本語（既存コードの流儀に合わせる）。

## Review Focus

1. 保存済みの翻訳先言語が空文字/空白のみ → `ja` にフォールバックする（Task 3 の `effective_translate_lang` テスト）。
2. サーバーが結果なし（204）を返す → エラーではなく「翻訳結果がありません」と表示する（Task 2 の `translate_note_returns_none_on_204`、Task 5 の結果なしテスト）。
3. 翻訳の読み込み中に別のノートへ入れ替わった（仮想リストで NoteCard が再利用される）→ 古い結果が新しいノートの下に出ない（Task 5 の stale 結果テスト）。
4. 純リノートの「翻訳」→ リノート自身ではなくリノート先ノートの id を翻訳する（Task 5 のテスト）。
5. 翻訳文に `<b>` や MFM 記法が含まれる → 解釈せずそのまま文字として表示する（Task 5 のテスト）。

---

### Task 1: `UiPrefs.translate_target_lang` を追加する

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`（`UiPrefs` 構造体 :87 付近、`default_*` 群 :279 付近、`Default` 実装 :307-345、`mod tests` :349〜）

**Interfaces:**
- Produces: `UiPrefs.translate_target_lang: String`（camelCase で `translateTargetLang`、既定 `"ja"`）、`fn default_translate_target_lang() -> String`。

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` の末尾（`developer_options_enabled_defaults_to_false_for_legacy_json` の近く）に追加:

```rust
    #[test]
    fn translate_target_lang_defaults_to_ja_for_legacy_json() {
        // translate_target_lang 追加前に保存されたJSONも読めること（#[serde(default)]）。
        let v: UiPrefs =
            serde_json::from_str(r#"{"theme":"dark","defaultColumnWidth":320}"#).unwrap();
        assert_eq!(v.translate_target_lang, "ja");
        assert_eq!(UiPrefs::default().translate_target_lang, "ja");
    }

    #[test]
    fn translate_target_lang_round_trips_camel_case() {
        let v: UiPrefs = serde_json::from_str(
            r#"{"theme":"dark","defaultColumnWidth":320,"translateTargetLang":"en"}"#,
        )
        .unwrap();
        assert_eq!(v.translate_target_lang, "en");
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["translateTargetLang"], "en");
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib translate_target_lang`
Expected: コンパイルエラー（`no field translate_target_lang on type UiPrefs`）。

- [ ] **Step 3: 実装する**

`UiPrefs` の `ui_scale` の直後に追加:

```rust
    /// ノート翻訳（Issue #440）の翻訳先言語コード。サーバーの `notes/translate` の `targetLang` へ
    /// そのまま渡す。空文字や空白のみの値は翻訳時に `ja` として扱う（`commands::note::effective_translate_lang`）。
    #[serde(default = "default_translate_target_lang")]
    pub translate_target_lang: String,
```

`default_search_engine_url` の近くに追加:

```rust
fn default_translate_target_lang() -> String {
    "ja".into()
}
```

`impl Default for UiPrefs` の `ui_scale: default_ui_scale(),` の直後に追加:

```rust
            translate_target_lang: default_translate_target_lang(),
```

既存テストの網羅リテラル（`let p = UiPrefs { ... ui_scale: 130, };` :441 付近）の `ui_scale: 130,` の直後に追加:

```rust
            translate_target_lang: "en".into(),
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd src-tauri && cargo test --lib domain::ui`
Expected: PASS（既存の round-trip テストも通る）。

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/src/domain/ui.rs
git commit -m "feat: UiPrefsに翻訳先言語を追加する"
```

（件名のみ。末尾に Co-Authored-By 行を付ける。以降のコミットも同様。）

---

### Task 2: `translate_note` / `fetch_translator_available` / AppState キャッシュ

**Files:**
- Modify: `src-tauri/src/domain/note.rs`（`Translation` 型を追加）、`src-tauri/src/domain/mod.rs`（`pub use note::{..., Translation}`）
- Modify: `src-tauri/src/api/notes.rs`（`translate_note` と、`mod tests` 内のテスト）
- Modify: `src-tauri/src/api/meta.rs`（`fetch_translator_available` と `meta_info_tests` 内のテスト）
- Modify: `src-tauri/src/state.rs`（`translator_available` キャッシュ、`new_with_sound` の構造体リテラル、`mod tests`）
- Modify: `src-tauri/src/commands/account.rs:94`（`remove_account` で破棄）

**Interfaces:**
- Produces:
  - `crate::domain::Translation { pub source_lang: String, pub text: String }`（`Debug, Clone, Serialize, Deserialize, Type, PartialEq`、camelCase）
  - `pub async fn translate_note(client: &MisskeyClient, note_id: &str, target_lang: &str) -> Result<Option<Translation>>`（`crate::api::notes`）
  - `pub async fn fetch_translator_available(client: &MisskeyClient) -> Result<bool>`（`crate::api::meta`）
  - `AppState::translator_available(&self, account_id: &str) -> Option<bool>` / `set_translator_available(&self, account_id: &str, v: bool)` / `forget_translator_available(&self, account_id: &str)`

- [ ] **Step 1: `Translation` 型を追加する**

`src-tauri/src/domain/note.rs` の末尾（`#[cfg(test)]` があればその前）に追加:

```rust
/// `notes/translate` の翻訳結果（Issue #440）。`source_lang` は検出された翻訳元言語コード。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    pub source_lang: String,
    pub text: String,
}
```

`src-tauri/src/domain/mod.rs` の `pub use note::{DriveFile, Note, Poll, PollChoice, Visibility};` を次に変更:

```rust
pub use note::{DriveFile, Note, Poll, PollChoice, Translation, Visibility};
```

- [ ] **Step 2: `translate_note` の失敗するテストを書く**

`src-tauri/src/api/notes.rs` の `mod tests` 内（末尾）に追加:

```rust
    fn mock_client(uri: String) -> MisskeyClient {
        MisskeyClient::new_with_api_base(reqwest::Client::new(), uri, None)
    }

    #[tokio::test]
    async fn translate_note_parses_200() {
        use wiremock::matchers::{body_partial_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/translate"))
            .and(body_partial_json(json!({ "noteId": "n1", "targetLang": "ja" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "sourceLang": "EN",
                "text": "こんにちは"
            })))
            .mount(&mock)
            .await;

        let t = translate_note(&mock_client(mock.uri()), "n1", "ja").await.unwrap();
        assert_eq!(
            t,
            Some(crate::domain::Translation {
                source_lang: "EN".into(),
                text: "こんにちは".into()
            })
        );
    }

    #[tokio::test]
    async fn translate_note_returns_none_on_204() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/translate"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock)
            .await;

        let t = translate_note(&mock_client(mock.uri()), "n1", "ja").await.unwrap();
        assert_eq!(t, None);
    }

    #[tokio::test]
    async fn translate_note_unavailable_is_api_error_with_code() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/translate"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": { "code": "UNAVAILABLE", "message": "Translate of notes unavailable." }
            })))
            .mount(&mock)
            .await;

        let err = translate_note(&mock_client(mock.uri()), "n1", "ja").await.unwrap_err();
        match err {
            crate::error::Error::Api(detail) => assert!(detail.contains("UNAVAILABLE"), "{detail}"),
            other => panic!("expected Api, got {other:?}"),
        }
    }
```

- [ ] **Step 3: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib translate_note`
Expected: コンパイルエラー（`cannot find function translate_note`）。

- [ ] **Step 4: `translate_note` を実装する**

`src-tauri/src/api/notes.rs` の `use crate::domain::{Note, ReactionUser, User, Visibility};` に `Translation` を足し、`search_notes` の直後に追加:

```rust
/// ノート翻訳（`notes/translate`、Issue #440）。結果なし（204、本文が空）は `Ok(None)`。
/// `client.post` は空ボディを `null` として読むので、戻り値を `Option` にすれば 204 を受けられる。
/// サーバーで翻訳が未設定だと 400 `UNAVAILABLE` で、`Error::Api` の detail にそのコードが入る。
pub async fn translate_note(
    client: &MisskeyClient,
    note_id: &str,
    target_lang: &str,
) -> Result<Option<Translation>> {
    client
        .post(
            "notes/translate",
            &json!({ "noteId": note_id, "targetLang": target_lang }),
        )
        .await
}
```

- [ ] **Step 5: `translate_note` のテストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib translate_note`
Expected: 3 件 PASS。

- [ ] **Step 6: `fetch_translator_available` の失敗するテストを書く**

`src-tauri/src/api/meta.rs` の `mod meta_info_tests` 内（`raw_version_defaults_missing_version_to_none` の後）に追加:

```rust
    #[test]
    fn raw_translator_reads_flag_and_defaults_to_false() {
        let t: RawTranslator = serde_json::from_str(r#"{"translatorAvailable":true}"#).unwrap();
        assert!(t.translator_available);
        let f: RawTranslator = serde_json::from_str(r#"{"translatorAvailable":false}"#).unwrap();
        assert!(!f.translator_available);
        // 古いサーバー等でフィールドが無い場合は非対応扱い。
        let m: RawTranslator = serde_json::from_str(r#"{"version":"2024.1.0"}"#).unwrap();
        assert!(!m.translator_available);
    }

    #[tokio::test]
    async fn fetch_translator_available_reads_meta_lite() {
        use wiremock::matchers::{body_partial_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/meta"))
            .and(body_partial_json(json!({ "detail": false })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "version": "2026.9.1",
                "translatorAvailable": true
            })))
            .mount(&mock)
            .await;

        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        assert!(fetch_translator_available(&client).await.unwrap());
    }
```

- [ ] **Step 7: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib translator`
Expected: コンパイルエラー（`RawTranslator` / `fetch_translator_available` が無い）。

- [ ] **Step 8: `fetch_translator_available` を実装する**

`src-tauri/src/api/meta.rs` の `fetch_server_version` の直後に追加:

```rust
/// `/api/meta` の `translatorAvailable` だけを読む。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTranslator {
    #[serde(default)]
    translator_available: bool,
}

/// 接続先サーバーでノート翻訳（DeepL / LibreTranslate）が使えるか（Issue #440）。
/// `translatorAvailable` は `MetaLite` に含まれるので `detail: false` で取れる。
/// `fetch_server_version` と同様、`InstanceInfo` には足さず独立に取得する。
pub async fn fetch_translator_available(client: &MisskeyClient) -> Result<bool> {
    let raw: RawTranslator = client.post("meta", &json!({ "detail": false })).await?;
    Ok(raw.translator_available)
}
```

- [ ] **Step 9: 通ることを確認する**

Run: `cd src-tauri && cargo test --lib translator`
Expected: PASS。

- [ ] **Step 10: AppState キャッシュの失敗するテストを書く**

`src-tauri/src/state.rs` の `mod tests` 内、`server_version_is_remembered_per_account_and_forgettable` の直後に追加:

```rust
    #[test]
    fn translator_available_is_remembered_per_account_and_forgettable() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        assert_eq!(state.translator_available("a1"), None);

        state.set_translator_available("a1", true);
        state.set_translator_available("a2", false);
        assert_eq!(state.translator_available("a1"), Some(true));
        assert_eq!(state.translator_available("a2"), Some(false));

        state.forget_translator_available("a1");
        assert_eq!(state.translator_available("a1"), None);
        assert_eq!(state.translator_available("a2"), Some(false));
    }
```

- [ ] **Step 11: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib translator_available_is_remembered`
Expected: コンパイルエラー。

- [ ] **Step 12: AppState を実装する**

`src-tauri/src/state.rs`:

`server_versions` フィールド（:108）の直後に追加:

```rust
    /// account_id -> 接続先サーバーでノート翻訳が使えるか(`/api/meta` の `translatorAvailable`、Issue #440)。
    /// 取得に成功した値だけ保存し、アプリ再起動まで再取得しない。
    pub translator_available: Mutex<HashMap<String, bool>>,
```

`new_with_sound` の構造体リテラルの `server_versions: Mutex::new(HashMap::new()),`（:170）の直後に追加:

```rust
            translator_available: Mutex::new(HashMap::new()),
```

`forget_server_version` の直後にメソッドを追加:

```rust
    /// account の接続先サーバーでノート翻訳が使えるか（取得済みの場合のみ）。
    pub fn translator_available(&self, account_id: &str) -> Option<bool> {
        self.translator_available.lock().unwrap().get(account_id).copied()
    }

    /// account の翻訳可否を保存する。
    pub fn set_translator_available(&self, account_id: &str, available: bool) {
        self.translator_available
            .lock()
            .unwrap()
            .insert(account_id.to_string(), available);
    }

    /// account の翻訳可否キャッシュを破棄する（アカウント削除時）。
    pub fn forget_translator_available(&self, account_id: &str) {
        self.translator_available.lock().unwrap().remove(account_id);
    }
```

`src-tauri/src/commands/account.rs` の `state.forget_server_version(&account_id);`（:94）の直後に追加:

```rust
    state.forget_translator_available(&account_id);
```

- [ ] **Step 13: 全体が通ることを確認する**

Run: `cd src-tauri && cargo test --lib`
Expected: PASS（`#[ignore]` のテストは実行されない）。

- [ ] **Step 14: コミットする**

```bash
git add src-tauri/src/domain/note.rs src-tauri/src/domain/mod.rs src-tauri/src/api/notes.rs src-tauri/src/api/meta.rs src-tauri/src/state.rs src-tauri/src/commands/account.rs
git commit -m "feat: ノート翻訳のREST取得と翻訳可否のキャッシュを追加する"
```

---

### Task 3: Tauri コマンドの公開とバインディング再生成

**Files:**
- Modify: `src-tauri/src/commands/note.rs`（import、コマンド 2 つ、ヘルパー、`mod tests`）
- Modify: `src-tauri/src/lib.rs:84-85`（`specta_builder()` へ登録）
- Modify（生成）: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: Task 1 `UiPrefs.translate_target_lang`、Task 2 `translate_note` / `fetch_translator_available` / `AppState::{translator_available,set_translator_available}` / `Translation`。
- Produces（TS 側 `commands`）: `getTranslatorAvailable(accountId: string): Promise<Result<boolean, Error>>`、`translateNote(accountId: string, noteId: string): Promise<Result<Translation | null, Error>>`、型 `Translation { sourceLang: string; text: string }`、`UiPrefs.translateTargetLang: string`。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/note.rs` の `mod tests` 内（`use super::*;` の下、既存テストの前後どこでもよい）に追加:

```rust
    #[test]
    fn effective_translate_lang_falls_back_to_ja_when_blank() {
        let mut prefs = crate::domain::UiPrefs::default();
        assert_eq!(effective_translate_lang(&prefs), "ja");

        prefs.translate_target_lang = "".into();
        assert_eq!(effective_translate_lang(&prefs), "ja");

        prefs.translate_target_lang = "   ".into();
        assert_eq!(effective_translate_lang(&prefs), "ja");

        prefs.translate_target_lang = " en ".into();
        assert_eq!(effective_translate_lang(&prefs), "en");
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib effective_translate_lang`
Expected: コンパイルエラー（`cannot find function effective_translate_lang`）。

- [ ] **Step 3: 実装する**

`src-tauri/src/commands/note.rs` の import を更新:

```rust
use crate::api::meta::{fetch_translator_available, list_emojis};
use crate::api::notes::{
    create_favorite, create_note, create_reaction, delete_favorite, delete_note, delete_reaction,
    get_reactions, get_renotes, renote as api_renote, translate_note as api_translate_note,
    vote_poll as api_vote_poll, NoteDraft, VisibilityInput,
};
```

（既存の `use crate::api::meta::list_emojis;` を置き換える。）`use crate::domain::{...}` に `Translation` と `UiPrefs` を追加:

```rust
use crate::domain::{
    DriveFile, EmojiDef, Note, ReactionUser, SourceItem, Translation, UiPrefs, UrlPreview, User,
};
```

`unfavorite_note` の直後に追加:

```rust
/// 翻訳先言語。未設定・空白のみのときは既定の `ja`（Issue #440）。
pub(crate) fn effective_translate_lang(prefs: &UiPrefs) -> String {
    let lang = prefs.translate_target_lang.trim();
    if lang.is_empty() {
        "ja".to_string()
    } else {
        lang.to_string()
    }
}

/// アカウントの接続先サーバーでノート翻訳が使えるか。値は `AppState` にキャッシュし、未取得なら
/// `/api/meta` から取得する。取得失敗は「使えない」扱いで、失敗はキャッシュしない（次回また取りに行く）。
/// 未登録アカウントだけはエラーを返す。
async fn translator_available_for(state: &AppState, account_id: &str) -> Result<bool> {
    if let Some(v) = state.translator_available(account_id) {
        return Ok(v);
    }
    let client = state.client_for(account_id)?;
    match fetch_translator_available(&client).await {
        Ok(v) => {
            state.set_translator_available(account_id, v);
            Ok(v)
        }
        Err(_) => Ok(false),
    }
}

/// NoteMenu の「翻訳」項目を出すか（Issue #440）。接続先サーバーで翻訳が使えるときだけ true。
#[tauri::command]
#[specta::specta]
pub async fn get_translator_available(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<bool> {
    translator_available_for(&state, &account_id).await
}

/// ノート本文を `UiPrefs.translate_target_lang` へ翻訳する（Issue #440）。結果なしは `None`。
/// サーバーで翻訳が未設定なら `Error::Api`（message に `UNAVAILABLE` を含む）。
#[tauri::command]
#[specta::specta]
pub async fn translate_note(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
) -> Result<Option<Translation>> {
    let lang = effective_translate_lang(&state.settings.load_ui()?);
    let client = state.client_for(&account_id)?;
    api_translate_note(&client, &note_id, &lang).await
}
```

`src-tauri/src/lib.rs` の `commands::note::unfavorite_note,`（:85）の直後に追加:

```rust
            commands::note::get_translator_available,
            commands::note::translate_note,
```

- [ ] **Step 4: テストとバインディング生成を確認する**

Run: `cd src-tauri && cargo test --lib effective_translate_lang && cargo test generates_frontend_bindings`
Expected: PASS。`git diff frontend/src/bindings/tauri.gen.ts` に `getTranslatorAvailable` / `translateNote` / `Translation` / `translateTargetLang` が増えている。

- [ ] **Step 5: 差分を確認し、コマンド全体が通ることを確認する**

Run: `git diff --stat frontend/src/bindings/tauri.gen.ts && cd src-tauri && cargo test`
Expected: bindings の差分は上記 4 つに関するものだけ。`cargo test` が全件 PASS。

- [ ] **Step 6: コミットする**

```bash
git add src-tauri/src/commands/note.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: ノート翻訳のTauriコマンドを追加する"
```

---

### Task 4: store メソッドと設定画面の翻訳先言語

**Files:**
- Create: `frontend/src/lib/translateLang.ts`、`frontend/src/lib/translateLang.test.ts`
- Modify: `frontend/src/lib/store.svelte.ts`（`getSearchCapabilities` :1421 の直後に 2 メソッド）
- Modify: `frontend/src/ui/settings/ExternalIntegrationSection.svelte`
- Create: `frontend/src/ui/settings/ExternalIntegrationSection.test.ts`

**Interfaces:**
- Consumes: Task 3 の `commands.getTranslatorAvailable` / `commands.translateNote`、`UiPrefs.translateTargetLang`。
- Produces:
  - `DEFAULT_TRANSLATE_LANG = "ja"`、`TRANSLATE_LANG_PRESETS: { code: string; label: string }[]`、`normalizeTranslateLang(v: string | null | undefined): string`（`lib/translateLang.ts`）
  - `app.getTranslatorAvailable(accountId: string): Promise<boolean>`（失敗は `false`、モーダルを出さない）
  - `app.translateNote(accountId: string, noteId: string): Promise<Translation | null>`（失敗は throw。モーダルは出さず呼び出し側がインライン表示する）

- [ ] **Step 1: `translateLang` の失敗するテストを書く**

`frontend/src/lib/translateLang.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { DEFAULT_TRANSLATE_LANG, TRANSLATE_LANG_PRESETS, normalizeTranslateLang } from "./translateLang";

describe("translateLang", () => {
  it("既定は ja で、プリセットの先頭にある", () => {
    expect(DEFAULT_TRANSLATE_LANG).toBe("ja");
    expect(TRANSLATE_LANG_PRESETS[0].code).toBe("ja");
  });

  it("プリセットのコードは重複しない", () => {
    const codes = TRANSLATE_LANG_PRESETS.map((p) => p.code);
    expect(new Set(codes).size).toBe(codes.length);
  });

  it("空・空白・未設定は既定へ戻し、それ以外は trim して返す", () => {
    expect(normalizeTranslateLang("")).toBe("ja");
    expect(normalizeTranslateLang("   ")).toBe("ja");
    expect(normalizeTranslateLang(undefined)).toBe("ja");
    expect(normalizeTranslateLang(null)).toBe("ja");
    expect(normalizeTranslateLang(" en ")).toBe("en");
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && pnpm vitest run src/lib/translateLang.test.ts`
Expected: FAIL（モジュールが無い）。

- [ ] **Step 3: `translateLang.ts` を実装する**

`frontend/src/lib/translateLang.ts`:

```ts
// ノート翻訳（Issue #440）の翻訳先言語。コードはサーバーの notes/translate の targetLang へ
// そのまま渡す（変換しない）。地域付きコード(EN-US 等)が必要な言語は、実インスタンスで
// 通ることを確認してからここへ足す。
export const DEFAULT_TRANSLATE_LANG = "ja";

export const TRANSLATE_LANG_PRESETS: { code: string; label: string }[] = [
  { code: "ja", label: "日本語" },
  { code: "en", label: "English" },
  { code: "zh", label: "中文" },
  { code: "ko", label: "한국어" },
  { code: "fr", label: "Français" },
  { code: "de", label: "Deutsch" },
  { code: "es", label: "Español" },
  { code: "pt", label: "Português" },
  { code: "ru", label: "Русский" },
];

/// 保存値を整える。空・空白のみ・未設定は既定（ja）へ戻す。
export function normalizeTranslateLang(v: string | null | undefined): string {
  const t = (v ?? "").trim();
  return t === "" ? DEFAULT_TRANSLATE_LANG : t;
}
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd frontend && pnpm vitest run src/lib/translateLang.test.ts`
Expected: PASS。

- [ ] **Step 5: store メソッドを実装する**

`frontend/src/lib/store.svelte.ts` の `getSearchCapabilities`（:1421）の直後に追加:

```ts
  /// NoteMenu の「翻訳」項目を出すか(Issue #440)。取得に失敗しても翻訳が使えない扱い(false)にし、
  /// メニューを開くたびに誤ってエラーモーダルを出さないよう #logFailure も呼ばない。
  async getTranslatorAvailable(accountId: string): Promise<boolean> {
    try {
      return await unwrapAcc(accountId, commands.getTranslatorAvailable(accountId));
    } catch {
      return false;
    }
  }

  /// ノート本文を翻訳する(Issue #440)。結果なしは null。失敗は throw するだけで、
  /// エラーモーダルは出さない(NoteCard の翻訳ブロックがインラインで表示するため)。
  async translateNote(accountId: string, noteId: string) {
    return await unwrapAcc(accountId, commands.translateNote(accountId, noteId));
  }
```

- [ ] **Step 6: 設定画面の失敗するテストを書く**

`frontend/src/ui/settings/ExternalIntegrationSection.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
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

const { default: ExternalIntegrationSection } = await import("./ExternalIntegrationSection.svelte");

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ExternalIntegrationSection 翻訳先言語", () => {
  it("既定は ja が選ばれている", () => {
    app.ui = { ...app.ui, translateTargetLang: "ja" };
    const { getByLabelText } = render(ExternalIntegrationSection);
    expect((getByLabelText("翻訳先言語") as HTMLSelectElement).value).toBe("ja");
  });

  it("変更して保存すると translateTargetLang だけが変わり、他の項目は保持される", async () => {
    app.ui = { ...app.ui, translateTargetLang: "ja", searchEngineUrl: "https://example.com/?q={query}" };
    const spy = vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const { getByLabelText, getByText } = render(ExternalIntegrationSection);

    const select = getByLabelText("翻訳先言語") as HTMLSelectElement;
    select.value = "en";
    select.dispatchEvent(new Event("change", { bubbles: true }));
    await getByText("保存").click();

    expect(spy).toHaveBeenCalledTimes(1);
    const saved = spy.mock.calls[0][0];
    expect(saved.translateTargetLang).toBe("en");
    expect(saved.searchEngineUrl).toBe("https://example.com/?q={query}");
  });

  it("プリセットに無い保存値(手編集した設定ファイル等)も選択肢に出して保持する", () => {
    app.ui = { ...app.ui, translateTargetLang: "pt-BR" };
    const { getByLabelText } = render(ExternalIntegrationSection);
    const select = getByLabelText("翻訳先言語") as HTMLSelectElement;
    expect(select.value).toBe("pt-BR");
  });
});
```

- [ ] **Step 7: 失敗を確認する**

Run: `cd frontend && pnpm vitest run src/ui/settings/ExternalIntegrationSection.test.ts`
Expected: FAIL（`翻訳先言語` のラベルが見つからない）。

- [ ] **Step 8: 設定画面を実装する**

`frontend/src/ui/settings/ExternalIntegrationSection.svelte`:

import を追加（`searchEngine` の import の下）:

```ts
  import { TRANSLATE_LANG_PRESETS, normalizeTranslateLang } from "../../lib/translateLang";
```

状態を追加（`summalyProxyUrl` の下）:

```ts
  let translateTargetLang = $state(normalizeTranslateLang(app.ui.translateTargetLang));
  // プリセットに無い保存値は、保存で上書きして消さないよう選択肢の末尾に足す。
  const translateLangOptions = $derived(
    TRANSLATE_LANG_PRESETS.some((p) => p.code === translateTargetLang)
      ? TRANSLATE_LANG_PRESETS
      : [...TRANSLATE_LANG_PRESETS, { code: translateTargetLang, label: translateTargetLang }],
  );
```

`save()` の `app.setUiPrefs({ ... })` に追加（`summalyProxyUrl: summalyProxyUrl.trim(),` の下）:

```ts
        translateTargetLang: normalizeTranslateLang(translateTargetLang),
```

テンプレートの、リンクプレビューのブロック（`</div>` で閉じる 2 つ目の `<div class="mb-3 flex ...">`）の直後、保存ボタンの `<div class="flex items-center justify-end gap-3">` の直前に追加:

```svelte
<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <label class="text-muted-foreground" for="translate-target-lang">翻訳先言語</label>
  <select
    id="translate-target-lang"
    class="w-fit rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground"
    bind:value={translateTargetLang}
  >
    {#each translateLangOptions as o (o.code)}
      <option value={o.code}>{o.label}</option>
    {/each}
  </select>
  <p class="mb-0 mt-0 text-xs text-muted-foreground">
    ノートのメニューの「翻訳」で使う言語です。翻訳は接続先サーバーの翻訳機能（DeepL / LibreTranslate）で行われるため、
    サーバー側で翻訳が設定されている場合のみ使えます。
  </p>
</div>
```

- [ ] **Step 9: 通ることを確認する**

Run: `cd frontend && pnpm vitest run src/ui/settings/ExternalIntegrationSection.test.ts src/lib/translateLang.test.ts && pnpm check`
Expected: PASS、`pnpm check` もエラー 0。`UiPrefs` の完全なリテラルを組んでいる既存テスト/コードで `translateTargetLang` 不足の型エラーが出たら、そのリテラルに `translateTargetLang: "ja"` を足す。

- [ ] **Step 10: コミットする**

```bash
git add frontend/src/lib/translateLang.ts frontend/src/lib/translateLang.test.ts frontend/src/lib/store.svelte.ts frontend/src/ui/settings/ExternalIntegrationSection.svelte frontend/src/ui/settings/ExternalIntegrationSection.test.ts
git commit -m "feat: 翻訳先言語の設定とstoreメソッドを追加する"
```

（Step 9 でリテラルを直したファイルがあれば、それも `git add` に含める。）

---

### Task 5: NoteMenu の「翻訳」項目と NoteCard の翻訳ブロック

**Files:**
- Modify: `frontend/src/ui/NoteMenu.svelte`（props、`translatorAvailable`、ボタン）
- Modify: `frontend/src/ui/NoteCard.svelte`（`NoteMenu` への `ontranslate`、翻訳状態、本文直下の翻訳ブロック。`NoteMenu` の呼び出しは :592、本文は :414-444 付近）
- Test: `frontend/src/ui/NoteCard.test.ts`（末尾に `describe("ノート翻訳", ...)` を追加）

**Interfaces:**
- Consumes: Task 4 の `app.getTranslatorAvailable` / `app.translateNote`。
- Produces: NoteMenu の任意 prop `ontranslate?: () => void`（渡されたときだけ項目を出す）。NoteCard の翻訳ブロック（`data-testid="note-translation"`）。

NoteCard の `inner`（純リノートではリノート先）が翻訳対象。NoteCard はすでに `NoteMenu` に `note={inner}` を渡しているため、`inner.id` を翻訳すれば純リノートでもリノート先を翻訳できる。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/NoteCard.test.ts` の末尾に追加。先頭の import に `waitFor` を足す: `import { cleanup, render, waitFor } from "@testing-library/svelte";`。

```ts
describe("ノート翻訳", () => {
  // app はファイル全体で共有されるシングルトンなので、spy とアカウントは afterEach で必ず戻す。
  let availSpy: ReturnType<typeof vi.spyOn> | null = null;
  let translateSpy: ReturnType<typeof vi.spyOn> | null = null;
  afterEach(() => {
    app.accounts.length = 0;
    availSpy?.mockRestore();
    translateSpy?.mockRestore();
    availSpy = null;
    translateSpy = null;
  });

  function setup(available: boolean, translateImpl?: (accountId: string, noteId: string) => Promise<unknown>) {
    app.accounts.push({
      id: "acc1",
      host: "misskey.example",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
    });
    availSpy = vi.spyOn(app, "getTranslatorAvailable").mockResolvedValue(available);
    translateSpy = vi
      .spyOn(app, "translateNote")
      .mockImplementation((translateImpl ?? (async () => ({ sourceLang: "EN", text: "こんにちは" }))) as never);
  }

  it("翻訳できないサーバーでは「翻訳」項目を出さない", async () => {
    setup(false);
    const { getByLabelText, queryByText } = render(NoteCard, {
      props: { note: makeNote({ text: "hello" }), accountId: "acc1" },
    });
    await getByLabelText("その他").click();
    await waitFor(() => expect(availSpy).toHaveBeenCalled());
    expect(queryByText("翻訳")).toBeNull();
  });

  it("本文のないノートでは「翻訳」項目を出さない", async () => {
    setup(true);
    const { getByLabelText, queryByText } = render(NoteCard, {
      props: { note: makeNote({ text: null }), accountId: "acc1" },
    });
    await getByLabelText("その他").click();
    await waitFor(() => expect(availSpy).toHaveBeenCalled());
    expect(queryByText("翻訳")).toBeNull();
  });

  it("翻訳すると本文の下に検出元言語と翻訳文が出て、閉じられる", async () => {
    setup(true);
    const { getByLabelText, findByText, getByTestId, queryByTestId, getByText } = render(NoteCard, {
      props: { note: makeNote({ id: "n-tr-1", text: "hello" }), accountId: "acc1" },
    });
    await getByLabelText("その他").click();
    await (await findByText("翻訳")).click();

    await waitFor(() => expect(getByTestId("note-translation").textContent).toContain("こんにちは"));
    expect(getByTestId("note-translation").textContent).toContain("EN");
    expect(translateSpy).toHaveBeenCalledWith("acc1", "n-tr-1");

    await getByText("翻訳を閉じる").click();
    expect(queryByTestId("note-translation")).toBeNull();
  });

  it("結果なし(null)はエラーではなく「翻訳結果がありません」と表示する", async () => {
    setup(true, async () => null);
    const { getByLabelText, findByText, getByTestId } = render(NoteCard, {
      props: { note: makeNote({ text: "hello" }), accountId: "acc1" },
    });
    await getByLabelText("その他").click();
    await (await findByText("翻訳")).click();
    await waitFor(() => expect(getByTestId("note-translation").textContent).toContain("翻訳結果がありません"));
  });

  it("サーバー未対応(UNAVAILABLE)と一般エラーで文言を出し分ける", async () => {
    setup(true, async () => {
      throw new Error("api: notes/translate: UNAVAILABLE Translate of notes unavailable.");
    });
    const first = render(NoteCard, { props: { note: makeNote({ text: "hello" }), accountId: "acc1" } });
    await first.getByLabelText("その他").click();
    await (await first.findByText("翻訳")).click();
    await waitFor(() =>
      expect(first.getByTestId("note-translation").textContent).toContain("このサーバーは翻訳に対応していません"),
    );
    cleanup();

    translateSpy!.mockImplementation((async () => {
      throw new Error("network: timeout");
    }) as never);
    const second = render(NoteCard, { props: { note: makeNote({ text: "hello" }), accountId: "acc1" } });
    await second.getByLabelText("その他").click();
    await (await second.findByText("翻訳")).click();
    await waitFor(() => expect(second.getByTestId("note-translation").textContent).toContain("翻訳に失敗しました"));
  });

  it("翻訳文の <b> やMFM記法は解釈せず、そのまま文字として表示する", async () => {
    setup(true, async () => ({ sourceLang: "EN", text: "<b>bold</b> $[spin x]" }));
    const { getByLabelText, findByText, getByTestId } = render(NoteCard, {
      props: { note: makeNote({ text: "hello" }), accountId: "acc1" },
    });
    await getByLabelText("その他").click();
    await (await findByText("翻訳")).click();
    await waitFor(() => expect(getByTestId("note-translation").textContent).toContain("<b>bold</b> $[spin x]"));
    expect(getByTestId("note-translation").querySelector("b")).toBeNull();
  });

  it("純リノートではリノート先のノートを翻訳する", async () => {
    setup(true);
    const target = makeNote({ id: "n-target", text: "original" });
    const rn = makeNote({ id: "n-rn", text: null, renoteId: "n-target", renote: target });
    const { getByLabelText, findByText } = render(NoteCard, { props: { note: rn, accountId: "acc1" } });
    await getByLabelText("その他").click();
    await (await findByText("翻訳")).click();
    await waitFor(() => expect(translateSpy).toHaveBeenCalledWith("acc1", "n-target"));
  });

  it("翻訳の読み込み中に別のノートへ入れ替わったら、古い結果を新しいノートの下に出さない", async () => {
    let resolveFirst!: (v: unknown) => void;
    setup(true, () => new Promise((r) => (resolveFirst = r)));
    const { getByLabelText, findByText, queryByTestId, rerender } = render(NoteCard, {
      props: { note: makeNote({ id: "n-old", text: "old" }), accountId: "acc1" },
    });
    await getByLabelText("その他").click();
    await (await findByText("翻訳")).click();

    await rerender({ note: makeNote({ id: "n-new", text: "new" }), accountId: "acc1" });
    resolveFirst({ sourceLang: "EN", text: "古い翻訳" });
    await Promise.resolve();
    await Promise.resolve();

    expect(queryByTestId("note-translation")).toBeNull();
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts -t "ノート翻訳"`
Expected: FAIL（「翻訳」項目・`note-translation` が無い）。

- [ ] **Step 3: NoteMenu に項目を追加する**

`frontend/src/ui/NoteMenu.svelte`:

import を更新:

```ts
  import { onMount } from "svelte";
  import { Star, Paperclip, ChevronRight, Trash2, Copy, Repeat2, ArrowDownToLine, Languages } from "@lucide/svelte";
```

props に追加（`onclose` の前）:

```ts
    ontranslate,
```

型に追加（`onclose: () => void;` の前）:

```ts
    /// 渡されたときだけ「翻訳」項目を出す（Issue #440）。実際の翻訳と結果表示は呼び出し側（NoteCard）が持つ。
    ontranslate?: () => void;
```

`confirmUndoRenoteOpen` の宣言の近くに追加:

```ts
  // 接続先サーバーで翻訳が使えるか(Issue #440)。取得できるまでは項目を出さない。
  let translatorAvailable = $state(false);
  onMount(() => {
    let cancelled = false;
    void app.getTranslatorAvailable(accountId).then((v) => {
      if (!cancelled) translatorAvailable = v;
    });
    return () => {
      cancelled = true;
    };
  });

  function translate() {
    ontranslate?.();
    onclose();
  }
```

テンプレートの「内容をコピー」ボタンのブロック（`{#if note.text} ... {/if}`）の直後に追加:

```svelte
  {#if note.text && ontranslate && translatorAvailable}
    <button type="button" class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted" onclick={translate}>
      <Languages size={16} />
      翻訳
    </button>
  {/if}
```

- [ ] **Step 4: NoteCard に翻訳状態とブロックを追加する**

`frontend/src/ui/NoteCard.svelte`:

`let cwOpen = $state(false);`（:285）の近くに追加:

```ts
  type TranslationState =
    | { status: "loading" }
    | { status: "done"; sourceLang: string; text: string }
    | { status: "empty" }
    | { status: "error"; unavailable: boolean };
  let translation = $state<TranslationState | null>(null);
  // 翻訳の読み込み中にノートが入れ替わっても古い結果を出さないための世代番号。
  let translateSeq = 0;
  // 仮想リスト等で別のノートに使い回されたら、翻訳ブロックを破棄する。
  $effect(() => {
    void inner.id;
    translateSeq++;
    translation = null;
  });

  async function startTranslate() {
    if (!accountId) return;
    const seq = ++translateSeq;
    translation = { status: "loading" };
    try {
      const r = await app.translateNote(accountId, inner.id);
      if (seq !== translateSeq) return;
      translation = r ? { status: "done", sourceLang: r.sourceLang, text: r.text } : { status: "empty" };
    } catch (e) {
      if (seq !== translateSeq) return;
      translation = { status: "error", unavailable: String(e).includes("UNAVAILABLE") };
    }
  }
```

（`app` と `accountId` は既に NoteCard 内で参照されている。`$effect` 内で `translateSeq` を読まないこと — 依存に入るのは `inner.id` のみ。）

`NoteMenu` の呼び出し（:592）に `ontranslate` を追加:

```svelte
<NoteMenu {accountId} note={inner} pureRenoteOf={isPureRenote ? note : undefined} {tabId} listNoteId={tabId ? note.id : undefined} ontranslate={accountId ? startTranslate : undefined} onclose={() => (noteMenuOpen = false)} />
```

本文ブロックの直後（`{#if inner.text} <div class="relative"> ... </div> {/if}` の `{/if}` の次、`{#if inner.files.length > 0}` の前）に追加:

```svelte
        {#if translation}
          <div class="mt-1 rounded-md border border-border bg-muted/50 px-2 py-1.5 text-sm" data-testid="note-translation">
            {#if translation.status === "loading"}
              <span class="text-muted-foreground">翻訳中…</span>
            {:else if translation.status === "done"}
              <div class="mb-0.5 text-xs text-muted-foreground">{translation.sourceLang} から翻訳</div>
              <div class="whitespace-pre-wrap break-words leading-[1.42] [-webkit-user-select:text] select-text">{translation.text}</div>
            {:else if translation.status === "empty"}
              <span class="text-muted-foreground">翻訳結果がありません</span>
            {:else}
              <span class="text-destructive">{translation.unavailable ? "このサーバーは翻訳に対応していません" : "翻訳に失敗しました"}</span>
            {/if}
            {#if translation.status !== "loading"}
              <div class="mt-1">
                <button type="button" class="cw-toggle rounded-md border border-border px-2 py-px text-sm text-foreground" onclick={() => (translation = null)}>翻訳を閉じる</button>
              </div>
            {/if}
          </div>
        {/if}
```

- [ ] **Step 5: 通ることを確認する**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts && pnpm check`
Expected: 全件 PASS、`pnpm check` エラー 0。`bg-muted/50` や `rounded-md` が `docs/design/style-guide.md` のスケールから外れていないか確認する（外れていたらスケール内の値へ直す）。

- [ ] **Step 6: フロント全体を確認する**

Run: `cd frontend && pnpm test`
Expected: 全件 PASS。

- [ ] **Step 7: コミットする**

```bash
git add frontend/src/ui/NoteMenu.svelte frontend/src/ui/NoteCard.svelte frontend/src/ui/NoteCard.test.ts
git commit -m "feat: ノートメニューから翻訳して本文の下に表示する"
```

---

### Task 6: ユーザーガイドと実機確認

**Files:**
- Modify: `docs/guide/user-guide.md`（:119-120 付近のノートメニューの項目、:138 の外部連携）

- [ ] **Step 1: ユーザーガイドを更新する**

`docs/guide/user-guide.md` のノートメニューの箇条書き（`クリップに追加（...）` の次の行）に追加:

```
  - 翻訳（接続先サーバーの翻訳機能で本文を翻訳し、投稿の本文の下に表示。サーバーで翻訳が設定されている場合のみ表示されます）
```

`外部連携` の行（:138）を次に置き換える:

```
- **外部連携**: MFM検索構文($[search]相当)で使う検索エンジンのURLテンプレート、投稿本文中のURLへのリンクプレビュー表示ON/OFFとカスタムsummalyプロキシURL、ノート翻訳の翻訳先言語。
```

- [ ] **Step 2: 全テストを通す**

Run: `cd src-tauri && cargo test && cd ../frontend && pnpm check && pnpm test`
Expected: すべて PASS。

- [ ] **Step 3: 実インスタンスで仕様を再確認する**

Run: `cd src-tauri && cargo test --lib real_misskey_meta -- --ignored`（既存の実サーバー疎通テスト）、加えて
`curl -s -A "Mozilla/5.0" https://misskey.omhnc.net/api.json | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['paths']['/notes/translate']['post']['responses'].keys())"`
Expected: `notes/translate` に 200 / 204 / 400 がある。（`User-Agent` なしの取得は 403 になる。）

- [ ] **Step 4: 実機で動作確認する（Xvfb 越し）**

検証用の起動は必ず仮想ディスプレイ越しにする（ユーザー環境は Wayland）。`WAYLAND_DISPLAY` を unset し、Xvfb と `dbus-run-session` 越しにリポジトリルートから `cargo tauri dev` を起動する（`e2e/README.md` / `run-app.sh` の手順に従う）。翻訳が有効な実インスタンスのアカウントで次を確認する:

1. 外国語ノートのメニューに「翻訳」が出て、押すと本文の下に「XX から翻訳」と翻訳文が出る。「翻訳を閉じる」で消える。
2. 設定 → 外部連携で翻訳先言語を変えて保存すると、次の翻訳がその言語になる。
3. 翻訳が未設定のサーバーのアカウントでは、メニューに「翻訳」が出ない。
4. 純リノートで翻訳すると、リノート先の本文が翻訳される。

確認後、起動したプロセス（Xvfb / dbus-run-session / vite / tsumugi）を**正確な PID で**kill する。`pkill`/`killall` は使わない。

- [ ] **Step 5: コミットする**

```bash
git add docs/guide/user-guide.md
git commit -m "docs: ノート翻訳の使い方をユーザーガイドに追記する"
```

- [ ] **Step 6: push して PR を作る（ユーザーの指示があってから）**

push と PR 作成はユーザーが指示してから行う。PR 本文は `.github/pull_request_template.md` の構成に手で従い、`Closes #440` を含める。マージは `gh pr merge --merge`（squash ではない）。CI は Monitor で待たない。
