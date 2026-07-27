# コードブロックのシンタックスハイライト（Issue #118） Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MFM の ```` ```lang ```` コードブロックに shiki でシンタックスハイライトを付ける。ハイライトテーマは UI 全体の配色プリセットとは独立に、「auto（OS追従）」「shiki同梱テーマ」「ユーザー作成のカスタムシンタックステーマ」から選べるようにする。

**Architecture:** Rust側 `UiPrefs` に `code_highlight_theme` / `custom_syntax_themes` を追加して永続化する。フロントは `lib/shiki.ts` にシングルトンの shiki ハイライタを持ち、`CodeBlock.svelte` が非同期にハイライトHTMLを生成して `MfmNode.svelte` の `blockCode` 分岐から使う。「auto」はshikiのデュアルテーマ機能（`--shiki-light`/`--shiki-dark` のCSS変数出力）＋ `app.css` の `prefers-color-scheme`/`data-theme` パターンで切り替える。「custom」はshiki組み込みの `css-variables` テーマ（`--shiki-token-*` 出力）＋ `lib/theme.ts` が `<html>` にユーザー配色を反映する形で切り替える。

**Tech Stack:** shiki (`shiki` + `@shikijs/engine-javascript`), Svelte 5, Rust/specta（`UiPrefs`拡張）。

## Global Constraints

- Rust側の型変更は必ず `#[serde(default = "...")]` で後方互換を保つ（既存 `UiPrefs` フィールド追加パターンに倣う。`src-tauri/src/domain/ui.rs`）。
- `src-tauri/src/lib.rs` の `specta_builder()` は型を自動収集するため、`UiPrefs` への型追加だけで良く新規コマンド登録は不要（既存の `get_ui_prefs`/`set_ui_prefs` コマンドがそのまま使える）。
- Rust側の型を変更したら `cd src-tauri && cargo test` を実行し、`frontend/src/bindings/tauri.gen.ts` が再生成されることを確認する（このリポジトリの標準フロー。手で編集しない）。
- WASM を使わない（`@shikijs/engine-javascript` の JS 正規表現エンジンを使う）。
- フロントに専用テストフレームワークは無いため、フロント側タスクの検証は `cd frontend && pnpm check`（型チェック）＋指定した手動確認手順で行う。
- インラインコード（`` `code` ``, `inlineCode`ノード）はスコープ外。従来どおり無地の `.mfm-code` のまま変更しない。

---

## Task 1: shiki 依存追加とAPI確認

**Files:**
- Modify: `frontend/package.json`

**Interfaces:**
- Produces: `shiki` パッケージの `createHighlighter`, `codeToHtml`, `loadLanguage`, `loadTheme` API、および `@shikijs/engine-javascript` の `createJavaScriptRegexEngine`（後続タスクが使う）。

- [ ] **Step 1: パッケージ追加**

```bash
cd frontend && pnpm add shiki @shikijs/engine-javascript
```

- [ ] **Step 2: インストールされたバージョンとAPIを確認する**

```bash
cd frontend && node -e "console.log(require('shiki/package.json').version)"
cd frontend && grep -n "\"exports\"" -A 30 node_modules/shiki/package.json | head -40
cd frontend && grep -rn "css-variables" node_modules/shiki/dist/*.d.ts node_modules/shiki/dist/**/*.d.ts 2>/dev/null | head -20
```

Expected: `createHighlighter`, `codeToHtml` がトップレベルexportに含まれること、`css-variables` という名前の組み込みテーマが存在することを確認する。

- [ ] **Step 3: `css-variables` テーマが出力する実際の変数名を確認する**

```bash
cd frontend && find node_modules/shiki -iname "*css-variables*"
```

見つかったファイル（`.mjs`または`.json`）を読み、出力される `--shiki-token-*` / `--shiki-color-*` の実際の変数名一覧を書き出す。Task 5・Task 8 ではこの一覧を使う（本計画のTask 5/8では `--shiki-token-comment` 等の名称で仮置きしているが、ここで確認した実名に合わせて読み替えること）。

- [ ] **Step 4: デュアルテーマAPIの引数名を確認する**

```bash
cd frontend && grep -n "defaultColor\|themes\b" node_modules/shiki/dist/*.d.ts 2>/dev/null | head -20
```

`codeToHtml(code, { lang, themes: { light, dark }, defaultColor: false })` の形でデュアルテーマ出力ができることを確認する（型定義に無ければ、READMEに記載の実際の使い方に合わせて後続タスクを調整する）。

- [ ] **Step 5: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/package.json frontend/pnpm-lock.yaml
git commit -m "build: shikiを依存に追加"
```

---

## Task 2: Rust側データモデル（CustomSyntaxTheme・UiPrefs拡張）

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`

