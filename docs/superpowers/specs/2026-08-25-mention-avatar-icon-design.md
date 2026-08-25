# メンションにアバターアイコンを表示する（Issue #102）

## 背景・目的

ノート本文中の `@mention` 表記は現状 `MfmNode.svelte` の `mention` ノードでテキスト（`@acct`）のみをレンダリングしており、対象ユーザーのアバターが見えない。他のUI要素（通知カードの通知元アバター、ノートカードの投稿者アバター等）と同様に、本文中のメンションにも対象ユーザーのアバターアイコンをインライン表示し、視認性を上げる。

対象は本文中の `@mention` のみ。通知カードのアバター表示（`NotificationCard.svelte`）は既に対応済みのため対象外。

## データ取得

MFM の `mention` ノードは `username` / `host` / `acct` のみを持ち、アバターURLを含まない。既存の `resolve_user_acct` コマンド（`users/show` を acct から解決、`src-tauri/src/commands/column.rs`）をそのまま再利用し、新規Rustコマンドは追加しない。

`accountId` の扱いは、mentionクリック時の `openProfile` と同じ既存の慣例に倣う: `Mfm` / `MfmNode` に新たに `accountId` propを配線せず、`app.defaultAccountId()` にフォールバックする。`<Mfm>` の呼び出し元は6箇所（`NoteCard.svelte` / `NotificationCard.svelte` / `ProfileModal.svelte` / `FollowListModal.svelte` / `ReactionUsersPopover.svelte` / 再帰呼び出しの `MfmNode.svelte` 自身）あり、すべてに配線するのはこの機能のスコープに対して過剰と判断。既存のクリック挙動（`forAccountId` 省略時に `defaultAccountId()` へフォールバック）と一貫性を保つ。

## キャッシュ

新規モジュール `frontend/src/lib/mentionAvatar.svelte.ts` を追加し、セッション内インメモリキャッシュを持つ:

- キー: `acct`（`username@host` 正規化形、ローカルユーザーは `host` なし）
- 値: `avatarUrl: string | null`（`null` は解決失敗・見つからない・avatarUrl自体がnullの場合）
- in-flightのPromiseもキーごとに保持し、同一acctへの同時マウント時の二重フェッチを防ぐ

キャッシュはページ内セッションのみ有効（永続化しない）。既存の絵文字プロキシキャッシュ（`lib/emoji.ts`）と同様、DBやTauri側の永続層は使わない。

## 取得タイミング

各 `mention` ノードのマウント時（`$effect`）にキャッシュを確認し、未解決ならフェッチする。IntersectionObserverなどの可視領域ベースの遅延読み込みは行わない（優先度lowのenhancementのためスコープを絞る）。

失敗時（リモートインスタンス到達不可・404等）はキャッシュに失敗として記録し、以後同じacctへの再フェッチを防ぐ。エラーは投げず、アイコン無しの現状表示にフォールバックする。

## 表示

`@acct` テキストの前に16px（`h-4 w-4`）・`rounded-md`（スタイルガイド§2のアバター規約に準拠）のアイコンをインラインで表示する。`object-cover` で切り抜き、`loading="lazy"` を付与する。

取得前・取得失敗時はアイコンを表示しない。固定幅のプレースホルダーは設けないため、取得完了時にテキスト位置がわずかに動く（レイアウトシフト）が、許容範囲と判断する。

## テスト

- `mentionAvatar.svelte.ts` のキャッシュ・重複フェッチ抑止・失敗時フォールバックの単体テスト
- `MfmNode.test.ts`（既存があれば追加、無ければ新規）で、アイコン表示・未解決時のフォールバック表示をテスト
