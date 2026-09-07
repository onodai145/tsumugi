# 猫対応: アバターに猫耳を表示する (Issue #41)

## 背景・目的

Issue #41「猫対応」: `isCat`なユーザーのアバターに猫耳を付ける。にゃん語化(nyaize)は`isCat`を条件に既に対応済みで、本設計はその続き。

Misskey本家(`MkAvatar.vue`)を参考実装として踏襲する:

- 画像アセット不要のCSSトリック(`border-radius`+`rotate`+`skew`)で耳の三角形を描画
- 耳の色はアバター画像の`avatarBlurhash`から抽出した平均色(`currentColor`経由)
- ホバー時に耳が揺れるwiggleアニメーション

## スコープ

**対象**(ユーザーのアバターが表示され、`isCat`判定が可能な箇所):

- `NoteCard.svelte`(投稿者アバター)
- `ProfileModal.svelte`(プロフィールアバター)
- `FollowListModal.svelte`(フォロー/フォロワー一覧)
- `NotificationCard.svelte`(通知の相手アバター)
- `ReactionUsersPopover.svelte`(リアクションしたユーザー一覧)
- `AccountSelect.svelte` / `AccountsSection.svelte`(ログイン中アカウント切替UI。自分自身のアバター)

**対象外**:

- `MfmNode.svelte`のメンションアバター(本文中の`@user`インライン表示) — 対象に含めるとメンションを含む投稿の描画コストが増えるうえ、インライン表示は小さすぎて耳の視認性が低いため見送り。将来必要になれば別Issueで検討する。
- 猫耳表示のON/OFF設定 — nyaizeと同様、`isCat`のみを条件とし、ユーザー設定トグルは設けない(本家もユーザー単位の表示設定は持たない)。

## データモデル変更

Misskeyの`avatarBlurhash`(base83エンコードされたBlurHash文字列)を新たに取り込む。

### `domain::User`(`src-tauri/src/domain/user.rs`)

```rust
pub avatar_blurhash: Option<String>,
```

`#[serde(default)]`を付与し、追加前にキャッシュされたJSON(`note.payload`)との後方互換を保つ(既存の`emojis`フィールドと同じ扱い)。

### `domain::Account`(`src-tauri/src/domain/account.rs`)

自アカウント切替UI(`AccountSelect`/`AccountsSection`)でも猫耳を出すため、`Account`にも同様のフィールドを追加する:

```rust
#[serde(default)]
pub is_cat: bool,
#[serde(default)]
pub avatar_blurhash: Option<String>,
```

`Account`は設定用JSONストア(`SettingsStore`、SQLiteではなくJSONファイル)に保存されるため、`#[serde(default)]`だけで既存データとの後方互換が取れる。SQLマイグレーションは不要。

## バックエンド変更

### 1. `api/normalize.rs`: `RawUser` → `User`

`RawUser`に`avatar_blurhash: Option<String>`(`#[serde(default)]`)を追加し、`From<RawUser> for User`のマッピングに`avatar_blurhash: r.avatar_blurhash`を追加。

### 2. `commands/account.rs`: `build_account`

`RawUser`は`is_cat`・`avatar_blurhash`を既に(または追加後)持つので、`build_account`内で`Account { ..., is_cat: raw.is_cat, avatar_blurhash: raw.avatar_blurhash.clone(), }`を設定する。

### 3. SQLiteノートキャッシュ(`store/db.rs` + `store/user_ref.rs`)

`user`テーブルは正規化されたユーザー参照キャッシュ(表示は`note.payload`のJSONが正だが、`user_ref.rs`はミューテーション時の補完・SQL射影用に列を持つ)。既存の`bio`/`banner_url`/`instance_name`列追加と同じパターンで:

- `db.rs`の`migrate_cache`に`ALTER TABLE user ADD COLUMN avatar_blurhash TEXT;`を追加するマイグレーションブロックを追加(既存の列追加ブロックと同様、`PRAGMA table_info`等で列有無を確認してから実行)
- `user_ref.rs`の`upsert_user`/`fill_user_from_snapshot`/`fetch_users_by_ids`のSELECT/INSERT列リストと`User`構築箇所に`avatar_blurhash`を追加