**Interfaces:**
- Produces: `pub struct CustomSyntaxTheme { id, name, background, text, comment, string, keyword, function, constant, parameter, string_expression, punctuation, link: String }`（すべて `String`、camelCase serde rename）。`UiPrefs.code_highlight_theme: String`（既定 `"auto"`）、`UiPrefs.custom_syntax_themes: Vec<CustomSyntaxTheme>`（既定 空Vec）。

- [ ] **Step 1: 失敗するテストを書く（legacy JSON デシリアライズ）**

`src-tauri/src/domain/ui.rs` の `#[cfg(test)] mod tests` 内、既存 `deserializes_legacy_json_without_new_fields` テストの末尾に追記:

```rust
        assert_eq!(v.code_highlight_theme, "auto");
        assert!(v.custom_syntax_themes.is_empty());
```

- [ ] **Step 2: テストが失敗することを確認**

```bash
cd src-tauri && cargo test deserializes_legacy_json_without_new_fields
```

Expected: FAIL（`code_highlight_theme`/`custom_syntax_themes` フィールドが存在せずコンパイルエラー）

- [ ] **Step 3: `CustomSyntaxTheme` struct と `UiPrefs` フィールドを追加**

`src-tauri/src/domain/ui.rs` の `CustomTheme` struct定義の直後に追加:

```rust
/// ユーザーが作成したカスタムシンタックス（コードハイライト）テーマ。
/// 各フィールドは shiki 組み込み特殊テーマ "css-variables" が出力する
/// `--shiki-token-*` 系トークンに1対1で対応する（Issue #118）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomSyntaxTheme {
    pub id: String,
    pub name: String,
    pub background: String,
    pub text: String,
    pub comment: String,
    pub string: String,
    pub keyword: String,
    pub function: String,
    pub constant: String,
    pub parameter: String,
    pub string_expression: String,
    pub punctuation: String,
    pub link: String,
}
```

`UiPrefs` struct内、`pub custom_themes: Vec<CustomTheme>,` フィールドの直後に追加:

```rust
    /// コードハイライトのテーマ選択。"auto"（OSのlight/darkに追従） |
    /// "shiki:<bundled-theme-id>"（shiki同梱テーマ） | "custom:<CustomSyntaxTheme.id>"。
    /// UI全体の配色（`theme`フィールド）とは独立（Issue #118）。
    #[serde(default = "default_code_highlight_theme")]
    pub code_highlight_theme: String,
    /// ユーザーが作成したカスタムシンタックステーマの一覧。
    #[serde(default)]
    pub custom_syntax_themes: Vec<CustomSyntaxTheme>,
```

ファイル末尾付近、既存の `default_*` 関数群の並びに追加:

```rust
fn default_code_highlight_theme() -> String {
    "auto".into()
}
```

`impl Default for UiPrefs` のフィールド列挙に追加:

```rust
            code_highlight_theme: default_code_highlight_theme(),
            custom_syntax_themes: Vec::new(),
```

- [ ] **Step 4: テストが通ることを確認**

```bash
cd src-tauri && cargo test deserializes_legacy_json_without_new_fields
```

Expected: PASS

- [ ] **Step 5: roundtrip テストを書いて実行**

既存 `roundtrips_keymap` テストの `UiPrefs { ... }` リテラルに以下を追加（末尾 `enable_file_logging: true,` の後）:

```rust
            code_highlight_theme: "shiki:github-dark".into(),
            custom_syntax_themes: vec![CustomSyntaxTheme {
                id: "s1".into(),
                name: "My Syntax Theme".into(),
                background: "#1e1e1e".into(),
                text: "#d4d4d4".into(),
                comment: "#6a9955".into(),
                string: "#ce9178".into(),
                keyword: "#569cd6".into(),
                function: "#dcdcaa".into(),
                constant: "#b5cea8".into(),
                parameter: "#9cdcfe".into(),
                string_expression: "#d7ba7d".into(),
                punctuation: "#d4d4d4".into(),
                link: "#569cd6".into(),
            }],
```

```bash
cd src-tauri && cargo test roundtrips_keymap
```

Expected: PASS（シリアライズ→デシリアライズで一致）

- [ ] **Step 6: フルテスト実行とTSバインディング再生成の確認**

```bash
cd src-tauri && cargo test
```

Expected: 全テストPASS。`generates_frontend_bindings` テストにより `frontend/src/bindings/tauri.gen.ts` が更新され、`CustomSyntaxTheme` / `codeHighlightTheme` / `customSyntaxThemes` が camelCase で生成されていることを、次のコマンドで確認する:

