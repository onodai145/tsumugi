# ComposeBar.svelte Tailwind移行 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Issue #174の継続バッチとして、`ComposeBar.svelte`(投稿バー、1175行)の手書きCSSをTailwindユーティリティクラスへ移行し、独立したアイコン/アクションボタンをshadcn `Button`プリミティブに統一する。

**Architecture:** `<style>`ブロックを、まだ未移行の`Dropdown.svelte`内部クラスへの`:global()`オーバーライド1件だけを残した最小限のものに縮小し、それ以外はTailwindユーティリティクラスへ置き換える。条件付きクラスの衝突は「1つの完全なクラス文字列を選ぶ三項演算子」で解消する。独立した13箇所のボタンはshadcn Buttonプリミティブに統一する(過去バッチで確立した「テキストラベルの単発アクション」「独立した小さなアイコンボタン」への適用パターン)。フル幅リスト行の添付メニュー項目は`CompletionPopover.svelte`の項目と同じ理由で生`<button>`のまま維持する。`<script>`ロジックは一切変更しない。

**Tech Stack:** Tailwind CSS v4、既存の`@theme`トークンブリッジ、shadcn Buttonプリミティブ

## Global Constraints

- `<script>`ブロックのロジックは一切変更しない
- surfaceカラーのマッピング規約: `--surface-1`→`bg-background`、`--surface-2`→`bg-muted`、`--surface-3`→`bg-accent`、`--text`→`text-foreground`、`--text-dim`→`text-muted-foreground`、`--accent`→`bg-primary`/`text-primary`/`border-primary`(用途に応じて)、`--border`→`border-border`、`--danger`→`bg-destructive`
- **条件付きクラスは必ず「1つの完全なクラス文字列を選ぶ三項演算子」の形にする。「同じCSSプロパティを設定する複数のクラスを`class:`ディレクティブや`class={[...]}`配列で個別にON/OFFする」書き方は禁止**(#176/#178/#180で見つかった同種バグの再発防止)
- Buttonコンポーネントへ`bind:this`は使えない(Buttonは`ref = $bindable(null)`という別名のbindableプロパティを持つ)。`bind:this={attachTrigger}`のように既存で生`<button>`に`bind:this`していた箇所をButtonに置き換える場合は`bind:ref={attachTrigger}`に変更すること
- ポータルで`document.body`直下に描画される要素(`attach-overlay`/`emoji-picker-pop`)の`z-index`は、`Modal.svelte`(`z-[1000]`)より前面に出す必要があるため`z-[1010]`にする(#180で発見したCompletionPopoverの教訓と同じ理由。このComposeBar自身のエラーモーダルもModal.svelte経由のため、理論上同時に開きうる)
- ピクセル値がTailwindの標準スペーシングスケールに正確に乗らない場合はアービトラリ値(`px-[9px]`等)を使う
- `.channel-select :global(.trigger)`は、まだ移行していない`Dropdown.svelte`内部の`.trigger`クラスへの外部オーバーライドのため`<style>`に残す。`Dropdown.svelte`自体は変更しない
- `color-mix()`パターンはこのファイルに無い
- テストファイルなし

---

### Task 1: `ComposeBar.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`

**Interfaces:**
- Consumes: 既存の`Button`(`$lib/components/ui/button`)、`AccountSelect`/`VisibilitySelect`/`Dropdown`/`DrivePicker`/`Modal`/`ReactionPicker`/`CompletionPopover`(いずれも変更なし)
- Produces: 見た目・挙動は現状維持。呼び出し元からの`onPosted`/`expanded` propsは変更しない

- [ ] **Step 1: `<script>`にButtonのimportを追加**

`<script lang="ts">`ブロック冒頭のimport群に追加:

```ts
import { Button } from "$lib/components/ui/button";
```

- [ ] **Step 2: `composewrap`/`composebox`/`context`(542〜563行目)**

```svelte
<div class="flex flex-1 min-w-0 items-start gap-1.5">
  <AccountSelect
    bind:value={
      () => accountId,
      (v) => {
        accountId = v;
        accountTouched = true;
      }
    }
    accounts={app.accounts}
    large={!expanded}
  />

  <div class="flex flex-1 min-w-0 flex-col gap-1">
  {#if replyTo || quoteOf}
    <div class="flex items-center gap-1.5 rounded-md border border-border bg-muted px-1.5 py-[3px] text-[0.78rem] text-muted-foreground">
      <span class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
        {replyTo ? "返信: " : "引用: "}@{(replyTo ?? quoteOf)!.user.username} — {(replyTo ?? quoteOf)!.text ?? ""}
      </span>
      <Button type="button" variant="ghost" size="icon-xs" class="flex-none text-muted-foreground" title="キャンセル" onclick={cancelContext}><X size={12} /></Button>
    </div>
  {/if}
```

- [ ] **Step 3: CW入力欄/本文欄/絵文字トリガー(565〜603行目)**

```svelte
  {#if useCw}
    <input class="w-full box-border rounded border border-border bg-muted px-[9px] py-1.5 font-[inherit] text-[0.84rem] text-foreground" placeholder="内容警告 (CW)" bind:value={cw} />
  {/if}

  <div class="relative">
    <textarea
      class={expanded
        ? "w-full box-border resize-y rounded-md border border-border bg-muted py-1.5 pr-[34px] pl-2 font-[inherit] text-[0.86rem] leading-[1.4] text-foreground min-h-24 [transition:min-height_0.12s_ease]"
        : compact
          ? "w-full box-border resize-none rounded-md border border-border bg-muted py-1.5 pr-[34px] pl-2 font-[inherit] text-[0.86rem] leading-[1.4] text-foreground min-h-[34px] [transition:min-height_0.12s_ease]"
          : "w-full box-border resize-y rounded-md border border-border bg-muted py-1.5 pr-[34px] pl-2 font-[inherit] text-[0.86rem] leading-[1.4] text-foreground min-h-20 [transition:min-height_0.12s_ease]"}
      rows={expanded ? 4 : 1}
      placeholder={placeholder}
      bind:value={text}
      bind:this={textarea}
      onkeydown={onKey}
      onkeyup={syncCursor}
      onclick={syncCursor}
      oninput={onTextareaInput}
      oncompositionstart={() => (composing = true)}
      oncompositionend={() => {
        composing = false;
        syncCursor();
      }}
      onfocus={() => (focused = true)}
      onblur={() => {
        focused = false;
        suppressAt = cursorPos;
      }}
      onpaste={handlePaste}
    ></textarea>
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      class={showEmojiPicker
        ? "absolute top-1.5 right-1.5 bg-background text-primary opacity-100 hover:bg-accent hover:text-foreground disabled:cursor-default disabled:opacity-40"
        : "absolute top-1.5 right-1.5 bg-background text-muted-foreground opacity-85 hover:bg-accent hover:text-foreground disabled:cursor-default disabled:opacity-40"}
      title="絵文字を挿入"
      bind:ref={emojiPickerTrigger}
      onmousedown={(e) => e.preventDefault()}
      onclick={toggleEmojiPicker}
      disabled={busy || !accountId}
    ><SmilePlus size={16} /></Button>
  </div>
```

補足: `bind:this={emojiPickerTrigger}`は`bind:ref={emojiPickerTrigger}`に変更している(Buttonコンポーネントは`ref`という名前のbindableプロパティを持つため)。

- [ ] **Step 4: 補完ポップアップ(変更なし、605〜615行目)はそのまま**

- [ ] **Step 5: 添付サムネイル一覧(617〜641行目)**

```svelte
  {#if attachments.length > 0}
    <div class="flex flex-wrap gap-1">
      {#each attachments as a (a.id)}
        <div class="relative h-7 w-7">
          {#if a.kind === "drive"}
            {#if a.file.mimeType.startsWith("image/")}
              <img class="h-7 w-7 rounded object-cover" src={a.file.thumbnailUrl ?? a.file.url} alt="" />
            {:else}
              <span class="grid h-7 w-7 place-items-center rounded bg-accent text-[0.6rem] text-muted-foreground">{a.file.mimeType.split("/")[0]}</span>
            {/if}
          {:else if a.previewUrl}
            <img class="h-7 w-7 rounded object-cover" src={a.previewUrl} alt="" />
          {:else}
            <span class="grid h-7 w-7 place-items-center rounded bg-accent text-[0.6rem] text-muted-foreground">{extLower(a.name).toUpperCase() || "FILE"}</span>
          {/if}
          {#if uploadingAttachmentId === a.id}
            <span class="absolute -bottom-1 -left-1 flex h-3.5 w-3.5 items-center justify-center rounded-full bg-black/60 text-[0.6rem] text-white" title="アップロード中">…</span>
          {:else if failedAttachmentId === a.id}
            <span class="absolute -bottom-1 -left-1 flex h-3.5 w-3.5 items-center justify-center rounded-full bg-destructive text-[0.6rem] text-white">!</span>
          {/if}
          <Button type="button" variant="ghost" size="icon-xs" class="absolute -top-1 -right-1 h-3.5 w-3.5 rounded-full bg-black/60 text-white hover:bg-black/60" title="削除" onclick={() => removeAttached(a.id)}><X size={10} /></Button>
        </div>
      {/each}
    </div>
  {/if}
```

- [ ] **Step 6: 投票フォーム(643〜694行目)**

```svelte
  {#if usePoll}
    <div class="flex flex-col gap-[5px]">
      {#each pollChoices as _, i}
        <div class="flex items-center gap-1">
          <input class="flex-1 box-border rounded border border-border bg-muted px-[9px] py-1.5 font-[inherit] text-[0.84rem] text-foreground" placeholder={`選択肢 ${i + 1}`} bind:value={pollChoices[i]} />
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            class="flex-none text-muted-foreground disabled:opacity-35"
            title="この選択肢を削除"
            disabled={pollChoices.length <= 2}
            onclick={() => (pollChoices = pollChoices.filter((_, j) => j !== i))}
          >
            <X size={12} />
          </Button>
        </div>
      {/each}
      <div class="flex flex-wrap items-center gap-3 text-[0.8rem] text-muted-foreground">
        <Button
          type="button"
          variant="outline"
          size="xs"
          disabled={pollChoices.length >= MAX_POLL_CHOICES}
          onclick={() => (pollChoices = [...pollChoices, ""])}
        >
          ＋選択肢
        </Button>
        <label><input type="checkbox" bind:checked={pollMultiple} /> 複数選択</label>
      </div>
      <div class="flex flex-wrap items-center gap-1.5 text-[0.8rem] text-muted-foreground">
        <span class="flex-none">期限:</span>
        {#each pollExpiryModes as m (m.value)}
          <Button
            type="button"
            variant="outline"
            size="xs"
            class={pollExpiryMode === m.value ? "border-primary text-primary" : ""}
            onclick={() => (pollExpiryMode = m.value)}
          >
            {m.label}
          </Button>
        {/each}
        {#if pollExpiryMode === "at"}
          <input type="datetime-local" bind:value={pollExpiresAt} class="rounded border border-border bg-muted px-1.5 py-[3px] font-[inherit] text-[0.78rem] text-foreground" />
        {:else if pollExpiryMode === "after"}
          <input
            type="number"
            min="1"
            class="w-[60px] rounded border border-border bg-muted px-1.5 py-[3px] font-[inherit] text-[0.78rem] text-foreground"
            bind:value={pollAfterAmount}
          />
          <div class="w-[90px]">
            <Dropdown bind:value={pollAfterUnit} options={pollAfterUnits} />
          </div>
        {/if}
      </div>
    </div>
  {/if}
```

- [ ] **Step 7: ツールバー(696〜735行目)**

```svelte
  <div class="flex items-center justify-between gap-2">
    <div class="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
      <VisibilitySelect bind:value={visibility} disabled={useChannel} />
      <Button
        type="button"
        variant="outline"
        size="icon-xs"
        title="画像を添付"
        bind:ref={attachTrigger}
        onclick={toggleAttachMenu}
        disabled={busy}
      ><ImagePlus size={16} /></Button>
      <Button type="button" variant="outline" size="xs" class={useCw ? "border-primary text-primary" : ""} onclick={() => (useCw = !useCw)}>CW</Button>
      <Button type="button" variant="outline" size="xs" class={usePoll ? "border-primary text-primary" : ""} onclick={() => (usePoll = !usePoll)}>投票</Button>
      <Button type="button" variant="outline" size="xs" class={useChannel ? "border-primary text-primary" : ""} onclick={() => (useChannel = !useChannel)}>チャンネル</Button>
      {#if useChannel}
        {#if channelsLoading}
          <span class="flex-none whitespace-nowrap text-[0.78rem] text-muted-foreground">読み込み中…</span>
        {:else if channelsError}
          <span class="flex-none whitespace-nowrap text-[0.78rem] text-muted-foreground">読み込みに失敗しました</span>
        {:else if channelOptions.length > 0}
          <div class="channel-select w-[140px]">
            <Dropdown bind:value={channelId} options={channelOptions} />
          </div>
        {:else}
          <span class="flex-none whitespace-nowrap text-[0.78rem] text-muted-foreground">フォロー中のチャンネルがありません</span>
        {/if}
      {/if}
      <label class="flex-none whitespace-nowrap text-[0.78rem] text-muted-foreground">
        <input
          type="checkbox"
          checked={useChannel || localOnly}
          disabled={useChannel}
          onchange={(e) => (localOnly = e.currentTarget.checked)}
        /> 連合なし
      </label>
    </div>
    <div class="flex flex-none flex-wrap items-center gap-1.5">
      <Button type="button" disabled={busy} onclick={submit}>{busy ? "…" : "投稿"}</Button>
    </div>
  </div>
  </div>
</div>
```

補足: `bind:this={attachTrigger}`は`bind:ref={attachTrigger}`に変更している。`.channel-select`のクラス名は`Dropdown.svelte`内部への`:global()`オーバーライドのターゲットとして維持する(Step 11参照)。

- [ ] **Step 8: エラーモーダル(738〜747行目)**

```svelte
{#if err}
  <Modal title="エラー" onclose={() => (err = null)}>
    {#snippet children()}
      <p class="mb-3.5 mt-0 whitespace-pre-wrap break-words text-[0.9rem] text-foreground">{err}</p>
      <div class="flex justify-end">
        <Button onclick={() => (err = null)}>わかった</Button>
      </div>
    {/snippet}
  </Modal>
{/if}
```

- [ ] **Step 9: 添付メニュー(749〜778行目)**

```svelte
{#if showAttachMenu && attachMenuPos}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (showAttachMenu = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="fixed min-w-[160px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
      style={`left:${attachMenuPos.left}px;top:${attachMenuPos.top}px`}
      onclick={(e) => e.stopPropagation()}
      role="menu"
      tabindex="-1"
    >
      <button
        class="block w-full rounded-md px-2.5 py-[7px] text-left font-[inherit] text-[0.82rem] text-foreground hover:bg-muted disabled:cursor-default disabled:opacity-50"
        type="button"
        disabled={!accountId}
        title={accountId ? undefined : "アカウントを選択してください"}
        onclick={chooseLocalUpload}
      >ローカルから選択</button>
      <button
        class="block w-full rounded-md px-2.5 py-[7px] text-left font-[inherit] text-[0.82rem] text-foreground hover:bg-muted disabled:cursor-default disabled:opacity-50"
        type="button"
        disabled={!accountId}
        title={accountId ? undefined : "アカウントを選択してください"}
        onclick={chooseDrivePicker}
      >ドライブから選択</button>
    </div>
  </div>
{/if}
```

補足: 添付メニュー項目は`CompletionPopover.svelte`の項目と同じ理由(フル幅リスト行)でButton化せず生`<button>`のまま維持する。`z-index`は`z-55`から`z-[1010]`に引き上げる。

- [ ] **Step 10: 絵文字ピッカー・DrivePicker呼び出し(780〜799行目)**

```svelte
{#if showEmojiPicker && emojiPickerPos && accountId}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (showEmojiPicker = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="fixed"
      style={`left:${emojiPickerPos.left}px;top:${emojiPickerPos.top}px`}
      onclick={(e) => e.stopPropagation()}
      role="presentation"
    >
      <ReactionPicker accountId={accountId} onpick={insertEmoji} />
    </div>
  </div>
{/if}

{#if showDrivePicker && accountId}
  <DrivePicker {accountId} onSelect={onDriveFilesSelected} onclose={() => (showDrivePicker = false)} />
{/if}
```

- [ ] **Step 11: `<style>`ブロックを縮小**

`<style>...</style>`ブロック全体(801〜1175行目)を以下に置き換える(`.channel-select :global(.trigger)`のみ残す):

```svelte
<style>
  .channel-select :global(.trigger) {
    padding: 5px 8px;
    font-size: 0.82rem;
    gap: 5px;
  }
</style>
```

- [ ] **Step 12: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 13: `pnpm test`を実行**

Run: `cd frontend && pnpm test`
Expected: このファイルにはテストが無いが、既存テスト(247/247)が壊れていないことを確認する

- [ ] **Step 14: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 15: Commit**

```bash
git add frontend/src/ui/ComposeBar.svelte
git commit -m "style: ComposeBar.svelteをTailwindクラス+Buttonプリミティブに移行"
```

---

### 手動確認(タスク完了後)

リポジトリルートから`cargo tauri dev`を起動し、以下を確認する:
- 投稿本文欄のコンパクト/展開/通常の3状態切り替え(フォーカス/未入力/モバイル投稿モーダル)
- 絵文字挿入ポップアップの表示位置・確定・アクティブ表示
- CW/投票/チャンネルトグルの表示切り替え、アクティブ時の枠線・文字色
- 投票フォーム(選択肢追加/削除、複数選択、期限モード切替(無期限/日時指定/期間指定))
- 画像添付(ローカルから選択/ドライブから選択)、サムネイル表示・削除・アップロード中/失敗表示
- 返信/引用コンテキスト表示とキャンセル
- 投稿エラーモーダルの表示・「わかった」ボタン
- メンション/ハッシュタグ/絵文字の入力中補完ポップアップ
- ライト/ダーク両テーマ
