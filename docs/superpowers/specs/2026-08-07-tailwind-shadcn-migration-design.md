# Tailwind CSS + shadcn-svelte 導入設計 (Issue #170)

## 背景・目的

Issue #112「UIがダサい」の根本原因は、生成AI(Claude)に明確なデザイン制約を与えずに実装を続けたため、機能追加のたびにCSSスタイルやレイアウトがブレていったこと。対策として、AIへの指示が通りやすく品質のブレを抑えられるデザインシステムを導入する。#112のサブIssueのうち、本設計は #170(UIフレームワークの導入)を対象とする。#171(スタイルガイドの作成)と #96(カラム追加/設定ボタンの位置変更)は本設計の対象外で、#170完了後に着手する。

## 現状

- フロントエンドはプレーンCSS(`frontend/src/app.css`, 466行)のみ。UIライブラリ・CSSフレームワークは未導入
- `.svelte`コンポーネントは39個(`frontend/src/ui/`配下)
- 独自のテーマ切替システムが既に存在する:
  - Rust構造体 `ThemeColors`(`src-tauri/src/domain/ui.rs`, 11フィールド: `surface1`, `surface2`, `surface3`, `border`, `text`, `textDim`, `accent`, `success`, `info`, `danger`, `warning`)
  - `UiPrefs.customThemes: Vec<CustomTheme>` としてユーザー定義テーマを `settings.json` にグローバル保存(アカウント非依存)
  - 13個のビルトインプリセット(`frontend/src/lib/theme.ts`の`PRESETS`)
  - `data-theme="light"/"dark"`属性 + `prefers-color-scheme`によるauto/light/darkの3状態切替(`frontend/src/lib/store.svelte.ts`の`#applyTheme()`)
  - `app.css`内で`--surface-1`等のCSS変数が定義され、コンポーネント側で約471箇所参照されている
- 後方互換のためのRustテストが既に存在(`src-tauri/src/store/settings.rs`): 旧JSON(`success`/`info`欠損)を読み込んでデフォルト値でバックフィルするテストなど

## 方針

### 採用技術

**Tailwind CSS v4 + shadcn-svelte** を導入する。

検討した代替案:
- Skeleton UI(Svelteネイティブ、テーマ管理をライブラリ側が担う) — 独自テーマエンジンとの二重管理になりやすく見送り
- Melt UI / Bits UI単体(ヘッドレスのみ、見た目は自前) — 既存CSS変数を変更せずに済むが、デザインのブレを抑える効果がTailwind+shadcnより弱く、Issue #112の課題認識と合わないため見送り
- Open Props等によるトークン整理のみ — 導入コストは最小だが同様に効果が弱いため見送り

shadcn-svelteはコンポーネントをリポジトリに直接コピーする方式(依存パッケージとしてブラックボックス化しない)であり、bits-ui上に構築されアクセシビリティも一定水準確保される。

### 適用範囲

既存39コンポーネント全てを本Issueの中で全面的にTailwindクラスへ移行する(段階移行は行わない)。shadcn-svelteのプリミティブ(Button, Dialog, Dropdown-menu等)は、移行の過程で必要になったものから`shadcn-svelte` CLIで逐次追加する(使わないプリミティブは入れない)。

### テーマ変数の移行

**Rust側 `ThemeColors` 構造体・`settings.json`のデータ形式・`CustomSyntaxTheme`(シンタックスハイライト用の別テーマ変数体系、`--shiki-*`)は一切変更しない。**

検討の結果、`ThemeColors`のフィールド(`surface1/surface2/surface3`等11個)からshadcn標準のトークン集合(`background`/`card`/`popover`/`muted`/`secondary`とそれぞれの`-foreground`ペア等、実質18種類前後)への変換は、対応関係が一意に定まらず(例: `surface1/2/3`のどれを`background`/`card`/`popover`に割り当てるか)、`primary-foreground`のように旧構造体に対応フィールドが存在しないトークンもある非可逆な変換になることが判明した。ビルトインプリセットは手動で作り直せるが、ユーザーが独自に作成したカスタムテーマは近似値への置き換えになり「保存済みのユーザーテーマを壊さない」という要件と両立しない。そのためRust側のデータモデルは変更せず、CSS層でのみブリッジする方針とする。

- Tailwind v4の`@theme`ディレクティブ(`frontend/src/app.css`)で、shadcn標準のCSS変数名を既存の`--surface-*`等の変数にマッピングする。例:
  ```css
  @theme {
    --color-background: var(--surface-1);
    --color-card: var(--surface-2);
    --color-popover: var(--surface-3);
    --color-primary: var(--accent);
    --color-destructive: var(--danger);
    --color-foreground: var(--text);
    --color-muted-foreground: var(--text-dim);
    --color-border: var(--border);
  }
  ```
  具体的な対応表は実装タスクの中で`--surface-1/2/3`, `--border`, `--text`, `--text-dim`, `--accent`, `--success`, `--info`, `--danger`, `--warning`の9変数を基に確定する。`-foreground`系トークンで対応する原色がないものは、既存の`--text`/`--surface-*`から妥当な組み合わせを選ぶ(新しいRustフィールドは追加しない)。
- `frontend/src/lib/theme.ts`の13プリセット・`PRESETS`配列・`ThemeColors`型・`applyThemeColors()`は変更不要(そのまま動く)
- Rust側 `src-tauri/src/domain/ui.rs`・`src-tauri/src/store/settings.rs`のテスト・マイグレーションコードは変更不要
- ダーク/ライト切替を`data-theme`属性ベースからshadcn標準の`.dark`クラスベースに変更する。`store.svelte.ts`の`#applyTheme()`と`app.css`のセレクタ(`:root[data-theme="dark"]`等)を書き換える。auto(OS追従)/light/darkの3状態という挙動自体は変えない

## エラーハンドリング

- Rust側のデータモデル・デシリアライズ処理は変更しないため、既存のカスタムテーマ関連の後方互換フォールバック(欠損値のデフォルト補完、選択中テーマが見つからない場合に auto へフォールバックして保存し直す等)はそのまま現行ロジックを維持する

## テスト

- Rust: `src-tauri/src/store/settings.rs`の既存テストは変更不要(データモデル自体を変更しないため)。念のため`cd src-tauri && cargo test`が全て通ることを確認する
- フロントエンド: 既存の`*.test.ts`(`NoteCard.test.ts`, `ProfileModal.test.ts`, `FollowListModal.test.ts`, `CompletionPopover.test.ts`)は挙動テストであり、Tailwindクラスへの書き換え後もそのまま通ることを確認する
- `cd frontend && pnpm check`(svelte-check + tsc)が通ることを確認する
- 手動確認: `cargo tauri dev`で以下を目視確認する
  - テーマ切替(auto/light/dark、13プリセット、カスタムテーマ)が期待通り反映される
  - 既存の(移行前に作成された)カスタムテーマがそのまま読み込める(データモデル不変のため原理的に壊れないが、CSSブリッジ経由で正しく表示されることを目視確認する)
  - 主要画面(カラム、設定、コンポーズバー、各種モーダル)の見た目に崩れがない

## スコープ外

- #171(スタイルガイドページの作成)は別Issueとして本設計の対象外
- #96(カラム追加/設定ボタンの位置変更)などの個別デザイン改善は別Issueとして対象外
- CLAUDE.mdへのデザインルール(カラーパレット、アイコン方針等)の追記は本設計の対象外(#171または別途検討)