```bash
grep -n "CustomSyntaxTheme\|codeHighlightTheme\|customSyntaxThemes" frontend/src/bindings/tauri.gen.ts
```

Expected: 3つとも見つかる。

- [ ] **Step 7: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/domain/ui.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: UiPrefsにコードハイライト設定を追加"
```

---

## Task 3: `lib/shiki.ts` — ハイライタとハイライト関数

**Files:**
- Create: `frontend/src/lib/shiki.ts`

**Interfaces:**
- Consumes: `shiki`（Task 1）、`bindings/tauri.gen.ts` の `CustomSyntaxTheme`（Task 2）。
- Produces:
  - `export const BUNDLED_LANGS: string[]` — 同梱言語ID一覧。
  - `export async function highlightCode(code: string, lang: string | null, themeSelection: string, customSyntaxThemes: CustomSyntaxTheme[]): Promise<string>` — ハイライト済みHTML文字列（またはプレーンフォールバックHTML）を返す。後続タスク（`CodeBlock.svelte`）が呼ぶ。

- [ ] **Step 1: ファイルを作成**

```typescript
// shiki によるコードブロックのシンタックスハイライト（Issue #118）。
// ハイライタはモジュールスカラーのシングルトンとして遅延生成する
// （マルチカラムで多数のノートが同時描画されるため、1インスタンスを使い回す）。
import type { HighlighterGeneric } from "shiki/core";
import { createJavaScriptRegexEngine } from "@shikijs/engine-javascript";
import type { CustomSyntaxTheme } from "../bindings/tauri.gen";

export const BUNDLED_LANGS = [
  "typescript",
  "tsx",
  "javascript",
  "jsx",
  "rust",
  "bash",
  "json",
  "yaml",
  "toml",
  "python",
  "go",
  "sql",
  "markdown",
  "html",
  "css",
] as const;

const AUTO_LIGHT_THEME = "github-light";
const AUTO_DARK_THEME = "github-dark";
const CSS_VARIABLES_THEME = "css-variables";

let highlighterPromise: Promise<HighlighterGeneric<any, any>> | null = null;

