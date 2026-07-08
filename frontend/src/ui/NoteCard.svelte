<script lang="ts">
  import type { Note } from "../bindings/tauri.gen";
  import Mfm from "../render/Mfm.svelte";
  import MediaGrid from "../render/MediaGrid.svelte";
  import CustomEmoji from "../render/CustomEmoji.svelte";
  import ReactionPicker from "../input/ReactionPicker.svelte";
  import Self from "./NoteCard.svelte";
  import { relativeTime } from "../lib/time";
  import { app } from "../lib/store.svelte";
  import { reactionEmoji } from "../lib/emoji";

  // accountId があれば操作ボタンを出す（引用ネスト時は undefined = 表示のみ）
  // tabId/selected はトップレベル表示時のみ（キーボード選択のハイライト/スクロール用）
  // emojiAccountId は絵文字解決専用（操作性に影響しない）。未指定なら accountId を使う。
  let {
    note,
    quoted = false,
    accountId,
    emojiAccountId,
    tabId,
    selected = false,
  }: {
    note: Note;
    quoted?: boolean;
    accountId?: string;
    emojiAccountId?: string;
    tabId?: string;
    selected?: boolean;
  } = $props();

  // 純粋Renote（本文なし＋renote先あり）は「誰が」を出して中身を委譲
  const isPureRenote = $derived(!note.text && !!note.renote);
  const inner = $derived(isPureRenote ? note.renote! : note);

  // 絵文字 name->url: ローカル絵文字（閲覧インスタンス）をフォールバックに、
  // note.emojis（リモート＋リアクション絵文字）を上書きで重ねる。
  const emojiAcct = $derived(emojiAccountId ?? accountId);
  const emojiMap = $derived(
    emojiAcct ? { ...app.localEmojiUrls(emojiAcct), ...inner.emojis } : inner.emojis,
  );

  // リアクションピッカーは store 管理（マウス/キーボードで一元化・同時に1つだけ開く）
  const showPicker = $derived(app.reactPickerNoteId === inner.id);
  function togglePicker() {
    app.reactPickerNoteId = showPicker ? null : inner.id;
  }

  function react(reaction: string) {
    app.reactPickerNoteId = null;
    if (accountId) app.toggleReaction(accountId, inner.id, reaction);
  }
  function doRenote() {
    if (accountId) app.renote(accountId, inner.id);
  }

  // キーボード選択中はスクロールで見える位置へ
  let el = $state<HTMLElement | null>(null);
  $effect(() => {
    if (selected && el) el.scrollIntoView({ block: "nearest" });
  });

  let cwOpen = $state(false);
  const displayName = (u: Note["user"]) => u.name ?? u.username;
  const acct = (u: Note["user"]) => (u.host ? `@${u.username}@${u.host}` : `@${u.username}`);
  // reactions: { key: count } を件数降順に
  const reactionList = $derived(
    Object.entries(inner.reactions).sort((a, b) => b[1] - a[1]),
  );

</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<article
  class="note"
  class:quoted
  class:selected={selected && !quoted}
  bind:this={el}
  onclick={tabId ? () => app.selectNote(tabId, note.id) : undefined}
