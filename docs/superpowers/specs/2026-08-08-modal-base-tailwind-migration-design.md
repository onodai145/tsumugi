# モーダル基盤コンポーネントのTailwind移行 設計 (Issue #174 第2バッチ)

## 背景・目的

Issue #174の第1バッチ(#176、レイアウト系: Column/Pane/Backstage)に続き、モーダル群のTailwind移行を進める。モーダル群は合計1447行(`Modal.svelte` 77行、`AddColumnModal.svelte` 706行、`ConfirmDialog.svelte` 99行、`ProfileModal.svelte` 358行、`FollowListModal.svelte` 207行)あり、特に`AddColumnModal.svelte`は`Modal.svelte`を使わない独自構造の大きな設定フォームで性質が異なるため、1バッチとして扱うには大きすぎる。本設計はその中から、共通ラッパーである`Modal.svelte`と、最も小さく独立した`ConfirmDialog.svelte`のみを対象とする第2バッチとする。`ProfileModal.svelte`/`FollowListModal.svelte`(`Modal.svelte`使用)と`AddColumnModal.svelte`(独立構造)は別バッチとして後日対応する。

## 対象コンポーネント

- `frontend/src/ui/Modal.svelte`(77行) — 汎用モーダルラッパー。`ProfileModal.svelte`/`FollowListModal.svelte`/`ComposeBar.svelte`から使われる
- `frontend/src/ui/ConfirmDialog.svelte`(99行) — 汎用確認ダイアログ

両者は`overlay`(画面全体を覆う半透明背景+portal配置)と`modal`(白背景・枠線・角丸の中央ダイアログ)という、ほぼ同じCSS構造を独立して持っている。

## 方針

### portal/フォーカス/Escapeキー処理は一切変更しない

`document.body.appendChild`によるportal実装、`onkeydown`でのEscape検知、`onclick`での外側クリック検知、`role="dialog"`/`aria-modal="true"`等のアクセシビリティ属性は現状のまま維持する。今回のタスクはスタイル(CSS)の置き換えのみを対象とする。

### 独自のoverlay/modal構造は維持し、Tailwindクラスに置き換えるのみ

shadcn-svelteのDialogプリミティブへの置き換えは行わない(フォーカストラップ・アニメーション等の機能追加はスコープ外。既存の軽量な独自実装を維持する)。

- `overlay`(`position:fixed; inset:0; background:rgba(0,0,0,.45); display:grid; place-items:start center; padding-top:8vh; z-index:1000;`)は`fixed inset-0 grid items-start justify-items-center bg-black/45 pt-[8vh] z-[1000]`に変換する
- `modal`(`border:1px solid var(--border); border-radius:14px; padding:16px; background:var(--surface-1);`)は`border border-border rounded-[14px] p-4 bg-background`に変換する(`border-radius:14px`はTailwindの標準スケールに一致しないため`rounded-[14px]`のアービトラリ値を使う)。幅(`Modal.svelte`は480px、`ConfirmDialog.svelte`は360px)もそれぞれ`w-[min(480px,92vw)]`/`w-[min(360px,92vw)]`のアービトラリ値で維持する
- `z-index:1000`は`z-[1000]`のアービトラリ値で維持する

### shadcn Buttonプリミティブを使用する

前バッチ(#176)で導入済みの`Button`を、以下のように置き換える:

- `Modal.svelte`の閉じる(×)ボタン: `Button variant="ghost" size="icon-xs"`
- `ConfirmDialog.svelte`のキャンセルボタン: `Button variant="secondary" size="sm"`
- `ConfirmDialog.svelte`の確定ボタン: `Button variant={danger ? "destructive" : "default"} size="sm"`

確定ボタンは元々`color: #fff`のハードコードだったが、Buttonの`default`/`destructive`バリアントの配色にそのまま任せる。`default`は`bg-primary text-primary-foreground`(塗りつぶし、テーマ追従の白系文字)。`destructive`は実装を確認したところ`destructive-foreground`への参照は無く、`bg-destructive/10 text-destructive`(薄い赤トーン、塗りつぶしではない)になる — shadcn標準のそのままの見た目として受け入れる(最終レビューで判明、2026-08-09時点でdanger propを使う呼び出し箇所は無いため実害なし)。

## エラーハンドリング

CSS/レイアウトの置き換えのみのため、エラーハンドリングロジックの変更は無い。`onConfirm`/`onCancel`/`onclose`コールバック、Escapeキー、外側クリックでのキャンセル動作は一切変更しない。

## テスト

- `Modal.svelte`/`ConfirmDialog.svelte`に既存の自動テストは無い
- `cd frontend && pnpm check`/`pnpm build`が通ることを確認する
- 手動確認(`cargo tauri dev`、リポジトリルートから起動):
  - モーダルの表示位置(画面上部寄り中央)、半透明の背景オーバーレイ
  - Escapeキー・背景クリックでの閉じる動作
  - `Modal.svelte`の閉じるボタンの見た目・クリック動作
  - `ConfirmDialog.svelte`の通常時(青系)/danger時(赤系)のボタン配色
  - `Modal.svelte`を使う既存画面(プロフィール表示、フォロー一覧、投稿欄の一部)で見た目が崩れていないこと

## スコープ外

- `ProfileModal.svelte`/`FollowListModal.svelte`(`Modal.svelte`使用)は別バッチ
- `AddColumnModal.svelte`(独立構造、706行)は別バッチ
- shadcn Dialogプリミティブへの置き換えは行わない。理由: (1)現在の独自portal実装は`role="dialog"`/`aria-modal`/Escape/外側クリックを備えており壊れていない、(2)モーダル的なUI(`AddColumnModal`の独自ダイアログ、`DrivePicker`、各種ポップオーバー等)が今後のバッチにも点在しており、Dialogプリミティブ化はそれら全体を棚卸ししてから一括で検討したほうが手戻りが少ない。Issue #174の全バッチ完了後、別Issueとして再検討する
