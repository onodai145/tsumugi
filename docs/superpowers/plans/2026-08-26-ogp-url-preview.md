# OGP/リンクプレビュー対応 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 投稿本文中のURLに、summalyプロキシ（自インスタンスの`/url`、またはユーザ設定のカスタムプロキシ）から取得したOGP相当のリンクプレビューカードを表示する（Issue #9）。

**Architecture:** Rust側に認証不要のGETラッパ（`api/url_preview.rs`）と`#[tauri::command] fetch_url_preview`を追加し、`UiPrefs`に表示ON/OFFとカスタムプロキシURLの設定を持たせる。フロントは`mentionAvatar.ts`と同構造のセッション内キャッシュ（`lib/urlPreview.ts`）でフェッチを集約し、`mfm-js`の`extract`で本文からURLノードを収集して`NoteCard.svelte`が`UrlPreviewCard.svelte`を並べる。

**Tech Stack:** Rust (reqwest, serde, specta), Svelte 5 (runes), mfm-js, Vitest, `@testing-library/svelte`。

## Global Constraints

- 設計書: `docs/superpowers/specs/2026-08-26-ogp-url-preview-design.md`（矛盾があればこちらが優先）
- 対象URLは本文中の全MFM `url`ノード（重複はURL文字列でデデュープ）。`link`ノード（カスタムテキストリンク）は対象外
- キャッシュはセッション内メモリのみ。永続化しない
- 取得失敗（ネットワークエラー・タイムアウト・非2xx・IPC層自体の失敗）はカードを描画せず、キャッシュもしない（次回再試行可能）
- 成功したが OGP フィールドが全て空の応答は「確定的にプレビュー無し」として`null`を永続キャッシュする
- `sensitive: true`は「ぼかし→クリックで開示」。`MediaGrid.svelte`の`.sensitive-cover`と同じ見た目パターンに揃える
- プレイヤーは自動再生せず、クリックで`<iframe sandbox="allow-scripts allow-same-origin">`に遅延差し替え
- 表示ON/OFFのトグル（既定ON）とカスタムsummalyプロキシURL（既定空＝自インスタンスの`/url`）は`UiPrefs`に追加し、設定UIに露出する
- カスタムプロキシ使用時は本文中の実URLが第三者プロキシへ直接送られる旨を設定UIに明記する
- `cargo test`の`generates_frontend_bindings`でTSバインディング生成が壊れていないことを都度確認する

---

### Task 1: Rust ドメイン型とUiPrefs設定フィールド

**Files:**
- Create: `src-tauri/src/domain/url_preview.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Modify: `src-tauri/src/domain/ui.rs`

**Interfaces:**
- Produces: `domain::UrlPreview { url: String, title: Option<String>, description: Option<String>, thumbnail: Option<String>, icon: Option<String>, sitename: Option<String>, sensitive: bool, player: Option<UrlPlayer> }`
- Produces: `domain::UrlPlayer { url: String, width: Option<i32>, height: Option<i32> }`
- Produces: `UiPrefs.url_preview_enabled: bool`（既定`true`）、`UiPrefs.summaly_proxy_url: String`（既定`""`）

- [ ] **Step 1: `domain/url_preview.rs`を作成する**

```rust
//! summalyプロキシから取得するリンクプレビュー（OGP相当）情報。
//! Issue #9: 投稿本文中のURLにタイトル・説明・サムネイルのカードを添える。

use serde::{Deserialize, Serialize};
use specta::Type;

/// プレビュー結果。`title`以下は summaly の応答でいずれも欠落しうるため `Option`。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UrlPreview {
    /// プレビュー対象のURL（summalyが返したものを優先し、無ければ要求したURLをそのまま使う）。
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub thumbnail: Option<String>,
    pub icon: Option<String>,
    pub sitename: Option<String>,
    /// センシティブ判定。フィールド自体が無い応答は false 扱い。
    #[serde(default)]
    pub sensitive: bool,
    pub player: Option<UrlPlayer>,
}

/// 動画/音声プレイヤー埋め込み情報（YouTube等のoEmbed player）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UrlPlayer {
    pub url: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
}
```

- [ ] **Step 2: `domain/mod.rs`にモジュールを登録する**

`mod ui;` の下に追加（アルファベット順: ui < url_preview < user）:

```rust
mod ui;
mod url_preview;
mod user;
```

`pub use ui::UiPrefs;` の下に追加:

```rust
pub use ui::UiPrefs;
pub use url_preview::{UrlPlayer, UrlPreview};
pub use user::User;
```

- [ ] **Step 3: `domain/ui.rs`に設定フィールドを追加する**

`UiPrefs`構造体の最後のフィールド（`search_engine_url`）の直後に追加:

```rust
    /// 投稿本文中のURLにリンクプレビュー(OGP相当)カードを表示するか（Issue #9）。既定はON。
    #[serde(default = "default_url_preview_enabled")]
    pub url_preview_enabled: bool,
    /// カスタムsummalyプロキシのベースURL。空文字なら接続先インスタンスの`/url`を使う（Issue #9）。
    /// 設定すると、プレビュー対象のURLはインスタンスではなくここへ直接送られる。
    #[serde(default)]
    pub summaly_proxy_url: String,
