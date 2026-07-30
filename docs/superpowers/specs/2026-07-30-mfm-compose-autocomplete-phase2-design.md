# MFM補完(ComposeBar) Phase2 設計書: メンション/ハッシュタグ

関連: Issue #22「MFMが補完されるようにする」(Phase 2)。Phase 1(絵文字/MFM関数)は `docs/superpowers/specs/2026-07-29-mfm-compose-autocomplete-design.md` および PR #135 で完了済み。

## スコープ

`ComposeBar.svelte` の本文textareaで、以下2つのトリガーを追加で補完する。

- メンション `@user` / `@user@host`
- ハッシュタグ `#tag`

Phase 1と異なり、候補はMisskeyインスタンスへのAPI呼び出し(`users/search` / `hashtags/search`)で取得するため、非同期・デバウンスが必要になる点が設計上の主な追加要素。

適用範囲・キー操作・ポップアップUIはPhase 1と同じ(`ComposeBar.svelte` 本文textareaのみ、`↑`/`↓`移動・`Tab`/`Enter`確定(矢印キーで明示的に選ぶまでEnterでは確定しない)・`Escape`で閉じる・`Ctrl+Enter`最優先)。

## バックエンド(Rust)

### `src-tauri/src/api/users.rs`(新規)

Misskey OpenAPI: `POST /users/search`(認証不要、`query`必須、`origin: local|remote|combined`、`limit`最大100)。レスポンスは `User[]`。

```rust
//! ユーザー検索 REST（メンション補完用）。

use crate::api::normalize::RawUser;
use crate::api::MisskeyClient;
use crate::domain::User;
use crate::error::Result;
use serde_json::json;

pub async fn search_users(client: &MisskeyClient, query: &str, limit: u32) -> Result<Vec<User>> {
    let body = json!({
        "query": query,
        "limit": limit,
        "origin": "combined",
        "detail": false,
    });
    let raw: Vec<RawUser> = client.post("users/search", &body).await?;
    Ok(raw.into_iter().map(Into::into).collect())
}
```

既存の `RawUser` → `User` の `From` 実装(`src-tauri/src/api/normalize.rs:35`)をそのまま利用する。`detail: false` はレスポンスを軽量化するための指定(UserLite相当、メンション候補表示に必要な `id`/`username`/`host`/`name`/`avatarUrl` は含まれる)。

### `src-tauri/src/api/hashtags.rs`(新規)

Misskey OpenAPI: `POST /hashtags/search`(認証不要、`query`必須、`limit`最大100)。レスポンスは `string[]`。

```rust
//! ハッシュタグ検索 REST（ハッシュタグ補完用）。

use crate::api::MisskeyClient;
use crate::error::Result;
use serde_json::json;

pub async fn search_hashtags(client: &MisskeyClient, query: &str, limit: u32) -> Result<Vec<String>> {
    let body = json!({
        "query": query,
        "limit": limit,
    });
    client.post("hashtags/search", &body).await
}
```

### Tauriコマンド

`src-tauri/src/commands/note.rs`(既存、compose関連コマンドが集まっているファイル)に追加。既存コマンド(例: `post_note`、`commands/note.rs:22-31`)と同じ形 — `state: State<'_, AppState>` から `state.client_for(&account_id)?` でクライアントを取得し、crateの `Result<T>` 型エイリアス(`crate::error::Result`)をそのまま返す:

```rust
use crate::api::hashtags::search_hashtags as api_search_hashtags;
use crate::api::users::search_users as api_search_users;

/// メンション補完用のユーザー検索。
#[tauri::command]
#[specta::specta]
pub async fn search_users(
    state: State<'_, AppState>,
    account_id: String,
    query: String,
) -> Result<Vec<User>> {
    let client = state.client_for(&account_id)?;
    api_search_users(&client, &query, 10).await
}

/// ハッシュタグ補完用のハッシュタグ検索。
#[tauri::command]
#[specta::specta]
pub async fn search_hashtags(
    state: State<'_, AppState>,
    account_id: String,
    query: String,
) -> Result<Vec<String>> {
    let client = state.client_for(&account_id)?;
    api_search_hashtags(&client, &query, 10).await
}
```

