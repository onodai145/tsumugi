# 通知カードでのノートアクション有効化 設計

- Issue: #50 「通知から返信とかリノートとかリアクションとかできるようにする」
- 日付: 2026-08-04

## 背景

通知カード(`frontend/src/ui/NotificationCard.svelte`)は、通知に紐づくノート(`notification.note`)のプレビューを表示する際に既存の`NoteCard`コンポーネントを再利用しているが、`quoted={true}`を渡し`accountId`を渡していないため、`NoteCard`側のアクションフッター(返信・Renote・引用・リアクション・その他メニュー)が常に非表示になっている。

バックエンド(Rust)側は`post_note`/`renote`/`react`/`unreact`の各コマンドが既に存在し、`Notification.note`フィールドも完全な`Note`型(リアクション・返信先・可視性などを含む)を保持している。したがって本対応は純粋にフロントエンドの配線タスクである。

## スコープ

対象は`notification.note`が存在する通知種別すべて: `mention` / `reply` / `renote` / `quote` / `reaction` / `pollEnded`。これらは全て同一のアクションフッター(タイムラインと同じフルセット)を表示する。種別ごとの出し分けは行わない。

`note`を含まない通知種別(`follow` / `receiveFollowRequest` / `followRequestAccepted` / `achievementEarned` / `app`)は、既存の`{#if n.note}`ガードによりそもそも対象外(変更なし)。

`renote`/`reaction`/`pollEnded`のように表示されるノートが自分自身のノートになりやすいケースについても、特別な制御は行わない。`NoteCard`の既存ロジック(`canRenote`の可視性判定など)がそのまま適用され、タイムラインで自分のノートを見る場合と完全に一貫した挙動になる。

## 設計

### 1. `NoteCard.svelte`: `quoted`とアクション表示可否の分離

現状、アクションフッターの表示条件は`{#if !quoted && accountId}`であり、`quoted`(コンパクトスタイリング用のprop)がアクション表示の可否も兼ねている。この結合を解消するため、新規prop`showActions?: boolean`を追加する。

- デフォルト値: `!quoted`(既存呼び出し元の挙動を完全に保持)
- フッター条件: `{#if showActions && accountId}`

これにより、「`quoted`スタイル(コンパクト表示)は使うがアクションは出したい」という`NotificationCard`のユースケースと、「`quoted`スタイルもアクション非表示も両方欲しい」という既存のネスト引用ノート(`NoteCard.svelte:331`の`Self`呼び出し、リノート内にネストされた引用元ノート)のユースケースを、1つの追加propで両立できる。

### 2. `NotificationCard.svelte`: `NoteCard`呼び出しの配線変更

`frontend/src/ui/NotificationCard.svelte:92`の`NoteCard`呼び出しに以下を追加する:

- `accountId={accountId}` — アクションフッター表示条件および`app.openCompose`/`app.renote`/`app.toggleReaction`等の呼び出しに必須
- `showActions={true}` — アクションフッターを明示的に有効化

`quoted={true}` / `hideActionBanner` / `hideReactions`は現状維持する(コンパクトスタイル、返信/RN元バナーの重複回避、既存リアクション一覧の非表示という既存方針を変えない)。

```svelte
<NoteCard
  note={n.note}
  quoted={true}
  showActions={true}
  hideReactions
  hideActionBanner
  accountId={accountId}
  emojiAccountId={accountId}
/>
```

### 3. 変更しない箇所

- バックエンド(Rust)側は変更不要。既存コマンドをそのまま利用する。
- `NoteCard.svelte:331`のネストされた引用ノート(`Self`コンポーネント)呼び出しは変更なし。`showActions`を渡さないため、デフォルトの`!quoted`(= `false`)が適用され、従来通りアクションボタンは出ない。
- 既存リアクション一覧の表示可否(`hideReactions`)。今回はアクションボタン(リアクションピッカー等)の有効化のみが目的であり、リアクション一覧自体の表示方針は変更しない。

## テスト

- `cd frontend && pnpm check` で型チェック(新規prop追加によるコンパイルエラーがないことを確認)。
- 手動確認: `cargo tauri dev`を起動し、通知カラムで以下を確認する。
  - `mention`/`reply`/`renote`/`quote`/`reaction`/`pollEnded`の各通知タイプでアクションフッターが表示されること。
  - 返信ボタン押下でコンポーズが正しい返信先で開くこと。
  - Renote/引用ボタン押下で正しく動作すること(可視性制限がある場合は非表示になること)。
  - リアクションボタンでピッカーが開き、リアクションが送信できること。
  - `follow`等note を含まない通知種別で、従来通りアクションフッターが出ないこと。
  - リノート内にネストされた引用元ノート(タイムライン)でアクションボタンが従来通り出ないこと(regressionがないこと)。
