# 下書き機能 設計

- 作成日: 2026-09-02
- 対象Issue: #251「下書き機能」
- 対象コンポーネント: `src-tauri/src/store/`, `src-tauri/src/commands/`, `frontend/src/ui/ComposeBar.svelte`

## 背景

現状 `ComposeBar.svelte` は投稿するかクリアするかのみで、書きかけの投稿内容を保存して後で呼び出す手段がない。閉じる・アプリを終了する・誤操作などで入力中の内容が失われる。既存の `NoteDraft`（`src-tauri/src/api/notes.rs`）は「投稿APIへ渡すペイロード型」であり、本Issueが指す「保存して後で呼び出す下書き」とは別物。

## 要件

- 投稿を書きかけの状態で保存し、後で呼び出して編集・投稿できる。
- 保存は次の2種類。
  - **手動保存**: ComposeBar内のボタンで明示的にスナップショットを保存する。複数件保持でき、一覧から選んで呼び出せる。
  - **自動一時保存**: 入力中に定期的に自動保存し、ComposeBarを閉じて再度開いた際に復元される（一覧には出さない、1アカウントにつき1件のみ）。
- 下書きはアカウントごとに分離して保存・表示する（tsumugiはマルチアカウント対応のため、選択中アカウントの下書きのみを一覧に出す）。
- 手動保存した下書きを呼び出して編集・投稿した場合、その下書きは投稿成功時に自動削除する。

## データモデル

新規テーブル `drafts`（`src-tauri/src/store/draft.rs`、既存の `settings.rs` / `user_ref.rs` と同じ rusqlite ベースのパターンに倣う）。

```rust
pub enum DraftKind { Manual, Auto }

pub struct Draft {
    pub id: String,               // uuid（Autoの場合はaccount_id単位で1件のみ存在、idは固定でなくupsert時に再生成でよい）
    pub account_id: String,
    pub kind: DraftKind,
    pub text: String,
    pub cw: Option<String>,
    pub visibility: VisibilityInput,
    pub local_only: bool,
    pub reaction_acceptance: ReactionAcceptanceInput,
    pub channel_id: Option<String>,
    pub poll: Option<PollDraftSnapshot>,   // choices, multiple, expires_at
    pub file_ids: Vec<String>,             // 既にドライブへアップロード済みのファイルID
    pub reply_note: Option<NoteSnapshot>,  // 表示用の最小限のスナップショット（id, user, text抜粋）
    pub quote_note: Option<NoteSnapshot>,
    pub created_at: i64,
    pub updated_at: i64,
}
```

- `reply_note` / `quote_note` は返信・引用先ノートの**保存時点でのスナップショット**を保持する（`id` + `user` + `text`の抜粋程度）。復元時にMisskey側へ再取得はしない。対象ノートが削除済み・アクセス不能でも、保存済みスナップショットをそのままバナー表示に使う。実際の投稿時に `replyId`/`renoteId` が無効であればMisskey APIがエラーを返すので、それは既存のエラー表示（`formatError`）に任せる。
- `file_ids` は既にアップロード済みのファイルIDのみを保持する（アップロード中の一時ファイルは対象外）。復元時にDrive APIからメタ情報を再取得してサムネイル表示する。取得できない（削除済み等）ファイルIDは復元時に静かに無視する。

## Rustコマンド

`src-tauri/src/commands/draft.rs` を新設し、`specta_builder()` に登録する。

- `list_drafts(account_id: String) -> Vec<Draft>` — 該当アカウントの `Manual` 下書きを更新日時降順で返す。
- `save_draft(account_id: String, draft: DraftInput) -> Draft` — 新規 `Manual` 下書きを1件作成する（既存の更新は行わない、常に新規追加）。
- `delete_draft(account_id: String, draft_id: String)` — 指定の `Manual` 下書きを削除する。
- `get_auto_draft(account_id: String) -> Option<Draft>` — 該当アカウントの `Auto` 下書きを返す。
- `save_auto_draft(account_id: String, draft: DraftInput)` — `Auto` 下書きをupsertする（1アカウント1件、既存があれば置き換え）。
- `clear_auto_draft(account_id: String)` — `Auto` 下書きを削除する。

## フロントエンド（ComposeBar.svelte）

- 投稿ボタン付近に「下書き」アイコンボタンを追加し、クリックでポップオーバーを開く。ポップオーバー内容:
  - 現在選択中アカウントの `Manual` 下書き一覧（本文の先頭数十文字プレビュー + 相対時刻）。項目クリックでComposeBarの各状態（text/cw/visibility/localOnly/reactionAcceptance/channelId/poll/attachments/replyTo/quoteOf）を復元し、ポップオーバーを閉じる。各項目に削除ボタンを添える。
  - 「現在の内容を下書き保存」ボタン。押下時点のComposeBar状態から `DraftInput` を組み立て `save_draft` を呼ぶ。
- **自動一時保存**: text/cw/添付/投票いずれかが非空の間、入力変更を2秒デバウンスして `save_auto_draft` を呼ぶ。全て空になったら `clear_auto_draft` を呼ぶ。
- **自動復元**: ComposeBarマウント時、現在の入力が空かつ返信/引用コンテキストが無い場合に限り `get_auto_draft` を呼び、結果が非空なら復元する（返信/引用モーダルとして開かれた場合は自動復元しない — 文脈の異なる下書きを誤って混入させないため）。
- **投稿成功時の後始末**:
  - 呼び出し元が `Manual` 下書きだった場合、投稿成功後にその `delete_draft` を呼ぶ。呼び出し元を追跡するため、ComposeBar内に「現在ロード中の下書きID」を保持するローカル状態を追加する。
  - `Manual` 下書き経由かどうかによらず、投稿成功時は常に `clear_auto_draft` を呼ぶ（投稿済み内容が次回自動復元されないように）。
- **エラーハンドリング**: 手動保存/削除の失敗は既存の `formatError` パターンでトースト通知する。自動保存(`save_auto_draft`)の失敗は頻度が高いため通知を出さずログのみに留める。

## テスト

- Rust: `store/draft.rs` に保存/一覧/削除/auto upsert のユニットテスト。`commands/draft.rs` に既存コマンドテストと同様のパターンでコマンドレベルテストを追加。
- Frontend: `ComposeBar.test.ts`（存在すれば）に、自動保存のデバウンス発火、非空→空遷移でのclear呼び出し、手動下書き選択時の状態復元、投稿成功後の自動削除、をユニットテストで追加する。

## 非対象（YAGNI）

- 下書き件数の上限設定。
- 下書きの並べ替え・検索・タグ付け。
- 返信/引用先ノートの復元時再取得（最新状態への追従）。
- 下書きの他デバイス間同期（あくまでローカルSQLite保存）。
- `Manual` 下書きの上書き保存（既存下書きの更新編集）。常に新規追加とする。
