# 設定画面Tailwind移行 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Issue #174の第7バッチ(最終)として、`Settings.svelte`(設定モーダルのシェル)と`frontend/src/ui/settings/`配下8ファイルの手書きCSSをTailwindユーティリティクラスへ移行する。

**Architecture:** `Modal.svelte`に任意の`width` prop(既定`"480px"`)を追加し、`Settings.svelte`をこれ経由に統一する(既存4呼び出し元には影響なし)。各セクションファイルの`<style>`ブロックを、`color-mix()`パターンだけを残した最小限のもの(またはゼロ)に縮小し、それ以外はTailwindユーティリティクラスに置き換える。条件付きクラスの衝突は「1つの完全なクラス文字列を選ぶ三項演算子」で解消する。`<script>`ロジックは一切変更しない。

**Tech Stack:** Tailwind CSS v4、既存の`@theme`トークンブリッジ

## Global Constraints

- 各ファイルの`<script>`ブロックのロジックは一切変更しない
- surfaceカラーのマッピング規約: `--surface-1`→`bg-background`、`--surface-2`→`bg-muted`、`--surface-3`→`bg-accent`、`--text`→`text-foreground`、`--text-dim`→`text-muted-foreground`、`--accent`→`bg-primary`/`text-primary`/`border-primary`(用途に応じて)、`--border`→`border-border`、`--danger`→`text-destructive`
- `--success`/`--warning`/`--info`は`@theme`ブリッジ未対応のため、アービトラリ値(`text-[var(--success)]`等)を使う
- 元CSSが保存ボタンの文字色に`color: #fff`をハードコードしている箇所(`.save`/`.del`)は、テーマの`--color-destructive-foreground`等のセマンティックトークンに変換せず`text-white`のまま維持する(そのトークンの実値はテーマにより`--surface-1`(ライト/ダークで白/黒)へ変わり、常に白だった元の見た目を変えてしまうため)
- **条件付きクラスは必ず「1つの完全なクラス文字列を選ぶ三項演算子」の形にする。「同じCSSプロパティを設定する複数のクラスを`class:`ディレクティブや`class={[...]}`配列で個別にON/OFFする」書き方は禁止**(#176/#178/#180で見つかった同種バグの再発防止)
- ピクセル値がTailwindの標準スペーシングスケールに正確に乗らない場合はアービトラリ値(`px-[9px]`等)を使う
- `color-mix(in srgb, var(--accent) N%, transparent)`パターンは既存バッチと同じくTailwindユーティリティに変換せず`<style>`に残す。この場合、要素には引き続き対応する素のクラス名も付与し、`<style>`側のセレクタが引き続きマッチするようにする
- `:global()`セレクタ(Lucideアイコンへのスタイリング)は、アイコンコンポーネントに直接`class`を渡す形に置き換える
- セグメントボタン群(`.seg-btn:last-child { border-right: none }`)は、Tailwindの`last:border-r-0`variantで表現する(ループのインデックスを意識せず宣言的に書けるため)
- いずれのファイルにも既存テストファイルは無いため、`data-testid`追加は不要

---

### Task 1: `Modal.svelte`に`width` propを追加し、`Settings.svelte`を移行

**Files:**
- Modify: `frontend/src/ui/Modal.svelte`
- Modify: `frontend/src/ui/Settings.svelte`

**Interfaces:**
- Consumes: なし
- Produces: `Modal.svelte`は新たに任意の`width?: string` prop(既定`"480px"`)を受け取れるようになる。既存呼び出し元(`AddColumnModal.svelte`/`ProfileModal.svelte`/`FollowListModal.svelte`/`ComposeBar.svelte`)は`width`を渡していないため、従来通り480pxで動作する。呼び出し元からの`onclose`/`onAddAccount`/`onReauth`/`initial` propsは変更しない

- [ ] **Step 1: `Modal.svelte`に`width` propを追加**

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import { X } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";

  let {
    title,
    onclose,
    children,
    width = "480px",
  }: { title: string; onclose: () => void; children: Snippet; width?: string } = $props();

  // 深くネストされたコンポーネントから呼ばれても
  // content-visibility/containの包含ブロックを脱出できるよう portal で body 直下に置く。
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  let modalEl: HTMLDivElement | undefined;

  $effect(() => {
    modalEl?.focus();
  });
</script>

<div
  class="fixed inset-0 z-[1000] grid items-start justify-items-center bg-black/45 pt-[8vh]"
  use:portal
  onclick={onclose}
  onkeydown={(e) => e.key === "Escape" && onclose()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class={`w-[min(${width},92vw)] rounded-[14px] border border-border bg-background p-4`}
    bind:this={modalEl}
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <header class="mb-3 flex items-center justify-between font-semibold">
      <span>{title}</span>
      <Button variant="ghost" size="icon-xs" onclick={onclose}><X size={16} /></Button>
    </header>
    {@render children()}
  </div>
</div>
```

補足: `` `w-[min(${width},92vw)] ...` `` は`width`という1つの値をテンプレート文字列へ埋め込んでいるだけで、条件によって複数のクラス文字列を切り替えるものではない(=Global Constraintsの禁止パターンには該当しない)。

- [ ] **Step 2: `Settings.svelte`のテンプレートを置き換え(43〜80行目)**

```svelte
<Modal title="設定" {onclose} width="640px">
  <div class="-m-4 flex max-h-[84vh] flex-col overflow-hidden">
    <div class="flex min-h-0 flex-1 border-t border-border">
      <nav class="flex w-40 flex-none flex-col gap-0.5 overflow-y-auto border-r border-border bg-muted px-2 py-2.5">
        {#each nav as item (item.id)}
          <button
            type="button"
            class={active === item.id
              ? "rounded-md bg-primary px-2.5 py-2 text-left text-[0.85rem] text-primary-foreground"
              : "rounded-md px-2.5 py-2 text-left text-[0.85rem] text-foreground hover:bg-background"}
            onclick={() => (active = item.id)}
          >
            {item.label}
          </button>
        {/each}
      </nav>
      <section class="min-w-0 flex-1 overflow-y-auto px-5 py-[18px]">
        {#if active === "accounts"}
          <AccountsSection {onAddAccount} {onReauth} />
        {:else if active === "display"}
          <DisplaySection />
        {:else if active === "reaction"}
          <ReactionSection />
        {:else if active === "data"}
          <DataSection />
        {:else if active === "notify"}
          <NotifySection />
        {:else if active === "mute"}
          <MuteSection />
        {:else if active === "keys"}
          <KeysSection />
        {:else if active === "about"}
          <AboutSection />
        {/if}
      </section>
    </div>
  </div>
</Modal>
```

`<script>`ブロック冒頭のimportに`import Modal from "./Modal.svelte";`を追加する(`X`のimportは不要になるため削除する — closeボタンは`Modal.svelte`が内蔵する)。

- [ ] **Step 3: `<style>`ブロックを削除**

`<style>...</style>`ブロック全体(82〜155行目)を削除する。`color-mix()`は使われておらず不要になる。

- [ ] **Step 4: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 5: `pnpm test`を実行**

Run: `cd frontend && pnpm test`
Expected: 既存テスト(247/247)が壊れていないことを確認する(`Modal.svelte`を使う既存コンポーネントのテストが影響を受けていないか特に確認)

- [ ] **Step 6: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 7: Commit**

```bash
git add frontend/src/ui/Modal.svelte frontend/src/ui/Settings.svelte
git commit -m "style: Modal.svelteにwidth propを追加しSettings.svelteをTailwind移行"
```

---

### Task 2: `AboutSection.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/settings/AboutSection.svelte`

**Interfaces:**
- Consumes: なし
- Produces: 見た目・挙動は現状維持。propsなし

- [ ] **Step 1: マークアップを置き換え(19〜44行目)**

```svelte
<div class="flex flex-col gap-1">
  <h2 class="m-0 text-[1.2rem] font-bold">tsumugi</h2>
  <p class="mb-3 mt-0 text-[0.85rem] text-muted-foreground">Misskey マルチカラムデスクトップクライアント</p>

  {#if app.updateAvailable}
    <button
      type="button"
      class="update-banner mb-3 mt-1 block w-full rounded-lg border border-primary px-2.5 py-2 text-left font-[inherit] text-[0.82rem] text-foreground"
      onclick={() => openUrl(app.updateAvailable!.url)}
    >
      新しいバージョン v{app.updateAvailable.version} が公開されています(クリックで開く)
    </button>
  {/if}

  <dl class="m-0 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5">
    <dt class="text-[0.82rem] text-muted-foreground">バージョン</dt>
    <dd class="m-0 break-all text-[0.85rem]">{appVersion ?? "…"}</dd>

    <dt class="text-[0.82rem] text-muted-foreground">コミット</dt>
    <dd class="m-0 break-all text-[0.85rem]">{commitHash ?? "…"}</dd>

    <dt class="text-[0.82rem] text-muted-foreground">ライセンス</dt>
    <dd class="m-0 break-all text-[0.85rem]">MIT</dd>

    <dt class="text-[0.82rem] text-muted-foreground">リポジトリ</dt>
    <dd class="m-0 break-all text-[0.85rem]">
      <button type="button" class="border-0 bg-transparent p-0 text-left text-[0.85rem] text-primary hover:underline" onclick={() => openUrl(REPO_URL)}>{REPO_URL}</button>
    </dd>
  </dl>
</div>
```

- [ ] **Step 2: `<style>`ブロックを縮小**

`<style>...</style>`ブロック全体(46〜103行目)を以下に置き換える(`color-mix()`パターンのみ残す):

```svelte
<style>
  .update-banner {
    background: color-mix(in srgb, var(--accent) 15%, transparent);
  }
</style>
```

- [ ] **Step 3: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 4: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/settings/AboutSection.svelte
git commit -m "style: AboutSection.svelteをTailwindクラスに移行"
```

---

### Task 3: `AccountsSection.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/settings/AccountsSection.svelte`

**Interfaces:**
- Consumes: なし
- Produces: 見た目・挙動は現状維持。呼び出し元(`Settings.svelte`)からの`onAddAccount`/`onReauth` propsは変更しない

- [ ] **Step 1: マークアップを置き換え(37〜81行目)**

```svelte
<h3 class="mb-3.5 mt-0 text-base font-semibold">アカウント</h3>

{#if app.accounts.length === 0}
  <p class="mb-3.5 mt-0 text-[0.76rem] text-muted-foreground">ログイン中のアカウントはありません。</p>
{:else}
  <ul class="m-0 mb-3 flex list-none flex-col gap-1.5 p-0">
    {#each app.accounts as a (a.id)}
      <li class="flex items-center gap-2.5 rounded-lg border border-border bg-muted p-2">
        {#if a.avatarUrl}
          <img class="h-[34px] w-[34px] flex-none rounded-lg object-cover" src={a.avatarUrl} alt="" />
        {:else}
          <div class="grid h-[34px] w-[34px] flex-none place-items-center rounded-lg bg-accent font-bold text-muted-foreground">{(a.displayName || a.username).charAt(0)}</div>
        {/if}
        <div class="min-w-0 flex-1">
          <div class="overflow-hidden text-ellipsis whitespace-nowrap text-[0.86rem] font-semibold">{a.displayName || a.username}{#if a.id === app.defaultAccountId()}<span class="default-badge ml-1.5 rounded px-1.5 py-px text-[0.68rem] font-semibold text-primary">既定</span>{/if}</div>
          <div class="overflow-hidden text-ellipsis whitespace-nowrap text-[0.76rem] text-muted-foreground">@{a.username}@{a.host}</div>
        </div>
        {#if confirmId === a.id}
          <div class="flex flex-none items-center gap-1.5 text-[0.78rem] text-muted-foreground">
            <span>削除？</span>
            <button type="button" class="rounded-md bg-destructive px-2.5 py-[5px] text-[0.78rem] text-white disabled:opacity-50" disabled={busyId === a.id} onclick={() => remove(a.id)}>
              {busyId === a.id ? "…" : "はい"}
            </button>
            <button type="button" class="rounded-md border border-border bg-background px-2.5 py-[5px] text-[0.78rem] text-foreground" onclick={() => (confirmId = null)}>いいえ</button>
          </div>
        {:else}
          {#if a.id !== app.defaultAccountId()}
            <button type="button" class="rounded-md border border-border bg-background px-2.5 py-[5px] text-[0.78rem] text-foreground" onclick={() => makeDefault(a.id)}>既定に設定</button>
          {/if}
          <button type="button" class="rounded-md border border-border bg-background px-2.5 py-[5px] text-[0.78rem] text-foreground" onclick={() => onReauth(a)}>再認証</button>
          <button type="button" class="rounded-md border border-border bg-background px-2.5 py-[5px] text-[0.78rem] text-foreground" onclick={() => (confirmId = a.id)}>削除</button>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<p class="mb-3.5 mt-0 text-[0.76rem] text-muted-foreground">
  アカウントを削除すると、そのアカウントのカラム(タブ)も表示されなくなり、保存済みトークンは keyring から破棄されます。
</p>

<div class="flex justify-start">
  <button type="button" class="rounded-md border border-primary bg-transparent px-4 py-[7px] font-semibold text-primary" onclick={onAddAccount}>＋ アカウントを追加</button>
</div>
{#if err}<p class="mt-2 mb-0 text-[0.82rem] text-destructive">{err}</p>{/if}
```

補足: 「ログイン中のアカウントはありません」時の`<p class="hint">`は、元CSSの`.hint`と同じ`margin: 0 0 14px`(mb-3.5 mt-0)クラスを使う。

- [ ] **Step 2: `<style>`ブロックを縮小**

`<style>...</style>`ブロック全体(83〜199行目)を以下に置き換える(`color-mix()`パターンのみ残す):

```svelte
<style>
  .default-badge {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
</style>
```

- [ ] **Step 3: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 4: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/settings/AccountsSection.svelte
git commit -m "style: AccountsSection.svelteをTailwindクラスに移行"
```

---

### Task 4: `DataSection.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/settings/DataSection.svelte`

**Interfaces:**
- Consumes: なし
- Produces: 見た目・挙動は現状維持。propsなし

- [ ] **Step 1: マークアップを置き換え(41〜70行目)**

```svelte
<label class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
  <span class="text-muted-foreground">ノートキャッシュの保持件数上限(件, 0〜100000。0で無制限)</span>
  <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="number" min="0" max="100000" step="500" bind:value={noteCacheLimit} />
</label>
<label class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
  <span class="text-muted-foreground">ノートキャッシュの保持日数上限(日, 0〜3650。0で無制限)</span>
  <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="number" min="0" max="3650" step="1" bind:value={noteCacheMaxAgeDays} />
</label>
<label class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
  <span class="text-muted-foreground">ノートキャッシュのサイズ上限(MB, 0〜10000。0で無制限)</span>
  <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="number" min="0" max="10000" step="50" bind:value={noteCacheMaxSizeMb} />
</label>
<p class="mb-4 mt-0 text-[0.75rem] text-muted-foreground">
  ローカルDBに保持するノートの上限です。件数・投稿からの経過日数・DBファイルサイズのいずれかを
  超えた分は古い順に自動で削除されます。すべて0にすると無制限に溜め続けます
  (ディスク容量を圧迫する可能性があります)。
</p>

<label class="mb-2 flex items-center gap-2 text-[0.88rem]"><input type="checkbox" bind:checked={enableFileLogging} /> 動作ログをファイルに残す(デバッグ用)</label>
<p class="mb-4 mt-0 text-[0.75rem] text-muted-foreground">
  WebSocket再接続やpingタイムアウトなどの内部ログを、アプリのログディレクトリにファイルとして
  永続化します。通知が来るタイミングがおかしい等の不具合調査用で、既定はOFFです。
  切り替えは次回起動から反映されます。
</p>

<div class="flex items-center justify-end gap-3">
  {#if saved}<span class="text-[0.8rem] text-[var(--success)]">保存しました</span>{/if}
  <button type="button" class="rounded-lg bg-primary px-4.5 py-[7px] font-semibold text-white disabled:cursor-default disabled:opacity-50" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</button>
</div>
{#if err}<p class="mt-2 mb-0 text-[0.82rem] text-destructive">{err}</p>{/if}
```

- [ ] **Step 2: `<style>`ブロックを削除**

`<style>...</style>`ブロック全体(72〜131行目)を削除する。`color-mix()`等の変換不能パターンは含まれていないため不要になる。

- [ ] **Step 3: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 4: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/settings/DataSection.svelte
git commit -m "style: DataSection.svelteをTailwindクラスに移行"
```

---

### Task 5: `KeysSection.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/settings/KeysSection.svelte`

**Interfaces:**
- Consumes: なし
- Produces: 見た目・挙動は現状維持。propsなし

- [ ] **Step 1: マークアップを置き換え(93〜146行目)**

```svelte
<div class="mb-2 flex items-center justify-between">
  <h3 class="m-0 text-base font-semibold">キー操作</h3>
  <button type="button" class="rounded-md border border-border bg-background px-2.5 py-1 text-[0.76rem] text-foreground disabled:cursor-default disabled:opacity-40" disabled={busy || Object.keys(overrides).length === 0} onclick={resetAll}>
    すべて既定に戻す
  </button>
</div>
<p class="my-1.5 mb-2.5 text-[0.76rem] text-muted-foreground">
  「変更」を押して割り当てたいキーを押してください(Esc でキャンセル)。タイムライン上でフォーカス中カラムの選択ノートを操作します。
</p>

<table class="my-1.5 w-full border-collapse text-[0.84rem]">
  <tbody>
    {#each ACTIONS as a (a.action)}
      <tr>
        <td class="w-[34%] whitespace-nowrap border-b border-border px-1.5 py-[5px] align-middle">
          {#if capturing === a.action}
            <span class="text-[0.8rem] text-primary">キー入力待ち…</span>
          {:else}
            <kbd class="inline-block rounded-[5px] border border-b-2 border-border bg-muted px-[7px] py-0.5 font-[ui-monospace,monospace] text-[0.78rem]">{prettyChord(effectiveChord(a.action, overrides))}</kbd>
            {#if isCustom(a.action)}<span class="ml-1.5 text-[0.68rem] text-primary">変更済</span>{/if}
          {/if}
        </td>
        <td class="border-b border-border px-1.5 py-[5px] align-middle text-foreground">{a.label}</td>
        <td class="w-[22%] whitespace-nowrap border-b border-border px-1.5 py-[5px] text-right align-middle">
          {#if capturing === a.action}
            <button type="button" class="ml-1 rounded-md border border-border bg-background px-2.5 py-[3px] text-[0.76rem] text-foreground" onclick={cancel}>キャンセル</button>
          {:else}
            <button type="button" class="ml-1 rounded-md border border-border bg-background px-2.5 py-[3px] text-[0.76rem] text-foreground disabled:cursor-default disabled:opacity-40" disabled={busy} onclick={() => startCapture(a.action)}>変更</button>
            {#if isCustom(a.action)}
              <button type="button" class="ml-1 rounded-md border border-border bg-background px-2.5 py-[3px] text-[0.76rem] text-foreground disabled:cursor-default disabled:opacity-40" disabled={busy} onclick={() => resetOne(a.action)}>既定</button>
            {/if}
          {/if}
        </td>
      </tr>
    {/each}
  </tbody>
</table>

{#if err}<p class="my-1.5 text-[0.82rem] text-destructive">{err}</p>{/if}

<div class="mt-3.5">
  <div class="mb-0.5 text-[0.74rem] text-muted-foreground">固定(変更不可)</div>
  <table class="my-1.5 w-full border-collapse text-[0.84rem]">
    <tbody>
      {#each fixed as f (f.combo)}
        <tr>
          <td class="w-[34%] whitespace-nowrap border-b border-border px-1.5 py-[5px] align-middle"><kbd class="inline-block rounded-[5px] border border-b-2 border-border bg-muted px-[7px] py-0.5 font-[ui-monospace,monospace] text-[0.78rem]">{f.combo}</kbd></td>
          <td class="border-b border-border px-1.5 py-[5px] align-middle text-foreground">{f.desc}</td>
          <td class="w-[22%] whitespace-nowrap border-b border-border px-1.5 py-[5px] text-right align-middle"></td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
```

- [ ] **Step 2: `<style>`ブロックを削除**

`<style>...</style>`ブロック全体(148〜247行目)を削除する。`color-mix()`等の変換不能パターンは含まれていないため不要になる。

- [ ] **Step 3: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 4: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/settings/KeysSection.svelte
git commit -m "style: KeysSection.svelteをTailwindクラスに移行"
```

---

### Task 6: `MuteSection.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/settings/MuteSection.svelte`

**Interfaces:**
- Consumes: なし
- Produces: 見た目・挙動は現状維持。propsなし

- [ ] **Step 1: マークアップを置き換え(37〜57行目)**

```svelte
<h3 class="mb-2 mt-0 text-base font-semibold">NG(ミュート)</h3>
<p class="mb-3.5 mt-0 text-[0.78rem] text-muted-foreground">1行につき1件。以降に受信するノートに適用され、表示中の該当ノートも消えます。</p>

<label class="mb-2.5 flex flex-col gap-1 text-[0.82rem]">
  <span class="text-muted-foreground">NGワード(本文/CWに含むと非表示・部分一致)</span>
  <textarea class="resize-y rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground" rows="3" placeholder={"ネタバレ\nspoiler"} bind:value={words}></textarea>
</label>
<label class="mb-2.5 flex flex-col gap-1 text-[0.82rem]">
  <span class="text-muted-foreground">NGユーザ(@user@host。@は省略可)</span>
  <textarea class="resize-y rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground" rows="2" placeholder={"@spammer@example.com"} bind:value={users}></textarea>
</label>
<label class="mb-2.5 flex flex-col gap-1 text-[0.82rem]">
  <span class="text-muted-foreground">NGインスタンス(host)</span>
  <textarea class="resize-y rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground" rows="2" placeholder={"spam.example"} bind:value={instances}></textarea>
</label>

<div class="mt-1 flex items-center justify-end gap-3">
  {#if saved}<span class="text-[0.8rem] text-[var(--success)]">保存しました</span>{/if}
  <button type="button" class="rounded-md bg-primary px-[18px] py-[7px] font-semibold text-white disabled:opacity-50" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</button>
</div>
{#if err}<p class="mt-2 mb-0 text-[0.82rem] text-destructive">{err}</p>{/if}
```

- [ ] **Step 2: `<style>`ブロックを削除**

`<style>...</style>`ブロック全体(59〜117行目)を削除する。`color-mix()`等の変換不能パターンは含まれていないため不要になる。

- [ ] **Step 3: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 4: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/settings/MuteSection.svelte
git commit -m "style: MuteSection.svelteをTailwindクラスに移行"
```

---

### Task 7: `NotifySection.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/settings/NotifySection.svelte`

**Interfaces:**
- Consumes: 既存の`Dropdown`(`../Dropdown.svelte`、変更なし)
- Produces: 見た目・挙動は現状維持。propsなし

- [ ] **Step 1: マークアップを置き換え(64〜99行目)**

```svelte
<h3 class="mb-3.5 mt-0 text-base font-semibold">通知</h3>

<label class="mb-2 flex items-center gap-2 text-[0.88rem]"><input type="checkbox" bind:checked={desktop} /> デスクトップ通知を出す(全体スイッチ)</label>
<label class="mb-2 flex items-center gap-2 text-[0.88rem]"><input type="checkbox" bind:checked={sound} /> 通知音を鳴らす(全体スイッチ)</label>

{#if sound}
  <div class="my-1 mb-3 flex flex-col gap-1.5 text-[0.82rem]">
    <span class="text-muted-foreground">通知音の種類(既定。タブごとに上書き可)</span>
    <Dropdown bind:value={soundMode} options={soundModeOptions} />
    {#if soundMode === "custom"}
      <div class="mb-2 flex items-center gap-2 text-[0.88rem]">
        <button type="button" class="rounded-md border border-border bg-muted px-3 py-1.5 text-[0.8rem] text-foreground hover:border-primary disabled:cursor-default disabled:opacity-50" disabled={pickingSound} onclick={pickSound}>
          {pickingSound ? "読み込み中…" : soundChoice.startsWith("data:") ? "音声を変更" : "音声ファイルを選択"}
        </button>
        {#if soundChoice.startsWith("data:")}
          <button type="button" class="rounded-md border border-border bg-muted px-3 py-1.5 text-[0.8rem] text-foreground hover:border-primary" onclick={() => playNotifySound(soundChoice)}>試聴</button>
        {/if}
      </div>
    {:else}
      <button type="button" class="rounded-md border border-border bg-muted px-3 py-1.5 text-[0.8rem] text-foreground hover:border-primary" onclick={() => playNotifySound(soundMode)}>試聴</button>
    {/if}
  </div>
{/if}

<p class="my-2 mb-4 text-[0.76rem] text-muted-foreground">
  通知は<b>通知カラムへの新着</b>、または<b>通知をONにしたタブへの新着ノート</b>で発火します。
  ここは全タブ共通のマスタースイッチで、タブごとの個別ON/OFFは各タブをダブルクリックして
  編集してください(両方ONのときのみ実際に発火します)。
  {#if !hasNotifyEnabledTab}<br /><span class="text-[var(--warning)]">※ 現在、通知がONのタブがありません。タブをダブルクリック→「このタブの通知」で有効にしてください。</span>{/if}
</p>

<div class="flex items-center justify-end gap-3">
  {#if saved}<span class="text-[0.8rem] text-[var(--success)]">保存しました</span>{/if}
  <button type="button" class="rounded-md bg-primary px-[18px] py-[7px] font-semibold text-white disabled:opacity-50" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</button>
</div>
{#if err}<p class="mt-2 mb-0 text-[0.82rem] text-destructive">{err}</p>{/if}
```

- [ ] **Step 2: `<style>`ブロックを削除**

`<style>...</style>`ブロック全体(101〜175行目)を削除する。`color-mix()`等の変換不能パターンは含まれていないため不要になる(`.mini-btn:hover`は素の`var(--accent)`参照)。

- [ ] **Step 3: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 4: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/settings/NotifySection.svelte
git commit -m "style: NotifySection.svelteをTailwindクラスに移行"
```

---

### Task 8: `ReactionSection.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/settings/ReactionSection.svelte`

**Interfaces:**
- Consumes: 既存の`ReactionPicker`(`../../input/ReactionPicker.svelte`、変更なし)/`UnicodeEmoji`(変更なし)
- Produces: 見た目・挙動は現状維持。propsなし

- [ ] **Step 1: マークアップを置き換え(90〜129行目)**

```svelte
<h3 class="mb-1.5 mt-0 text-base font-semibold">リアクション</h3>
<p class="mb-3.5 mt-0 text-[0.8rem] text-muted-foreground">絵文字ピッカーの「ピン留め」タブに表示する絵文字を編集できます(本家Misskeyのピン留め絵文字に相当)。ドラッグで並べ替えられます。</p>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="flex flex-wrap items-center gap-2" onpointermove={onPointerMove} onpointerup={onPointerEnd} onpointercancel={onPointerEnd}>
  {#each displayOrder as key, i (key)}
    {@const custom = isCustomEmojiKey(key) ? customEmojiByName(parseCustomEmojiPinKey(key).name) : null}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class={draggingIndex === i
        ? "flex items-center gap-1 rounded-lg border border-border bg-muted px-1.5 py-1 opacity-40"
        : "flex items-center gap-1 rounded-lg border border-border bg-muted px-1.5 py-1"}
      data-chip-index={i}
    >
      <span class="-my-1 flex touch-none cursor-grab items-center justify-center p-2 text-muted-foreground" onpointerdown={(e) => onPointerDown(i, e)} title="ドラッグで並べ替え">
        <GripVertical size={12} />
      </span>
      <span class="flex text-[1.2rem] leading-none">
        {#if isCustomEmojiKey(key)}
          {#if custom}
            <img class="h-[1.3em] w-[1.3em] object-contain" src={custom.url} alt={key} />
          {:else}
            {parseCustomEmojiPinKey(key).name}
          {/if}
        {:else}
          <UnicodeEmoji char={key} />
        {/if}
      </span>
      <button type="button" class="flex rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground" onclick={() => remove(i)} title="削除"><X size={12} /></button>
    </div>
  {/each}
  <button type="button" class="flex h-[34px] w-[34px] items-center justify-center rounded-lg border border-dashed border-border text-muted-foreground hover:border-primary hover:text-primary" onclick={() => (picking = !picking)} title="ピン留めを追加">
    <Plus size={16} />
  </button>
</div>
{#if pinned.length === 0}
  <p class="mb-3.5 mt-0 text-[0.8rem] text-muted-foreground">ピン留めがありません。「＋」から追加できます。</p>
{/if}

{#if picking}
  <div class="mt-3">
    <ReactionPicker {accountId} showPinned={false} onpick={add} />
  </div>
{/if}
{#if err}<p class="mt-2 mb-0 text-[0.82rem] text-destructive">{err}</p>{/if}
```

- [ ] **Step 2: `<style>`ブロックを削除**

`<style>...</style>`ブロック全体(131〜220行目)を削除する。`.chip.dragging`はStep 1の三項演算子で解消済みのため、`color-mix()`等の残す必要のあるパターンは無い。

- [ ] **Step 3: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 4: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/settings/ReactionSection.svelte
git commit -m "style: ReactionSection.svelteをTailwindクラスに移行"
```

---

### Task 9: `DisplaySection.svelte`のTailwind移行(最大)

**Files:**
- Modify: `frontend/src/ui/settings/DisplaySection.svelte`

**Interfaces:**
- Consumes: なし
- Produces: 見た目・挙動は現状維持。propsなし

- [ ] **Step 1: マークアップを置き換え(292〜604行目)**

```svelte
<h3 class="mb-3.5 mt-0 text-base font-semibold">表示</h3>

<div class="mb-3 flex flex-col gap-1.5 text-[0.82rem]">
  <span class="text-muted-foreground">UIモード</span>
  <div class="inline-flex w-fit overflow-hidden rounded-md border border-border">
    {#each uiModes as m (m.id)}
      <button
        type="button"
        class={uiMode === m.id
          ? "border-r border-border bg-primary px-3.5 py-1.5 text-[0.82rem] text-primary-foreground last:border-r-0"
          : "border-r border-border bg-muted px-3.5 py-1.5 text-[0.82rem] text-foreground last:border-r-0"}
        onclick={() => (uiMode = m.id)}
      >{m.label}</button>
    {/each}
  </div>
  <p class="mb-4 mt-0 text-[0.76rem] text-muted-foreground">モバイル版は投稿欄がFAB+モーダルに、PC版は投稿欄が常時表示になります。</p>
</div>

<div class="mb-3 flex flex-col gap-1.5 text-[0.82rem]">
  <span class="text-muted-foreground">テーマ</span>
  <div class="inline-flex w-fit overflow-hidden rounded-md border border-border">
    {#each themes as t (t.id)}
      <button
        type="button"
        class={theme === t.id
          ? "border-r border-border bg-primary px-3.5 py-1.5 text-[0.82rem] text-primary-foreground last:border-r-0"
          : "border-r border-border bg-muted px-3.5 py-1.5 text-[0.82rem] text-foreground last:border-r-0"}
        onclick={() => (theme = t.id)}
      >{t.label}</button>
    {/each}
  </div>
</div>

{#snippet swatchStrip(colors: ThemeColors)}
  <span class="flex h-[30px] w-full flex-none">
    {#each THEME_VAR_KEYS as v (v.key)}
      <span class="h-full flex-1" style={`background:${colors[v.key]}`}></span>
    {/each}
  </span>
{/snippet}

<div class="mb-3 flex flex-col gap-1.5 text-[0.82rem]">
  <span class="text-muted-foreground">プリセットテーマ</span>
  <div class="grid grid-cols-[repeat(auto-fill,minmax(132px,1fr))] gap-2.5">
    {#each PRESETS as p (p.id)}
      {@const isActive = theme === `preset:${p.id}`}
      <button
        type="button"
        class={isActive
          ? "flex w-full flex-col overflow-hidden rounded-lg border border-primary bg-muted p-0 text-left text-[0.78rem] text-foreground shadow-[0_0_0_1px_var(--accent)]"
          : "flex w-full flex-col overflow-hidden rounded-lg border border-border bg-muted p-0 text-left text-[0.78rem] text-foreground hover:border-primary"}
        onclick={() => (theme = `preset:${p.id}`)}
      >
        {@render swatchStrip(p.colors)}
        <span class="flex items-center justify-between gap-1 px-2.5 py-[7px]">
          {p.name}
          {#if isActive}<Check size={13} class="flex-none text-primary" />{/if}
        </span>
      </button>
    {/each}
  </div>
</div>

<div class="mb-3 flex flex-col gap-1.5 text-[0.82rem]">
  <span class="text-muted-foreground">カスタムテーマ</span>
  <div class="grid grid-cols-[repeat(auto-fill,minmax(132px,1fr))] gap-2.5">
    {#each customThemes as t (t.id)}
      {@const isActive = theme === `custom:${t.id}`}
      <div class="flex flex-col gap-1">
        <button
          type="button"
          class={isActive
            ? "flex w-full flex-col overflow-hidden rounded-lg border border-primary bg-muted p-0 text-left text-[0.78rem] text-foreground shadow-[0_0_0_1px_var(--accent)]"
            : "flex w-full flex-col overflow-hidden rounded-lg border border-border bg-muted p-0 text-left text-[0.78rem] text-foreground hover:border-primary"}
          onclick={() => (theme = `custom:${t.id}`)}
        >
          {@render swatchStrip(t.colors)}
          <span class="flex items-center justify-between gap-1 px-2.5 py-[7px]">
            {t.name}
            {#if isActive}<Check size={13} class="flex-none text-primary" />{/if}
          </span>
        </button>
        <div class="flex gap-1">
          <button type="button" class="flex flex-1 items-center justify-center rounded-md border border-border bg-muted py-[5px] text-muted-foreground hover:border-primary hover:text-primary" title="編集" onclick={() => startEditTheme(t)}><Pencil size={13} /></button>
          <button type="button" class="flex flex-1 items-center justify-center rounded-md border border-border bg-muted py-[5px] text-muted-foreground hover:border-primary hover:text-primary" title="削除" onclick={() => removeCustomTheme(t.id)}><Trash2 size={13} /></button>
        </div>
      </div>
    {/each}
  </div>
  <button type="button" class="mt-2 inline-flex items-center gap-1 rounded-md border border-border bg-muted px-3 py-1.5 text-[0.8rem] text-foreground hover:border-primary" onclick={startCreateTheme}><Plus size={13} /> 新規作成</button>

  {#if editingTheme}
    <div class="mt-2.5 flex flex-col gap-2 rounded-lg border border-border bg-muted p-3">
      <input type="text" class="rounded-md border border-border bg-background px-2.5 py-[7px] font-[inherit] text-foreground" placeholder="テーマ名" bind:value={editingTheme.name} />
      {#each THEME_VAR_KEYS as v (v.key)}
        <div class="flex items-center gap-2">
          <span class="w-20 flex-none text-[0.8rem] text-muted-foreground">{colorLabels[v.key]}</span>
          <span class="h-[22px] w-[22px] flex-none rounded-[5px] border border-border" style={`background:${editingTheme.colors[v.key]}`}></span>
          <input type="text" class="w-[100px] rounded-md border border-border bg-background px-2 py-[5px] font-[ui-monospace,monospace] text-[0.82rem] text-foreground" bind:value={editingTheme.colors[v.key]} />
        </div>
      {/each}
      {#if editErr}<p class="mt-2 mb-0 text-[0.82rem] text-destructive">{editErr}</p>{/if}
      <div class="mt-1 flex justify-end gap-2">
        <button type="button" class="inline-flex items-center gap-1 rounded-md border border-border bg-muted px-3 py-1.5 text-[0.8rem] text-foreground hover:border-primary" onclick={cancelEditTheme}><X size={13} /> キャンセル</button>
        <button type="button" class="rounded-md bg-primary px-[18px] py-[7px] font-semibold text-white" onclick={saveCustomTheme}>このテーマを保存</button>
      </div>
    </div>
  {/if}
</div>

<div class="mb-3 flex flex-col gap-1.5 text-[0.82rem]">
  <span class="text-muted-foreground">コードハイライトテーマ</span>
  <div class="grid grid-cols-[repeat(auto-fill,minmax(132px,1fr))] gap-2.5">
    <button
      type="button"
      class={codeHighlightTheme === "auto"
        ? "flex w-full flex-col overflow-hidden rounded-lg border border-primary bg-muted p-0 text-left text-[0.78rem] text-foreground shadow-[0_0_0_1px_var(--accent)]"
        : "flex w-full flex-col overflow-hidden rounded-lg border border-border bg-muted p-0 text-left text-[0.78rem] text-foreground hover:border-primary"}
      onclick={() => (codeHighlightTheme = "auto")}
    >
      <span class="flex items-center justify-between gap-1 px-2.5 py-[7px]">
        自動(OSに合わせる)
        {#if codeHighlightTheme === "auto"}<Check size={13} class="flex-none text-primary" />{/if}
      </span>
    </button>
    {#each BUNDLED_SHIKI_THEMES as t (t.id)}
      {@const isActive = codeHighlightTheme === `shiki:${t.id}`}
      <button
        type="button"
        class={isActive
          ? "flex w-full flex-col overflow-hidden rounded-lg border border-primary bg-muted p-0 text-left text-[0.78rem] text-foreground shadow-[0_0_0_1px_var(--accent)]"
          : "flex w-full flex-col overflow-hidden rounded-lg border border-border bg-muted p-0 text-left text-[0.78rem] text-foreground hover:border-primary"}
        onclick={() => (codeHighlightTheme = `shiki:${t.id}`)}
      >
        <span class="flex h-[30px] w-full flex-none">
          <span class="h-full flex-1" style={`background:${t.swatch.bg}`}></span>
          <span class="h-full flex-1" style={`background:${t.swatch.fg}`}></span>
          <span class="h-full flex-1" style={`background:${t.swatch.accent}`}></span>
        </span>
        <span class="flex items-center justify-between gap-1 px-2.5 py-[7px]">
          {t.label}
          {#if isActive}<Check size={13} class="flex-none text-primary" />{/if}
        </span>
      </button>
    {/each}
  </div>
</div>

<div class="mb-3 flex flex-col gap-1.5 text-[0.82rem]">
  <span class="text-muted-foreground">カスタムシンタックステーマ</span>
  <div class="grid grid-cols-[repeat(auto-fill,minmax(132px,1fr))] gap-2.5">
    {#each customSyntaxThemes as t (t.id)}
      {@const isActive = codeHighlightTheme === `custom:${t.id}`}
      <div class="flex flex-col gap-1">
        <button
          type="button"
          class={isActive
            ? "flex w-full flex-col overflow-hidden rounded-lg border border-primary bg-muted p-0 text-left text-[0.78rem] text-foreground shadow-[0_0_0_1px_var(--accent)]"
            : "flex w-full flex-col overflow-hidden rounded-lg border border-border bg-muted p-0 text-left text-[0.78rem] text-foreground hover:border-primary"}
          onclick={() => (codeHighlightTheme = `custom:${t.id}`)}
        >
          <span class="flex h-[30px] w-full flex-none">
            {#each SYNTAX_VAR_KEYS as v (v.key)}
              <span class="h-full flex-1" style={`background:${t[v.key]}`}></span>
            {/each}
          </span>
          <span class="flex items-center justify-between gap-1 px-2.5 py-[7px]">
            {t.name}
            {#if isActive}<Check size={13} class="flex-none text-primary" />{/if}
          </span>
        </button>
        <div class="flex gap-1">
          <button type="button" class="flex flex-1 items-center justify-center rounded-md border border-border bg-muted py-[5px] text-muted-foreground hover:border-primary hover:text-primary" title="編集" onclick={() => startEditSyntaxTheme(t)}><Pencil size={13} /></button>
          <button type="button" class="flex flex-1 items-center justify-center rounded-md border border-border bg-muted py-[5px] text-muted-foreground hover:border-primary hover:text-primary" title="削除" onclick={() => removeCustomSyntaxTheme(t.id)}><Trash2 size={13} /></button>
        </div>
      </div>
    {/each}
  </div>
  <button type="button" class="mt-2 inline-flex items-center gap-1 rounded-md border border-border bg-muted px-3 py-1.5 text-[0.8rem] text-foreground hover:border-primary" onclick={startCreateSyntaxTheme}><Plus size={13} /> 新規作成</button>

  {#if editingSyntaxTheme}
    <div class="mt-2.5 flex flex-col gap-2 rounded-lg border border-border bg-muted p-3">
      <input type="text" class="rounded-md border border-border bg-background px-2.5 py-[7px] font-[inherit] text-foreground" placeholder="テーマ名" bind:value={editingSyntaxTheme.name} />
      {#each SYNTAX_VAR_KEYS as v (v.key)}
        <div class="flex items-center gap-2">
          <span class="w-20 flex-none text-[0.8rem] text-muted-foreground">{syntaxColorLabels[v.key]}</span>
          <span class="h-[22px] w-[22px] flex-none rounded-[5px] border border-border" style={`background:${editingSyntaxTheme[v.key]}`}></span>
          <input type="text" class="w-[100px] rounded-md border border-border bg-background px-2 py-[5px] font-[ui-monospace,monospace] text-[0.82rem] text-foreground" bind:value={editingSyntaxTheme[v.key]} />
        </div>
      {/each}
      {#if syntaxEditErr}<p class="mt-2 mb-0 text-[0.82rem] text-destructive">{syntaxEditErr}</p>{/if}
      <div class="mt-1 flex justify-end gap-2">
        <button type="button" class="inline-flex items-center gap-1 rounded-md border border-border bg-muted px-3 py-1.5 text-[0.8rem] text-foreground hover:border-primary" onclick={cancelEditSyntaxTheme}><X size={13} /> キャンセル</button>
        <button type="button" class="rounded-md bg-primary px-[18px] py-[7px] font-semibold text-white" onclick={saveCustomSyntaxTheme}>このテーマを保存</button>
      </div>
    </div>
  {/if}
</div>

<label class="mb-2.5 flex flex-col gap-1 text-[0.82rem]">
  <span class="text-muted-foreground">新規カラムの既定幅(px, 220〜720)</span>
  <input class="w-[140px] rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground" type="number" min="220" max="720" step="10" bind:value={width} />
</label>
<p class="mb-4 mt-0 text-[0.76rem] text-muted-foreground">既定幅は次に追加するカラムから適用されます。既存カラムはカラム端のドラッグで個別調整できます。</p>

<label class="mb-2.5 flex flex-col gap-1 text-[0.82rem]">
  <span class="text-muted-foreground">起動時のギャップ埋め(件, 0〜1000。0で無効)</span>
  <input class="w-[140px] rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground" type="number" min="0" max="1000" step="50" bind:value={gapFillLimit} />
</label>
<p class="mb-4 mt-0 text-[0.76rem] text-muted-foreground">
  アプリを閉じていた間に流れたノートを、起動時にこの件数まで遡ってREST取得します。
  0にすると従来どおりキャッシュのみ表示します。
</p>

<label class="mb-2.5 flex flex-col gap-1 text-[0.82rem]">
  <span class="text-muted-foreground">メディアサムネイルの高さ上限(px, 80〜600)</span>
  <input class="w-[140px] rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground" type="number" min="80" max="600" step="20" bind:value={mediaThumbnailHeight} />
</label>
<p class="mb-4 mt-0 text-[0.76rem] text-muted-foreground">
  ノートに添付された画像/動画のサムネイル最大高さです。小さくするとノートを詰めて表示でき、
  大きくすると画像を大きく見られます。
</p>

<div class="mb-3 flex flex-col gap-1.5 text-[0.82rem]">
  <span class="text-muted-foreground">絵文字のスタイル</span>
  <div class="inline-flex w-fit overflow-hidden rounded-md border border-border">
    {#each emojiStyles as s (s.id)}
      <button
        type="button"
        class={emojiStyle === s.id
          ? "border-r border-border bg-primary px-3.5 py-1.5 text-[0.82rem] text-primary-foreground last:border-r-0"
          : "border-r border-border bg-muted px-3.5 py-1.5 text-[0.82rem] text-foreground last:border-r-0"}
        onclick={() => (emojiStyle = s.id)}
      >
        {#if emojiPreviewUrl(s.id)}
          <img class="mr-1 h-[1.2em] w-[1.2em] object-contain align-[-0.25em]" src={emojiPreviewUrl(s.id)} alt="" />
        {/if}
        {s.label}
      </button>
    {/each}
  </div>
  <p class="mb-4 mt-0 flex flex-wrap items-center gap-1 text-[0.76rem] text-muted-foreground">
    Unicode絵文字(リアクション等)の見た目です。プレビュー:
    {#each ["😺", "👍", "🎉"] as c}
      {#if emojiPreviewUrl(emojiStyle)}
        <img class="h-[1.3em] w-[1.3em] object-contain" src={unicodeEmojiUrl(c, emojiStyle) ?? undefined} alt={c} />
      {:else}
        {c}
      {/if}
    {/each}
  </p>
</div>

<label class="mb-2 flex items-center gap-2 text-[0.85rem]"
  ><input type="checkbox" bind:checked={mfmAnimationEnabled} /> MFMアニメーション($[shake]等)を有効にする</label
>
<p class="mb-4 mt-0 text-[0.76rem] text-muted-foreground">
  他人の投稿に含まれる装飾($[shake]/$[spin]/$[rainbow]等)のアニメーション表示です。
  環境によってはこの描画コストが高く、CPU使用率が上がることがあります
  (Linux/Wayland環境で特に発生しやすい既知の問題です)。気になる場合はOFFにしてください
  (静的な装飾は残ります)。
</p>

<div class="mb-3 flex flex-col gap-1.5 text-[0.82rem]">
  <span class="text-muted-foreground">フォント</span>
  <div class="inline-flex w-fit overflow-hidden rounded-md border border-border">
    {#each fontPresets as p (p.value)}
      <button
        type="button"
        class={fontFamily === p.value
          ? "border-r border-border bg-primary px-3.5 py-1.5 text-[0.82rem] text-primary-foreground last:border-r-0"
          : "border-r border-border bg-muted px-3.5 py-1.5 text-[0.82rem] text-foreground last:border-r-0"}
        onclick={() => (fontFamily = p.value)}
      >
        {p.label}
      </button>
    {/each}
  </div>
  <input
    type="text"
    class="mt-1.5 w-full rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground"
    placeholder='CSS の font-family 値(例: "Noto Sans JP", sans-serif)'
    bind:value={fontFamily}
  />
</div>
<p class="mb-4 mt-0 text-[0.76rem] text-muted-foreground" style={fontFamily ? `font-family: ${fontFamily}` : undefined}>
  プレビュー: あいうえお ABCDEFG 123
</p>

<div class="mb-3 flex flex-col gap-1.5 text-[0.82rem]">
  <span class="text-muted-foreground">背景画像</span>
  <div class="flex items-center gap-2.5">
    {#if backgroundImage}
      <img class="h-9 w-14 rounded-md border border-border object-cover" src={backgroundImage} alt="背景プレビュー" />
    {/if}
    <button type="button" class="rounded-md border border-border bg-muted px-3 py-1.5 text-[0.8rem] text-foreground hover:border-primary disabled:cursor-default disabled:opacity-50" disabled={pickingImage} onclick={pickImage}>
      {pickingImage ? "読み込み中…" : backgroundImage ? "画像を変更" : "画像を選択"}
    </button>
    {#if backgroundImage}
      <button type="button" class="rounded-md border border-border bg-muted px-3 py-1.5 text-[0.8rem] text-foreground hover:border-primary" onclick={clearImage}>解除</button>
    {/if}
  </div>
</div>

{#if backgroundImage}
  <div class="mb-3 flex flex-col gap-1.5 text-[0.82rem]">
    <span class="text-muted-foreground">背景画像の配置方法</span>
    <div class="inline-flex w-fit overflow-hidden rounded-md border border-border">
      {#each BACKGROUND_FIT_MODE_OPTIONS as m (m.value)}
        <button
          type="button"
          class={backgroundFitMode === m.value
            ? "border-r border-border bg-primary px-3.5 py-1.5 text-[0.82rem] text-primary-foreground last:border-r-0"
            : "border-r border-border bg-muted px-3.5 py-1.5 text-[0.82rem] text-foreground last:border-r-0"}
          onclick={() => (backgroundFitMode = m.value)}
        >
          {m.label}
        </button>
      {/each}
    </div>
  </div>
  {#if backgroundFitMode !== "fill"}
    <div class="mb-3 flex flex-col gap-1.5 text-[0.82rem]">
      <span class="text-muted-foreground">基準点</span>
      <div class="grid w-fit grid-cols-[repeat(3,28px)] grid-rows-[repeat(3,28px)] gap-1">
        {#each BACKGROUND_POSITION_GRID as p (p)}
          <button
            type="button"
            class={backgroundPosition === p
              ? "h-[28px] w-[28px] rounded border border-primary bg-primary p-0"
              : "h-[28px] w-[28px] rounded border border-border bg-muted p-0 hover:border-primary"}
            title={positionLabels[p]}
            aria-label={positionLabels[p]}
            onclick={() => (backgroundPosition = p)}
          ></button>
        {/each}
      </div>
    </div>
  {/if}
  <label class="mb-2.5 flex flex-col gap-1 text-[0.82rem]">
    <span class="text-muted-foreground">背景の暗さ({backgroundDim}%)</span>
    <input class="w-full max-w-[320px] accent-primary" type="range" min="0" max="100" step="5" bind:value={backgroundDim} />
  </label>
  <label class="mb-2.5 flex flex-col gap-1 text-[0.82rem]">
    <span class="text-muted-foreground">背景のぼかし({backgroundBlur}px)</span>
    <input class="w-full max-w-[320px] accent-primary" type="range" min="0" max="40" step="2" bind:value={backgroundBlur} />
  </label>
  <label class="mb-2.5 flex flex-col gap-1 text-[0.82rem]">
    <span class="text-muted-foreground">カラムの不透明度({columnOpacity}%)</span>
    <input class="w-full max-w-[320px] accent-primary" type="range" min="60" max="100" step="5" bind:value={columnOpacity} />
  </label>
  <p class="mb-4 mt-0 text-[0.76rem] text-muted-foreground">数値が低いほど背景画像が透けて見えます。</p>
{/if}

<div class="flex items-center justify-end gap-3">
  {#if saved}<span class="text-[0.8rem] text-[var(--success)]">保存しました</span>{/if}
  <button type="button" class="rounded-md bg-primary px-[18px] py-[7px] font-semibold text-white disabled:opacity-50" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</button>
</div>
{#if err}<p class="mt-2 mb-0 text-[0.82rem] text-destructive">{err}</p>{/if}
```

補足:
- 全ての`.seg-btn`ループ(UIモード/テーマ/絵文字スタイル/フォント/背景配置方法、計5箇所)は同一の三項演算子パターン(`last:border-r-0`でグループ最後のボタンだけ右枠線を消す)を使う
- 全ての`.theme-card`ループ(プリセット/カスタムテーマ/コードハイライト自動/バンドル済みシンタックス/カスタムシンタックス、計5箇所)も同一の三項演算子パターンを使う
- `Check`アイコンには直接`class="flex-none text-primary"`を渡し、`:global(.theme-card-check)`セレクタを解消する
- スウォッチ(`style={`background:${colors[...]}`}`)・フォントプレビュー行の`style={fontFamily ? ... : undefined}`はいずれも`<script>`側で計算した動的な値のインラインstyleであり、今回のバッチでは変更しない

- [ ] **Step 2: `<style>`ブロックを削除**

`<style>...</style>`ブロック全体(606〜905行目)を削除する。`color-mix()`は使われておらず不要になる。

- [ ] **Step 3: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 4: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/settings/DisplaySection.svelte
git commit -m "style: DisplaySection.svelteをTailwindクラスに移行"
```

---

### 手動確認(全タスク完了後)

リポジトリルートから`cargo tauri dev`を起動し、以下を確認する:
- 設定モーダルの開閉(幅640px・サイドナビ+ペインの2列レイアウト・高さ上限が効いていること)、他のモーダル(AddColumnModal等)との見た目の一貫性
- サイドナビのアクティブタブ表示・切り替え
- 各セクションの表示・保存動作(About/Accounts/Data/Keys/Mute/Notify/Reaction/Display)
- `DisplaySection`: UIモード/テーマ/絵文字スタイル/フォント/背景配置方法のセグメントボタン、プリセット/カスタムテーマカード、コードハイライトテーマカード、背景配置の基準点グリッド、それぞれのアクティブ状態表示
- `DisplaySection`: カスタムテーマ・カスタムシンタックステーマの新規作成/編集/削除フォーム
- `ReactionSection`: ピン留め絵文字のドラッグ並べ替え(ドラッグ中の半透明表示)
- `KeysSection`: キー割り当て変更・リセット
- ライト/ダーク両テーマ
