# ColumnSettings.svelte Tailwind移行 設計

## 背景

Issue #174(既存コンポーネントのTailwindクラスへの移行)の一環。`frontend/src/ui/ColumnSettings.svelte`(カラム自体の幅/高さ設定モーダル)を手書きCSSからTailwindユーティリティクラスへ移行する。これがIssue #174の対象コンポーネントとして最後の1ファイルとなる。

## 対象

`frontend/src/ui/ColumnSettings.svelte`(59行、`<style>`ブロック除く)。`class:`ディレクティブは無い(ラジオボタンは`checked`属性で直接制御)。

## 方針

### モーダル構造

オーバーレイ/モーダル箱の外観クラス値は`Modal.svelte`と揃える(`bg-black/45`, `rounded-[14px]`, `border-border bg-background`)。z-indexは既存の`50`のまま`z-[50]`とする(他の未移行モーダルとの重なり順を変えないため)。

### `×`閉じるボタン

`Button variant="ghost" size="icon-xs"`に変換する(`Modal.svelte`の閉じるボタンと統一)。

### `.field > span:first-child`

複合セレクタを、対象の`<span>`要素(「幅」「高さ」等のラベル)に直接`text-muted-foreground`を付与する形に変換する。

### 数値入力欄

既存規約(`DataSection.svelte`/`AddAccount.svelte`)に合わせ`rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground`とする。

### ラジオボタン(ユーザー指定によるスコープ追加)

当初案では「ネイティブ要素のまま未スタイル」としていたが、レビューで「ラジオボタンも移行して」との指示を受けた。`DisplaySection.svelte`のレンジスライダーで既に使われている規約(`accent-primary`)を踏襲し、ブラウザネイティブの形状は保ったまま選択色をアプリのテーマ色(`--accent`)に合わせる。カスタムラジオコンポーネントの新規作成は本移行のスコープ外とする(新規コンポーネント設計・a11y対応が必要になり、1ファイルの移行作業の範囲を超えるため)。

## テスト方針

既存の自動テストはない。`pnpm check`と`pnpm build`のグリーンを確認し、コンパイル後CSSに想定クラスが実際に生成されていることをgrepで確認する。
