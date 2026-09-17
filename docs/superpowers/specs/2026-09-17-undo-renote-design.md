# Renoteの取り消し (Issue #352)

## 背景

Misskeyの「Renote」(引用なしの純粋なRenote)を取り消す手段がtsumugiに存在しない。Misskeyの公式Webクライアント(misskey-dev/misskey)のソースを調査したところ、この機能は以下のように単純な仕組みで実現されている:

- 判定: 表示しているRenoteノート自身(`rawNote`)の投稿者IDが自分のIDと一致するかどうかだけを見る(`isMyRenote = $i.id === rawNote.userId`)。サーバーへの追加問い合わせや状態追跡は行わない。
- 取り消し: そのRenoteノート自身のID(元ノートのIDではない)を指定して `notes/delete` を呼ぶ。専用の `notes/unrenote` エンドポイントは公式Webクライアントでは使われていない。

この方式は「自分のRenoteノートを直接見ているとき(自分のタイムライン、通知など)しか取り消せない」という制約を伴うが、公式クライアントと同じ制約であり、状態追跡やAPI呼び出しの追加が一切不要という利点がある。tsumugiもこれに倣う。

対象外とする範囲:
- 引用付きRenote(quote renote)は通常の投稿として扱われ、既存の「削除」メニューで取り消せるため対象外。
- 「元ノートを見ている状態から、自分が過去にRenoteしたかどうかを判定して取り消す」機能は対象外(公式クライアントにもない機能であり、Misskey APIに `isRenotedByMe` 相当のフィールドが存在しないため、正確な判定にはセッション内トラッキングや追加API呼び出しが必要になり、スコープ外とする)。

## 現状の実装

- Renote作成: `src-tauri/src/api/notes.rs` の `renote()` (`notes/create` に `renoteId` を渡す) → `src-tauri/src/commands/note.rs` の `renote` コマンド → `frontend/src/lib/store.svelte.ts` の `app.renote()`。
- ノート削除: `src-tauri/src/api/notes.rs` の `delete_note()` (`notes/delete`) → `commands/note.rs` の `delete_note_cmd` → `frontend/src/lib/store.svelte.ts` の `app.deleteNote(accountId, noteId)`。ローカルの全タブから該当IDのノートを除去する処理込みで実装済み。
- UI: `frontend/src/ui/NoteMenu.svelte` に「削除」メニュー項目があり、`isOwnNote`(= 表示中ノートの投稿者IDが自分のuserIdと一致)で表示を制御している。ただし `frontend/src/ui/NoteCard.svelte` は純粋Renoteの場合、`NoteMenu` に常に `inner`(Renote先の中身のノート)を渡しており、Renoteノート自身(outer)を渡していない。そのためRenoteノート自身に対する削除(=取り消し)を行う手段がUI上に存在しない。
- ドメイン型 `Note`(`src-tauri/src/domain/note.rs`)の `is_renoted_by_me` フィールドは常に `false` 固定で未使用。今回のスコープでは使わない。

## 変更内容

### バックエンド

変更なし。既存の `delete_note_cmd` / `app.deleteNote()` をRenoteノート自身のIDで呼び出すだけで実現できる。

### フロントエンド

**`frontend/src/ui/NoteCard.svelte`**
- 純粋Renote(`isPureRenote`)かつ、そのRenoteノート自身の投稿者(`note.user.id`)が現在の操作アカウントのuserIdと一致する場合に限り、`NoteMenu` へ新しいprop `pureRenoteOf={note}` を渡す(それ以外は `undefined`)。既存の `note={inner}` prop はそのまま維持し、コピー/お気に入り/クリップ追加などは従来通りRenote先の中身に対して作用させる。

**`frontend/src/ui/NoteMenu.svelte`**
- 新しいprop `pureRenoteOf?: Note` を受け取る。
- `canUndoRenote = pureRenoteOf != null && app.accounts.find(a => a.id === accountId)?.userId === pureRenoteOf.user.id` を算出。
- `canUndoRenote` が真のとき、「Renote取り消し」メニュー項目を表示する(既存の「削除」ボタンとは独立した項目。アイコンは `Repeat2` を使用し、`text-destructive` で危険操作であることを示す)。
- 押下で確認ダイアログ(既存の `ConfirmDialog` を再利用。文言: タイトル「Renoteの取り消し」、メッセージ「このRenoteを取り消します。取り消せません。よろしいですか？」、confirmLabel「取り消す」)を出し、確認後に `app.deleteNote(accountId, pureRenoteOf.id)` を呼ぶ。
- 実装は既存の `requestDelete`/`confirmDelete` と同様のパターン(`confirmUndoRenoteOpen` state, `requestUndoRenote`/`confirmUndoRenote` 関数)を並列に追加する形にする。

### テスト

- `NoteMenu.svelte` のVitestコンポーネントテスト(既存テストファイルがあればそこに追加、なければ新規作成):
  - `pureRenoteOf` が未指定のとき「Renote取り消し」項目が表示されないこと。
  - `pureRenoteOf` があるが投稿者が自分でないとき、項目が表示されないこと。
  - `pureRenoteOf` があり投稿者が自分のとき、項目が表示され、クリック→確認→`app.deleteNote` が `pureRenoteOf.id` で呼ばれること。
- 手動確認: `cargo tauri dev`(Xvfb越し)で実際にRenote→自分のタイムラインでRenoteカードのメニューから取り消し→ノートが消えることを確認する。

## 影響しない箇所

- Rustドメイン型・API層・Tauriコマンド一覧(specta登録)は無変更。
- `is_renoted_by_me` フィールドは今回のスコープでは使わない(将来、元ノート視点での「Renote済み」表示をやる場合に再検討)。
