# ユーザープロフィールページ（Issue #91）

## 背景

現状、ノートに表示されるアバター・表示名・acct、および本文中の `@mention` はいずれもクリックできない単なる表示・テキストで、そのユーザーの詳細情報（自己紹介・バナー・フォロー数など）やフォロー操作にアクセスする手段がない。`ColumnKind::User`（`src-tauri/src/domain/column.rs`）によりユーザーのノート一覧をカラムとして追加する機能は既に実装済みだが、これは常設カラムであり、都度ちょっと覗く用途には向かない。本Issueでは、ノート一覧カラムとは別に、詳細プロフィールを一時的なモーダルとして閲覧できるようにする。

## スコープ

- モーダル/オーバーレイ方式でプロフィール（アバター・バナー・表示名・acct・自己紹介・フォロー中/フォロワー数・ノート数）を表示する。
- フォロー/フォロー解除ボタンを含む（自分自身の場合は非表示）。
- モーダル内にそのユーザーのノート一覧を埋め込み、スクロールで閲覧できる。
- 「カラムとして追加」ボタンで既存の `ColumnKind::User` カラムを追加できる導線を残す。
- フォロー中/フォロワー数をクリックすると、それぞれの一覧モーダルを開ける。
- 起点は (1) `NoteCard.svelte` のアバター/表示名/acctクリック、(2) ノート本文中の `@mention` クリックの2箇所。
- ブロック機能・ミュート操作・自己紹介以外のプロフィールフィールド（ピン留めノート等）は対象外（YAGNI）。

## アーキテクチャ

### バックエンド（`src-tauri/`）

- `domain/user.rs`: `User` に `bio: Option<String>`、`banner_url: Option<String>` を追加。
- `api/users.rs`: 既存の `search_users()` に加え、以下を追加。
  - `show(user_id: Option<&str>, username: Option<&str>, host: Option<&str>)` — `users/show` のラッパー。`user_id` またはユーザー名(+ host)のいずれかで解決する。
  - `relation(user_id: &str)` — `users/relation` のラッパー。フォロー状態を返す。
  - `follow(user_id: &str)` / `unfollow(user_id: &str)` — `users/follow` / `users/unfollow` のラッパー。
  - `followers(user_id: &str, until_id: Option<&str>)` / `following(user_id: &str, until_id: Option<&str>)` — `users/followers` / `users/following` のラッパー（カーソルページネーション）。
- `commands/user.rs`（新規）: 上記を束ねる `#[tauri::command]` ハンドラを実装し、`specta_builder()` に登録する。
  - `get_user_profile(account_id, user_id | (username, host))` — プロフィール本体 + フォロー状態（自分自身の場合は `None`）を返す。
  - `follow_user(account_id, user_id)` / `unfollow_user(account_id, user_id)`
  - `get_user_followers(account_id, user_id, until_id)` / `get_user_following(account_id, user_id, until_id)`

### フロントエンド（`frontend/src/`）

- `lib/profileModal.svelte.ts`（新規）: `store.svelte.ts` と同型のSvelte 5 runesベースのグローバル状態。
  ```ts
  type ProfileTarget = { userId: string } | { username: string; host: string | null };
  let target = $state<ProfileTarget | null>(null);
  export function openProfile(t: ProfileTarget) { target = t; }
  export function closeProfile() { target = null; }
  export function currentProfileTarget() { return target; }
  ```
  NoteCardのクリックとMfmNodeのmentionクリックの両方が、経路の長さに関わらずこの1つのストアを叩く。表示側（トップレベルレイアウト）は1箇所だけ `{#if currentProfileTarget()}` を見ればよい。