function getHighlighter() {
  if (!highlighterPromise) {
    highlighterPromise = import("shiki").then(({ createHighlighter }) =>
      createHighlighter({
        themes: [AUTO_LIGHT_THEME, AUTO_DARK_THEME, CSS_VARIABLES_THEME],
        langs: [],
        engine: createJavaScriptRegexEngine(),
      }),
    );
  }
  return highlighterPromise;
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function plainHtml(code: string): string {
  return `<pre class="shiki-plain"><code>${escapeHtml(code)}</code></pre>`;
}

/// customSyntaxThemes から id を引いて --shiki-token-* を <html> に反映するのは
/// lib/theme.ts の applySyntaxTheme() の責務。ここではハイライト結果HTMLの生成のみ行う。
export async function highlightCode(
  code: string,
  lang: string | null,
  themeSelection: string,
  customSyntaxThemes: CustomSyntaxTheme[],
): Promise<string> {
  const normalizedLang = (lang ?? "").toLowerCase();
  if (!BUNDLED_LANGS.includes(normalizedLang as (typeof BUNDLED_LANGS)[number])) {
    return plainHtml(code);
  }
  try {
    const highlighter = await getHighlighter();
    const loadedLangs = highlighter.getLoadedLanguages();
    if (!loadedLangs.includes(normalizedLang)) {
      await highlighter.loadLanguage(normalizedLang as any);
    }

    if (themeSelection === "auto") {
      return highlighter.codeToHtml(code, {
        lang: normalizedLang,
        themes: { light: AUTO_LIGHT_THEME, dark: AUTO_DARK_THEME },
        defaultColor: false,
      });
    }
    if (themeSelection.startsWith("custom:")) {
      return highlighter.codeToHtml(code, { lang: normalizedLang, theme: CSS_VARIABLES_THEME });
    }
    if (themeSelection.startsWith("shiki:")) {
      const themeId = themeSelection.slice("shiki:".length);
      const loadedThemes = highlighter.getLoadedThemes();
      if (!loadedThemes.includes(themeId)) {
        await highlighter.loadTheme(themeId as any);
      }
      return highlighter.codeToHtml(code, { lang: normalizedLang, theme: themeId });
    }
    // 未知の themeSelection 値（旧データ等）は auto 相当にフォールバック
    return highlighter.codeToHtml(code, {
      lang: normalizedLang,
      themes: { light: AUTO_LIGHT_THEME, dark: AUTO_DARK_THEME },
      defaultColor: false,
    });
  } catch {
    return plainHtml(code);
  }
}
```

- [ ] **Step 2: 型チェック**

```bash
cd frontend && pnpm check
```

Expected: `lib/shiki.ts` に関するエラーが無いこと。Task 1 Step 3 で確認した実際の型名（`HighlighterGeneric`のエクスポート元パス等）と食い違うエラーが出た場合は、そこで確認した実名に合わせて import を修正する。

- [ ] **Step 3: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/shiki.ts
git commit -m "feat: shikiハイライタのシングルトンとhighlightCodeを追加"
```

---

## Task 4: `lib/theme.ts` — カスタムシンタックステーマのCSS変数反映

**Files:**
- Modify: `frontend/src/lib/theme.ts`

**Interfaces:**
- Consumes: `CustomSyntaxTheme`（Task 2）。
- Produces: `export const SYNTAX_VAR_KEYS: { css: string; key: keyof CustomSyntaxTheme }[]`、`export function applySyntaxColors(colors: CustomSyntaxTheme | null): void`。後続の `store.svelte.ts`（Task 5）が呼ぶ。

- [ ] **Step 1: `theme.ts` に追記**

`frontend/src/lib/theme.ts` の import に追加:

```typescript
import type { ThemeColors, CustomSyntaxTheme } from "../bindings/tauri.gen";
```

（既存が `import type { ThemeColors } from "../bindings/tauri.gen";` の場合は上記のように `CustomSyntaxTheme` を追加する形に書き換える。）

`THEME_VAR_KEYS` 定義の直後に追加:

```typescript
// CSS変数名 <-> CustomSyntaxTheme のフィールド名対応。
// shiki組み込み特殊テーマ "css-variables" が出力する --shiki-token-* / --shiki-color-*
// と一致させる必要がある（実際の変数名は Task 1 Step 3 で確認したものに合わせること）。
export const SYNTAX_VAR_KEYS: { css: string; key: keyof CustomSyntaxTheme }[] = [
  { css: "--shiki-color-background", key: "background" },
  { css: "--shiki-color-text", key: "text" },
  { css: "--shiki-token-comment", key: "comment" },
  { css: "--shiki-token-string", key: "string" },
  { css: "--shiki-token-keyword", key: "keyword" },
  { css: "--shiki-token-function", key: "function" },
  { css: "--shiki-token-constant", key: "constant" },
  { css: "--shiki-token-parameter", key: "parameter" },
  { css: "--shiki-token-string-expression", key: "stringExpression" },
  { css: "--shiki-token-punctuation", key: "punctuation" },
  { css: "--shiki-token-link", key: "link" },
];
```

`applyThemeColors` 関数の直後に追加:

```typescript
/// <html> にカスタムシンタックステーマの配色を反映する。
/// null なら inline指定を全解除する（"auto"/"shiki:<id>" 選択時はshikiが実色をベタ書きするため不要）。
export function applySyntaxColors(colors: CustomSyntaxTheme | null) {
  const root = document.documentElement;
  for (const { css, key } of SYNTAX_VAR_KEYS) {
    const value = colors?.[key];
    if (value) root.style.setProperty(css, value);
    else root.style.removeProperty(css);
  }
}
```

- [ ] **Step 2: 型チェック**

```bash
cd frontend && pnpm check
```

Expected: エラー無し。

- [ ] **Step 3: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/theme.ts
git commit -m "feat: カスタムシンタックステーマのCSS変数反映を追加"
```

---

## Task 5: `store.svelte.ts` — 設定の読み込み・保存・適用配線

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`

**Interfaces:**
- Consumes: `applySyntaxColors`（Task 4）、`UiPrefs.codeHighlightTheme` / `UiPrefs.customSyntaxThemes`（Task 2）。
- Produces: `app.ui.codeHighlightTheme: string`, `app.ui.customSyntaxThemes: CustomSyntaxTheme[]` が起動時・設定変更時に常に埋まっていることを後続タスク（`CodeBlock.svelte`）が前提にできる。

- [ ] **Step 1: import追記**

`frontend/src/lib/store.svelte.ts` の該当行を書き換え:

```typescript
import { applyThemeColors, applySyntaxColors, findPreset, parseThemeRef } from "./theme";
```

- [ ] **Step 2: `boot()` 内のデフォルト埋め処理に追加**

`boot()` 内、`this.ui = { ...ui, ... }` の中、`customThemes: ui.customThemes ?? [],` の直後に追加:

```typescript
        codeHighlightTheme: ui.codeHighlightTheme ?? "auto",
        customSyntaxThemes: ui.customSyntaxThemes ?? [],
```

同じ `boot()` 内、`this.#applyTheme(this.ui.theme);` の直後に追加:

```typescript
      this.#applySyntaxTheme(this.ui.codeHighlightTheme, this.ui.customSyntaxThemes);
```

- [ ] **Step 3: `setUiPrefs()` にも同様に追加**

`setUiPrefs()` 内、`this.ui = { ...prefs, ... }` の中、`customThemes: prefs.customThemes ?? [],` の直後に追加:

```typescript
      codeHighlightTheme: prefs.codeHighlightTheme ?? "auto",
      customSyntaxThemes: prefs.customSyntaxThemes ?? [],
```

`this.#applyTheme(prefs.theme);` の直後に追加:

```typescript
    this.#applySyntaxTheme(prefs.codeHighlightTheme, this.ui.customSyntaxThemes);
```

- [ ] **Step 4: `#applySyntaxTheme` メソッドを追加**

`#applyTheme` メソッドの直後に追加:

```typescript
  /// codeHighlightTheme("auto"/"shiki:<id>"/"custom:<id>")のうち "custom:<id>" の場合のみ
  /// 対応する CustomSyntaxTheme の配色を <html> に反映する。それ以外は解除する
  /// （shikiが実色をベタ書き、または auto はデュアルテーマCSSで切り替わるため）。
  #applySyntaxTheme(codeHighlightTheme: string, customSyntaxThemes: CustomSyntaxTheme[]) {
    const customId = parseThemeRef(codeHighlightTheme, "custom:");
    if (customId) {
      const found = customSyntaxThemes.find((t) => t.id === customId);
      if (found) {
        applySyntaxColors(found);
        return;
      }
      // 選択中のカスタムシンタックステーマが削除済み: autoにフォールバックして保存し直す
      applySyntaxColors(null);
      void this.setUiPrefs({ ...this.ui, codeHighlightTheme: "auto" });
      return;
    }
    applySyntaxColors(null);
  }
```

`CustomSyntaxTheme` 型を type import に追加（`UiPrefs,` の直後）:

```typescript
  UiPrefs,
  CustomSyntaxTheme,
```

- [ ] **Step 5: 型チェック**

```bash
cd frontend && pnpm check
```

Expected: エラー無し（`parseThemeRef` は `"preset:" | "custom:"` の合併型引数なので `#applyTheme` と同じ呼び方で通ることを確認）。

- [ ] **Step 6: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/store.svelte.ts
git commit -m "feat: storeにコードハイライト設定の読み込み・適用を配線"
```

---

## Task 6: `app.css` — 既定変数と auto テーマ切替CSS

**Files:**
- Modify: `frontend/src/app.css`

**Interfaces:**
- Consumes: なし。
- Produces: `.mfm-codeblock` 内の shiki 出力（`.shiki` / `.shiki-plain`）に対する見た目。後続の `CodeBlock.svelte`（Task 7）が生成するHTMLがこのCSSに依存する。

- [ ] **Step 1: `.mfm-codeblock` ルールを書き換え、直後にshiki用CSSを追加**

`frontend/src/app.css` の既存

```css
.mfm-codeblock {
  background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  border-radius: 8px;
  padding: 10px;
  overflow-x: auto;
}
```

を、以下に置き換える:

```css
.mfm-codeblock {
  background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  border-radius: 8px;
  padding: 10px;
  overflow-x: auto;
}
.mfm-codeblock pre {
  margin: 0;
  background: transparent !important;
  font-family: ui-monospace, monospace;
  font-size: 0.9em;
}
.mfm-codeblock .shiki-plain code {
  color: var(--text);
}

/* ---- shiki "auto"（OS追従）テーマのライト/ダーク切替。
   app.css 冒頭の :root / prefers-color-scheme / [data-theme] パターンと同じ条件で、
   shikiのデュアルテーマ出力（--shiki-light / --shiki-dark）のどちらを使うか決める。 */
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) .mfm-codeblock .shiki,
  :root:not([data-theme="light"]) .mfm-codeblock .shiki span {
    color: var(--shiki-dark);
    background-color: var(--shiki-dark-bg) !important;
  }
}
:root[data-theme="dark"] .mfm-codeblock .shiki,
:root[data-theme="dark"] .mfm-codeblock .shiki span {
  color: var(--shiki-dark);
  background-color: var(--shiki-dark-bg) !important;
}
```

- [ ] **Step 2: 動作確認は Task 9（`CodeBlock.svelte` 組み込み後）の手動確認でまとめて行う**

このタスク単体ではビルド確認のみ:

```bash
cd frontend && pnpm check
```

Expected: エラー無し（CSSファイルなので型チェック自体には影響しないが、他ファイルを壊していないことを確認する）。

- [ ] **Step 3: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/app.css
git commit -m "style: コードブロックのshikiテーマ切替CSSを追加"
```

