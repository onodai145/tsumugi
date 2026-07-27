# エラー表示のモーダル化（Issue #123）

## 背景

現在、投稿欄（`ComposeBar.svelte`）のエラーは以下の2箇所で `err` state を通じて表示されている。

- ツールバー内の小さな赤い `!` アイコン（`title` 属性のツールチップでメッセージを表示）— 投稿(submit)失敗、アカウント未選択、クリップボード画像読み込み失敗などで使われる。
- 添付サムネイル上の赤い `!` バッジ（`title` 属性のツールチップ）— アップロード失敗時。

どちらも目立たず気づきにくい上、`err` は次のアクション開始時（`pickFiles`/`submit`/`handlePaste` の先頭）にしかクリアされないため、エラーを解消した後もユーザー自身の操作では消せない。

## 方針

`err` が truthy な間、エラーメッセージをモーダルダイアログで表示する。既存の overlay/modal CSS パターン（`AddColumnModal.svelte` に実装済み）を汎用コンポーネント `Modal.svelte` として切り出し、新規の ErrorModal 表示に使う。`AddColumnModal.svelte` 自体は今回変更しない。

## コンポーネント設計

### `frontend/src/ui/Modal.svelte`（新規）

`AddColumnModal.svelte` の overlay/modal パターンを汎用化した薄いラッパー。

- Props: `title: string`, `onclose: () => void`, `children: Snippet`（Svelte 5 snippet）。
- 振る舞い: 背景クリックで `onclose()`、Escapeキーで `onclose()`、ヘッダーに閉じる `X` アイコンボタン。
- スタイル: `AddColumnModal.svelte` の `.overlay` / `.modal` / `.head` / `.x` の CSS をそのまま移植する（`position: fixed` オーバーレイ、中央上寄せ配置、`role="dialog" aria-modal="true"`）。
- 中身（本文・フッターのボタン類）は呼び出し側が `children` snippet で渡す。

### `ComposeBar.svelte` の変更

- ツールバー内の `{#if err}<span class="err" ...>{/if}`（424行目）を削除。
- 添付サムネイル上の `!` バッジ（350行目）は「どの添付が失敗したか」を示す位置マーカーとして残すが、`title` によるツールチップは削除する（メッセージはモーダルに一本化するため）。
- `err` が truthy なら `Modal` を表示する:
  - `title="エラー"`
  - 本文に `err` のメッセージ本文を表示
  - フッターに「わかった」ボタンを1つ配置。クリックで `err = null` にしてモーダルを閉じる。
  - `Modal` の `onclose`（背景クリック/Escape/ヘッダーXボタン）も同じく `err = null` にする。
- 既存の `err = null` リセット箇所（`pickFiles`、`submit`、`handlePaste` の先頭）はそのまま維持する。新しいエラーが発生した場合はモーダル内容が新しいメッセージに置き換わる。

## スコープ外

- `AddColumnModal.svelte` を `Modal.svelte` を使う形にリファクタリングすることは今回のスコープ外（既存動作に影響を与えないため）。
- `err` 以外のエラー表示箇所（他コンポーネント）は対象外。

## テスト方針

- 手動確認: `cargo tauri dev` で実際にアカウント未選択のまま投稿ボタンを押し、モーダルが表示されること、「わかった」ボタン・Escape・背景クリックそれぞれで閉じられることを確認する。
- 添付アップロード失敗時も同様にモーダルが表示され、サムネイル上の `!` バッジが該当添付ファイルを示すことを確認する。
- `pnpm check` で型エラーがないことを確認する。