`src-tauri/src/lib.rs` の `specta_builder()` に両コマンドを登録し、`cargo test` で `frontend/src/bindings/tauri.gen.ts` が再生成されることを確認する。

## フロントエンド: トリガー検出の拡張

`frontend/src/lib/mfmCompletion.ts` の `Trigger` 型に追加:

```ts
export type Trigger =
  | { kind: "emoji"; query: string; start: number; end: number }
  | { kind: "fnName"; query: string; start: number; end: number }
  | { kind: "argName"; fnName: string; query: string; start: number; end: number }
  | { kind: "argValue"; fnName: string; argName: string; query: string; start: number; end: number }
  | { kind: "mention"; query: string; start: number; end: number }
  | { kind: "hashtag"; query: string; start: number; end: number };
```

境界判定は既存の絵文字トリガーと同じ正規表現パターン(直前が行頭/空白/開き括弧類)を `@`/`#` にも適用し、`user@example.com` のようなメールアドレスや文中の `#` を誤爆させない。

- **メンション**: `@[a-zA-Z0-9_-]+(@[a-zA-Z0-9_.-]+)?` — ユーザー名部分に続けて、host部(`@host`)まで1つのトリガーとして扱う。`query` にはユーザー名+host部の文字列全体(例: `alice` または `alice@example.com`)をそのまま渡し、Misskey側の `users/search` の `query` パラメータに渡す(部分一致検索はMisskey側の実装に委ねる)。
- **ハッシュタグ**: `#` に続く非空白文字列すべてを `query` とする(Misskeyのハッシュタグは記号を含みうるため、絵文字/fn名のような `[a-zA-Z0-9_]` 限定にしない)。

`detectTrigger` 内での優先順位: 既存の `$[...]` 系検出 → 絵文字検出、に加えてメンション/ハッシュタグ検出を追加する。1つの `:`/`@`/`#` は同時に複数のトリガーにマッチしないよう、カーソル直前から最も近いトリガー開始位置を採用する(既存の絵文字トリガー探索と同様、カーソル直前から後方に走査して最初に見つかったものを採用)。

クエリの最小文字数: **1文字以上**(`@`/`#` だけの状態では検索しない。1文字入力された時点で発火)。

## フロントエンド: 非同期候補取得

Phase 1の `buildCompletionItems` は同期・純粋関数で完結していたが、メンション/ハッシュタグはAPI呼び出しが必要なため、`ComposeBar.svelte` に非同期候補の状態を追加する。

```ts
let asyncCandidates = $state<CompletionItem[]>([]);
let asyncSearchToken = 0; // 古い応答を無視するための世代カウンタ
let debounceTimer: ReturnType<typeof setTimeout> | undefined;

$effect(() => {
  const t = trigger;
  clearTimeout(debounceTimer);
  // トリガーが変わるたびに、非同期/同期を問わず無条件で古い候補と世代トークンを破棄する。
  // ここを早期returnパスだけに限定すると、別のトリガーに移った直後の300ms窓の間
  // 直前のトリガーの候補が残り、誤ったトリガー位置に確定挿入されてしまう。
  asyncCandidates = [];
  asyncSearchToken++;
  if (!t || (t.kind !== "mention" && t.kind !== "hashtag") || t.query.length < 1) return;
  const token = asyncSearchToken;
  debounceTimer = setTimeout(async () => {
    if (!accountId) return;
    try {
      const items =
        t.kind === "mention"
          ? await searchMentionItems(accountId, t.query)
          : await searchHashtagItems(accountId, t.query);
      if (token === asyncSearchToken) asyncCandidates = items; // 古い応答は無視
    } catch {
      if (token === asyncSearchToken) asyncCandidates = []; // 失敗時は黙って0件扱い(ポップアップは自動的に閉じる)
    }
  }, 300);
  // アンマウント時にタイマーが残っていると、コンポーネント破棄後にstateを更新しようとする
  return () => clearTimeout(debounceTimer);
});
```

`candidates` の `$derived` を、トリガー種別に応じて同期/非同期を切り替えるよう変更する:

```ts
const candidates = $derived<CompletionItem[]>(
  !trigger
    ? []
    : trigger.kind === "mention" || trigger.kind === "hashtag"
      ? asyncCandidates
      : buildCompletionItems(trigger, customEmojiList),
);
```

