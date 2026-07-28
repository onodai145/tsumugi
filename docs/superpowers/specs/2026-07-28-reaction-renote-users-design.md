# リアクション・Renoteした人を見れるようにする（Issue #90）

## 背景

`NoteCard.svelte` はリアクション絵文字ごとの件数（`reactionList`）と Renote 件数（`inner.renoteCount`）を表示しているが、誰が押した/RenoteしたかをUIから確認する手段がない。Misskey API には以下が既にある。

- `notes/reactions`（`noteId`, `type?`, `limit?`, `sinceId?`, `untilId?` → `NoteReaction[]`）: `type` を指定すると特定の絵文字キーで絞り込める。
- `notes/renotes`（`noteId`, `limit?`, `sinceId?`, `untilId?` → `Note[]`）: 返る `Note.user` がRenoteしたユーザー。

いずれも1リクエストの上限は100件。

## 方針

リアクションバッジ・Renote件数それぞれに**ホバーで自動表示するポップオーバー**を追加する。クリック操作（リアクショントグル/Renote実行）とは独立させ、既存の操作性を変えない。

- リアクションは**絵文字ごと**にポップオーバーを出す（`type` を指定して該当絵文字を押したユーザーのみ取得）。
- Renoteは件数部分（吹き出しアイコンではなく数字側）のホバーでポップオーバーを出す。アイコン側クリックは従来通りRenote実行のまま。
- 表示件数は最初の100件のみ（`limit: 100`固定）。ページングは実装せず、100件を超える場合は末尾に「他n件」と表示する（`reactionCount`/`renoteCount` との差分から算出）。

## バックエンド設計

### `src-tauri/src/api/notes.rs`

```rust
pub struct ReactionUser {
    pub user: User,
    pub reaction: String,
}

pub async fn get_reactions(client, note_id: &str, reaction_type: Option<&str>, limit: u8) -> Result<Vec<ReactionUser>>
pub async fn get_renotes(client, note_id: &str, limit: u8) -> Result<Vec<User>>
```

- `get_reactions` は `notes/reactions` を POST し、レスポンスの `RawUser` を既存の `User::from` で変換。
- `get_renotes` は `notes/renotes` を POST（`RawNote` 経由）し、各要素の `user` のみ抽出して返す。
- `ReactionUser` は `specta::Type` を付けてTS export対象にする。

### `src-tauri/src/commands/note.rs`

```rust
#[tauri::command]
#[specta::specta]
async fn get_note_reactions(account_id, note_id, reaction_type: Option<String>) -> Result<Vec<ReactionUser>>

#[tauri::command]
#[specta::specta]
async fn get_note_renotes(account_id, note_id) -> Result<Vec<User>>
```

- 両コマンドとも `limit` はサーバ側で100固定（フロントからは渡さない）。
- 既存コマンドと同様に `AppState` からアカウントのクライアントを取得して呼ぶ。
- `specta_builder()` に登録を追加。

## フロントエンド設計

### `frontend/src/ui/ReactionUsersPopover.svelte`（新規）

- Props: `accountId`, `noteId`, `reactionKey: string | null`（`null` ならRenote用モード）, `totalCount: number`。
- `onmouseenter`（親から委譲）でIPC呼び出し。150msデバウンスしてから `get_note_reactions` / `get_note_renotes` を叩く。
- 結果はアバター(`avatarUrl`)+表示名(`Mfm` + `emojis`)の縦リスト。`totalCount` が取得件数(最大100)より多ければ末尾に「他n件」。
- ローディング中は簡易スピナー表示、エラー時は非表示（コンソールに warn）。
- 同一 `noteId` + `reactionKey` の結果はコンポーネント内 `Map` にキャッシュし、再ホバーで再フェッチしない。

### `NoteCard.svelte` の変更

- リアクションバッジ (`.reaction` 、245行目付近) に `onmouseenter`/`onmouseleave` を追加し、ホバー中は該当 `key` を state に持って `ReactionUsersPopover` をバッジ直下に表示。既存の `onclick`（リアクショントグル）はそのまま維持。
- Renote件数表示（273行目付近、`{inner.renoteCount || ""}`）を独立した `<span>` に分離し、そこに `onmouseenter`/`onmouseleave` を追加。アイコン部分 (`<Repeat2>`) 側のクリックは従来通り `doRenote()` を維持し、件数側のクリックもRenote実行のままとする（挙動変更なし、ホバー検出だけ追加）。

## スコープ外

- ページング（`sinceId`/`untilId` を使った追加読み込み）は今回実装しない。
- リアクション/Renote一覧からのユーザープロフィール遷移（クリックでユーザーページを開く等）は既存のユーザー表示コンポーネントの挙動に準拠するのみで、新規のプロフィール画面等は作らない。

## テスト方針

- `cd src-tauri && cargo test`: 既存の `generates_frontend_bindings` が新規コマンド/型を含めてTSバインディング再生成できることを確認。
- 手動確認（`cargo tauri dev`）: 実際のMisskeyインスタンスでリアクション付きノート・Renoteされたノートに対し、各ホバーでユーザー一覧が表示されること、既存のリアクショントグル/Renote実行が壊れていないことを確認する。
- `cd frontend && pnpm check` で型エラーがないことを確認する。
