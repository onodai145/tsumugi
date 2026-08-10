# NoteCard周辺Tailwind移行 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Issue #174の第5バッチとして、`NoteCard.svelte`/`NotificationCard.svelte`/`ReactionPicker.svelte`/`NoteMenu.svelte`/`ReactionUsersPopover.svelte`の手書きCSSをTailwindユーティリティクラスへ移行する。

**Architecture:** 各ファイルの`<style>`ブロックを、`color-mix()`ベースの背景(既存バッチと同じ例外パターン)だけを残した最小限のもの(またはゼロ)に縮小し、それ以外はTailwindユーティリティクラスに置き換える。条件付きクラスの衝突は「1つの完全なクラス文字列を選ぶ三項演算子」で解消する。データ取得・ポータル配置・ホバー遅延・クールダウン等の`<script>`ロジックは一切変更しない。`NoteCard`/`ReactionUsersPopover`の`z-index: 1000`は、#180で発見した`Modal.svelte`(`z-[1000]`)との重なり順不安定化を避けるため`z-index: 1010`に引き上げる。

**Tech Stack:** Tailwind CSS v4、既存の`@theme`トークンブリッジ

## Global Constraints

- 各ファイルの`<script>`ブロックのロジックは一切変更しない
- surfaceカラーのマッピング規約: `--surface-1`→`bg-background`、`--surface-2`→`bg-muted`、`--surface-3`→`bg-accent`、`--text`→`text-foreground`、`--text-dim`→`text-muted-foreground`、`--accent`→`bg-primary`/`text-primary`/`border-primary`(用途に応じて)、`--border`→`border-border`/`bg-border`、`--danger`→`text-destructive`
- `--success`/`--warning`/`--info`は`@theme`ブリッジ未対応のため、アービトラリ値(`text-[var(--success)]`等)を使う
- **条件付きクラスは必ず「1つの完全なクラス文字列を選ぶ三項演算子」の形にする。「同じCSSプロパティを設定する複数のクラスを`class:`ディレクティブや`class={[...]}`配列で個別にON/OFFする」書き方は禁止**(#176/#178/#180で見つかった同種バグの再発防止)
- `color-mix(in srgb, var(--surface-*) var(--column-opacity, 100%), transparent)`パターン(カラム背景不透明度用)は既存バッチと同じくTailwindユーティリティに変換せず`<style>`に残す。この場合、要素には引き続き対応する素のクラス名(例: `avatar`/`reaction`)も付与し、`<style>`側のセレクタが引き続きマッチするようにする
- ピクセル値がTailwindの標準スペーシングスケール(4px刻み、`--spacing: 0.25rem`)に正確に乗らない場合はアービトラリ値(`px-[9px]`等)を使う。既存バッチ(`text-[0.85rem]`/`rounded-[14px]`等)と同じ方針
- `-webkit-user-select`が必要な箇所(WebKitGTKが無印字プロパティを反映しないため)は、Tailwindの`select-none`/`select-text`ユーティリティに加えてアービトラリ値`[-webkit-user-select:none]`/`[-webkit-user-select:text]`を併記する
- `z-index: 1000`だった`.picker-overlay`(NoteCard)/`.popover`(ReactionUsersPopover)は`z-[1010]`に引き上げる(Modal.svelteの`z-[1000]`との重なり順不安定化対策、#180のCompletionPopoverと同じ理由)

---

### Task 1: `NoteCard.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte`
- Modify: `frontend/src/ui/NoteCard.test.ts`

**Interfaces:**
- Consumes: なし(既存の子コンポーネント呼び出しはそのまま)
- Produces: 見た目・挙動は現状維持。呼び出し元からの`note`/`quoted`/`showActions`/`hideReactions`/`hideActionBanner`/`accountId`/`emojiAccountId`/`tabId`/`selected` propsは変更しない

- [ ] **Step 1: `<article>`のクラスを置き換え(247〜253行目)**

```svelte
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<article
  class={quoted
    ? "select-none rounded-sm border border-border px-[7px] py-[5px] mt-1.5 [-webkit-user-select:none] [content-visibility:visible]"
    : selected
      ? "selected select-none border-b border-border px-[9px] py-1.5 [-webkit-user-select:none] [content-visibility:auto] [contain-intrinsic-size:auto_92px]"
      : "select-none border-b border-border px-[9px] py-1.5 [-webkit-user-select:none] [content-visibility:auto] [contain-intrinsic-size:auto_92px]"}
  bind:this={el}
  onclick={tabId ? () => app.selectNote(tabId, note.id) : undefined}
>
```

- [ ] **Step 2: renote-banner/reply-banner(254〜267行目)**

```svelte
  {#if isPureRenote && !hideActionBanner}
    <div class="mb-0.5 inline-flex items-center gap-1 text-[0.74rem] text-[var(--success)]">
      <Repeat2 size={13} /> <Mfm
        text={displayName(note.user)}
        emojis={proxiedEmojiMap(note.user.emojis, instanceHost)}
        simple
      /> がRenote
    </div>
  {/if}
  {#if inner.replyId && !hideActionBanner}
    <div class="mb-0.5 inline-flex items-center gap-1 text-[0.74rem] text-[var(--info)]">
      <Reply size={13} /> 返信
    </div>
  {/if}
```

- [ ] **Step 3: row/avatar/body/head(269〜317行目)**

```svelte
  <div class="flex gap-[7px]">
    {#if inner.user.avatarUrl}
      <img
        class="h-[34px] w-[34px] flex-none rounded-[5px] object-cover"
        data-testid="note-avatar"
        src={inner.user.avatarUrl}
        alt=""
        loading="lazy"
        onclick={() => openProfile({ userId: inner.user.id }, accountId)}
        style="cursor: pointer"
      />
    {:else}
      <div
        class="avatar h-[34px] w-[34px] flex-none rounded-[5px]"
        data-testid="note-avatar"
        onclick={() => openProfile({ userId: inner.user.id }, accountId)}
        role="button"
        tabindex="0"
        onkeydown={(e) => e.key === "Enter" && openProfile({ userId: inner.user.id }, accountId)}
      ></div>
    {/if}
    <div class="min-w-0 flex-1">
      <header class="flex flex-wrap items-baseline gap-[5px]">
        <span
          class="text-[0.86rem] font-semibold"
          data-testid="note-name"
          onclick={() => openProfile({ userId: inner.user.id }, accountId)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === "Enter" && openProfile({ userId: inner.user.id }, accountId)}
          style="cursor: pointer"
        ><Mfm
          text={displayName(inner.user)}
          emojis={proxiedEmojiMap(inner.user.emojis, instanceHost)}
          simple
        /></span>
        <span
          class="text-[0.76rem] text-muted-foreground"
          data-testid="note-acct"
          onclick={() => openProfile({ userId: inner.user.id }, accountId)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === "Enter" && openProfile({ userId: inner.user.id }, accountId)}
          style="cursor: pointer"
        >{acct(inner.user)}</span>
        <span class="ml-auto text-[0.76rem] text-muted-foreground" title={new Date(inner.createdAt * 1000).toLocaleString()}>
          {relativeTime(inner.createdAt)}
        </span>
        {#if inner.visibility !== "public"}
          {@const VisIcon = VIS_ICON[inner.visibility]}
          <span class="inline-flex items-center rounded-[3px] border border-border p-0.5 text-[0.76rem] text-muted-foreground" title={VIS_LABEL[inner.visibility]}><VisIcon size={12} /></span>
        {/if}
      </header>
```

- [ ] **Step 4: CW/本文/メディア/投票/引用ネスト(319〜366行目)**

```svelte
      {#if inner.cw}
        <div class="mt-0.5">
          <span class="text-[0.9rem] [-webkit-user-select:text] select-text"><Mfm text={inner.cw} emojis={emojiMap} nyaize={inner.user.isCat} /></span>
          <button type="button" class="cw-toggle ml-2 rounded-md border border-border px-2 py-px text-[0.8rem] text-foreground" onclick={() => (cwOpen = !cwOpen)}>
            {cwOpen ? "隠す" : `続きを見る${inner.text ? "" : ""}`}
          </button>
        </div>
      {/if}

      {#if !inner.cw || cwOpen}
        {#if inner.text}
          <div class="mt-px whitespace-pre-wrap break-words text-[0.9rem] leading-[1.42] [-webkit-user-select:text] select-text"><Mfm text={inner.text} emojis={emojiMap} nyaize={inner.user.isCat} /></div>
        {/if}
        {#if inner.files.length > 0}
          <MediaGrid files={inner.files} />
        {/if}
        {#if inner.poll}
          <div class="mt-2 flex flex-col gap-1">
            {#each inner.poll.choices as choice, i}
              <button
                type="button"
                class={choice.isVoted
                  ? "poll-choice flex w-full items-center justify-between rounded-md px-2 py-[5px] text-left font-[inherit] text-[0.88rem] text-foreground outline outline-1 outline-primary disabled:cursor-default"
                  : "poll-choice flex w-full items-center justify-between rounded-md px-2 py-[5px] text-left font-[inherit] text-[0.88rem] text-foreground disabled:cursor-default"}
                disabled={!accountId || pollExpired || pollAlreadyVoted || choice.isVoted}
                onclick={() => requestVote(i)}
              >
                <span>{choice.text}</span>
                <span>{choice.votes}</span>
              </button>
            {/each}
          </div>
          {#if pollExpired}
            <p class="mt-1 mb-0 text-[0.78rem] text-muted-foreground">投票は締め切られました</p>
          {/if}
          {#if confirmChoice !== null}
            <ConfirmDialog
              title="投票の確認"
              message={`「${inner.poll.choices[confirmChoice].text}」に投票します。取り消せません。よろしいですか？`}
              confirmLabel="投票する"
              onConfirm={confirmVote}
              onCancel={() => (confirmChoice = null)}
            />
          {/if}
        {/if}
        {#if inner.text && inner.renote}
          <Self note={inner.renote} quoted={true} hideReactions emojiAccountId={emojiAcct} />
        {/if}
      {/if}
```

補足: `.poll-choice`の子`<span>`2つ(`.poll-text`/`.poll-votes`)は元CSSにレイアウト用の個別スタイルが無かった(親の`justify-content:space-between`のみで整列)ため、子`<span>`にクラスは不要。

- [ ] **Step 5: リアクション一覧(368〜397行目)**

```svelte
      {#if !hideReactions && reactionList.length > 0}
        <div class="mt-2 flex flex-wrap gap-[5px]">
          {#each reactionList as [key, count]}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span
              class="inline-flex"
              data-testid="note-reaction-wrap"
              onmouseenter={(e) => enterHover({ kind: "reaction", key }, e.currentTarget as HTMLElement)}
              onmouseleave={leaveHover}
            >
              <button
                type="button"
                class={inner.myReaction === key
                  ? "reaction mine inline-flex items-center gap-[3px] rounded-[3px] border border-primary px-[7px] py-px text-[0.85rem] text-foreground disabled:cursor-default disabled:opacity-60"
                  : "reaction inline-flex items-center gap-[3px] rounded-[3px] border border-border px-[7px] py-px text-[0.85rem] text-foreground disabled:cursor-default disabled:opacity-60"}
                disabled={!accountId || isRemoteCustomEmoji(key)}
                aria-label={isRemoteCustomEmoji(key) ? "このインスタンスに無い絵文字のためリアクションできません" : undefined}
                onclick={() => react(key)}
              >
                {#if key.startsWith(":")}
                  {@const e = reactionEmoji(key, emojiMap, instanceHost)}
                  <CustomEmoji name={e.name} url={e.url} showTitle={false} />
                {:else}
                  <UnicodeEmoji char={key} showTitle={false} />
                {/if}
                <span class="text-muted-foreground">{count}</span>
              </button>
            </span>
          {/each}
        </div>
      {/if}
```

- [ ] **Step 6: アクションフッター(399〜477行目)**

```svelte
      {#if effectiveShowActions && accountId}
        <footer class="actions mt-2 flex items-center gap-[14px] text-[0.8rem] text-muted-foreground">
          <button type="button" class="inline-flex items-center gap-1 rounded-md px-1 py-0.5 text-[0.82rem] text-muted-foreground" aria-label="返信" onclick={() => app.openCompose(accountId!, { replyTo: inner })}>
            <Reply size={15} /> {inner.replyCount || ""}
          </button>
          {#if canRenote}
            <button
              type="button"
              class={renoteBusy
                ? "inline-flex items-center gap-1 rounded-md px-1 py-0.5 text-[0.82rem] text-muted-foreground opacity-50"
                : "inline-flex items-center gap-1 rounded-md px-1 py-0.5 text-[0.82rem] text-muted-foreground"}
              aria-label="Renote"
              onclick={doRenote}
              onmouseenter={(e) => enterHover({ kind: "renote" }, e.currentTarget as HTMLElement)}
              onmouseleave={leaveHover}
            >
              <Repeat2 size={15} />
              {#if inner.renoteCount > 0}
                <span>{inner.renoteCount}</span>
              {/if}
            </button>
            <button type="button" class="inline-flex items-center gap-1 rounded-md px-1 py-0.5 text-[0.82rem] text-muted-foreground" aria-label="引用" onclick={() => app.openCompose(accountId!, { quoteOf: inner })}>
              <Quote size={15} />
            </button>
          {/if}
          <div class="relative">
            <button
              type="button"
              bind:this={pickerBtn}
              class={showPicker
                ? "on inline-flex items-center gap-1 rounded-md px-1 py-0.5 text-[0.82rem] text-muted-foreground"
                : "inline-flex items-center gap-1 rounded-md px-1 py-0.5 text-[0.82rem] text-muted-foreground"}
              aria-label="リアクション"
              onclick={togglePicker}
            >
              <SmilePlus size={15} /> {inner.reactionCount || ""}
            </button>
            {#if showPicker && pickerPos}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (app.reactPicker = null)} role="presentation">
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="fixed"
                  style={`left:${pickerPos.left}px;top:${pickerPos.top}px`}
                  onclick={(e) => e.stopPropagation()}
                  role="presentation"
                >
                  <ReactionPicker {accountId} onpick={react} />
                </div>
              </div>
            {/if}
          </div>
          <div class="relative">
            <button
              type="button"
              bind:this={noteMenuBtn}
              class={noteMenuOpen
                ? "on inline-flex items-center gap-1 rounded-md px-1 py-0.5 text-[0.82rem] text-muted-foreground"
                : "inline-flex items-center gap-1 rounded-md px-1 py-0.5 text-[0.82rem] text-muted-foreground"}
              aria-label="その他"
              onclick={() => {
                app.reactPicker = null;
                noteMenuOpen = !noteMenuOpen;
              }}
            >
              <MoreHorizontal size={15} />
            </button>
            {#if noteMenuOpen && noteMenuPos}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (noteMenuOpen = false)} role="presentation">
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="fixed"
                  style={`left:${noteMenuPos.left}px;top:${noteMenuPos.top}px`}
                  onclick={(e) => e.stopPropagation()}
                  role="presentation"
                >
                  <NoteMenu {accountId} note={inner} onclose={() => (noteMenuOpen = false)} />
                </div>
              </div>
            {/if}
          </div>
        </footer>
      {/if}
    </div>
  </div>
  {#if hoverTarget && hoverPos && accountId}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      use:portal
      style={`position:fixed; left:0; top:0;`}
      onmouseenter={keepHover}
      onmouseleave={leaveHover}
    >
      <ReactionUsersPopover
        {accountId}
        noteId={inner.id}
        reactionKey={hoverTarget.kind === "reaction" ? hoverTarget.key : null}
        totalCount={hoverTarget.kind === "reaction" ? (inner.reactions[hoverTarget.key] ?? 0) : inner.renoteCount}
        left={hoverPos.left}
        top={hoverPos.top}
        {emojiMap}
        {instanceHost}
      />
    </div>
  {/if}
</article>
```

- [ ] **Step 7: `<style>`ブロックを縮小(502〜722行目)**

`<style>...</style>`ブロック全体を以下に置き換える(`color-mix()`パターンのみ残す):

```svelte
<style>
  .selected {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .avatar {
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  }
  .cw-toggle {
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
  }
  .poll-choice {
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
  }
  .poll-choice:hover:not(:disabled) {
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  }
  .reaction {
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
  }
  .reaction.mine {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .actions button:hover,
  .actions button.on {
    color: var(--accent);
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
  }
</style>
```

補足: `.avatar`セレクタは(元の`.avatar.placeholder`から`.placeholder`が不要になったため)`avatar`単一クラスに整理。この`<style>`ブロックの`.avatar`はプレースホルダーdivにのみ`avatar`クラスが付与されている(img要素には付与しない)ため、背景色は従来通りプレースホルダー表示時のみ効く。

- [ ] **Step 8: `NoteCard.test.ts`のセレクタを更新**

`.reaction-wrap` → `[data-testid="note-reaction-wrap"]`、`.avatar`/`img.avatar` → `[data-testid="note-avatar"]`、`.name` → `[data-testid="note-name"]`、`.acct` → `[data-testid="note-acct"]` に変更する:

```diff
--- a/frontend/src/ui/NoteCard.test.ts
+++ b/frontend/src/ui/NoteCard.test.ts
@@
     const { container } = render(NoteCard, { props: { note } });
-    expect(container.querySelector(".reaction-wrap")).toBeNull();
+    expect(container.querySelector('[data-testid="note-reaction-wrap"]')).toBeNull();
@@
     const { container } = render(NoteCard, { props: { note, accountId: "acc1" } });
-    const avatar = container.querySelector(".avatar") as HTMLElement;
+    const avatar = container.querySelector('[data-testid="note-avatar"]') as HTMLElement;
     avatar.click();
     expect(openProfile).toHaveBeenCalledWith({ userId: note.user.id }, "acc1");
   });

   it("アバタークリックでopenProfileが呼ばれる（imgタグ: avatarUrl設定あり）", () => {
     const note = makeNote({ user: makeUser({ avatarUrl: "https://example.com/a.png" }) });
     const { container } = render(NoteCard, { props: { note, accountId: "acc1" } });
-    const avatar = container.querySelector("img.avatar") as HTMLElement;
+    const avatar = container.querySelector('img[data-testid="note-avatar"]') as HTMLElement;
     expect(avatar).toBeTruthy();
     avatar.click();
     expect(openProfile).toHaveBeenCalledWith({ userId: note.user.id }, "acc1");
   });

   it("表示名クリックでopenProfileが呼ばれる", () => {
     const note = makeNote();
     const { container } = render(NoteCard, { props: { note, accountId: "acc1" } });
-    const name = container.querySelector(".name") as HTMLElement;
+    const name = container.querySelector('[data-testid="note-name"]') as HTMLElement;
     name.click();
     expect(openProfile).toHaveBeenCalledWith({ userId: note.user.id }, "acc1");
   });

   it("acctクリックでopenProfileが呼ばれる", () => {
     const note = makeNote();
     const { container } = render(NoteCard, { props: { note, accountId: "acc1" } });
-    const acctEl = container.querySelector(".acct") as HTMLElement;
+    const acctEl = container.querySelector('[data-testid="note-acct"]') as HTMLElement;
     acctEl.click();
     expect(openProfile).toHaveBeenCalledWith({ userId: note.user.id }, "acc1");
   });
```

- [ ] **Step 9: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 10: `pnpm test`を実行**

Run: `cd frontend && pnpm test`
Expected: `NoteCard.test.ts`の全テストが通る

- [ ] **Step 11: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 12: Commit**

```bash
git add frontend/src/ui/NoteCard.svelte frontend/src/ui/NoteCard.test.ts
git commit -m "style: NoteCard.svelteをTailwindクラスに移行"
```

---

### Task 2: `NotificationCard.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/NotificationCard.svelte`
- Modify: `frontend/src/ui/NotificationCard.test.ts`

**Interfaces:**
- Consumes: なし
- Produces: 見た目・挙動は現状維持。呼び出し元からの`notification`/`accountId` propsは変更しない

- [ ] **Step 1: マークアップを置き換え(72〜121行目)**

```svelte
<article class="border-b border-border px-3 py-2 [content-visibility:auto] [contain-intrinsic-size:auto_80px]">
  <div class="flex items-center gap-2 text-[0.86rem]">
    <span class="inline-flex flex-none text-muted-foreground"><IconComp size={15} /></span>
    {#if n.user?.avatarUrl}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <img
        class="h-6 w-6 flex-none rounded-md object-cover"
        data-testid="notification-avatar"
        src={n.user.avatarUrl}
        alt=""
        loading="lazy"
        onclick={() => n.user && openProfile({ userId: n.user.id }, accountId)}
        style="cursor: pointer"
      />
    {/if}
    <span class="min-w-0 flex-1">
      {#if actor}<b
          data-testid="notification-actor"
          onclick={() => n.user && openProfile({ userId: n.user.id }, accountId)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === "Enter" && n.user && openProfile({ userId: n.user.id }, accountId)}
          style="cursor: pointer"
          ><Mfm text={actor} emojis={proxiedEmojiMap(n.user?.emojis, instanceHost)} simple /></b
        >{/if}
      {labels[n.type] ?? n.type}
      {#if n.type === "reaction" && n.reaction}
        <span class="ml-0.5">
          {#if reaction}
            <CustomEmoji name={reaction.name} url={reaction.url} />
          {:else}<UnicodeEmoji char={n.reaction} />{/if}
        </span>
      {/if}
    </span>
    <span class="text-[0.78rem] text-muted-foreground">{relativeTime(n.createdAt)}</span>
  </div>
  {#if n.note}
    <div class="ml-[30px]" data-testid="notification-note-preview">
      <NoteCard
        note={n.note}
        quoted={true}
        showActions={true}
        hideReactions
        hideActionBanner
        accountId={accountId}
        emojiAccountId={accountId}
      />
    </div>
  {/if}
</article>
```

- [ ] **Step 2: `<style>`ブロックを削除**

`<style>...</style>`ブロック全体(123〜162行目)を削除する。`color-mix()`等の変換不能パターンは含まれていないため不要になる。

- [ ] **Step 3: `NotificationCard.test.ts`のセレクタを更新**

```diff
--- a/frontend/src/ui/NotificationCard.test.ts
+++ b/frontend/src/ui/NotificationCard.test.ts
@@
     const { container } = render(NotificationCard, {
       props: { notification, accountId: "a1" },
     });
-    expect(container.querySelector(".note-preview")).toBeNull();
+    expect(container.querySelector('[data-testid="notification-note-preview"]')).toBeNull();
   });
 });
@@
     const { container } = render(NotificationCard, {
       props: { notification, accountId: "a1" },
     });
-    const avatar = container.querySelector(".avatar") as HTMLElement;
+    const avatar = container.querySelector('[data-testid="notification-avatar"]') as HTMLElement;
     await fireEvent.click(avatar);
     expect(currentProfileTarget()).toEqual({ userId: "u9" });
     expect(currentProfileAccountId()).toBe("a1");
   });
@@
     const { container } = render(NotificationCard, {
       props: { notification, accountId: "a1" },
     });
-    const actor = container.querySelector(".actor") as HTMLElement;
+    const actor = container.querySelector('[data-testid="notification-actor"]') as HTMLElement;
     await fireEvent.click(actor);
     expect(currentProfileTarget()).toEqual({ userId: "u9" });
   });
```

- [ ] **Step 4: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 5: `pnpm test`を実行**

Run: `cd frontend && pnpm test`
Expected: `NotificationCard.test.ts`の全テストが通る

- [ ] **Step 6: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 7: Commit**

```bash
git add frontend/src/ui/NotificationCard.svelte frontend/src/ui/NotificationCard.test.ts
git commit -m "style: NotificationCard.svelteをTailwindクラスに移行"
```

---

### Task 3: `ReactionPicker.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/input/ReactionPicker.svelte`

**Interfaces:**
- Consumes: なし
- Produces: 見た目・挙動は現状維持。呼び出し元(`NoteCard.svelte`)からの`accountId`/`onpick`/`showPinned` propsは変更しない

- [ ] **Step 1: マークアップを置き換え(83〜176行目)**

```svelte
<div class="w-[300px] rounded-[10px] border border-border bg-background p-2 shadow-[0_8px_24px_rgba(0,0,0,0.25)]">
  <input class="mb-1.5 box-border w-full rounded-md border border-border bg-muted px-2 py-1.5 text-foreground" placeholder="絵文字を検索…" bind:value={query} />
  <div class="max-h-[320px] overflow-y-auto overflow-x-hidden">
    {#if queryLower}
      <div class="flex flex-wrap gap-0.5">
        {#each customMatches as e (e.name)}
          <button type="button" class="rounded-md p-1 text-[1.1rem] leading-none hover:bg-accent" title={`:${e.name}:`} onclick={() => onpick(customEmojiKey(e.name))}>
            <img class="block h-[1.4em] w-[1.4em] object-contain" src={e.url} alt={`:${e.name}:`} loading="lazy" />
          </button>
        {/each}
        {#each unicodeMatches as e (e.char)}
          <button type="button" class="rounded-md p-1 text-[1.1rem] leading-none hover:bg-accent" title={`:${e.name}:`} onclick={() => onpick(e.char)}>
            <UnicodeEmoji char={e.char} />
          </button>
        {/each}
        {#if unicodeMatches.length === 0 && customMatches.length === 0}
          <span class="p-2 text-[0.8rem] text-muted-foreground">絵文字がありません</span>
        {/if}
      </div>
    {:else}
      {#if showPinned && recentEntries.length > 0}
        <section class="mb-1">
          <h4 class="mb-1 mt-1.5 text-[0.72rem] font-semibold text-muted-foreground">最近使った</h4>
          <div class="flex flex-wrap gap-0.5">
            {#each recentEntries as e (e.key)}
              <button type="button" class="rounded-md p-1 text-[1.1rem] leading-none hover:bg-accent" title={e.key} onclick={() => onpick(reactionKeyOf(e))}>
                {#if e.custom}
                  <img class="block h-[1.4em] w-[1.4em] object-contain" src={e.custom.url} alt={e.key} loading="lazy" />
                {:else}
                  <UnicodeEmoji char={e.key} />
                {/if}
              </button>
            {/each}
          </div>
        </section>
      {/if}

      {#if showPinned}
        <section class="mb-1">
          <h4 class="mb-1 mt-1.5 text-[0.72rem] font-semibold text-muted-foreground">ピン留め</h4>
          <div class="flex flex-wrap gap-0.5">
            {#each pinnedEntries as e (e.key)}
              <button type="button" class="rounded-md p-1 text-[1.1rem] leading-none hover:bg-accent" title={e.key} onclick={() => onpick(reactionKeyOf(e))}>
                {#if e.custom}
                  <img class="block h-[1.4em] w-[1.4em] object-contain" src={e.custom.url} alt={e.key} loading="lazy" />
                {:else}
                  <UnicodeEmoji char={e.key} />
                {/if}
              </button>
            {/each}
            {#if pinnedEntries.length === 0}
              <span class="p-2 text-[0.8rem] text-muted-foreground">ピン留めした絵文字がありません（設定→リアクションで追加できます）</span>
            {/if}
          </div>
        </section>
      {/if}

      <section class="mb-1">
        <h4 class="mb-1 mt-1.5 text-[0.72rem] font-semibold text-muted-foreground">カスタム絵文字</h4>
        {#each customByCategory as group (group.category ?? "")}
          <details open={customByCategory.length <= 1}>
            <summary class="cursor-pointer px-0.5 py-1 text-[0.72rem] text-muted-foreground">{group.category ?? "その他"}（{group.emojis.length}）</summary>
            <div class="flex flex-wrap gap-0.5">
              {#each group.emojis as e (e.name)}
                <button type="button" class="rounded-md p-1 text-[1.1rem] leading-none hover:bg-accent" title={`:${e.name}:`} onclick={() => onpick(customEmojiKey(e.name))}>
                  <img class="block h-[1.4em] w-[1.4em] object-contain" src={e.url} alt={`:${e.name}:`} loading="lazy" />
                </button>
              {/each}
            </div>
          </details>
        {/each}
        {#if customByCategory.length === 0}
          <span class="p-2 text-[0.8rem] text-muted-foreground">カスタム絵文字がありません</span>
        {/if}
      </section>

      <section class="mb-1">
        <h4 class="mb-1 mt-1.5 text-[0.72rem] font-semibold text-muted-foreground">絵文字</h4>
        {#each UNICODE_EMOJI_CATEGORIES as c (c.index)}
          <details>
            <summary class="cursor-pointer px-0.5 py-1 text-[0.72rem] text-muted-foreground">{c.label}</summary>
            <div class="flex flex-wrap gap-0.5">
              {#each UNICODE_EMOJIS.filter((e) => e.category === c.index) as e (e.char)}
                <button type="button" class="rounded-md p-1 text-[1.1rem] leading-none hover:bg-accent" title={`:${e.name}:`} onclick={() => onpick(e.char)}>
                  <UnicodeEmoji char={e.char} />
                </button>
              {/each}
            </div>
          </details>
        {/each}
      </section>
    {/if}
  </div>
</div>
```

- [ ] **Step 2: `<style>`ブロックを削除**

`<style>...</style>`ブロック全体(178〜245行目)を削除する。`color-mix()`は使われておらず(`.emoji-btn:hover`は素の`var(--surface-3)`参照)不要になる。

- [ ] **Step 3: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 4: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 5: Commit**

```bash
git add frontend/src/input/ReactionPicker.svelte
git commit -m "style: ReactionPicker.svelteをTailwindクラスに移行"
```

---

### Task 4: `NoteMenu.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/NoteMenu.svelte`

**Interfaces:**
- Consumes: なし
- Produces: 見た目・挙動は現状維持。呼び出し元(`NoteCard.svelte`)からの`accountId`/`note`/`onclose` propsは変更しない

- [ ] **Step 1: マークアップを置き換え(64〜110行目)**

```svelte
<div class="w-[200px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]">
  <button type="button" class="box-border flex w-full items-center gap-1.5 rounded-[5px] px-2 py-1.5 text-left text-[0.82rem] text-foreground hover:bg-muted" onclick={toggleFavorite}>
    <Star size={14} />
    {note.isFavoritedByMe ? "お気に入り解除" : "お気に入り登録"}
  </button>

  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="relative" role="presentation" bind:this={clipRowEl} onmouseenter={openClipSubmenu}>
    <button type="button" class="box-border flex w-full items-center gap-1.5 rounded-[5px] px-2 py-1.5 text-left text-[0.82rem] text-foreground hover:bg-muted" onclick={openClipSubmenu}>
      <Paperclip size={14} />
      クリップに追加
      <ChevronRight size={14} class="ml-auto" />
    </button>

    {#if clipSubmenuOpen}
      <div
        class={submenuSide === "left"
          ? "absolute right-full top-0 max-h-[280px] w-[200px] overflow-y-auto rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
          : "absolute left-full top-0 max-h-[280px] w-[200px] overflow-y-auto rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"}
      >
        {#if creatingClip}
          <div class="flex gap-1 p-1">
            <input
              class="box-border min-w-0 flex-1 rounded-[5px] border border-border bg-muted px-1.5 py-1 text-[0.82rem] text-foreground"
              placeholder="クリップ名"
              bind:value={newClipName}
              onkeydown={(e) => e.key === "Enter" && confirmCreateClip()}
            />
            <button type="button" class="rounded-[5px] bg-primary px-2 py-1 text-[0.78rem] text-primary-foreground disabled:cursor-default disabled:opacity-50" disabled={!newClipName.trim()} onclick={confirmCreateClip}>
              作成
            </button>
          </div>
        {:else}
          {#if clipsLoading}
            <span class="block px-2 py-1.5 text-[0.78rem] text-muted-foreground">読み込み中…</span>
          {:else if clipsError}
            <span class="block px-2 py-1.5 text-[0.78rem] text-muted-foreground">読み込みに失敗しました</span>
          {:else if clips && clips.length === 0}
            <span class="block px-2 py-1.5 text-[0.78rem] text-muted-foreground">クリップがありません</span>
          {:else if clips}
            {#each clips as clip (clip.id)}
              <button type="button" class="box-border flex w-full items-center gap-1.5 rounded-[5px] px-2 py-1.5 text-left text-[0.82rem] text-foreground hover:bg-muted" onclick={() => pickClip(clip)}>{clip.name}</button>
            {/each}
          {/if}
          <button type="button" class="box-border flex w-full items-center gap-1.5 rounded-[5px] px-2 py-1.5 text-left text-[0.82rem] text-primary hover:bg-muted" onclick={startCreateClip}>＋ 新規クリップを作成</button>
        {/if}
      </div>
    {/if}
  </div>
</div>
```

- [ ] **Step 2: `<style>`ブロックを削除**

`<style>...</style>`ブロック全体(112〜200行目)を削除する。`color-mix()`は使われておらず不要になる。

- [ ] **Step 3: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 4: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/NoteMenu.svelte
git commit -m "style: NoteMenu.svelteをTailwindクラスに移行"
```

---

### Task 5: `ReactionUsersPopover.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/ReactionUsersPopover.svelte`

**Interfaces:**
- Consumes: なし
- Produces: 見た目・挙動は現状維持。呼び出し元(`NoteCard.svelte`)からの全propsは変更しない

- [ ] **Step 1: マークアップを置き換え(64〜106行目)**

```svelte
{#if !hideWhenEmpty}
  <div class="fixed z-[1010] min-w-[160px] max-w-[240px] max-h-[280px] overflow-y-auto rounded-md border border-border bg-muted p-1 shadow-[0_4px_16px_rgba(0,0,0,0.25)]" style={`left:${left}px;top:${top}px`}>
    {#if failed}
      <div class="px-2 py-1.5 text-[0.8rem] text-muted-foreground">取得に失敗しました</div>
    {:else if users === null}
      <div class="px-2 py-1.5 text-[0.8rem] text-muted-foreground">読み込み中…</div>
    {:else if users.length === 0}
      <div class="px-2 py-1.5 text-[0.8rem] text-muted-foreground">なし</div>
    {:else}
      <ul class="m-0 list-none p-0">
        {#each users as u (u.id)}
          <li class="flex items-center gap-1.5 px-1.5 py-[3px] text-[0.8rem]">
            {#if u.avatarUrl}
              <img class="h-5 w-5 flex-shrink-0 rounded-full object-cover" src={u.avatarUrl} alt="" loading="lazy" />
            {:else}
              <div class="h-5 w-5 flex-shrink-0 rounded-full bg-border"></div>
            {/if}
            <span class="flex min-w-0 flex-1 flex-col">
              <span class="overflow-hidden text-ellipsis whitespace-nowrap text-foreground"><Mfm
                text={displayName(u)}
                emojis={proxiedEmojiMap(u.emojis, instanceHost)}
                simple
              /></span>
              <span class="overflow-hidden text-ellipsis whitespace-nowrap text-[0.72rem] text-muted-foreground">{acct(u)}</span>
            </span>
            {#if reactionKey && emoji}
              <span class="ml-auto inline-flex flex-shrink-0 items-center">
                {#if reactionKey.startsWith(":")}
                  <CustomEmoji name={emoji.name} url={emoji.url} showTitle={false} />
                {:else}
                  <UnicodeEmoji char={reactionKey} showTitle={false} />
                {/if}
              </span>
            {/if}
          </li>
        {/each}
      </ul>
      {#if moreCount > 0}
        <div class="px-1.5 py-[3px] text-[0.74rem] text-muted-foreground">他{moreCount}件</div>
      {/if}
    {/if}
  </div>
{/if}
```

- [ ] **Step 2: `<style>`ブロックを削除**

`<style>...</style>`ブロック全体(108〜179行目)を削除する。`color-mix()`は使われておらず(`.avatar.placeholder`は素の`var(--border)`参照)不要になる。

- [ ] **Step 3: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 4: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/ReactionUsersPopover.svelte
git commit -m "style: ReactionUsersPopover.svelteをTailwindクラスに移行、z-indexをModal.svelteより前面に引き上げ"
```

---

### 手動確認(全タスク完了後、最終レビュー前後いずれかで実施)

リポジトリルートから`cargo tauri dev`を起動し、以下を確認する:
- タイムライン表示(通常ノート/引用Renoteのネスト表示/純粋Renoteバナー/返信バナー)
- CW(内容の警告)の開閉
- 投票(未投票/投票済み/期限切れ/確認ダイアログ)
- リアクション付与・取消、自分がつけたリアクションの枠線ハイライト
- リアクションピッカー・ノートメニュー(クリップ追加、新規クリップ作成)の開閉・位置
- Renoteボタンのクールダウン(連打で一時的に薄くなる)
- リアクション/Renoteホバー時の「誰が」ポップオーバー
- 選択中ノートのハイライト(キーボード操作時)
- 通知一覧(NotificationCard)のアバター/表示名クリックでのプロフィール遷移、ノートプレビュー
- ライト/ダーク両テーマ
- モーダル(ProfileModal等)を開いた状態でリアクションピッカーを開いても隠れないこと(z-index引き上げの確認)