### 4. Postgresキャッシュバックエンド(`store/postgres_backend.rs` + `store/postgres_user_ref.rs`)

SQLite版と同じ変更をPostgres側にも反映(この2ファイルは意図的に同じ列構成を保つ設計):

- `postgres_backend.rs`の`ensure_schema`に`avatar_blurhash TEXT`列を追加(既存テーブルへの`ALTER TABLE ... ADD COLUMN IF NOT EXISTS`)
- `postgres_user_ref.rs`の`upsert_user`/`fill_user_from_snapshot`/`fetch_users_by_ids`を同様に更新

### 5. `Account`構築箇所の更新

`grep -rn "Account {" src-tauri/src`で見つかる全リテラル構築箇所(`state.rs`・`commands/user.rs`のテストヘルパー・`commands/account.rs::build_account`・`stream/connection.rs`・`store/settings.rs`本体とテストヘルパー・`session/account_manager.rs`のテストヘルパー)に`is_cat`・`avatar_blurhash`フィールドを追加。`store/settings.rs::migrate_from_legacy_sqlite`(旧SQLite設定からの一回限り移行)は列自体が存在しないため`is_cat: false, avatar_blurhash: None`で固定する。

### 6. TS bindings再生成

`cargo test`実行時に`frontend/src/bindings/tauri.gen.ts`が自動再生成される(既存の仕組み)。手動編集不要。

## フロントエンド変更

### 1. BlurHash平均色抽出ユーティリティ

新規`frontend/src/lib/blurhash.ts`に、Misskey本家(`extract-avg-color-from-blurhash.ts`)と等価な関数を移植する:

```ts
export function extractAvgColorFromBlurhash(hash: string | null | undefined): string | undefined {
  if (typeof hash !== "string") return undefined;
  const chars = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~";
  const value = [...hash.slice(2, 6)]
    .map((c) => chars.indexOf(c))
    .reduce((a, c) => a * 83 + c, 0);
  return "#" + value.toString(16).padStart(6, "0");
}
```

BlurHashの先頭数文字はDC成分(画像全体の平均色)を符号化しているため、フルデコードせずこの計算だけで平均色が求まる(本家と同一のロジック)。BlurHashデコードライブラリの依存追加は不要。

### 2. `Avatar.svelte`(新規、`frontend/src/ui/`)

既存の`<img>`/placeholder markupを包む薄いラッパー。サイズ・角丸などは呼び出し側が`class`で指定し、コンポーネント自身は「耳の重ね描画」だけを担当する。

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import { extractAvgColorFromBlurhash } from "../lib/blurhash";

  let {
    isCat = false,
    avatarBlurhash = null,
    class: className = "",
    children,
  }: {
    isCat?: boolean;
    avatarBlurhash?: string | null;
    class?: string;
    children: Snippet;
  } = $props();

  const earColor = $derived(extractAvgColorFromBlurhash(avatarBlurhash) ?? "var(--border)");
</script>

