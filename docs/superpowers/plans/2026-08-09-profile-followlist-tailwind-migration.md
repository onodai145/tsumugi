# ProfileModal/FollowListModalのTailwind移行 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Issue #174の第3バッチとして、`FollowListModal.svelte`/`ProfileModal.svelte`の手書きCSSをTailwindユーティリティクラスへ置き換え、既存のshadcn Buttonプリミティブを使う。

**Architecture:** 両コンポーネントの`<style>`ブロックを、`color-mix()`ベースの背景(既存バッチと同じ例外)とセレクタ結合(`li + li`)だけを残した最小限のものに縮小し、それ以外はTailwindユーティリティクラス+Buttonプリミティブに置き換える。データ取得ロジック(`load()`/`loadMore()`/世代番号/`$effect`/`untrack`)は一切変更しない。

**Tech Stack:** Tailwind CSS v4、既存の`@theme`トークンブリッジ、shadcn-svelte Buttonプリミティブ(`$lib/components/ui/button`、新規追加なし)

## Global Constraints

- `Modal`コンポーネントの呼び出し方(`title`/`onclose`のprops)、データ取得ロジック(`load()`/`loadMore()`/`requestGen`/`$effect`/`untrack`)、イベントハンドラ(`onScroll`/`onNotesScroll`/`toggleFollow`/`addAsColumn`)は一切変更しない
- surfaceカラーのマッピング規約: `--surface-1`→`bg-background`、`--surface-2`→`bg-card`、`--surface-3`→`bg-popover`、`--text`→`text-foreground`、`--text-dim`→`text-muted-foreground`、`--accent`→`bg-primary`等、`--danger`→`text-destructive`、`--border`→`border-border`
- `color-mix(in srgb, var(--surface-*) var(--column-opacity, 100%), transparent)`は既存バッチと同じくTailwindユーティリティに変換せず`<style>`に残す
- **条件付きクラスは必ず「1つの完全なクラス文字列を選ぶ三項演算子」の形にする。「同じCSSプロパティを設定する複数のクラスを`class:`ディレクティブや`class={[...]}`配列で個別にON/OFFする」書き方は禁止**(#176の最終レビューで見つかったバグと同じ、生成後CSSのアルファベット順で優先順位が決まり意図通りに上書きされないため)。ただし、shadcn Buttonプリミティブの`class`propに渡す追加クラスは、Buttonが内部で`cn()`(=`tailwind-merge`)経由でBase/variantクラスとマージするため、Buttonのvariantクラスと同じプロパティを上書きする追加クラスを渡しても安全に解決される(`tailwind-merge`が競合を検出し後勝ちで正しく片方を除去するため)。生の要素に直接`class={[...]}`で複数クラスを積む場合とは扱いが異なる点に注意
- Rust側・`theme.ts`・`@theme`ブリッジ(`frontend/src/app.css`)は変更しない
- Buttonプリミティブは既存のものをそのまま使う(shadcn-svelte CLIの再実行は不要)

---

### Task 1: `FollowListModal.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/FollowListModal.svelte`

**Interfaces:**
- Consumes: 既存の`Button`(`$lib/components/ui/button`)
- Produces: 見た目・挙動は現状維持。`ProfileModal.svelte`からの呼び出し方(`kind`/`userId`/`accountId`/`onclose` props)は変更しない

- [ ] **Step 1: `<script>`にButtonのimportを追加**

`<script lang="ts">`ブロック冒頭のimport群に追加:

```ts
import { Button } from "$lib/components/ui/button";
```

- [ ] **Step 2: マークアップを置き換え**

`<Modal title={...} {onclose}>`の中身を以下に置き換える(`<script>`のロジックは変更しない):

```svelte
<Modal title={kind === "followers" ? "フォロワー" : "フォロー中"} {onclose}>
  <ul class="-mx-4 mt-1 max-h-[55vh] list-none overflow-y-auto p-0" onscroll={onScroll}>
    {#each users as entry (entry.user.id)}
      <li>
        <button
          class="list-row flex w-full items-center gap-2.5 px-4 py-[9px] text-left text-foreground"
          onclick={() => openProfile({ userId: entry.user.id }, accountId)}
        >
          {#if entry.user.avatarUrl}
            <img class="h-10 w-10 flex-none rounded-lg object-cover" src={entry.user.avatarUrl} alt="" />
          {:else}
            <div class="avatar-ph h-10 w-10 flex-none rounded-lg"></div>
          {/if}
          <span class="flex min-w-0 flex-col gap-0.5">
            <span class="overflow-hidden text-ellipsis whitespace-nowrap text-[0.88rem] font-semibold"
              ><Mfm
                text={displayName(entry.user)}
                emojis={proxiedEmojiMap(entry.user.emojis, instanceHost)}
                simple
              /></span
            >
            <span class="overflow-hidden text-ellipsis whitespace-nowrap text-[0.78rem] text-muted-foreground"
              >{acct(entry.user)}</span
            >
          </span>
        </button>
      </li>
    {/each}
    {#if busy}<li class="p-2.5 text-center text-[0.8rem] text-muted-foreground">読み込み中…</li>{/if}
  </ul>
  {#if err}
    <p class="my-2 text-[0.82rem] text-destructive">{err}</p>
    <Button variant="outline" size="sm" onclick={loadMore} disabled={busy}>再試行</Button>
  {/if}
</Modal>
```

- [ ] **Step 3: `<style>`ブロックを縮小**

`<style>`ブロック全体を以下に置き換える(`color-mix()`パターンと`li + li`の隣接セレクタだけを残す):

```svelte
<style>
  li + li {
    border-top: 1px solid var(--border);
  }
  .list-row:hover {
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
  }
  .avatar-ph {
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  }
</style>
```

- [ ] **Step 4: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 5: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 6: `cargo tauri dev`で目視確認**

リポジトリルートから`cargo tauri dev`を起動し、プロフィールモーダルからフォロー中/フォロワー一覧を開いて以下を確認する:
- 一覧の表示(アバター/表示名/アカウント名)、行区切り線
- 行ホバー時のハイライト
- 行タップでのプロフィール遷移(`openProfile`)
- スクロールでの追加読み込み、エラー時の再試行ボタン

- [ ] **Step 7: Commit**

```bash
git add frontend/src/ui/FollowListModal.svelte
git commit -m "style: FollowListModal.svelteをTailwindクラス+Buttonプリミティブに移行"
```

---

### Task 2: `ProfileModal.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/ProfileModal.svelte`

**Interfaces:**
- Consumes: 既存の`Button`(`$lib/components/ui/button`)。Task 1と独立して実施可能(`FollowListModal`はコンポーネントとして呼び出すだけで、Task 1の内部実装に依存しない)
- Produces: 見た目・挙動は現状維持。呼び出し元からの`target`/`accountId`/`onclose` propsは変更しない

- [ ] **Step 1: `<script>`にButtonのimportを追加**

`<script lang="ts">`ブロック冒頭のimport群に追加:

```ts
import { Button } from "$lib/components/ui/button";
```

- [ ] **Step 2: マークアップを置き換え**

`<Modal title="プロフィール" {onclose}>`の中身を以下に置き換える(`<script>`のロジックは変更しない):

```svelte
<Modal title="プロフィール" {onclose}>
  {#if profileState.status === "loading"}
    <p class="text-[0.85rem] text-muted-foreground">読み込み中…</p>
  {:else if profileState.status === "error"}
    <p class="my-2 text-[0.82rem] text-destructive">{profileState.message}</p>
    <Button variant="outline" size="sm" onclick={load}>再試行</Button>
  {:else}
    {@const profile = profileState.profile}
    {#if profile.user.bannerUrl}
      <img class="block aspect-[3/1] w-[calc(100%+32px)] -mx-4 rounded-md object-cover" src={profile.user.bannerUrl} alt="" />
    {/if}
    <div class={profile.user.bannerUrl ? "flex items-end gap-2.5 -mt-[22px] pl-1" : "mt-2 flex items-end gap-2.5"}>
      {#if profile.user.avatarUrl}
        <img class="h-14 w-14 flex-none rounded-[10px] border-2 border-background object-cover" src={profile.user.avatarUrl} alt="" />
      {:else}
        <div class="avatar-ph h-14 w-14 flex-none rounded-[10px] border-2 border-background"></div>
      {/if}
      <div class="flex min-w-0 flex-1 flex-col gap-px">
        <span class="text-[0.95rem] font-semibold"
          ><Mfm text={displayName(profile.user)} emojis={proxiedEmojiMap(profile.user.emojis, instanceHost)} simple
          /></span
        >
        <span class="text-[0.78rem] text-muted-foreground">{acct(profile.user)}</span>
      </div>
      {#if !profile.isSelf}
        <Button
          size="sm"
          variant={profile.isFollowing ? "outline" : "default"}
          class="flex-none rounded-full {profile.isFollowing ? 'hover:border-destructive hover:text-destructive' : ''}"
          disabled={followBusy}
          onclick={toggleFollow}
        >
          {profile.isFollowing ? "フォロー解除" : "フォロー"}
        </Button>
      {/if}
    </div>
    {#if followErr}<p class="my-2 text-[0.82rem] text-destructive">{followErr}</p>{/if}
    {#if profile.user.bio}
      <p class="mt-2.5 whitespace-pre-wrap break-words text-[0.88rem] leading-normal"
        ><Mfm text={profile.user.bio} emojis={proxiedEmojiMap(profile.user.emojis, instanceHost)} /></p
      >
    {/if}
    <div class="mt-2.5 flex gap-1">
      <!-- aria-label で明示: "フォロー中" の文字列を含む accessible name にすると
           フォロー/フォロー解除トグルボタンを name=/フォロー/ で探すクエリと衝突するため -->
      <Button variant="ghost" size="xs" aria-label="following-count" onclick={() => (followListKind = "following")}>
        <strong class="font-semibold text-foreground">{profile.user.followingCount}</strong> フォロー中
      </Button>
      <Button variant="ghost" size="xs" aria-label="followers-count" onclick={() => (followListKind = "followers")}>
        <strong class="font-semibold text-foreground">{profile.user.followersCount}</strong> フォロワー
      </Button>
      <span class="px-1.5 py-[3px] text-[0.78rem] text-muted-foreground"
        ><strong class="font-semibold text-foreground">{profile.user.notesCount}</strong> ノート</span
      >
    </div>
    <Button variant="outline" size="sm" class="mt-2.5" onclick={addAsColumn}>カラムとして追加</Button>
    <div class="mt-3 flex max-h-[40vh] flex-col gap-2 overflow-y-auto border-t border-border pt-2.5" onscroll={onNotesScroll}>
      {#each notes as note (note.id)}
        <NoteCard {note} {accountId} />
      {/each}
      {#if notesBusy}<p class="m-0 text-center text-[0.85rem] text-muted-foreground">読み込み中…</p>{/if}
      {#if notesErr}
        <p class="my-2 text-[0.82rem] text-destructive">{notesErr}</p>
        <Button variant="outline" size="sm" onclick={() => loadMoreNotes(profile.user.id)} disabled={notesBusy}>再試行</Button>
      {/if}
    </div>
    {#if followListKind}
      <FollowListModal
        kind={followListKind}
        userId={profile.user.id}
        {accountId}
        onclose={() => (followListKind = null)}
      />
    {/if}
  {/if}
</Modal>
```

補足:
- バナーの有無による`.head`の余白切り替えは、`mt-2`(通常時)と`-mt-[22px] pl-1`(バナーあり時)を1つの三項演算子で丸ごと切り替える形にしている(Global Constraintsの「条件付きクラスは完全な文字列を選ぶ三項演算子にする」ルールに従う)。
- フォローボタンの`class`propに渡す`hover:border-destructive hover:text-destructive`は、Buttonが`cn()`(tailwind-merge)経由でvariantクラスとマージするため、`variant="outline"`が持つ`hover:text-foreground`等と衝突せず安全に上書きされる(Global Constraints参照)。
- `stat-btn`/`stat-static`内の`<strong>`要素の色・太さ(元々`.stat-btn strong, .stat-static strong`という共有セレクタだった)は、各`<strong>`に直接`font-semibold text-foreground`を付けることで再現している。

- [ ] **Step 3: `<style>`ブロックを縮小**

`<style>`ブロック全体を以下に置き換える(`color-mix()`パターンだけを残す):

```svelte
<style>
  .avatar-ph {
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  }
</style>
```

- [ ] **Step 4: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 5: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 6: `cargo tauri dev`で目視確認**

リポジトリルートから`cargo tauri dev`を起動し、プロフィールモーダルを開いて以下を確認する:
- バナーあり/なしそれぞれのアバター位置(バナーありの場合はアバターがバナーに重なる配置)
- フォローボタンの色切り替え(未フォロー=塗りつぶし、フォロー中=枠線のみ)、フォロー中ボタンをホバーした時に赤系のハイライトになること
- 自分自身のプロフィールではフォローボタンが表示されないこと
- bio(自己紹介文)の表示、改行が保持されること
- フォロー中/フォロワー数タップでの`FollowListModal`表示(Task 1の内容)
- 「カラムとして追加」ボタンの動作
- 投稿一覧の無限スクロール、エラー時の再試行ボタン

- [ ] **Step 7: Commit**

```bash
git add frontend/src/ui/ProfileModal.svelte
git commit -m "style: ProfileModal.svelteをTailwindクラス+Buttonプリミティブに移行"
```
