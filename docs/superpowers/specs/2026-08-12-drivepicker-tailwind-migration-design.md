# DrivePicker.svelte Tailwind移行 設計

## 背景

Issue #174(既存コンポーネントのTailwindクラスへの移行)の一環。`frontend/src/ui/DrivePicker.svelte`(投稿添付用のドライブファイル選択モーダル)を手書きCSSからTailwindユーティリティクラスへ移行する。

## 対象

`frontend/src/ui/DrivePicker.svelte`(172行、`<style>`ブロック除く)。使用箇所は`ComposeBar.svelte`のみ(添付メニューの「ドライブから選択」)。

## 方針

### モーダル構造

`Modal.svelte`と非常に似た構造(オーバーレイ+モーダル箱+ヘッダー+×閉じる)だが、パンくず・スクロールするグリッド・フッター(選択件数+添付ボタン)という独自レイアウトを持つため、共有`Modal`コンポーネントへの置き換えは本移行のスコープ外とする(構造変更は既存の見た目・挙動を壊すリスクがあり、YAGNIの観点からも今回は見送る)。オーバーレイ/モーダル箱の外観クラス値は`Modal.svelte`と揃える(`bg-black/45`, `rounded-[14px]`, `border-border bg-background`)が、独自の`flex flex-col max-h-[78vh]`構造はそのまま維持する。

z-indexは既存の`60`のまま`z-[60]`とする。`ColumnSettings.svelte`(z-index: 50、未移行)など他の同種モーダルとの重なり順を変えないため、今回はMode.svelte由来の`z-[1000]`系には合わせず据え置く。

### `×`閉じるボタン

`Button variant="ghost" size="icon-xs"`に変換する(`Modal.svelte`の閉じるボタンと統一)。

### パンくず(`.crumb`)

`class:active`は`color`/`font-weight`という同一プロパティを切り替えるため、既存の禁止パターンに該当する。ルート用・各階層用それぞれ三項演算子で単一クラス文字列に統一する。

### ファイルセルの選択状態(`.cell.file.selected`)

`class:selected`は`outline`関連プロパティを切り替えるため、同様に三項演算子で単一クラス文字列に統一する。フォルダセルは常に同一クラス(トグルなし)なのでそのままTailwindクラス化する。

### グリッドレイアウト

`grid-template-columns: repeat(auto-fill, minmax(84px, 1fr))`と`grid-auto-rows: 84px`はTailwindの既定スケールに無いため、任意値クラス`grid-cols-[repeat(auto-fill,minmax(84px,1fr))]`と`auto-rows-[84px]`で表現する。

### 「もっと見る」ボタン

`Button variant="outline" size="sm"`に変換する(既存規約: 標準サイズの単独アクションボタン)。

### 「添付」ボタン(CTA)

`Button variant="default" size="sm"`に変換する。

## テスト方針

既存の自動テストはない。`pnpm check`と`pnpm build`のグリーンを確認し、`pnpm build`後のコンパイル済みCSSに想定クラス(`grid-cols-[repeat(auto-fill,minmax(84px,1fr))]`等)が実際に生成されていることをgrepで確認する。