`mfmCompletion.ts` はPhase 1では「DOM非依存の純粋関数のみ」という一貫した責務だったため、IPC呼び出し(副作用)をそのまま追加すると責務が混ざる。Phase 2では新規ファイル `frontend/src/lib/mfmSearch.ts` を切り出し、IPC呼び出し+`CompletionItem`変換をここに置く(`mfmCompletion.ts` は純粋関数のみのまま維持する):

```ts
// frontend/src/lib/mfmSearch.ts
import { commands, unwrap } from "./ipc";
import type { CompletionItem } from "./mfmCompletion";

export async function searchMentionItems(accountId: string, query: string): Promise<CompletionItem[]> {
  const users = await unwrap(commands.searchUsers(accountId, query));
  return users.map((u) => {
    const acct = u.host ? `@${u.username}@${u.host}` : `@${u.username}`;
    return {
      key: `user:${u.id}`,
      label: acct,
      insertText: acct,
      thumbnail: u.avatarUrl ? { type: "avatar" as const, url: u.avatarUrl } : undefined,
    };
  });
}

export async function searchHashtagItems(accountId: string, query: string): Promise<CompletionItem[]> {
  const tags = await unwrap(commands.searchHashtags(accountId, query));
  return tags.map((tag) => ({ key: `tag:${tag}`, label: `#${tag}`, insertText: `#${tag}` }));
}
```

`ComposeBar.svelte` の `$effect`(前掲)はこの2関数をインポートして呼ぶだけにする。

## `CompletionThumbnail` の拡張

`frontend/src/lib/mfmCompletion.ts`:

```ts
export interface CompletionThumbnail {
  type: "custom" | "unicode" | "avatar";
  url?: string;
  char?: string;
}
```

`frontend/src/ui/CompletionPopover.svelte` の描画分岐を1箇所だけ変更し、`"avatar"` を `"custom"` と同じ `<img>` 描画に合流させる:

```svelte
{#if item.thumbnail?.type === "custom" || item.thumbnail?.type === "avatar"}
  <img class="completion-thumb" src={item.thumbnail.url} alt="" />
{:else if item.thumbnail?.type === "unicode"}
  ...
```

## 確定時の挿入

- メンション: `@username`(ローカル)または `@username@host`(リモート)。`applyCompletion` の既存ロジック(トリガー範囲を `insertText` に置換)をそのまま使う。
- ハッシュタグ: `#tag`

## ローディング・エラー時の表示

- API応答待ちの間はポップアップを表示しない(`asyncCandidates` が空のまま = `candidates.length === 0` = `popoverOpen` が `false` になる、既存ロジックのまま)。
- 検索失敗時も黙って0件扱いにする(エラーメッセージ等は表示しない)。

## テスト方針

- `mentionToItem`/`hashtagToItem`、および `detectTrigger` のメンション/ハッシュタグ境界ケース(メールアドレス誤爆防止、`@user@host` の一体トリガー化、ハッシュタグの記号許容)は `frontend/src/lib/mfmCompletion.test.ts` に追記してVitestで検証する。
- デバウンス・IPC呼び出しを含む `ComposeBar.svelte` の非同期ロジックは、Phase 1と同じ理由(グローバルストア一式のモックコストが不釣り合い)で自動テストを追加せず、`cargo tauri dev` での手動確認とする。
- バックエンドの `search_users`/`search_hashtags` は、Misskeyのレスポンスをそのまま右から左へ流す薄いラッパー(リクエストボディ組み立て+`RawUser`→`User`変換のみ)であり、変換ロジック自体は既存の `RawUser`→`User` の `From` 実装で別途担保されているため、専用の自動テストは追加しない。

## 非対応・既知の制約

- `@user@host` 入力中、host部の入力途中でも `users/search` の `query` にそのまま渡す(Misskey側の検索精度に依存する)。ユーザー名部分とhost部分を分離して別々にAPIへ渡す高度な補完(本家Misskeyクライアントの一部実装)は本Phaseでは行わない。
- ハッシュタグの大文字小文字正規化・トレンド順ソートなど、Misskey側の検索結果の並び順にそのまま従う(クライアント側での独自ソートは行わない)。
