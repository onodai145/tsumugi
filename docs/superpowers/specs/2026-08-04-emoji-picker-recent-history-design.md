# 絵文字ピッカーの使用履歴（Issue #108）

## 背景

絵文字ピッカー（`ReactionPicker.svelte`）には既に「ピン留め」機能があるが、実際にリアクションした絵文字の使用履歴は残らない。よく使う絵文字がピン留めされていない場合、毎回スクロールや検索が必要になる。

## スコープ

- 記録対象は**ノートへのリアクション**（`NoteCard.svelte`の`react()`）のみ。
- 設定画面でのピン留め絵文字選択（`ReactionSection.svelte`、`ReactionPicker`を`showPinned={false}`で使用）は記録しない。
- 投稿本文への`:emoji_name:`テキスト補完（`ComposeBar.svelte`の`CompletionPopover`）は対象外。これは`ReactionPicker`とは別実装であり、リアクションとは性質が異なる。

## データモデル

`src-tauri/src/domain/ui.rs`の`UiPrefs`に`recent_emojis: Vec<String>`を追加する。

```rust
/// リアクションピッカーで最近使った絵文字（Issue #108）。キー形式は pinned_emojis と同じ
/// （Unicode絵文字はそのまま、カスタム絵文字は ":name:" 形式）。先頭が最新。
#[serde(default)]
pub recent_emojis: Vec<String>,
```

- `#[serde(default)]`なので既存ユーザーの設定ファイルは空リストにフォールバックする（マイグレーション不要）。
- 順序で最新度を表現する（タイムスタンプは持たない）。使用のたびに「既存の同一キーを除去 → 先頭に追加 → 最大16件に切り詰め」を行うことで、重複なく最新順を保つ。
- 上限は16件（ピン留め既定8件の2倍、2行相当）。

## フロント永続化

`frontend/src/lib/store.svelte.ts`に`setPinnedEmojis`と同じパターンで追加：

```ts
/// リアクションピッカーで絵文字を使ったことを記録する（Issue #108）。
/// 既存の同一キーは除去してから先頭に追加し、最大16件に切り詰める。
async recordEmojiUsage(key: string) {
  const list = [key, ...this.ui.recentEmojis.filter((k) => k !== key)].slice(0, 16);
  await unwrap(commands.setUiPrefs({ ...this.ui, recentEmojis: list }));
  this.ui = { ...this.ui, recentEmojis: list };
}
```

`NoteCard.svelte`の`react()`から呼ぶ：

```ts
function react(reaction: string) {
  app.reactPicker = null;
  if (accountId) {
    app.toggleReaction(accountId, inner.id, reaction);
    app.recordEmojiUsage(reaction);
  }
}
```

## 表示（`ReactionPicker.svelte`）

- 「ピン留め」セクションの直前に「最近使った」セクションを追加。
- `showPinned`が`true`のときのみ表示する（設定画面のピン留め選択フロー、`showPinned={false}`では出さない）。
- キー解決は`pinnedEntries`と同じロジック（Unicode文字はそのまま、カスタム絵文字は`customEmojis`から`name`を引いて解決。アカウントのhostと不一致・削除済みは除外）を流用する。
- **既にピン留め済みのキーは除外**して表示する（同じ絵文字が2箇所に重複して出るのを避ける）。
- 検索中（`queryLower`が非空）は非表示（ピン留めセクションと同じ扱い）。
- 0件の場合はセクション自体を非表示にする（ピン留めのような「〜がありません」という空状態メッセージは出さない。未使用の状態は単に見えないほうが自然）。

## テスト方針

- Rust: `UiPrefs`のシリアライズ/デシリアライズ・既定値フォールバックのテスト（既存の`pinned_emojis`テストと同じ形）に`recent_emojis`のケースを追加。
- フロント: `store.svelte.ts`の`recordEmojiUsage`の単体テスト（重複除去・16件切り詰め・順序）があれば追加。既存のテスト基盤（Vitest）を踏襲。
- 手動確認: `cargo tauri dev`でNoteCardからリアクション → ピッカーを開き直して「最近使った」に反映されること、ピン留め済み絵文字が重複表示されないこと、設定画面のピン留め選択では「最近使った」が出ないことを確認する。

## 非スコープ（YAGNI）

- 使用回数によるランキング表示（頻度順ソート）はやらない。単純な最終使用順のみ。
- アカウント別・インスタンス別の履歴分離はしない（`pinned_emojis`と同様、グローバル1本）。
- 履歴のクリア機能（設定画面のUI）は今回追加しない。将来必要になれば別Issueで検討する。