```

`default_search_engine_url()`関数の直後に追加:

```rust
fn default_url_preview_enabled() -> bool {
    true
}
```

`impl Default for UiPrefs`の`search_engine_url: default_search_engine_url(),`の直後に追加:

```rust
            url_preview_enabled: default_url_preview_enabled(),
            summaly_proxy_url: String::new(),
```

- [ ] **Step 4: 既存テストに新フィールドの後方互換アサーションを追加する**

`tests`モジュール内、`deserializes_legacy_json_without_new_fields`の末尾（`assert_eq!(v.custom_syntax_themes...`より後、関数の最後）に追加:

```rust
        assert!(v.url_preview_enabled);
        assert_eq!(v.summaly_proxy_url, "");
```

`roundtrips_keymap`内、`UiPrefs { ... }`リテラルの`search_engine_url: "https://duckduckgo.com/?q={query}".into(),`の直後に追加:

```rust
            url_preview_enabled: false,
            summaly_proxy_url: "https://my-proxy.example.com/preview".into(),
```

`tests`モジュールの末尾に新規テストを追加:

```rust
    #[test]
    fn url_preview_enabled_defaults_to_true_for_legacy_json() {
        let v: UiPrefs =
            serde_json::from_str(r#"{"theme":"dark","defaultColumnWidth":320}"#).unwrap();
        assert!(v.url_preview_enabled);
        assert_eq!(v.summaly_proxy_url, "");
    }
```

- [ ] **Step 5: テストを実行する**

Run: `cd src-tauri && cargo test ui::`
Expected: PASS（`deserializes_legacy_json_without_new_fields`, `roundtrips_keymap`, `url_preview_enabled_defaults_to_true_for_legacy_json`含む全件）

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/domain/url_preview.rs src-tauri/src/domain/mod.rs src-tauri/src/domain/ui.rs
git commit -m "feat: UrlPreview型とUiPrefsのリンクプレビュー設定を追加"
```

---

### Task 2: Rust `api/url_preview.rs`（summalyプロキシ取得・正規化）

**Files:**
- Create: `src-tauri/src/api/url_preview.rs`
- Modify: `src-tauri/src/api/mod.rs`

**Interfaces:**
- Consumes: `domain::{UrlPlayer, UrlPreview}`（Task 1）
- Produces: `pub async fn fetch_url_preview(http: &reqwest::Client, proxy_base: &str, target_url: &str) -> crate::error::Result<UrlPreview>`

- [ ] **Step 1: 正規化ロジックの失敗するテストを書く**

`src-tauri/src/api/url_preview.rs`を新規作成し、まずテストのみ書く:

```rust
//! summalyプロキシ（Misskeyインスタンス組み込みの `/url` ルート、または任意のカスタムプロキシ）
//! を叩いてリンクプレビュー(OGP相当)を取得する（Issue #9）。
//!
//! `/url` は Misskey の `/api/*` REST APIコマンドではなく、認証不要のプレーンなWebルート
//! （summalyプロキシの公開口）という理解。現行スナップショット `openapi/misskey-api-doc.json`
//! には掲載がない（非APIルートのため）。本モジュール下部の `#[ignore]` テストで実インスタンス
//! に対する実際のパス・レスポンス形を確認できる。

use crate::domain::{UrlPlayer, UrlPreview};
use crate::error::{Error, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawUrlPreview {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    thumbnail: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    sitename: Option<String>,
    #[serde(default)]
    sensitive: bool,
    #[serde(default)]
    player: Option<RawPlayer>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPlayer {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    width: Option<i32>,
    #[serde(default)]
    height: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_full_response() {
        let raw: RawUrlPreview = serde_json::from_str(
            r#"{
                "url": "https://example.com/article",
                "title": "記事タイトル",
                "description": "説明文",
                "thumbnail": "https://example.com/thumb.png",
                "icon": "https://example.com/favicon.ico",
                "sitename": "Example",
                "sensitive": true,
                "player": {"url": "https://example.com/embed", "width": 640, "height": 360}
            }"#,
        )
        .unwrap();
        let preview = normalize(raw, "https://example.com/article");
        assert_eq!(preview.url, "https://example.com/article");
        assert_eq!(preview.title.as_deref(), Some("記事タイトル"));
        assert_eq!(preview.description.as_deref(), Some("説明文"));
        assert_eq!(preview.thumbnail.as_deref(), Some("https://example.com/thumb.png"));
        assert_eq!(preview.icon.as_deref(), Some("https://example.com/favicon.ico"));
        assert_eq!(preview.sitename.as_deref(), Some("Example"));
        assert!(preview.sensitive);
        let player = preview.player.unwrap();
        assert_eq!(player.url, "https://example.com/embed");
        assert_eq!(player.width, Some(640));
        assert_eq!(player.height, Some(360));
    }

    #[test]
    fn falls_back_to_target_url_when_url_field_missing() {
        let raw: RawUrlPreview = serde_json::from_str(r#"{}"#).unwrap();
        let preview = normalize(raw, "https://example.com/no-og");
        assert_eq!(preview.url, "https://example.com/no-og");
        assert!(preview.title.is_none());
        assert!(!preview.sensitive);
        assert!(preview.player.is_none());
    }

    #[test]
    fn drops_player_without_url() {
        let raw: RawUrlPreview =
            serde_json::from_str(r#"{"player": {"width": 640, "height": 360}}"#).unwrap();
        let preview = normalize(raw, "https://example.com/x");
        assert!(preview.player.is_none());
    }
}
```

- [ ] **Step 2: テストが「`normalize`が無い」で失敗することを確認する**

Run: `cd src-tauri && cargo test --lib api::url_preview::tests -- --nocapture`
Expected: コンパイルエラー `cannot find function 'normalize' in this scope`

- [ ] **Step 3: `normalize`と`fetch_url_preview`を実装する**

`RawPlayer`構造体の直後、`#[cfg(test)]`の直前に追加:

```rust
/// `raw` を `UrlPreview` へ正規化する。`raw.url` が欠落していれば要求した `target_url` を使う。
/// `player` は `url` が無ければ埋め込みようがないため丸ごと `None` に落とす。
fn normalize(raw: RawUrlPreview, target_url: &str) -> UrlPreview {
    UrlPreview {
        url: raw.url.unwrap_or_else(|| target_url.to_string()),
        title: raw.title,
        description: raw.description,
        thumbnail: raw.thumbnail,
        icon: raw.icon,
        sitename: raw.sitename,
        sensitive: raw.sensitive,
        player: raw.player.and_then(|p| {
            p.url.map(|url| UrlPlayer {
                url,
                width: p.width,
                height: p.height,
            })
        }),
    }
}

/// `proxy_base`（例: `"https://misskey.io/url"` またはユーザ設定のカスタムプロキシURL）に
/// `?url=<target_url>` を付与してGETし、応答を [`UrlPreview`] へ正規化する。
/// 認証不要（トークンは付与しない）。
pub async fn fetch_url_preview(
    http: &reqwest::Client,
    proxy_base: &str,
    target_url: &str,
) -> Result<UrlPreview> {
    let resp = http
        .get(proxy_base)
        .query(&[("url", target_url)])
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Api(format!("url preview: HTTP {status}")));
    }
    let raw: RawUrlPreview = resp.json().await?;
    Ok(normalize(raw, target_url))
}
```

- [ ] **Step 4: テストを実行し成功を確認する**

Run: `cd src-tauri && cargo test --lib api::url_preview::tests`
Expected: PASS（3件）

- [ ] **Step 5: 実インスタンス疎通確認用の`#[ignore]`テストを追加する**

`tests`モジュール末尾に追加:

```rust
    /// 実インスタンスの `/url` エンドポイントに対する疎通確認。
    /// パス・レスポンス形が想定と異なる場合、このテストの失敗内容を見て本モジュールを直す。
    /// ネットワーク依存のため既定では実行しない: `cargo test -- --ignored real_url_preview`
    #[ignore]
    #[tokio::test]
    async fn real_url_preview_from_misskey_io() {
        let http = reqwest::Client::new();
        let preview = fetch_url_preview(&http, "https://misskey.io/url", "https://misskey.io/")
            .await
            .expect("fetch_url_preview should succeed against misskey.io");
        assert!(
            preview.title.as_deref().is_some_and(|t| !t.is_empty()),
            "expected a non-empty title, got {:?}",
            preview.title
        );
    }
```

- [ ] **Step 6: `api/mod.rs`にモジュールを登録する**

`pub mod users;`の直後に追加:

```rust
pub mod users;
pub mod url_preview;
pub mod normalize;
```

（既存の`pub mod normalize;`行を移動するのではなく、`users`と`normalize`の間に`url_preview`を挿入する）

- [ ] **Step 7: 手動で実連携テストを実行し、エンドポイントの想定を検証する（任意・要ネットワーク）**

Run: `cd src-tauri && cargo test --lib api::url_preview::tests::real_url_preview_from_misskey_io -- --ignored --nocapture`
Expected: PASS。失敗した場合はエラーメッセージ（HTTPステータス・JSONパースエラー等）を見て、`fetch_url_preview`のパス構築（`/url`）やレスポンス形（`RawUrlPreview`のフィールド名）を実際の応答に合わせて修正する。

- [ ] **Step 8: 通常テストスイート全体を実行する**

Run: `cd src-tauri && cargo test`
Expected: PASS（`#[ignore]`のテストはスキップされる）

- [ ] **Step 9: コミット**

```bash
git add src-tauri/src/api/url_preview.rs src-tauri/src/api/mod.rs
git commit -m "feat: summalyプロキシからのリンクプレビュー取得・正規化を追加"
```

---

### Task 3: `#[tauri::command] fetch_url_preview`とTSバインディング生成

**Files:**
- Modify: `src-tauri/src/commands/note.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `api::url_preview::fetch_url_preview`（Task 2）、`domain::UrlPreview`（Task 1）、`AppState::client_for`・`AppState.http`・`AppState.settings.load_ui()`（既存）
- Produces: Tauriコマンド `fetch_url_preview(account_id: String, url: String) -> Result<UrlPreview>`。TS側は `commands.fetchUrlPreview(accountId, url)`

- [ ] **Step 1: `commands/note.rs`にコマンドを追加する**

`use crate::domain::{...}`のインポート行を編集し、`UrlPreview`を追加:

```rust
use crate::domain::{DriveFile, EmojiDef, Note, ReactionUser, SourceItem, UrlPreview, User};
```

ファイル冒頭の`use crate::api::...`群に以下を追加:

```rust
use crate::api::url_preview::fetch_url_preview as fetch_url_preview_api;
```

ファイル末尾（`search_hashtags`コマンドの後）に追加:

```rust
/// 投稿本文中のURLのリンクプレビュー(OGP相当)を取得する（Issue #9）。
/// `UiPrefs.summaly_proxy_url` が設定されていればそれを、空ならアカウントの接続先インスタンスの
/// `/url` を使う。いずれも認証不要（トークンは送らない）。
#[tauri::command]
#[specta::specta]
pub async fn fetch_url_preview(
    state: State<'_, AppState>,
    account_id: String,
    url: String,
) -> Result<UrlPreview> {
    let prefs = state.settings.load_ui()?;
    let proxy_base = if prefs.summaly_proxy_url.trim().is_empty() {
        let client = state.client_for(&account_id)?;
        format!("https://{}/url", client.host())
    } else {
        prefs.summaly_proxy_url.clone()
    };
    fetch_url_preview_api(&state.http, &proxy_base, &url).await
}
```

- [ ] **Step 2: `lib.rs`の`specta_builder()`にコマンドを登録する**

`commands::note::search_hashtags,`の直後に追加:

```rust
            commands::note::search_hashtags,
            commands::note::fetch_url_preview,
```

- [ ] **Step 3: コンパイルを確認する**

Run: `cd src-tauri && cargo check`
Expected: エラーなし

- [ ] **Step 4: TSバインディングを再生成し、規約を検証する**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`frontend/src/bindings/tauri.gen.ts`に`fetchUrlPreview`とcamelCase化された`UrlPreview`/`UrlPlayer`型が生成される

- [ ] **Step 5: 生成されたバインディングを確認する**

Run: `grep -n "fetchUrlPreview\|UrlPreview\|UrlPlayer" frontend/src/bindings/tauri.gen.ts`
Expected: `fetchUrlPreview`関数と`UrlPreview`/`UrlPlayer`型定義が出力されている

- [ ] **Step 6: Rustテストスイート全体を実行する**

Run: `cd src-tauri && cargo test`
Expected: PASS

- [ ] **Step 7: コミット**

```bash
git add src-tauri/src/commands/note.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: fetch_url_previewコマンドを追加してTSバインディングを生成"
```

---

### Task 4: フロントエンド `extractPreviewUrls`（本文からのURL抽出）

**Files:**
- Create: `frontend/src/lib/extractPreviewUrls.ts`
- Test: `frontend/src/lib/extractPreviewUrls.test.ts`

**Interfaces:**
- Produces: `export function extractPreviewUrls(text: string): string[]`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/extractPreviewUrls.test.ts`を作成:

```ts
import { describe, expect, it } from "vitest";
import { extractPreviewUrls } from "./extractPreviewUrls";

describe("extractPreviewUrls", () => {
  it("returns an empty array for text without URLs", () => {
    expect(extractPreviewUrls("hello world")).toEqual([]);
  });

  it("extracts a bare URL", () => {
    expect(extractPreviewUrls("見て https://example.com/a")).toEqual(["https://example.com/a"]);
  });

  it("dedupes repeated URLs", () => {
    expect(extractPreviewUrls("https://example.com/a https://example.com/a")).toEqual([
      "https://example.com/a",
    ]);
  });

  it("finds URLs nested inside a quote block", () => {
    expect(extractPreviewUrls("> https://example.com/a")).toEqual(["https://example.com/a"]);
  });

  it("ignores empty text", () => {
    expect(extractPreviewUrls("")).toEqual([]);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/lib/extractPreviewUrls.test.ts`
Expected: FAIL（`extractPreviewUrls.ts`が存在しない）

- [ ] **Step 3: 実装する**

`frontend/src/lib/extractPreviewUrls.ts`を作成:

```ts
import { extract, parse } from "mfm-js";
import type { MfmNode } from "mfm-js";

/// 本文中のMFM `url`ノード（裸URL）のURLを重複排除して返す。
/// カスタムテキストの`link`ノード（`[text](url)`）は対象外（Issue #9）。
export function extractPreviewUrls(text: string): string[] {
  if (!text) return [];
  const nodes = extract(parse(text), (node) => node.type === "url") as Extract<
    MfmNode,
    { type: "url" }
  >[];
  return [...new Set(nodes.map((n) => n.props.url))];
}
```

- [ ] **Step 4: テストを実行し成功を確認する**

Run: `cd frontend && pnpm vitest run src/lib/extractPreviewUrls.test.ts`
Expected: PASS（5件）。「finds URLs nested inside a quote block」が失敗する場合、`console.log(JSON.stringify(parse("> https://example.com/a"), null, 2))`で実際のノード構造を確認し、テストの入力文字列を実際にurlノードを生成する構文に置き換える（実装ロジック自体は変更不要）

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/extractPreviewUrls.ts frontend/src/lib/extractPreviewUrls.test.ts
git commit -m "feat: 本文からリンクプレビュー対象URLを抽出するユーティリティを追加"
```

---

### Task 5: フロントエンド `lib/urlPreview.ts`（セッションキャッシュ）

**Files:**
- Create: `frontend/src/lib/urlPreview.ts`
- Test: `frontend/src/lib/urlPreview.test.ts`

**Interfaces:**
- Consumes: `commands.fetchUrlPreview(accountId: string, url: string)`（Task 3のTSバインディング）、`app.defaultAccountId()`（`lib/store.svelte.ts`既存）
- Produces: `export function cachedUrlPreview(url: string): UrlPreview | null | undefined`、`export async function fetchUrlPreview(url: string): Promise<UrlPreview | null>`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/urlPreview.test.ts`を作成:

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";

const fetchUrlPreviewMock = vi.fn();
vi.mock("./ipc", () => ({ commands: { fetchUrlPreview: fetchUrlPreviewMock } }));
vi.mock("./store.svelte", () => ({ app: { defaultAccountId: () => "acc1" } }));

const { cachedUrlPreview, fetchUrlPreview } = await import("./urlPreview");

const PREVIEW = {
  url: "https://example.com/a",
  title: "タイトル",
  description: null,
  thumbnail: null,
  icon: null,
  sitename: null,
  sensitive: false,
  player: null,
};

beforeEach(() => {
  fetchUrlPreviewMock.mockReset();
});

describe("urlPreview cache", () => {
  it("is undefined before the first fetch", () => {
    expect(cachedUrlPreview("https://example.com/never-fetched")).toBeUndefined();
  });

  it("caches a successful response with content", async () => {
    fetchUrlPreviewMock.mockResolvedValue({ status: "ok", data: PREVIEW });
    const result = await fetchUrlPreview("https://example.com/a");
    expect(result).toEqual(PREVIEW);
    expect(cachedUrlPreview("https://example.com/a")).toEqual(PREVIEW);
  });

  it("caches null permanently when the response has no OG fields", async () => {
    const empty = { ...PREVIEW, url: "https://example.com/empty", title: null };
    fetchUrlPreviewMock.mockResolvedValue({ status: "ok", data: empty });
    const result = await fetchUrlPreview("https://example.com/empty");
    expect(result).toBeNull();
    expect(cachedUrlPreview("https://example.com/empty")).toBeNull();
  });

  it("does not cache a typed error (transient failure)", async () => {
    fetchUrlPreviewMock.mockResolvedValue({
      status: "error",
      error: { kind: "network", message: "boom" },
    });
    const result = await fetchUrlPreview("https://example.com/net-error");
    expect(result).toBeNull();
    expect(cachedUrlPreview("https://example.com/net-error")).toBeUndefined();
  });

  it("does not cache when the IPC call itself throws", async () => {
    fetchUrlPreviewMock.mockRejectedValue(new Error("command not registered"));
    const result = await fetchUrlPreview("https://example.com/ipc-fail");
    expect(result).toBeNull();
    expect(cachedUrlPreview("https://example.com/ipc-fail")).toBeUndefined();
  });

  it("dedupes concurrent fetches for the same URL", async () => {
    let resolveFn: (v: unknown) => void = () => {};
    fetchUrlPreviewMock.mockReturnValue(
      new Promise((resolve) => {
        resolveFn = resolve;
      }),
    );
    const p1 = fetchUrlPreview("https://example.com/concurrent");
    const p2 = fetchUrlPreview("https://example.com/concurrent");
    resolveFn({ status: "ok", data: PREVIEW });
    await Promise.all([p1, p2]);
    expect(fetchUrlPreviewMock).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/lib/urlPreview.test.ts`
Expected: FAIL（`urlPreview.ts`が存在しない）

- [ ] **Step 3: 実装する**

`frontend/src/lib/urlPreview.ts`を作成:

```ts
import { commands } from "./ipc";
import { app } from "./store.svelte";
import type { UrlPreview } from "../bindings/tauri.gen";

// リンクプレビュー(OGP相当)のセッション内キャッシュ(Issue #9)。lib/mentionAvatar.tsと同構造だが、
// プレビュー内容自体はアカウントに依存しない公開情報のため、キーはURLのみ(accountIdを含めない)。
//   - キャッシュ未登録(Map.getがundefined): 未取得
//   - null: 確定的にプレビュー無し(OGPフィールドが全て空の応答、または解決不能なアカウント)。
//     以後リトライしない
//   - UrlPreview: 取得済み
// ネットワークエラー・タイムアウト・IPC層自体の失敗等の一時的な失敗はキャッシュしない
// (次回呼び出しで再試行される)。
const cache = new Map<string, UrlPreview | null>();
// 同一URLへの同時フェッチを1回のリクエストに集約するための in-flight Promise。
const inflight = new Map<string, Promise<UrlPreview | null>>();

/// キャッシュ済みなら即値を返す（同期的にレンダリング判定するため）。未取得ならundefined。
export function cachedUrlPreview(url: string): UrlPreview | null | undefined {
  return cache.get(url);
}

/// URLのリンクプレビューを取得する。`app.defaultAccountId()`のアカウントを使う
/// (lib/mentionAvatar.tsと同じ慣例)。
export async function fetchUrlPreview(url: string): Promise<UrlPreview | null> {
  const cached = cache.get(url);
  if (cached !== undefined) return cached;

  const existing = inflight.get(url);
  if (existing) return existing;

  const promise = resolve(url)
    .then(({ data, permanent }) => {
      if (permanent) cache.set(url, data);
      return data;
    })
    .finally(() => inflight.delete(url));
  inflight.set(url, promise);
  return promise;
}

/// OGPフィールドが1つでもあれば「内容あり」とみなす。
function hasContent(p: UrlPreview): boolean {
  return !!(p.title || p.description || p.thumbnail || p.icon || p.sitename || p.player);
}

async function resolve(url: string): Promise<{ data: UrlPreview | null; permanent: boolean }> {
  const accountId = app.defaultAccountId();
  if (!accountId) return { data: null, permanent: false };
  try {
    const r = await commands.fetchUrlPreview(accountId, url);
    if (r.status === "ok") return { data: hasContent(r.data) ? r.data : null, permanent: true };
    return { data: null, permanent: false };
  } catch {
    // IPC層自体の失敗(開発時のコマンド未登録等)も一時的エラーとして扱い、キャッシュしない
    return { data: null, permanent: false };
  }
}
```

- [ ] **Step 4: テストを実行し成功を確認する**

Run: `cd frontend && pnpm vitest run src/lib/urlPreview.test.ts`
Expected: PASS（6件）

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/urlPreview.ts frontend/src/lib/urlPreview.test.ts
git commit -m "feat: リンクプレビューのセッションキャッシュを追加"
```

---

### Task 6: フロントエンド `render/UrlPreviewCard.svelte`

**Files:**
- Create: `frontend/src/render/UrlPreviewCard.svelte`
- Test: `frontend/src/render/UrlPreviewCard.test.ts`

**Interfaces:**
- Consumes: `cachedUrlPreview`, `fetchUrlPreview`（`lib/urlPreview.ts`, Task 5）
- Produces: Svelteコンポーネント `UrlPreviewCard`、props: `{ url: string }`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/render/UrlPreviewCard.test.ts`を作成:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/svelte";
import UrlPreviewCard from "./UrlPreviewCard.svelte";

const cachedUrlPreviewMock = vi.fn();
const fetchUrlPreviewMock = vi.fn();
vi.mock("../lib/urlPreview", () => ({
  cachedUrlPreview: (url: string) => cachedUrlPreviewMock(url),
  fetchUrlPreview: (url: string) => fetchUrlPreviewMock(url),
}));

afterEach(() => {
  cleanup();
  cachedUrlPreviewMock.mockReset();
  fetchUrlPreviewMock.mockReset();
});

const PREVIEW = {
  url: "https://example.com/a",
  title: "記事タイトル",
  description: "説明文",
  thumbnail: null,
  icon: null,
  sitename: "Example",
  sensitive: false,
  player: null,
};

describe("UrlPreviewCard", () => {
  it("renders nothing while the preview has not been fetched yet", () => {
    cachedUrlPreviewMock.mockReturnValue(undefined);
    fetchUrlPreviewMock.mockReturnValue(new Promise(() => {}));
    render(UrlPreviewCard, { props: { url: "https://example.com/a" } });
    expect(screen.queryByText("記事タイトル")).toBeNull();
  });

  it("renders nothing when the fetch resolves to null", async () => {
    cachedUrlPreviewMock.mockReturnValue(undefined);
    fetchUrlPreviewMock.mockResolvedValue(null);
    render(UrlPreviewCard, { props: { url: "https://example.com/a" } });
    await waitFor(() => expect(fetchUrlPreviewMock).toHaveBeenCalled());
    expect(screen.queryByText("記事タイトル")).toBeNull();
  });

  it("renders the cached preview synchronously", () => {
    cachedUrlPreviewMock.mockReturnValue(PREVIEW);
    render(UrlPreviewCard, { props: { url: "https://example.com/a" } });
    expect(screen.getByText("記事タイトル")).toBeInTheDocument();
    expect(screen.getByText("説明文")).toBeInTheDocument();
    expect(screen.getByText("Example")).toBeInTheDocument();
  });

  it("blurs a sensitive preview until clicked", async () => {
    const sensitive = { ...PREVIEW, thumbnail: "https://example.com/t.png", sensitive: true };
    cachedUrlPreviewMock.mockReturnValue(sensitive);
    render(UrlPreviewCard, { props: { url: "https://example.com/a" } });
    const cover = screen.getByText("閲覧注意（クリックで表示）");
    expect(screen.queryByRole("img")).toBeNull();
    cover.click();
    await waitFor(() => expect(screen.getByRole("img")).toBeInTheDocument());
  });

  it("does not embed the iframe until the play button is clicked", async () => {
    const withPlayer = {
      ...PREVIEW,
      thumbnail: "https://example.com/t.png",
      player: { url: "https://example.com/embed", width: 640, height: 360 },
    };
    cachedUrlPreviewMock.mockReturnValue(withPlayer);
    render(UrlPreviewCard, { props: { url: "https://example.com/a" } });
    expect(document.querySelector("iframe")).toBeNull();
    const playButton = screen.getByRole("button", { name: "再生" });
    playButton.click();
    await waitFor(() => expect(document.querySelector("iframe")).not.toBeNull());
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/render/UrlPreviewCard.test.ts`
Expected: FAIL（`UrlPreviewCard.svelte`が存在しない）

- [ ] **Step 3: 実装する**

`frontend/src/render/UrlPreviewCard.svelte`を作成:

```svelte
<script lang="ts">
  import type { UrlPreview } from "../bindings/tauri.gen";
  import { cachedUrlPreview, fetchUrlPreview } from "../lib/urlPreview";

  let { url }: { url: string } = $props();

  let preview = $state<UrlPreview | null | undefined>(cachedUrlPreview(url));
  let revealed = $state(false);
  let playing = $state(false);

  $effect(() => {
    if (preview !== undefined) return;
    let cancelled = false;
    fetchUrlPreview(url).then((p) => {
      if (!cancelled) preview = p;
    });
    return () => {
      cancelled = true;
    };
  });
</script>

{#if preview}
  <div class="url-preview-card mt-2 overflow-hidden rounded-md border border-border text-sm">
    {#if preview.thumbnail || preview.player}
      <div class="relative aspect-[21/9] w-full">
        {#if preview.sensitive && !revealed}
          <button
            type="button"
            class="sensitive-cover h-full w-full border-0 text-sm text-muted-foreground"
            onclick={() => (revealed = true)}
          >
            閲覧注意（クリックで表示）
          </button>
        {:else if playing && preview.player}
          <iframe
            src={preview.player.url}
            title={preview.title ?? preview.url}
            sandbox="allow-scripts allow-same-origin"
            class="h-full w-full border-0"
          ></iframe>
        {:else}
          {#if preview.thumbnail}
            <img src={preview.thumbnail} alt="" loading="lazy" class="h-full w-full object-cover" />
          {/if}
          {#if preview.player}
            <button
              type="button"
              class="play-button absolute inset-0 flex items-center justify-center border-0 bg-black/30 text-2xl text-white"
              onclick={() => (playing = true)}
              aria-label="再生"
            >
              ▶
            </button>
          {/if}
        {/if}
      </div>
    {/if}
    <a
      class="block px-2 py-1.5 text-foreground no-underline"
      href={preview.url}
      target="_blank"
      rel="noreferrer noopener"
    >
      {#if preview.sitename}<div class="truncate text-xs text-muted-foreground">{preview.sitename}</div>{/if}
      {#if preview.title}<div class="line-clamp-1 font-semibold">{preview.title}</div>{/if}
      {#if preview.description}<div class="line-clamp-2 text-xs text-muted-foreground">{preview.description}</div>{/if}
    </a>
  </div>
{/if}

<style>
  .sensitive-cover {
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  }
</style>
```

- [ ] **Step 4: テストを実行し成功を確認する**

Run: `cd frontend && pnpm vitest run src/render/UrlPreviewCard.test.ts`
Expected: PASS（5件）

- [ ] **Step 5: コミット**

```bash
git add frontend/src/render/UrlPreviewCard.svelte frontend/src/render/UrlPreviewCard.test.ts
git commit -m "feat: UrlPreviewCardコンポーネントを追加"
```

---

### Task 7: `NoteCard.svelte`への組み込みと設定UI

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte`
- Modify: `frontend/src/ui/settings/AppearanceSection.svelte`

**Interfaces:**
- Consumes: `extractPreviewUrls`（Task 4）、`UrlPreviewCard`（Task 6）、`app.ui.urlPreviewEnabled` / `app.ui.summalyProxyUrl`（Task 1のTSバインディング経由）、`app.setUiPrefs`（`lib/store.svelte.ts`既存）

- [ ] **Step 1: `NoteCard.svelte`にインポートを追加する**

`import MediaGrid from "../render/MediaGrid.svelte";`の直後に追加:

```svelte
  import MediaGrid from "../render/MediaGrid.svelte";
  import UrlPreviewCard from "../render/UrlPreviewCard.svelte";
```

`import { app } from "../lib/store.svelte";`の直後に追加:

```svelte
  import { app } from "../lib/store.svelte";
  import { extractPreviewUrls } from "../lib/extractPreviewUrls";
```

- [ ] **Step 2: `MediaGrid`直後にプレビューカードを並べる**

既存の

```svelte
        {#if inner.files.length > 0}
          <MediaGrid files={inner.files} />
        {/if}
```

を、次のように直後に追加する形へ変更:

```svelte
        {#if inner.files.length > 0}
          <MediaGrid files={inner.files} />
        {/if}
        {#if app.ui.urlPreviewEnabled ?? true}
          {#each extractPreviewUrls(inner.text ?? "") as previewUrl (previewUrl)}
            <UrlPreviewCard url={previewUrl} />
          {/each}
        {/if}
```

- [ ] **Step 3: `svelte-check`を実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 4: 既存の`NoteCard.test.ts`が壊れていないことを確認する**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts`
Expected: PASS。もし`commands.fetchUrlPreview`未モックによる例外でテストが失敗する場合、`NoteCard.test.ts`冒頭の`vi.mock("@tauri-apps/api/core", ...)`は既に`invoke: vi.fn()`（未実装スタブ）なので、`lib/urlPreview.ts`の`try/catch`により例外は握りつぶされカードは描画されないだけのはず。テスト失敗時のみ、原因のスタックトレースを見て`lib/urlPreview.ts`側の防御漏れを直す

- [ ] **Step 5: `AppearanceSection.svelte`に設定フィールドを追加する**

`let searchEngineUrl = $state(app.ui.searchEngineUrl ?? DEFAULT_SEARCH_ENGINE_URL);`の直後に追加:

```svelte
  let urlPreviewEnabled = $state(app.ui.urlPreviewEnabled ?? true);
  let summalyProxyUrl = $state(app.ui.summalyProxyUrl ?? "");
```

`save()`内、`await app.setUiPrefs({ ... searchEngineUrl: searchEngineUrl.trim() || DEFAULT_SEARCH_ENGINE_URL, });`を次のように変更:

```svelte
      await app.setUiPrefs({
        ...app.ui,
        theme,
        codeHighlightTheme,
        fontFamily,
        emojiStyle,
        mfmAnimationEnabled,
        searchEngineUrl: searchEngineUrl.trim() || DEFAULT_SEARCH_ENGINE_URL,
        urlPreviewEnabled,
        summalyProxyUrl: summalyProxyUrl.trim(),
      });
```

「MFM検索構文($[search]相当)で使う検索エンジン」の`<div>`ブロック終了直後（`フォント`の`<div>`の直前）に追加:

```svelte
<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <label class="flex items-center gap-2"
    ><input type="checkbox" bind:checked={urlPreviewEnabled} /> 投稿本文中のURLにリンクプレビューを表示する</label
  >
  <span class="text-muted-foreground">カスタムsummalyプロキシURL（任意）</span>
  <input
    type="text"
    class="w-full rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground"
    placeholder="空欄なら接続先インスタンスの /url を使用"
    bind:value={summalyProxyUrl}
  />
  <p class="mb-0 mt-0 text-xs text-muted-foreground">
    設定すると、リンクプレビュー対象のURLは接続先インスタンスではなく指定したプロキシへ直接送信されます。
    信頼できるプロキシのみを指定してください。
  </p>
</div>
```

- [ ] **Step 6: `svelte-check`とVitestを実行する**

Run: `cd frontend && pnpm check && pnpm vitest run src/ui/settings`
Expected: エラーなし・PASS

- [ ] **Step 7: コミット**

```bash
git add frontend/src/ui/NoteCard.svelte frontend/src/ui/settings/AppearanceSection.svelte
git commit -m "feat: ノートにリンクプレビューを表示し設定UIを追加"
```

---

### Task 8: 通しの検証

**Files:** なし（既存ファイルの検証のみ）

**Interfaces:** なし

- [ ] **Step 1: Rust全体テストを実行する**

Run: `cd src-tauri && cargo test`
Expected: PASS

- [ ] **Step 2: フロントエンド型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 3: フロントエンド全体テストを実行する**

Run: `cd frontend && pnpm test`
Expected: PASS

- [ ] **Step 4: `cargo tauri dev`でアプリを起動し、実際のMisskeyアカウントでURL付きノートを投稿またはタイムラインに表示させ、以下を目視確認する**

- 通常のURL: タイトル・説明・サムネイルのカードが表示される
- センシティブ扱いのURL（該当インスタンス・URLがあれば）: ぼかし表示→クリックで開示
- YouTube等プレイヤー付きURL: サムネイル上に再生ボタン→クリックでiframe再生
- 設定画面でリンクプレビューをOFF→カードが消える。再度ON→カードが戻る
- 設定画面でカスタムsummalyプロキシURLを設定→そのプロキシ経由でプレビューが取得される（またはプロキシが無効なら失敗してカードが出ない）ことを確認

確認後、検証用に自分で起動した`cargo tauri dev`のプロセスを終了する。

- [ ] **Step 5: 最終コミット（Step 4で修正が発生した場合のみ）**

```bash
git add -A
git commit -m "fix: 実機確認での修正"
```
