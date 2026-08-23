# Issue #234: 投稿を削除できるようにする

## 背景

Misskey上の自分の投稿を、tsumugiのUIから削除できるようにする。Issue #234で要望されている。

調査の結果、バックエンド（`src-tauri/src/commands/note.rs` の `delete_note_cmd`、`src-tauri/src/api/notes.rs` の `delete_note`）とフロントエンドのstore層（`frontend/src/lib/store.svelte.ts` の `AppState.deleteNote()`）は既に実装済みで、`specta_builder()` にも登録済み。しかしUI側から呼び出す導線が存在せず、未使用のまま残っていた。

本designのスコープは、フッター（NoteCardの「その他」メニュー）に削除項目を追加するUI配線のみ。

## 変更内容

### `frontend/src/ui/NoteMenu.svelte`

- 自分の投稿かどうかを判定する `$derived`（`isOwnNote`）を追加する。
  - `note.user.id === app.accounts.find((a) => a.id === accountId)?.userId` で判定する。
  - 判定には `note`（既存 prop）と `accountId`（既存 prop）のみを使う。追加の API 呼び出しは不要。
- `isOwnNote` が `true` の場合のみ、メニュー最下部に「削除」ボタンを追加表示する。
  - アイコンは `Trash2`（`@lucide/svelte`）。
  - 危険操作であることが視覚的にわかるよう、テキスト色を赤系（`text-destructive` 相当。既存のTailwindユーティリティ・カラートークンに準拠し、`style-guide.md` にあれば従う。なければ既存の危険操作系ボタンの配色を踏襲する）にする。
- クリック時、即削除はせず `ConfirmDialog`（既存コンポーネント、投票確認等で使用中のもの）を `danger` フラグ付きで開く。
  - `title`: 「投稿の削除」
  - `message`: 「この投稿を削除します。取り消せません。よろしいですか？」
  - `confirmLabel`: 「削除する」
- `ConfirmDialog` の `onConfirm` で `app.deleteNote(accountId, note.id)` を呼び出し、完了後 `onclose()` でメニューを閉じる。
  - `note` は `NoteMenu` に渡された prop の `note`（`NoteCard.svelte` 側で `inner`＝純粋Renote考慮後のノート）をそのまま使う。
- `onCancel` ではダイアログを閉じるのみで、メニュー自体は開いたままにする（既存の投票確認ダイアログの挙動に合わせる）。

### 変更不要な既存実装（確認のみ）

- `AppState.deleteNote()`（`store.svelte.ts:1575`）: 成功時に全タブから該当ノートIDを `filter` で除去済み。追加変更不要。
- `#applyNoteUpdate()` の `note/deleted` ストリーミングイベント処理（`store.svelte.ts:921`）: `tab.notes.filter((n) => n.id !== p.noteId)` で除去しており、既にUI操作で消えたノートIDに対して再度イベントが来ても冪等（該当なしのfilterは無害）。追加変更不要。
- バックエンド（`delete_note_cmd` / `delete_note`）: 変更不要。

## エラーハンドリング

- `app.deleteNote()` は失敗時に内部で `#logFailure(e)` を呼びログに残した上で例外を re-throw する（既存実装）。`NoteMenu.svelte` 側は削除呼び出しを `catch` し、失敗時もメニューを閉じる（他の危険操作 `confirmCreateClip()` 同様、エラーは `console.error` 相当のログに留め、UIをブロックしない）。

## テスト

- `frontend/src/ui/NoteCard.test.ts`（既存テストファイル、NoteMenuの統合テストもここに存在する可能性が高い）に以下を追加:
  - 自分の投稿では「削除」項目が表示されること。
  - 他人の投稿では「削除」項目が表示されないこと。
  - 「削除」クリック → ConfirmDialogが開く → confirmで `app.deleteNote` が呼ばれること。
  - ConfirmDialogをキャンセルした場合 `app.deleteNote` が呼ばれないこと。

## スコープ外

- バックエンド・store層の変更（既に実装済みのため）。
- 削除対象ノートの取り消し（アンドゥ）機能。
- 引用ネスト表示（`quoted`）時の特別扱い（`NoteMenu` は `accountId` があれば通常通り動作するため、特別扱い不要と判断）。
