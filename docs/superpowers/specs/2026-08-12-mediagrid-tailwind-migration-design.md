# MediaGrid.svelte Tailwind移行 設計

## 背景

Issue #174(既存コンポーネントのTailwindクラスへの移行)の一環。`frontend/src/render/MediaGrid.svelte`(ノート添付メディアのグリッド表示)を手書きCSSからTailwindユーティリティクラスへ移行する。

## 対象

`frontend/src/render/MediaGrid.svelte`(82行、`<style>`ブロック除く)。

## 方針

### `.media-grid`

`class:single={files.length === 1}`は`grid-template-columns`という同一プロパティを切り替えるため、既存の禁止パターン(`class:`によるプロパティ衝突トグル)に該当する。三項演算子で単一クラス文字列に統一する:

```
class={files.length === 1
  ? "mt-2 grid grid-cols-1 gap-1 overflow-hidden rounded-[5px]"
  : "mt-2 grid grid-cols-2 gap-1 overflow-hidden rounded-[5px]"}
```

### `.media-cell`

レイアウト(`relative`, `aspect-[16/10]`, `flex items-center justify-center`)はTailwindクラス化する。以下の2点は既存の「`color-mix()`+`--column-opacity`は`<style>`にフッククラスとして残す」規約に従い、`media-cell`というクラス名自体をフックとして維持する:

- `background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent)`
- `max-height: var(--media-thumbnail-height, 200px)`(ユーザー設定で可変。`var()`のフォールバックにカンマを含むため、Tailwindの任意値記法での静的解析リスクを避け`<style>`に残す)

### `img`/`video`/`audio`

`.media-cell img, .media-cell video`の`width/height/object-fit/cursor`は、対象の`<img>`/`<video>`要素に直接Tailwindクラス(`h-full w-full object-cover cursor-zoom-in`)を付与する形に変える(複合セレクタを個別要素への直接クラス付与に変換)。`.media-cell audio`の`width: calc(100% - 16px)`は`w-[calc(100%-16px)]`(カンマを含まないため任意値クラスとして安全)。

### `.sensitive-cover` / `.file-link` / `.video-save`

複合UI(メディアセル)に埋め込まれた小さいオーバーレイ/リンク/アイコンボタンのため、既存規約(NoteCardのチップ削除×等)に倣いrawな`<button>`のままTailwind化する(`Button`プリミティブ化はしない)。`color-mix()`を使わない`.file-link`/`.video-save`は完全にTailwindクラスへ移行できる。

### `:global(.viewer-download::before)`

サードパーティ(viewerjs)が生成するDOM要素へのグローバルセレクタのため、Tailwind化せずそのまま`<style>`に残す。

## テスト方針

既存の自動テストはない。`pnpm check`と`pnpm build`のグリーンを確認し、`pnpm build`後のコンパイル済みCSSに想定クラスが実際に生成されていることをgrepで確認する。