<span class="avatar-frame relative inline-block {className}">
  {@render children()}
  {#if isCat}
    <span class="ears" style="color: {earColor}" aria-hidden="true">
      <span class="ear-left"></span>
      <span class="ear-right"></span>
    </span>
  {/if}
</span>

<style>
  /* Misskey本家 MkAvatar.vue の .cat > .ears 相当を移植。%ベースなので
     avatar-frame のサイズ(呼び出し側のclassで決まる)に自動追従する。 */
  .ears {
    contain: strict;
    position: absolute;
    top: -50%;
    left: -50%;
    width: 100%;
    height: 100%;
    padding: 50%;
    pointer-events: none;
  }
  .ear-left,
  .ear-right {
    contain: strict;
    display: inline-block;
    height: 50%;
    width: 50%;
    background: currentColor;
  }
  .ear-left::after,
  .ear-right::after {
    content: "";
    display: block;
    width: 60%;
    height: 60%;
    margin: 20%;
    background: #df548f;
  }
  .ear-left {
    transform: rotate(37.5deg) skew(30deg);
  }
  .ear-left,
  .ear-left::after {
    border-radius: 25% 75% 75%;
  }
  .ear-right {
    transform: rotate(-37.5deg) skew(-30deg);
  }
  .ear-right,
  .ear-right::after {
    border-radius: 75% 25% 75% 75%;
  }

  @keyframes earwiggleleft {
    from, to { transform: rotate(37.6deg) skew(30deg); }
    25% { transform: rotate(10deg) skew(30deg); }
    50% { transform: rotate(20deg) skew(30deg); }
    75% { transform: rotate(0deg) skew(30deg); }
  }
  @keyframes earwiggleright {
    from, to { transform: rotate(-37.6deg) skew(-30deg); }
    30% { transform: rotate(-10deg) skew(-30deg); }
    55% { transform: rotate(-20deg) skew(-30deg); }
    75% { transform: rotate(0deg) skew(-30deg); }
  }
  @media (prefers-reduced-motion: no-preference) {
    .avatar-frame:hover .ear-left {
      animation: earwiggleleft 1s infinite;
    }
    .avatar-frame:hover .ear-right {
      animation: earwiggleright 1s infinite;
    }
  }
</style>
```

耳の色は`avatarBlurhash`があればその平均色、無ければ既存のアバタープレースホルダーで使われている`var(--border)`(ライト/ダーク両対応済みの中立グレー)にフォールバックする。

### 3. 各呼び出し箇所の変更

既存の`{#if user.avatarUrl}<img class="h-X w-X ... rounded-...">{:else}<placeholder>{/if}`パターンを、サイズ・flex指定を`<Avatar>`側に、`rounded-[var(--avatar-radius,20%)] object-cover`等を中身の`<img>`/placeholderに`h-full w-full`で残す形に変更する。例(`NoteCard.svelte`):

```svelte
<Avatar isCat={inner.user.isCat} avatarBlurhash={inner.user.avatarBlurhash} class="h-[34px] w-[34px] flex-none">
  {#if inner.user.avatarUrl}
    <img class="h-full w-full rounded-[var(--avatar-radius,20%)] object-cover" src={inner.user.avatarUrl} alt="" />
  {:else}
    <div class="avatar h-full w-full rounded-[var(--avatar-radius,20%)] ..."></div>
  {/if}
</Avatar>
```

対象6ファイルすべてで同様の変更を行う。`AccountSelect.svelte`/`AccountsSection.svelte`は`User`ではなく`Account`型の`isCat`/`avatarBlurhash`を使う。

## テスト方針

- **Rust**: `normalize.rs`に`avatarBlurhash`を含むJSONのデシリアライズテストを追加。`user_ref.rs`/`postgres_user_ref.rs`(`#[ignore]`のPostgresテストも既存パターンに合わせて追加)にupsert/fetchのラウンドトリップテストを追加。`db.rs`の`migrate_cache`テストに列追加の検証を1件追加(既存の`migrate_cache_adds_user_normalization_columns`と同パターン)。`commands/account.rs`の`build_account`テストに`is_cat`/`avatar_blurhash`が伝播することを検証するケースを追加。
- **Frontend**: `frontend/src/lib/blurhash.test.ts`を新規作成し、既知のBlurHash文字列→期待する色コードのケースで検証(本家のテストケースがあれば流用、無ければ手計算で1〜2ケース作成)。`Avatar.svelte`は`isCat=true`で耳要素(`.ears`)がDOMに現れ、`false`で現れないことを検証する簡単なコンポーネントテストを追加。既存の`NoteCard.test.ts`/`FollowListModal.test.ts`等はマークアップ変更に伴うセレクタ調整が必要な箇所のみ最小限修正する。

## 非目標

- メンションアバター(`MfmNode.svelte`)への適用
- 猫耳表示のON/OFF設定
- `avatarBlurhash`を使った他の用途(画像読み込み中のプレースホルダー表示など)への展開 — 今回はデータモデルとして持つのみで、猫耳の色計算以外には使わない