---

## Task 7: `CodeBlock.svelte` — 非同期ハイライト表示コンポーネント

**Files:**
- Create: `frontend/src/render/CodeBlock.svelte`

**Interfaces:**
- Consumes: `highlightCode`（Task 3）、`app.ui.codeHighlightTheme` / `app.ui.customSyntaxThemes`（Task 5）。
- Produces: `<CodeBlock code={string} lang={string | null} />`。Task 8（`MfmNode.svelte`）が使う。

- [ ] **Step 1: ファイルを作成**

```svelte
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { highlightCode } from "../lib/shiki";

  let { code, lang }: { code: string; lang: string | null } = $props();

  let html = $state<string | null>(null);

  $effect(() => {
    const currentCode = code;
    const currentLang = lang;
    const themeSelection = app.ui.codeHighlightTheme ?? "auto";
    const customSyntaxThemes = app.ui.customSyntaxThemes ?? [];
    let cancelled = false;
    highlightCode(currentCode, currentLang, themeSelection, customSyntaxThemes).then((result) => {
      if (!cancelled) html = result;
    });
    return () => {
      cancelled = true;
    };
  });
</script>

<div class="mfm-codeblock">
  {#if html}
    {@html html}
  {:else}
    <pre class="shiki-plain"><code>{code}</code></pre>
  {/if}
</div>
```

