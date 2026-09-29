# カスタムCSS（Issue #93）

## 背景・目的

既存のカスタムテーマは配色CSS変数（`--surface-*` など）とシンタックス配色しか差し替えられない。
余白・フォント・角丸・特定要素の非表示といった見た目の調整をしたいユーザー向けに、任意のCSSを書いてアプリ全体へ適用できるようにする。

## スコープ

- 粒度: アプリ全体に効く単一のCSSテキスト1つ。
- 保存: `UiPrefs` の1フィールド。既存の UiPrefs 保存経路（`get_ui_prefs` / `set_ui_prefs`）に乗る。

### 範囲外

- カラム別・アカウント別のCSS切替。
- 外部CSSファイルの読み込み・ホットリロード。
- CSSの検証・サニタイズ、`@import` / `url()` の制限（ローカルの信頼できる単一ユーザーのアプリで、ユーザー自身のCSSを実行するため。CSP は現状 `null` のまま変更しない）。
- Androidでの復旧手段（下記「既知の制限」）。

## 設計

### 設定値（Rust）

`UiPrefs`（`src-tauri/src/domain/ui.rs`）に `custom_css: String` を追加する（camelCase で `customCss`、`#[serde(default)]` で既定は空文字）。
`Default` 実装にも追加する。`customCss` を持たない既存の設定JSONは空文字として読める。

### セーフモード判定（Rust）

`commands/app.rs` に `is_safe_mode() -> bool` を追加し、`lib.rs` の `specta_builder()` に登録する。

- 環境変数 `TSUMUGI_SAFE_MODE` が設定済みで、かつ空でも `0` でもなければ true。
- 環境変数の読み取りと判定ロジック（`&str`/`Option` を受ける純関数）を分け、判定ロジックだけをテストする。環境変数はプロセス共有なので、並列テストからは触らない。

セーフモードは「カスタムCSSを適用しない」だけで、保存値は消さない。設定セクションが `...app.ui` を丸ごと保存するため、セーフモード中に他の設定を保存しても `customCss` は保持される。

### 適用（フロント）

`frontend/src/lib/customCss.ts` に集約する。

```ts
export function applyCustomCss(css: string, safeMode: boolean): void
```

- `<head>` 内の `<style id="tsumugi-custom-css">` を、無ければ作成して `textContent` を差し替える。
- `css` が空、または `safeMode` が true なら `textContent` を空にする。
- 要素は `app.css` より後ろ（`<head>` 末尾）に置き、同詳細度ならユーザーCSSが勝つようにする。

`store.svelte.ts`:

- ロード時に `isSafeMode()` を1回取得して保持する（`app.safeMode`）。
- `ui` の組み立て2箇所（起動時ロードと `setUiPrefs`）に `customCss: prefs.customCss ?? ""` を追加する。
- `#applyCustomCss` を追加し、既存の `#applyTheme` 等と同じ2箇所から呼ぶ。

### UI

`AppearanceSection.svelte` の末尾に「カスタムCSS」項目を追加する。

- 等幅フォントの `<textarea>`。既存と同じ保存ボタンで保存する。
- 説明文に、CSSで画面を壊した場合の復旧方法（`TSUMUGI_SAFE_MODE=1` を付けて起動するとカスタムCSSが適用されない）を書く。
- セーフモード中は「セーフモードで起動中のためカスタムCSSは適用されていません」の警告を表示する。編集・保存は可能。
- 新規UIの値は `docs/design/style-guide.md` のスケールに従い、即値を増やさない。

### 既知の制限

Androidは環境変数を渡せないため `is_safe_mode` は常に false になる。CSSで設定画面を操作不能にした場合、アプリデータの消去か再インストールでしか復旧できない。
将来の対策候補（別Issue）: 起動時の不具合検知による自動セーフモード、ファイルフラグ方式。

## テスト

- Rust: `customCss` を含まない旧JSONから `UiPrefs` が読めて空文字になること。セーフモード判定の純関数（未設定 / 空 / `0` / `1` / 任意の非空文字列）。`cargo test` の bindings 生成テストで `customCss` と `isSafeMode` が出力されること。
- Vitest: `applyCustomCss` の作成・更新・空文字・セーフモードの各ケース（jsdom）。
- 実UI確認: Xvfb 越しに `cargo tauri dev` で、入力→即反映、再起動後の保持、空で復元、`TSUMUGI_SAFE_MODE=1` での非適用を確認する。

## ドキュメント

`docs/guide/user-guide.md`:

- 「設定画面」の外観の項目にカスタムCSSを追記する。
- 「トラブルシューティング」に、CSSで画面が壊れた場合の `TSUMUGI_SAFE_MODE=1` での起動手順と、Androidでは使えないことを追記する。