>
  {#if isPureRenote}
    <div class="renote-banner">🔁 {displayName(note.user)} がRenote</div>
  {/if}

  <div class="row">
    {#if inner.user.avatarUrl}
      <img class="avatar" src={inner.user.avatarUrl} alt="" loading="lazy" />
    {:else}
      <div class="avatar placeholder"></div>
    {/if}
    <div class="body">
      <header class="head">
        <span class="name">{displayName(inner.user)}</span>
        <span class="acct">{acct(inner.user)}</span>
        <span class="time" title={new Date(inner.createdAt * 1000).toLocaleString()}>
          {relativeTime(inner.createdAt)}
        </span>
        {#if inner.visibility !== "public"}<span class="vis">{inner.visibility}</span>{/if}
      </header>

      {#if inner.cw}
        <div class="cw">
          <span class="cw-text"><Mfm text={inner.cw} emojis={emojiMap} /></span>
          <button class="cw-toggle" onclick={() => (cwOpen = !cwOpen)}>
            {cwOpen ? "隠す" : `続きを見る${inner.text ? "" : ""}`}
          </button>
        </div>
      {/if}

      {#if !inner.cw || cwOpen}
        {#if inner.text}
          <div class="text"><Mfm text={inner.text} emojis={emojiMap} /></div>
        {/if}
        {#if inner.files.length > 0}
          <MediaGrid files={inner.files} />
        {/if}
        {#if inner.poll}
          <div class="poll">
            {#each inner.poll.choices as choice}
              <div class="poll-choice" class:voted={choice.isVoted}>
                <span class="poll-text">{choice.text}</span>
                <span class="poll-votes">{choice.votes}</span>
              </div>
            {/each}
          </div>
        {/if}
        <!-- 引用Renote: 本文ありで renote 先がある場合、中身をネスト表示 -->
        {#if inner.text && inner.renote}
          <Self note={inner.renote} quoted={true} />
        {/if}
      {/if}

      {#if reactionList.length > 0}
        <div class="reactions">
          {#each reactionList as [key, count]}
            <button
              class="reaction"
              class:mine={inner.myReaction === key}
              disabled={!accountId}
              onclick={() => react(key)}
            >
              {#if key.startsWith(":")}
                {@const e = reactionEmoji(key, emojiMap)}
                <CustomEmoji name={e.name} url={e.url} />
              {:else}
                {key}
              {/if}
              <span class="rcount">{count}</span>
            </button>
          {/each}
        </div>
      {/if}

      {#if !quoted && accountId}
        <footer class="actions">
          <button title="返信" onclick={() => app.openCompose(accountId!, { replyTo: inner })}>
            💬 {inner.replyCount || ""}
          </button>
          <button title="Renote" onclick={doRenote}>🔁 {inner.renoteCount || ""}</button>
          <button title="引用" onclick={() => app.openCompose(accountId!, { quoteOf: inner })}>❝</button>
          <div class="react-wrap">
            <button
              title="リアクション"
              class:on={showPicker}
              onclick={togglePicker}
            >
              ➕ {inner.reactionCount || ""}
            </button>
            {#if showPicker}
              <div class="picker-pop">
                <ReactionPicker {accountId} onpick={react} />
              </div>
            {/if}
          </div>
        </footer>
      {/if}
    </div>
  </div>
</article>

<style>
  .note {
    padding: 6px 9px;
    border-bottom: 1px solid var(--border);
    /* 仮想化-lite: 画面外は描画スキップ */
    content-visibility: auto;
    contain-intrinsic-size: auto 92px;
  }
  .note.quoted {
    border: 1px solid var(--border);
    border-radius: 4px;
    margin-top: 6px;
    padding: 5px 7px;
    content-visibility: visible;
  }
  .note.selected {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .renote-banner {
    font-size: 0.74rem;
    color: var(--text-dim);
    margin-bottom: 2px;
  }
  .row {
    display: flex;
    gap: 7px;
  }
  .avatar {
    width: 34px;
    height: 34px;
    border-radius: 5px;
    object-fit: cover;
    flex: none;
  }
  .avatar.placeholder {
    background: var(--surface-3);
  }
  .body {
    min-width: 0;
    flex: 1;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 5px;
    flex-wrap: wrap;
  }
  .name {
    font-weight: 600;
    font-size: 0.86rem;
  }
  .acct,
  .time,
  .vis {
    color: var(--text-dim);
    font-size: 0.76rem;
  }
  .time {
    margin-left: auto;
  }
  .vis {
    padding: 0 4px;
    border: 1px solid var(--border);
    border-radius: 3px;
  }
  .text {
    margin-top: 1px;
    white-space: pre-wrap;
    word-break: break-word;
    line-height: 1.42;
    font-size: 0.9rem;
  }
  .cw {
    margin-top: 2px;
  }
  .cw-toggle {
    margin-left: 8px;
    font-size: 0.8rem;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text);
    border-radius: 6px;
    padding: 1px 8px;
    cursor: pointer;
  }
  .poll {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .poll-choice {
    display: flex;
    justify-content: space-between;
    padding: 5px 8px;
    background: var(--surface-2);
    border-radius: 6px;
    font-size: 0.88rem;
  }
  .poll-choice.voted {
    outline: 1px solid var(--accent);
  }
  .reactions {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin-top: 8px;
  }
  .reaction {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 7px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius:  3px;
    font-size: 0.85rem;
    color: var(--text);
    cursor: pointer;
  }
  .reaction:disabled {
    cursor: default;
  }
  .reaction.mine {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .rcount {
    color: var(--text-dim);
  }
  .actions {
    display: flex;
    gap: 14px;
    align-items: center;
    margin-top: 8px;
    color: var(--text-dim);
    font-size: 0.8rem;
  }
  .actions button {
    background: transparent;
    border: none;
    color: var(--text-dim);
    cursor: pointer;
    font-size: 0.82rem;
    padding: 2px 4px;
    border-radius: 6px;
  }
  .actions button:hover,
  .actions button.on {
    color: var(--accent);
    background: var(--surface-2);
  }
  .react-wrap {
    position: relative;
  }
  .picker-pop {
    position: absolute;
    bottom: 100%;
    left: 0;
    z-index: 20;
    margin-bottom: 6px;
  }
</style>
