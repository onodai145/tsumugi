# DrivePicker/ColumnSettingsのモーダル実装を共有Modal.svelteに統一する

Issue: #196

## 背景

Issue #174のモーダル基盤バッチ(2026-08-08)で共有`Modal.svelte`/`ConfirmDialog.svelte`のオーバーレイ・モーダル箱を`z-[1000]`に統一したが、`DrivePicker.svelte`(`z-[60]`)と`ColumnSettings.svelte`(`z-[50]`)は独自のオーバーレイ実装のまま個別のz-index値を持ち続けている。見た目のクラス値は`Modal.svelte`と揃えてあるが、portal・フォーカス管理・Escape処理・z-indexの実装自体は独自のままで重複している。

## 対応方針

### 1. `Modal.svelte`への最小拡張

新しい任意prop `maxHeight?: string` を追加する。指定時のみボックスに高さ制約と`flex-col`レイアウトを与え、`children`を`flex-1 min-h-0`のラッパーで包む。未指定時(既存の呼び出しすべて)は現状と完全に同じ挙動を維持する。

```ts
let {
  title,
  onclose,
  children,
  width = "480px",
  maxHeight,
}: {
  title: string;
  onclose: () => void;
  children: Snippet;
  width?: string;
  maxHeight?: string;
} = $props();
```

- ボックスのclassに`maxHeight`指定時だけ`flex flex-col`を追加、styleに`max-height:${maxHeight}`を追加する。
- `<header>`には常に`flex-none`を追加する(flex parentでない場合は無害なので条件分岐不要)。
- `children`は`maxHeight`指定時のみ`<div class="flex flex-1 flex-col min-h-0">`でラップする。未指定時は`{@render children()}`をそのまま置く。

この拡張により、DrivePickerの「パンくず(flex-none) / グリッド(flex-1 min-h-0 overflow-y-auto) / フッター(flex-none)」という内部レイアウトを、children snippetの中身としてそのまま移植できる。

### 2. `ColumnSettings.svelte`

独自のoverlay/box/header divを削除し、`<Modal title="カラム設定" {onclose} width="360px">`で既存のフォーム部分(`{#if group}...{/if}`)を`children`として渡す。`maxHeight`は不要(自然な高さで収まる)。

### 3. `DrivePicker.svelte`

独自のoverlay/box/header divを削除し、`<Modal title="ドライブから選択" {onclose} width="520px" maxHeight="78vh">`に置き換える。中身(`<nav>`パンくず、grid、もっと見るボタン、エラー表示、フッター)はそのままchildren snippetに移す。close処理(×ボタン、Escapeキー、オーバーレイクリック)はModal側の実装に一本化し、DrivePicker側の重複実装(独自のoverlay onclick/onkeydown、×ボタン)は削除する。

## 副作用として発生する見た目の差分(許容)

- 両モーダルとも、Modalのオーバーレイ仕様(`pb-[max(8vh,env(safe-area-inset-bottom))]`)を新たに受け取るため、下端にも安全マージンが付く(移行前は上端のみ`pt-[max(8vh,...)]`)。これはissueの「統一」意図に合致する想定内の差分であり、修正対象ではない。
- z-indexが自動的に`z-[1000]`系に統一される(本issueの主目的)。

## 確認事項(issue記載どおり)

- `DrivePicker.svelte`(添付メニュー→ドライブから選択)・`ColumnSettings.svelte`(カラムのグリップをダブルクリック)双方の表示・スクロール・フォーカス・Escapeでの閉じる動作が既存どおりであること。
- 特にDrivePickerのグリッドが`maxHeight="78vh"`指定下でも内部スクロールし、外側オーバーレイ全体はスクロールしない(=既存動作維持)ことを実機で目視確認する。
- 他のモーダル(Settings/AddColumnModal等)と同時に開いた場合でも重なり順が破綻しないこと。
- `Modal.svelte`の既存利用箇所(Settings/AddColumnModal等、`maxHeight`未指定)の見た目・挙動に変化がないこと。

## テスト方針

- Vitestユニットテストの有無を実装時に確認し、既存テストがあれば通すこと。新規のスナップショット/挙動テストは、既存の慣習(このリポジトリのフロントエンドテストの厚み)に合わせて必要に応じて追加する。
- 上記「確認事項」は`cargo tauri dev`での実機確認で担保する(自動テストで完全に代替できるUI要素のため)。
