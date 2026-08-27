# カラムヘッダーのミートボールメニュー化

Issue #209（親: #112）

## 背景

`frontend/src/ui/Column.svelte` のタブバー末尾には現在「＋」（タブ追加）「⬓」（下に分割）の
2ボタンが常設されており、カラム設定はグリップ（⠿）のダブルクリックという隠れた導線で開いている。

Issue #209 のコメントで、右端にミートボールメニューを作り「カラム設定」「カラムの分割」
「タブの追加」をその中に畳み込む方針が示された。グリップは並べ替え用にそのまま残す。

## 変更内容

### タブバー末尾
- 「＋」「⬓」の2ボタンを廃止。
- 代わりに「⋯」（`MoreHorizontal`、`@lucide/svelte`）の1ボタンを配置。
- クリックでメニューを開く。メニュー項目（上から順）:
  1. タブを追加（`onAddTab`）
  2. 下に分割（`onSplitDown`）
  3. カラム設定（`onEditGroup`）

アイコンを `MoreHorizontal`（横向き三点）にするのは、`NoteCard.svelte` の投稿メニュー
トリガーと同じアイコンで統一感を出すため。

### グリップ（⠿）
- `ondblclick={() => onEditGroup(group.id)}` を削除。
- `title` からも「ダブルクリックでカラム設定」の文言を除去し、「ドラッグでカラムを並べ替え」のみにする。
- ドラッグでの並べ替え動作自体は変更しない。

## 実装方針

`AppMenu.svelte`（Issue #96／PR #207 で導入済みのハンバーガーメニュー）と同じパターンを踏襲する。

- `open` / `trigger` / `pos` を `$state` で持ち、`toggle()` でトリガー要素の
  `getBoundingClientRect()` から開く位置を計算する。
- ポータルは `frontend/src/lib/portal.ts` の共有 `portal` アクションを使う
  （`Column.svelte` は現状ローカル `portal` 関数を持たないため新規importになる）。
- 開閉は backdrop パターン：`fixed inset-0` の透明レイヤーを `use:portal` でbody直下に置き、
  そのクリックで閉じる。メニュー本体はその内側にあり、クリックが backdrop まで伝播しないよう
  `stopPropagation` する。
- メニュー本体の見た目は `NoteMenu.svelte` / `AppMenu.svelte` と同じクラス
  （`rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]`）を流用し、
  各項目は `role="menuitem"` のボタン。
- 開く方向は `AppMenu.svelte`（画面最下部のバー、`bottom` 基準で上に開く）とは異なり、
  カラムのタブバーは画面上部寄りに来ることが多いため、`Dropdown.svelte` / `NoteMenu.svelte` と同様
  トリガーの下端を起点に `top` 基準で下に開く。
- 新規コンポーネントとして切り出すか（`ColumnMenu.svelte`）、`Column.svelte` に直書きするかは
  実装時に判断する。項目数が3つで済み、既存の `Column.svelte` も216行程度と大きくないため、
  インラインで十分に収まる見込み。

## 影響範囲

- `Column.svelte` の `props`（`onAddTab` / `onEditGroup` / `onSplitDown`）のシグネチャは変更しない。
  呼び出し元（親コンポーネント）の変更は不要。
- `e2e/` 配下を grep した限り、「＋」「⬓」ボタンやグリップのダブルクリックを直接セレクタで
  参照しているテストは存在しない。既存E2Eテストへの影響はない見込み。

## テスト

- Rust側の変更はないため `cargo test` は対象外。
- フロントエンドは `pnpm check`（svelte-check + tsc）を通す。
- 手動確認: `cargo tauri dev` でカラムを表示し、
  - 「⋯」クリックでメニューが開き、3項目が期待順で並んでいること
  - 各項目クリックで対応する既存動作（タブ追加ダイアログ／下分割／カラム設定）が発火すること
  - メニュー外クリックで閉じること
  - グリップのダブルクリックではカラム設定が開かなくなったこと
  - グリップのドラッグによる並べ替えは従来通り動作すること
