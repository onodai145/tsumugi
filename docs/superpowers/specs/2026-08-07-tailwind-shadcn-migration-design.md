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

Rust側 `ThemeColors` 構造体のフィールド名を、shadcn標準命名(`background`/`foreground`/`card`/`primary`/`secondary`/`muted`/`accent`/`destructive`/`border`/`input`/`ring`等)に変更する。これに伴い:

- `frontend/src/lib/theme.ts`の13プリセットを新フィールド名で作り直す。プリセットの`id`/`name`(例: `tokyo-night`, `dracula`)は維持し、ユーザーから見た選択状態が変わらないようにする
- `src-tauri/src/domain/ui.rs`の`ThemeColors`構造体を新フィールド名に変更(specta経由で`frontend/src/bindings/tauri.gen.ts`が再生成される)
- **後方互換マイグレーション**: `settings.json`に保存済みの旧フィールド名(`surface1`等)のカスタムテーマを、新フィールド名へ変換してデシリアライズするフォールバック処理をRust側に追加する。値が欠損する場合は既存の`success`/`info`バックフィルと同様、妥当なデフォルト値を補う。ユーザーが保存したカスタムテーマ設定を壊さないことを最優先とする
- ダーク/ライト切替を`data-theme`属性ベースからshadcn標準の`.dark`クラスベースに変更する。`store.svelte.ts`の`#applyTheme()`と`app.css`のセレクタ(`:root[data-theme="dark"]`等)を書き換える。auto(OS追従)/light/darkの3状態という挙動自体は変えない

## エラーハンドリング

- 旧フィールド名のカスタムテーマJSONを読み込んだ際、新フィールドへの変換に失敗する値があってもクラッシュせず、デフォルト値にフォールバックする(既存パターンを踏襲)
- 選択中のカスタムテーマ/プリセットが見つからない場合の挙動(auto にフォールバックして保存し直す)は現状のロジックをそのまま維持する

## テスト

- Rust: `src-tauri/src/store/settings.rs`の既存後方互換テスト(`theme_colors_deserializes_legacy_json_without_success_info`等)を新フィールド名向けに更新し、「旧フィールド名JSON→新フィールド名への変換」を検証するテストを追加する
- フロントエンド: 既存の`*.test.ts`(`NoteCard.test.ts`, `ProfileModal.test.ts`, `FollowListModal.test.ts`, `CompletionPopover.test.ts`)は挙動テストであり、Tailwindクラスへの書き換え後もそのまま通ることを確認する
- `cd frontend && pnpm check`(svelte-check + tsc)が通ることを確認する
- 手動確認: `cargo tauri dev`で以下を目視確認する
  - テーマ切替(auto/light/dark、13プリセット、カスタムテーマ)が期待通り反映される
  - 既存の(移行前に作成された)カスタムテーマが壊れずに読み込める
  - 主要画面(カラム、設定、コンポーズバー、各種モーダル)の見た目に崩れがない

## スコープ外

- #171(スタイルガイドページの作成)は別Issueとして本設計の対象外
- #96(カラム追加/設定ボタンの位置変更)などの個別デザイン改善は別Issueとして対象外
- CLAUDE.mdへのデザインルール(カラーパレット、アイコン方針等)の追記は本設計の対象外(#171または別途検討)
