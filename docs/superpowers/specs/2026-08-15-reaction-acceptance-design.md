# リアクション受け入れ設定 (Issue #169)

## 背景

Misskey の `notes/create` は `reactionAcceptance` フィールドを受け付ける（OpenAPI仕様で確認済み）。
値は `null`（全員）/ `likeOnly` / `likeOnlyForRemote` / `nonSensitiveOnly` /
`nonSensitiveOnlyForLocalLikeOnlyForRemote` の5択。**投稿ごとの設定であり、アカウント単位の既定値は存在しない**。

## スコープ

投稿バー（`ComposeBar.svelte`）に、既存の CW / 投票 / チャンネル ボタンと同じ並びで
リアクション受け入れを選ぶドロップダウンを追加する。投稿後の表示（NoteCard 側）への反映は行わない
（本家 Misskey クライアントも同様、投稿後に選択内容を表示しない）。

## 変更内容

### バックエンド (`src-tauri/src/api/notes.rs`)

- `ReactionAcceptanceInput` enum を追加（`VisibilityInput` と同じパターン）:
  `All` / `LikeOnly` / `LikeOnlyForRemote` / `NonSensitiveOnly` / `NonSensitiveOnlyForLocalLikeOnlyForRemote`。
- `NoteDraft` に `reaction_acceptance: Option<ReactionAcceptanceInput>` を追加。
  `All`/`None` はフィールドごと省略（`skip_serializing_if`）し、Misskey 側のデフォルト（`null`＝全員）に委ねる。

### フロントエンド

- 新規 `ReactionAcceptanceSelect.svelte`（`VisibilitySelect.svelte` と同じ構造: portal メニュー + `bind:value`）。
  ラベル:
  - すべて（既定）
  - いいねのみ
  - いいねのみ（リモート）
  - 非センシティブ絵文字のみ
  - 非センシティブ（ローカル）/ いいねのみ（リモート）
- `ComposeBar.svelte`:
  - `reactionAcceptance` state を追加（既定 `"all"`）。
  - CW/投票/チャンネル ボタンの並びに配置。
  - 送信時 `"all"` → `null`（省略）に変換して `NoteDraft` に載せる。
  - 投稿完了後、他の一過性フィールドと同様に `"all"` へリセット。

### バインディング

`cargo test generates_frontend_bindings`（または `cargo tauri dev`）で `tauri.gen.ts` を再生成。

### テスト

- `src-tauri/src/api/notes.rs` の既存ユニットテストに倣い、
  - 既定（`All`/`None`）では `reactionAcceptance` が JSON に出ないこと
  - 非既定選択時は正しい camelCase 文字列で出ること
  を検証。
- フロントエンドは `pnpm check` で型チェック。

## 非スコープ

- 投稿済みノートへのリアクション制限表示（NoteCard 等）は対象外。
- アカウント単位の既定値設定は Misskey 側に存在しないため対象外。
