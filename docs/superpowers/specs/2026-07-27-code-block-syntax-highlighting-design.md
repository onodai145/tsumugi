# コードブロックのシンタックスハイライト（Issue #118）

## 背景

現在 `MfmNode.svelte` の `blockCode` ノードは `<pre class="mfm-codeblock"><code>{p.code}</code></pre>` とプレーンテキストのまま描画されており、言語ごとの色分けが無い。mfm-js の `blockCode` ノードは ```` ```lang ```` のフェンス言語を `props.lang` として渡してくるが未使用。

## 方針

[shiki](https://shiki.style/) を使ってハイライトする。ハイライトテーマは、アプリのUI配色プリセット（`ThemeColors`）には追従させず、**独立した設定**として扱う。

- 通常はshiki同梱テーマ（github-dark 等、shikiが標準で持つエディタテーマ集）から選ぶ。
- それ以外の配色が欲しい場合は、ユーザーが独自の「カスタムシンタックステーマ」を作成できる。

WASM初期化を避けるため `@shikijs/engine-javascript`（正規表現ベースのJSエンジン）を使う。

## データモデル（Rust側: `src-tauri/src/domain/ui.rs`）

`UiPrefs` に以下を追加する。既存の `theme` / `custom_themes`（UI配色）とは完全に独立させ、`#[serde(default)]` で後方互換を確保する。

```rust
pub struct CustomSyntaxTheme {
    pub id: String,
    pub name: String,
    pub background: String,
    pub text: String,              // 既定の文字色
    pub comment: String,
    pub string: String,
    pub keyword: String,
    pub function: String,
    pub constant: String,          // 数値・定数
    pub parameter: String,         // 関数引数
    pub string_expression: String, // テンプレートリテラル補間など
    pub punctuation: String,
    pub link: String,              // Markdown内リンク等
}
```

```rust
// UiPrefs に追加
/// "shiki:<bundled-theme-id>" | "custom:<CustomSyntaxTheme.id>"
#[serde(default = "default_code_highlight_theme")]
pub code_highlight_theme: String,
#[serde(default)]
pub custom_syntax_themes: Vec<CustomSyntaxTheme>,
```

- 既定値: `"shiki:github-dark"`。
- 11項目（background/text + 9トークン色）は shiki 組み込みの特殊テーマ `css-variables` が実際に出力する変数集合と一致させる。この変数名はこちらで自由に決められるものではないため、実装時に `node_modules/shiki` 内の `css-variables` テーマ定義を確認し、変数名・項目数を確定させる（設計時点では未検証）。
- `ThemeColors` / `CustomTheme` と同様、legacy JSON（フィールド追加前に保存された設定）を読める後方互換テストを追加する。

## フロントエンド構成

```
frontend/src/lib/shiki.ts (新規)
  - createHighlighter() の遅延シングルトン（@shikijs/engine-javascript、WASM不使用）
  - 同梱言語: ts, tsx, js, jsx, rust, bash, json, yaml, toml, python, go, sql, markdown, html, css
  - highlightCode(code, lang, themeSelection): Promise<string>
    - lang が同梱言語に無い/未指定 → escapeHtmlしたプレーンHTMLを返す（ハイライト無し）
    - themeSelection が "shiki:<id>" → highlighter.loadTheme(id) を遅延ロードして codeToHtml()（実色ベタ書き）
    - themeSelection が "custom:<id>" → 組み込み特殊テーマ 'css-variables' で codeToHtml()
      （出力は --shiki-token-* の var() 参照。実色は lib/theme.ts が <html> に設定する）
    - codeToHtml が例外を投げた場合も try/catch でプレーンHTMLにフォールバックする

frontend/src/render/CodeBlock.svelte (新規)
  - props: code: string, lang: string | null
  - app.ui.codeHighlightTheme / customSyntaxThemes を購読
  - $effect で非同期ハイライトを実行し、結果を $state に保持
  - ロード完了までは現状と同じ <pre class="mfm-codeblock"><code>{code}</code></pre> を表示（ノート描画をブロックしない）
  - 完了後は {@html html} に差し替える（.mfm-codeblock クラスは維持し、既存の背景/padding/overflow-xを継承）

frontend/src/render/MfmNode.svelte
  - blockCode ノードの分岐を <CodeBlock code={p.code} lang={p.lang} /> に置き換える
```

`frontend/src/lib/theme.ts` に `applySyntaxTheme(prefs: UiPrefs)` を追加する（既存 `applyTheme` と同じパターン）:

- `codeHighlightTheme` が `"custom:<id>"` のとき、対応する `CustomSyntaxTheme` の11色を `--shiki-token-*` として `<html>` に設定する。
- `"shiki:<id>"` のときはこれらの変数を解除する（shikiが実色をベタ書きするため不要）。

## 設定UI（`frontend/src/ui/settings/DisplaySection.svelte`）

既存のカスタムUIテーマ編集UIと同じ操作感で、隣接する「コードハイライト」小セクションを追加する。

- ドロップダウン: shiki同梱テーマ一覧（検索可能） + 区切り + 作成済みカスタムシンタックステーマ
- 「新規カスタムテーマを作成」ボタン → 11色分のカラーピッカーを持つフォーム（background/text/comment を上段、残り8トークン色をグリッド表示）。保存・編集・削除は既存 `customThemes` の実装（`startEditTheme` / `saveCustomTheme` / `removeCustomTheme` 相当）に倣う。

## スコープ外

- インラインコード（`inlineCode` ノード、`` `code` ``）のハイライトは対象外。言語情報が無いため従来どおり無地の `<code class="mfm-code">` のまま。
- UI全体の配色プリセット（`ThemeColors`）とコードハイライトテーマの連動は行わない（今回の方針として明示的に分離）。

## テスト方針

- Rust: `CustomSyntaxTheme` / `code_highlight_theme` / `custom_syntax_themes` の legacy JSON デシリアライズテスト、roundtrip テストを既存パターン（`ui.rs` の `#[cfg(test)]`）に倣って追加する（`cargo test`）。
- フロントエンド: 型チェックは `pnpm check`。専用テストフレームワークは無いため、`cargo tauri dev` で以下を手動確認する。
  - 同梱言語（例: rust, ts）のコードブロックが色分けされること
  - 未対応言語・言語未指定のコードブロックがプレーン表示のままであること
  - 設定でshiki同梱テーマを切り替えると表示中のコードブロックの色が変わること
  - カスタムシンタックステーマを作成・選択すると、その配色が反映されること