- [ ] **Step 2: 型チェック**

```bash
cd frontend && pnpm check
```

Expected: エラー無し。

- [ ] **Step 3: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/render/CodeBlock.svelte
git commit -m "feat: CodeBlock.svelteを追加"
```

---

## Task 8: `MfmNode.svelte` — blockCode の描画差し替え

**Files:**
- Modify: `frontend/src/render/MfmNode.svelte`

**Interfaces:**
- Consumes: `CodeBlock.svelte`（Task 7）。

- [ ] **Step 1: import追加**

`frontend/src/render/MfmNode.svelte` 冒頭の import群に追加:

```typescript
  import CodeBlock from "./CodeBlock.svelte";
```

- [ ] **Step 2: blockCode分岐を置き換え**

既存の

```svelte
{:else if node.type === "blockCode"}
  <pre class="mfm-codeblock"><code>{p.code}</code></pre>
```

を、以下に置き換える:

```svelte
{:else if node.type === "blockCode"}
  <CodeBlock code={p.code} lang={p.lang ?? null} />
```

- [ ] **Step 3: 型チェック**

```bash
cd frontend && pnpm check
```

Expected: エラー無し。

- [ ] **Step 4: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/render/MfmNode.svelte
git commit -m "feat: blockCodeノードをCodeBlock.svelteで描画するよう変更"
```

---

## Task 9: 手動確認（同梱言語・未対応言語・autoの明暗切替）

**Files:** なし（動作確認のみ）

- [ ] **Step 1: アプリを起動**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && cargo tauri dev
```

- [ ] **Step 2: 同梱言語のハイライトを確認**

投稿欄で以下を投稿し、タイムライン上でキーワード・文字列・コメントが色分けされることを確認する:

````
```rust
fn main() {
    // comment
    let x: i32 = 42;
    println!("hello {}", x);
}
```
````

- [ ] **Step 3: 未対応言語・言語未指定のフォールバックを確認**

````
```brainfuck
++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.
```
````

および

````
```
plain code block
```
````

を投稿し、どちらも色分け無しのプレーン表示になることを確認する（エラーやクラッシュが起きないこと）。

- [ ] **Step 4: auto（既定）のOS追従を確認**

OS側のダーク/ライト設定を切り替え、コードブロックの背景・文字色がアプリ全体のテーマ（`prefers-color-scheme`）と連動して変わることを確認する。

- [ ] **Step 5: `pnpm check` の最終確認**

```bash
cd frontend && pnpm check
```

Expected: エラー無し。

（コミット無し。次タスクの設定UI実装後、まとめて動作確認する。）

---

## Task 10: 設定UI — コードハイライトテーマの選択

**Files:**
- Modify: `frontend/src/ui/settings/DisplaySection.svelte`

**Interfaces:**
- Consumes: `app.ui.codeHighlightTheme`, `app.ui.customSyntaxThemes`（Task 5）。

- [ ] **Step 1: importとstateを追加**

`frontend/src/ui/settings/DisplaySection.svelte` の import群に追加:

```typescript
  import type { CustomSyntaxTheme } from "../../bindings/tauri.gen";
  import { SYNTAX_VAR_KEYS } from "../../lib/theme";
  import { BUNDLED_SHIKI_THEMES } from "../../lib/shikiThemeList";
```

`theme` state宣言の直後に追加:

```typescript
  let codeHighlightTheme = $state(app.ui.codeHighlightTheme ?? "auto");