- `ui/ProfileModal.svelte`（新規）: 既存 `Modal.svelte` を土台に実装。
  - マウント時に `get_user_profile` を呼び出し、ローディング → プロフィール表示（バナー・アバター・`Mfm` で描画した表示名/bio・acct・統計値）。
  - 自分自身のプロフィールでない場合のみフォローボタンを表示。クリックで楽観的にトグル表示 → `follow_user`/`unfollow_user` 呼び出し → 失敗時はロールバックしてエラーメッセージ表示。
  - フォロー中/フォロワー数クリックで `FollowListModal` を開く。
  - 「カラムとして追加」ボタン: `AddColumnModal.svelte` が持つ `{ type: "user", userId }` 組み立てロジックを呼び出してカラムを追加し、`closeProfile()` する。
  - 下部にノート一覧セクション: `get_user_profile` で得た `userId` を使い、既存の `ColumnKind::User` が使っているものと同じ `users/notes` 取得 + ページネーションロジック（`rest_request()` 系）を呼び出し、`NoteCard` で描画。モーダル内スクロールで末尾に到達したら追加取得。
  - エラー時（ユーザーが見つからない、削除済みアカウント等）はモーダル内にエラーメッセージを表示し、リトライボタンを出す。
- `ui/FollowListModal.svelte`（新規）: `get_user_followers` / `get_user_following` を呼び、ユーザー行（アバター・表示名・acct）を一覧表示。無限スクロールで追加取得。行クリックで `openProfile({ userId })` し、その上に新しい `ProfileModal` を積む（モーダルの多重表示を許容する）。
- 配線:
  - `NoteCard.svelte`: アバター/表示名/acctに `onclick={() => openProfile({ userId: inner.user.id })}` を追加。
  - `render/MfmNode.svelte`: mentionノード（`p.acct` を保持）に `onclick` を追加し、acctを `username`/`host` に分解して `openProfile({ username, host })` を呼ぶ。
  - トップレベルレイアウト（`App.svelte` 等、既存の実装を確認して最も適切な1箇所）に `{#if currentProfileTarget()}<ProfileModal target={currentProfileTarget()} />{/if}` を設置。

## データフロー

1. ユーザーがアバター/表示名/acct/mentionをクリック → `openProfile(target)` でグローバル状態を更新。
2. トップレベルの `{#if}` が反応し `ProfileModal` をマウント。
3. `ProfileModal` が `get_user_profile` を呼び、プロフィール本体とフォロー状態を取得・表示。並行して1ページ目のノート一覧も取得。
4. フォローボタン操作、フォロー中/フォロワー数クリック、「カラムとして追加」操作はそれぞれ独立したコマンド呼び出し・モーダル遷移として処理される。
5. `closeProfile()`（閉じるボタン・Escape・オーバーレイクリック、いずれも既存 `Modal.svelte` の挙動）でグローバル状態をクリアしてモーダルを閉じる。

## エラーハンドリング

- `get_user_profile` 失敗（ユーザー削除済み・ネットワークエラー等）: モーダル内にエラーメッセージとリトライボタンを表示。モーダル自体は開いたまま。
- フォロー/フォロー解除の失敗: 楽観的更新をロールバックし、モーダル内に一時的なエラーメッセージを表示。
- ノート一覧の追加取得失敗: 一覧末尾にリトライ可能なエラー表示を出す（既存カラムのページネーションエラー処理があればそれに合わせる）。

## テスト

- Rust: `api/users.rs` の各ラッパー、`commands/user.rs` のハンドラ（アカウント未存在・自分自身のuser_id指定時にrelationがNoneになる等の分岐）。
- フロントエンド: `profileModal.svelte.ts` の状態遷移、`ProfileModal.svelte`（ローディング/成功/エラー/フォロー中→フォロー解除等の表示切り替え）、`NoteCard.svelte`・`MfmNode.svelte` のクリックハンドラが `openProfile` を正しい引数で呼ぶこと。
- 既存のMisskey実接続テスト（`#[ignore]`）パターンに倣い、`users/show`・`users/relation` 系も同様に用意するか検討（実装計画側で判断）。
