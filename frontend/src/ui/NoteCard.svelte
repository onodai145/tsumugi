<script lang="ts">
  import { onDestroy } from "svelte";
  import type { Note } from "../bindings/tauri.gen";
  import Mfm from "../render/Mfm.svelte";
  import MediaGrid from "../render/MediaGrid.svelte";
  import CustomEmoji from "../render/CustomEmoji.svelte";
  import UnicodeEmoji from "../render/UnicodeEmoji.svelte";
  import ReactionPicker from "../input/ReactionPicker.svelte";
  import NoteMenu from "./NoteMenu.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import ReactionUsersPopover from "./ReactionUsersPopover.svelte";
  import Self from "./NoteCard.svelte";
  import { relativeTime } from "../lib/time";
  import { acct, displayName } from "../lib/userDisplay";
  import { app } from "../lib/store.svelte";
  import { reactionEmoji, isRemoteCustomEmoji, proxiedEmojiMap } from "../lib/emoji";
  import { isCustomEmojiKey, customEmojiPinKey, parseCustomEmojiPinKey } from "../lib/emojiKey";
  import { Reply, Repeat2, Quote, SmilePlus, Globe, House, Lock, Mail, MoreHorizontal } from "@lucide/svelte";
  import { openProfile } from "../lib/profileModal.svelte";

  // ノートは content-visibility:auto で contain され fixed の包含ブロック＆クリップ源に
  // なるため、ピッカーは body 直下へ portal して封じ込めを脱出させる。
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  // accountId があれば操作ボタンを出す（引用ネスト時は undefined = 表示のみ）
  // tabId/selected はトップレベル表示時のみ（キーボード選択のハイライト/スクロール用）
  // emojiAccountId は絵文字解決専用（操作性に影響しない）。未指定なら accountId を使う。
  let {
    note,
    quoted = false,
    showActions,
    hideReactions = false,
    hideActionBanner = false,
    accountId,
    emojiAccountId,
    tabId,
    selected = false,
  }: {
    note: Note;
    quoted?: boolean;
    showActions?: boolean;
    hideReactions?: boolean;
    hideActionBanner?: boolean;
    accountId?: string;
    emojiAccountId?: string;
    tabId?: string;
    selected?: boolean;
  } = $props();

  // 純粋Renote（本文なし＋renote先あり）は「誰が」を出して中身を委譲
  const isPureRenote = $derived(!note.text && !!note.renote);
  const inner = $derived(isPureRenote ? note.renote! : note);

  // quoted はスタイリング(コンパクト表示)専用。アクション表示可否は showActions で制御し、
  // 未指定時は従来通り !quoted にフォールバックする。
  const effectiveShowActions = $derived(showActions ?? !quoted);

  const emojiAcct = $derived(emojiAccountId ?? accountId);
  const instanceHost = $derived(
    emojiAcct ? app.accounts.find((a) => a.id === emojiAcct)?.host : undefined,
  );
  // 絵文字 name->url: ローカル絵文字（閲覧インスタンス、既にhome instance配信なのでそのまま）を
  // フォールバックに、note.emojis（リモート＋リアクション絵文字、生URLなのでプロキシ変換）を
  // 上書きで重ねる。
  const emojiMap = $derived(
    emojiAcct
      ? { ...app.localEmojiUrls(emojiAcct), ...proxiedEmojiMap(inner.emojis, instanceHost) }
      : inner.emojis,
  );

  // リアクションピッカーは store 管理（マウス/キーボードで一元化・同時に1つだけ開く）。
  // 同じノートがRenote直後の並列表示や複数カラムで重複して描画されうるため、
  // noteId一致だけでなく自インスタンス固有トークン（マウス）/ tabId+selected（キーボード）
  // でも一致を確認し、開いた側のインスタンスだけに表示する。
  const myToken = { kind: "instance" as const, id: crypto.randomUUID() };
  const showPicker = $derived.by(() => {
    const p = app.reactPicker;
    if (!p || p.noteId !== inner.id) return false;
    if (p.token.kind === "instance") return p.token.id === myToken.id;
    return tabId !== undefined && p.token.tabId === tabId && selected;
  });
  function togglePicker() {
    noteMenuOpen = false;
    app.reactPicker = showPicker ? null : { noteId: inner.id, token: myToken };
  }

  // ピッカーは position:fixed でスクロール領域(.notes の overflow)を脱出させる。
  // ボタン位置から算出し、上下スペースを見て開く向きを決めビューポート内にクランプ。
  // （キーボード起動でも $effect で計算されるよう showPicker に依存）
  const PICKER_W = 260;
  const PICKER_H = 290;
  let pickerBtn = $state<HTMLElement | null>(null);
  let pickerPos = $state<{ left: number; top: number } | null>(null);
  $effect(() => {
    if (!showPicker || !pickerBtn) return;
    const r = pickerBtn.getBoundingClientRect();
    const left = Math.min(Math.max(8, r.left), window.innerWidth - PICKER_W - 8);
    const spaceBelow = window.innerHeight - r.bottom;
    const top =
      spaceBelow >= PICKER_H + 8 ? r.bottom + 6 : Math.max(8, r.top - PICKER_H - 6);
    pickerPos = { left, top };
  });

  function react(reaction: string) {
    app.reactPicker = null;
    if (accountId) {
      const wasMine = inner.myReaction === reaction;
      app.toggleReaction(accountId, inner.id, reaction);
      if (!wasMine) {
        const host = app.accounts.find((a) => a.id === accountId)?.host;
        const stored =
          isCustomEmojiKey(reaction) && host
            ? customEmojiPinKey(parseCustomEmojiPinKey(reaction).name, host)
            : reaction;
        void app.recordEmojiUsage(stored);
      }
    }
  }

  // ノートメニュー(お気に入り/クリップ)。リアクションピッカーと同じ position:fixed
  // portal パターンで .notes の overflow クリップを脱出させる。
  let noteMenuOpen = $state(false);
  let noteMenuBtn = $state<HTMLElement | null>(null);
  let noteMenuPos = $state<{ left: number; top: number } | null>(null);
  const MENU_W = 200;
  $effect(() => {
    if (!noteMenuOpen || !noteMenuBtn) return;
    const r = noteMenuBtn.getBoundingClientRect();
    const left = Math.min(Math.max(8, r.right - MENU_W), window.innerWidth - MENU_W - 8);
    noteMenuPos = { left, top: r.bottom + 6 };
  });

  // 投票済み(multiple=falseは1択でもう投票不可)・期限切れなら投票不可。
  const pollExpired = $derived(!!inner.poll?.expiresAt && inner.poll.expiresAt * 1000 < Date.now());
  const pollAlreadyVoted = $derived(!inner.poll?.multiple && !!inner.poll?.choices.some((c) => c.isVoted));
  // 投票は取り消せない(Misskeyに取消APIが無い)ので、必ず確認してから送信する。
  let confirmChoice = $state<number | null>(null);
  function requestVote(choice: number) {
    if (!accountId || !inner.poll) return;
    if (pollExpired || pollAlreadyVoted || inner.poll.choices[choice].isVoted) return;
    confirmChoice = choice;
  }
  function confirmVote() {
    if (confirmChoice === null || !accountId) return;
    app.votePoll(accountId, inner.id, confirmChoice);
    confirmChoice = null;
  }
  // 連打対策: リクエスト完了を待つだけだとIPCが速く人間の連打間隔より早く終わってしまうため、
  // クリックのたびに一定時間はボタンを無効化するクールダウンを設ける。
  const RENOTE_COOLDOWN_MS = 3000;
  let renoteBusy = $state(false);
  let renoteCooldownTimer: ReturnType<typeof setTimeout> | null = null;
  onDestroy(() => {
    if (renoteCooldownTimer) clearTimeout(renoteCooldownTimer);
  });
  function doRenote() {
    if (!accountId || renoteBusy) return;
    renoteBusy = true;
    void app.renote(accountId, inner.id);
    renoteCooldownTimer = setTimeout(() => {
      renoteBusy = false;
      renoteCooldownTimer = null;
    }, RENOTE_COOLDOWN_MS);
  }

  // リアクション/Renoteの「誰が」ポップオーバー。ホバーで表示、150msのin/outディレイで
  // ボタン→ポップオーバー間のマウス移動中に消えないようにする。
  type HoverTarget = { kind: "reaction"; key: string } | { kind: "renote" };
  let hoverTarget = $state<HoverTarget | null>(null);
  let hoverBtn = $state<HTMLElement | null>(null);
  let hoverShowTimer: ReturnType<typeof setTimeout> | null = null;
  let hoverHideTimer: ReturnType<typeof setTimeout> | null = null;
  const POPOVER_W = 240;

  function enterHover(target: HoverTarget, btn: HTMLElement) {
    if (!accountId) return;
    if (hoverHideTimer) {
      clearTimeout(hoverHideTimer);
      hoverHideTimer = null;
    }
    if (hoverShowTimer) clearTimeout(hoverShowTimer);
    hoverShowTimer = setTimeout(() => {
      hoverTarget = target;
      hoverBtn = btn;
    }, 150);
  }
  function leaveHover() {
    if (hoverShowTimer) {
      clearTimeout(hoverShowTimer);
      hoverShowTimer = null;
    }
    hoverHideTimer = setTimeout(() => {
      hoverTarget = null;
      hoverBtn = null;
    }, 150);
  }
  function keepHover() {
    if (hoverHideTimer) {
      clearTimeout(hoverHideTimer);
      hoverHideTimer = null;
    }
  }

  let hoverPos = $state<{ left: number; top: number } | null>(null);
  $effect(() => {
    if (!hoverTarget || !hoverBtn) {
      hoverPos = null;
      return;
    }
    const r = hoverBtn.getBoundingClientRect();
    const left = Math.min(Math.max(8, r.left), window.innerWidth - POPOVER_W - 8);
    hoverPos = { left, top: r.bottom + 10 };
  });

  // キーボード選択中はスクロールで見える位置へ
  let el = $state<HTMLElement | null>(null);
  $effect(() => {
    if (selected && el) el.scrollIntoView({ block: "nearest" });
  });

  let cwOpen = $state(false);
  const VIS_ICON = { public: Globe, home: House, followers: Lock, specified: Mail } as const;
  const VIS_LABEL = { public: "公開", home: "ホーム", followers: "フォロワー", specified: "ダイレクト" } as const;
  // reactions: { key: count } を件数降順に
  const reactionList = $derived(
    Object.entries(inner.reactions).sort((a, b) => b[1] - a[1]),
  );

  // 本家準拠(use-note.ts canRenote): public/home は誰でも可、followers は本人のみ、
  // specified(ダイレクト) は不可。RN/引用ボタンはこの条件を満たす時だけ表示する。
  const canRenote = $derived.by(() => {
    if (inner.visibility === "public" || inner.visibility === "home") return true;
    if (inner.visibility === "followers") {
      const acc = accountId ? app.accounts.find((a) => a.id === accountId) : undefined;
      return !!acc && acc.userId === inner.user.id;
    }
    return false;
  });