```

- [ ] **Step 2: shiki同梱テーマ一覧を定義するファイルを作成**

新規ファイル `frontend/src/lib/shikiThemeList.ts`:

```typescript
// 設定UIのドロップダウンに出す shiki 同梱テーマの一覧（表示名 + shikiのテーマID）。
// Task 1 で確認した shiki のバンドルテーマ一覧から抜粋。
export const BUNDLED_SHIKI_THEMES: { id: string; label: string }[] = [
  { id: "github-dark", label: "GitHub Dark" },
  { id: "github-light", label: "GitHub Light" },
  { id: "dracula", label: "Dracula" },
  { id: "nord", label: "Nord" },
  { id: "one-dark-pro", label: "One Dark Pro" },
  { id: "monokai", label: "Monokai" },
  { id: "solarized-dark", label: "Solarized Dark" },
  { id: "solarized-light", label: "Solarized Light" },
  { id: "tokyo-night", label: "Tokyo Night" },
  { id: "catppuccin-mocha", label: "Catppuccin Mocha" },
  { id: "catppuccin-latte", label: "Catppuccin Latte" },
  { id: "min-dark", label: "Min Dark" },
  { id: "min-light", label: "Min Light" },
];
```

- [ ] **Step 3: カスタムシンタックステーマ編集用のstate・関数を追加**

`customThemes` derived の直後に追加:

```typescript
  // ---- カスタムシンタックステーマ ----
  const customSyntaxThemes = $derived(app.ui.customSyntaxThemes ?? []);
  const syntaxColorLabels: Record<keyof CustomSyntaxTheme, string> = {
    id: "id",
    name: "名前",
    background: "背景",
    text: "文字（既定）",
    comment: "コメント",
    string: "文字列",
    keyword: "キーワード",
    function: "関数",
    constant: "定数・数値",
    parameter: "引数",
    stringExpression: "文字列内の式展開",
    punctuation: "記号",
    link: "リンク",
  };
  function blankSyntaxColors(): CustomSyntaxTheme {
    return {
      id: crypto.randomUUID(),
      name: "",
      background: "#1e1e1e",
      text: "#d4d4d4",
      comment: "#6a9955",
      string: "#ce9178",
      keyword: "#569cd6",
      function: "#dcdcaa",
      constant: "#b5cea8",
      parameter: "#9cdcfe",
      stringExpression: "#d7ba7d",
      punctuation: "#d4d4d4",
      link: "#569cd6",
    };
  }
  let editingSyntaxTheme = $state<CustomSyntaxTheme | null>(null);
  let syntaxEditErr = $state<string | null>(null);

  function startCreateSyntaxTheme() {
    editingSyntaxTheme = blankSyntaxColors();
    syntaxEditErr = null;
  }
  function startEditSyntaxTheme(t: CustomSyntaxTheme) {
    editingSyntaxTheme = { ...t };
    syntaxEditErr = null;
  }
  function cancelEditSyntaxTheme() {
    editingSyntaxTheme = null;
    syntaxEditErr = null;
  }
  async function saveCustomSyntaxTheme() {
    if (!editingSyntaxTheme) return;
    if (!editingSyntaxTheme.name.trim()) {
      syntaxEditErr = "名前を入力してください";
      return;
    }
    for (const { key } of SYNTAX_VAR_KEYS) {
      if (!HEX_RE.test(editingSyntaxTheme[key] ?? "")) {
        syntaxEditErr = `${syntaxColorLabels[key]}は #rrggbb 形式で入力してください`;
        return;
      }
    }
    const exists = customSyntaxThemes.some((t) => t.id === editingSyntaxTheme!.id);
    const next = exists
      ? customSyntaxThemes.map((t) => (t.id === editingSyntaxTheme!.id ? editingSyntaxTheme! : t))
      : [...customSyntaxThemes, editingSyntaxTheme];
    await app.setUiPrefs({ ...app.ui, customSyntaxThemes: next });
    editingSyntaxTheme = null;
    syntaxEditErr = null;
  }
  async function removeCustomSyntaxTheme(id: string) {
    const next = customSyntaxThemes.filter((t) => t.id !== id);
    const clearing = codeHighlightTheme === `custom:${id}`;
    await app.setUiPrefs({
      ...app.ui,
      customSyntaxThemes: next,
      codeHighlightTheme: clearing ? "auto" : app.ui.codeHighlightTheme,
    });
    if (clearing) codeHighlightTheme = "auto";
  }
