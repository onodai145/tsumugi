# クリップ / お気に入り 設計 (Issue #14, #15)

## 背景

- Issue #14: ノートをクリップに登録できるようにする
- Issue #15: ノートをお気に入りに登録できるようにする

いずれも Misskey のノートアクションで、既存の返信/Renote/引用/リアクションと同じ「ノート単位の操作」カテゴリに属する。UI 側のアクション行がこれ以上横に伸びないよう、両機能は新設する `⋯` オーバーフローメニューにまとめる。

## スコープ

- お気に入り: ノートへの登録/解除トグルのみ。
- クリップ: 既存クリップへの追加、および新規クリップ作成 + 追加のみ。クリップ一覧の中身を見る専用カラムやクリップ管理(名前変更・削除・公開設定変更)は対象外。

## バックエンド

### お気に入り (`api/notes.rs`, `commands/note.rs`)

Misskey API: `notes/favorites/create` / `notes/favorites/delete`(ともに `{ noteId }` のみ、トグルではなく明示的な作成/削除)。

- `api/notes.rs` に `create_favorite(client, note_id)` / `delete_favorite(client, note_id)` を追加。
- `commands/note.rs` に `favorite_note(state, account_id, note_id)` / `unfavorite_note(state, account_id, note_id)` を追加(`react`/`unreact` と同じ形)。
- `lib.rs` の `specta_builder()` に2コマンドを登録。

`domain::Note::is_favorited_by_me` は既存フィールドだが、Misskey のタイムライン応答(`Note` スキーマ)には `isFavorited` が含まれず、個別に `notes/state` を呼ばない限り取得できない。バックフィルは今回のスコープ外とし、常に `false` 初期値のまま、フロントの楽観的更新でのみ状態を反映する(`toggleReaction` と同じ方針)。

### クリップ (`api/clips.rs` 新設, `domain/clip.rs` 新設, `commands/clip.rs` 新設)

Misskey API:
- `clips/list` — 自分のクリップ一覧取得(引数なし)
- `clips/create` — `{ name, isPublic?, description? }` で新規作成、作成された `Clip` を返す
- `clips/add-note` — `{ clipId, noteId }` でノートを追加

`domain/clip.rs` に `Clip` 型を追加(`specta::Type` 付与、TS 側へ export):

```rust
pub struct Clip {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_public: bool,
    pub notes_count: i64,
}
```

`api/clips.rs`:
- `list_clips(client) -> Result<Vec<Clip>>`
- `create_clip(client, name) -> Result<Clip>`(`is_public`/`description` は今回のUIから渡さず、Misskey 側のデフォルト値=非公開・説明なしで作成)
- `add_note_to_clip(client, clip_id, note_id) -> Result<()>`

`commands/clip.rs`:
- `list_clips(state, account_id) -> Result<Vec<Clip>>`
- `create_clip(state, account_id, name) -> Result<Clip>`
- `add_note_to_clip(state, account_id, clip_id, note_id) -> Result<()>`

`lib.rs` の `specta_builder()` に `commands::clip` の3コマンドと `Clip` 型を登録。`commands/mod.rs` に `pub mod clip;` を追加。

## フロントエンド

### store (`lib/store.svelte.ts`)

- `toggleFavorite(accountId, noteId)`: `toggleReaction` と同様に `#collectNotes(noteId)` で同一ノートの全コピーを取得し、`isFavoritedByMe` を反転させて楽観的に更新。失敗時はロールバックして `#fail(e)`。成功時は `favorite_note`/`unfavorite_note` を呼び分け、`#log` で通知。
- `listClips(accountId): Promise<Clip[]>` — `commands.listClips` をラップ。
- `createClip(accountId, name): Promise<Clip>` — `commands.createClip` をラップし、成功時に返る `Clip` をそのまま返す(呼び出し側でメニューの一覧に追加)。
- `addNoteToClip(accountId, clipId, noteId)` — `commands.addNoteToClip` をラップし、成功/失敗を `#log`/`#fail` で通知。

### UI (`ui/NoteCard.svelte`, 新設 `ui/NoteMenu.svelte`)

- アクション行の右端(リアクションボタンの右)に `⋯` ボタンを追加。クリックで `NoteMenu` を portal オーバーレイとして表示(`ReactionPicker` の `picker-overlay`/`picker-pop` パターンを流用)。
- `NoteMenu.svelte`:
  - Props: `accountId`, `note`(対象の `Note`)
  - 項目1: 「☆ お気に入り登録」/「★ お気に入り解除」(`note.isFavoritedByMe` でラベル切替) → クリックで `app.toggleFavorite` を呼びメニューを閉じる。
  - 項目2: 「クリップに追加 ▸」→ ホバー/タップでサブメニューを展開。
    - サブメニューは開いたタイミングで `app.listClips(accountId)` を呼びクリップ一覧を表示(ロード中は簡易スピナー/「読み込み中」テキスト)。
    - 一覧の各クリップをクリック → `app.addNoteToClip(accountId, clip.id, note.id)` を呼びメニューを閉じる。
    - 一覧末尾に「＋ 新規クリップを作成」→ クリックでサブメニュー内がテキスト入力+確定ボタンに切り替わる(名前必須、空なら確定ボタン無効)。確定で `app.createClip(accountId, name)` → 成功したら続けて `app.addNoteToClip(accountId, newClip.id, note.id)` を呼びメニューを閉じる。
- メニューは `note` ではなく `inner`(純粋Renoteの委譲先)を対象にする。既存の返信/Renote/引用/リアクションと同じ扱い。

## エラーハンドリング

- 各コマンドは他の note 系コマンドと同様、失敗時は `Error`(既存の `error.rs`)をそのまま返し、フロントは `unwrap`/`#fail` で捕捉してエラーログ表示する。特別なリトライ・オフライン考慮は行わない(既存 note アクションと同水準)。

## テスト

- Rust: `api/clips.rs` / `api/notes.rs` の新規関数について、既存の note 系テストと同様のユニットテスト(モックレスポンスの normalize 確認)を追加。実 Misskey 接続が要るものは `#[ignore]`。
- フロント: `pnpm check` で型チェックが通ることを確認。UI の手動確認は `cargo tauri dev` 上で実施(お気に入りトグル、クリップ追加、新規クリップ作成の3系統)。

## 非対象(YAGNI)

- クリップの中身を表示する専用カラム/フィルタ
- クリップの公開設定・説明文編集、削除、名前変更
- お気に入り一覧を表示する専用カラム
- 起動時のお気に入り状態バックフィル(`notes/state` の一括版が Misskey に無いため)