</script>

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
  {#if isPureRenote && !hideActionBanner}
    <div class="mb-0.5 inline-flex items-center gap-1 text-xs text-[var(--success)]">
      <Repeat2 size={12} /> <Mfm
        text={displayName(note.user)}
        emojis={proxiedEmojiMap(note.user.emojis, instanceHost)}
        simple
      /> がRenote
    </div>
  {/if}
  {#if inner.replyId && !hideActionBanner}
    <div class="mb-0.5 inline-flex items-center gap-1 text-xs text-[var(--info)]">
      <Reply size={12} /> 返信
    </div>
  {/if}

  <div class="flex gap-[7px]">
    {#if inner.user.avatarUrl}
      <img
        class="h-[34px] w-[34px] flex-none rounded-md object-cover"
        data-testid="note-avatar"
        src={inner.user.avatarUrl}
        alt=""
        loading="lazy"
        onclick={() => openProfile({ userId: inner.user.id }, accountId)}
        style="cursor: pointer"
      />
    {:else}
      <!-- role="button"だがButtonプリミティブ非経由のため、キーボードフォーカス時の視認性を
           Buttonのfocus-visibleパターン（スタイルガイド§7、border-ringは無枠のため省略）で個別に補う -->
      <div
        class="avatar h-[34px] w-[34px] flex-none rounded-md outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
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
          class="rounded-sm text-sm font-semibold outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
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
          class="rounded-sm text-xs text-muted-foreground outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
          data-testid="note-acct"
          onclick={() => openProfile({ userId: inner.user.id }, accountId)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === "Enter" && openProfile({ userId: inner.user.id }, accountId)}
          style="cursor: pointer"
        >{acct(inner.user)}</span>
        <span class="ml-auto text-xs text-muted-foreground" title={new Date(inner.createdAt * 1000).toLocaleString()}>
          {relativeTime(inner.createdAt)}
        </span>
        {#if inner.visibility !== "public"}
          {@const VisIcon = VIS_ICON[inner.visibility]}
          <span class="inline-flex items-center rounded-sm border border-border p-0.5 text-xs text-muted-foreground" title={VIS_LABEL[inner.visibility]}><VisIcon size={12} /></span>
        {/if}
      </header>

      {#if inner.cw}
        <div class="mt-0.5">
          <span class="text-sm [-webkit-user-select:text] select-text"><Mfm text={inner.cw} emojis={emojiMap} nyaize={inner.user.isCat} /></span>
          <button type="button" class="cw-toggle ml-2 rounded-md border border-border px-2 py-px text-sm text-foreground" onclick={() => (cwOpen = !cwOpen)}>
            {cwOpen ? "隠す" : `続きを見る${inner.text ? "" : ""}`}
          </button>
        </div>
      {/if}

      {#if !inner.cw || cwOpen}
        {#if inner.text}
          <div class="mt-px whitespace-pre-wrap break-words text-sm leading-[1.42] [-webkit-user-select:text] select-text"><Mfm text={inner.text} emojis={emojiMap} nyaize={inner.user.isCat} /></div>
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
                  ? "poll-choice flex w-full items-center justify-between rounded-md px-2 py-[5px] text-left font-[inherit] text-sm text-foreground outline outline-1 outline-primary disabled:cursor-default"
                  : "poll-choice flex w-full items-center justify-between rounded-md px-2 py-[5px] text-left font-[inherit] text-sm text-foreground disabled:cursor-default"}
                disabled={!accountId || pollExpired || pollAlreadyVoted || choice.isVoted}
                onclick={() => requestVote(i)}
              >
                <span>{choice.text}</span>
                <span>{choice.votes}</span>
              </button>
            {/each}
          </div>
          {#if pollExpired}
            <p class="mt-1 mb-0 text-sm text-muted-foreground">投票は締め切られました</p>
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
                  ? "reaction mine inline-flex items-center gap-[3px] rounded-sm border border-primary px-[7px] py-px text-sm text-foreground disabled:cursor-default disabled:opacity-60"
                  : "reaction inline-flex items-center gap-[3px] rounded-sm border border-border px-[7px] py-px text-sm text-foreground disabled:cursor-default disabled:opacity-60"}
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

      {#if effectiveShowActions && accountId}
        <footer class="actions mt-2 flex items-center gap-[14px] text-sm text-muted-foreground">
          <button type="button" class="inline-flex h-6 items-center gap-1 rounded-md px-1 text-sm leading-none text-muted-foreground" aria-label="返信" onclick={() => app.openCompose(accountId!, { replyTo: inner })}>
            <Reply size={16} /> {inner.replyCount || ""}
          </button>
          {#if canRenote}
            <button
              type="button"
              class={renoteBusy
                ? "inline-flex h-6 items-center gap-1 rounded-md px-1 text-sm leading-none text-muted-foreground opacity-50"
                : "inline-flex h-6 items-center gap-1 rounded-md px-1 text-sm leading-none text-muted-foreground"}
              aria-label="Renote"
              onclick={doRenote}
              onmouseenter={(e) => enterHover({ kind: "renote" }, e.currentTarget as HTMLElement)}
              onmouseleave={leaveHover}
            >
              <Repeat2 size={16} />
              {#if inner.renoteCount > 0}
                <span>{inner.renoteCount}</span>
              {/if}
            </button>
            <button type="button" class="inline-flex h-6 items-center gap-1 rounded-md px-1 text-sm leading-none text-muted-foreground" aria-label="引用" onclick={() => app.openCompose(accountId!, { quoteOf: inner })}>
              <Quote size={16} />
            </button>
          {/if}
          <div class="relative inline-flex h-6 items-center">
            <button
              type="button"
              bind:this={pickerBtn}
              class={showPicker
                ? "on inline-flex h-6 items-center gap-1 rounded-md px-1 text-sm leading-none text-muted-foreground"
                : "inline-flex h-6 items-center gap-1 rounded-md px-1 text-sm leading-none text-muted-foreground"}
              aria-label="リアクション"
              onclick={togglePicker}
            >
              <SmilePlus size={16} /> {inner.reactionCount || ""}
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
          <div class="relative inline-flex h-6 items-center">
            <button
              type="button"
              bind:this={noteMenuBtn}
              class={noteMenuOpen
                ? "on inline-flex h-6 items-center gap-1 rounded-md px-1 text-sm leading-none text-muted-foreground"
                : "inline-flex h-6 items-center gap-1 rounded-md px-1 text-sm leading-none text-muted-foreground"}
              aria-label="その他"
              onclick={() => {
                app.reactPicker = null;
                noteMenuOpen = !noteMenuOpen;
              }}
            >
              <MoreHorizontal size={16} />
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