```

- [ ] **Step 4: `save()` 内で `codeHighlightTheme` も保存対象に追加**

`save()` 内の `await app.setUiPrefs({ ...app.ui, theme, ... })` 呼び出しに `codeHighlightTheme,` を追加する。

- [ ] **Step 5: マークアップ追加**

`</div>`（プリセットテーマの`field`の閉じタグ、カスタムテーマ`field`の直前）の後、カスタムテーマの `field` の直後に新セクションを追加:

```svelte
<div class="field">
  <span>コードハイライト</span>
  <select bind:value={codeHighlightTheme}>
    <option value="auto">自動（OSに合わせる）</option>
    <optgroup label="同梱テーマ">
      {#each BUNDLED_SHIKI_THEMES as t (t.id)}
        <option value={`shiki:${t.id}`}>{t.label}</option>
      {/each}
    </optgroup>
    {#if customSyntaxThemes.length > 0}
      <optgroup label="カスタムテーマ">
        {#each customSyntaxThemes as t (t.id)}
          <option value={`custom:${t.id}`}>{t.name}</option>
        {/each}
      </optgroup>
    {/if}
  </select>
</div>

<div class="field">
  <span>カスタムシンタックステーマ</span>
  <div class="theme-grid">
    {#each customSyntaxThemes as t (t.id)}
      <div class="theme-card-wrap">
        <span class="theme-card">
          <span class="swatch-strip">
            {#each SYNTAX_VAR_KEYS as v (v.key)}
              <span class="sw" style={`background:${t[v.key]}`}></span>
            {/each}
          </span>
          <span class="theme-card-name">{t.name}</span>
        </span>
        <div class="theme-card-actions">
          <button class="icon-btn" title="編集" onclick={() => startEditSyntaxTheme(t)}><Pencil size={13} /></button>
          <button class="icon-btn" title="削除" onclick={() => removeCustomSyntaxTheme(t.id)}><Trash2 size={13} /></button>
        </div>
      </div>
    {/each}
  </div>
  <button class="mini-btn add-theme" onclick={startCreateSyntaxTheme}><Plus size={13} /> 新規作成</button>

  {#if editingSyntaxTheme}
    <div class="theme-editor">
      <input type="text" class="theme-name-input" placeholder="テーマ名" bind:value={editingSyntaxTheme.name} />
      {#each SYNTAX_VAR_KEYS as v (v.key)}
        <div class="color-row">
          <span class="color-label">{syntaxColorLabels[v.key]}</span>
          <span class="swatch" style={`background:${editingSyntaxTheme[v.key]}`}></span>
          <input type="text" class="hex-input" bind:value={editingSyntaxTheme[v.key]} />
        </div>
      {/each}
      {#if syntaxEditErr}<p class="err">{syntaxEditErr}</p>{/if}
      <div class="editor-actions">
        <button class="mini-btn" onclick={cancelEditSyntaxTheme}><X size={13} /> キャンセル</button>
        <button class="save" onclick={saveCustomSyntaxTheme}>このテーマを保存</button>
      </div>
    </div>
  {/if}
</div>
```

- [ ] **Step 6: 型チェック**

```bash
cd frontend && pnpm check
```

Expected: エラー無し。既存の `.field` / `.theme-grid` / `.theme-card` 等のCSSクラスは同ファイル内の `<style>` に既に定義済みなので追加CSSは不要（無ければこのステップのエラーで判明するので、その場合のみ既存クラス定義をコピーして追加する）。

- [ ] **Step 7: 手動確認**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && cargo tauri dev
```

設定画面で「コードハイライト」のドロップダウンから同梱テーマ（例: Dracula）を選び保存、Task 9 で投稿した ```` ```rust ```` ブロックの色が変わることを確認する。次に「新規作成」でカスタムシンタックステーマを1つ作り、選択して保存、その配色が反映されることを確認する。作成したテーマを削除し、選択中だった場合に `auto` へ自動的に戻ることを確認する。

- [ ] **Step 8: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/settings/DisplaySection.svelte frontend/src/lib/shikiThemeList.ts
git commit -m "feat: 設定画面にコードハイライトテーマの選択UIを追加"
```

---

## Task 11: 最終確認

**Files:** なし

- [ ] **Step 1: Rust側フルテスト**

```bash
cd src-tauri && cargo test
```

Expected: 全PASS。

- [ ] **Step 2: フロント型チェック**

```bash
cd frontend && pnpm check
```

Expected: エラー無し。

- [ ] **Step 3: Issue #118 のクローズ用にPRを作成する準備**

`git log --oneline main..HEAD` で本ブランチのコミット一覧を確認し、`superpowers:finishing-a-development-branch` スキルの案内に従ってPR作成に進む（[[feedback-pr-closing-keyword]] のとおり、PR本文に `Fixes #118` を含めること）。
