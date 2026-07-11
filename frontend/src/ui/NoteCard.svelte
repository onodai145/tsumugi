<script lang="ts">
  import type { Note } from "../bindings/tauri.gen";
  import Mfm from "../render/Mfm.svelte";
  import MediaGrid from "../render/MediaGrid.svelte";
  import CustomEmoji from "../render/CustomEmoji.svelte";
  import UnicodeEmoji from "../render/UnicodeEmoji.svelte";
  import ReactionPicker from "../input/ReactionPicker.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Self from "./NoteCard.svelte";
  import { relativeTime } from "../lib/time";
  import { app } from "../lib/store.svelte";
  import { reactionEmoji, isRemoteCustomEmoji } from "../lib/emoji";
  import { Reply, Repeat2, Quote, SmilePlus, Globe, House, Lock, Mail } from "@lucide/svelte";

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
    app.reactPickerNoteId = null;
    if (accountId) app.toggleReaction(accountId, inner.id, reaction);
  }

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
  const VIS_ICON = { public: Globe, home: House, followers: Lock, specified: Mail } as const;
  const VIS_LABEL = { public: "公開", home: "ホーム", followers: "フォロワー", specified: "ダイレクト" } as const;
  const acct = (u: Note["user"]) => (u.host ? `@${u.username}@${u.host}` : `@${u.username}`);
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
  class="note"
  class:quoted
  class:selected={selected && !quoted}
  bind:this={el}
  onclick={tabId ? () => app.selectNote(tabId, note.id) : undefined}
>
  {#if isPureRenote}
    <div class="renote-banner"><Repeat2 size={13} /> {displayName(note.user)} がRenote</div>
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
        {#if inner.visibility !== "public"}
          {@const VisIcon = VIS_ICON[inner.visibility]}
          <span class="vis" title={VIS_LABEL[inner.visibility]}><VisIcon size={12} /></span>
        {/if}
      </header>

      {#if inner.cw}
        <div class="cw">
          <span class="cw-text"><Mfm text={inner.cw} emojis={emojiMap} nyaize={inner.user.isCat} /></span>
          <button class="cw-toggle" onclick={() => (cwOpen = !cwOpen)}>
            {cwOpen ? "隠す" : `続きを見る${inner.text ? "" : ""}`}
          </button>
        </div>
      {/if}

      {#if !inner.cw || cwOpen}
        {#if inner.text}
          <div class="text"><Mfm text={inner.text} emojis={emojiMap} nyaize={inner.user.isCat} /></div>
        {/if}
        {#if inner.files.length > 0}
          <MediaGrid files={inner.files} />
        {/if}
        {#if inner.poll}
          <div class="poll">
            {#each inner.poll.choices as choice, i}
              <button
                class="poll-choice"
                class:voted={choice.isVoted}
                disabled={!accountId || pollExpired || pollAlreadyVoted || choice.isVoted}
                onclick={() => requestVote(i)}
              >
                <span class="poll-text">{choice.text}</span>
                <span class="poll-votes">{choice.votes}</span>
              </button>
            {/each}
          </div>
          {#if pollExpired}
            <p class="poll-hint">投票は締め切られました</p>
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
              disabled={!accountId || isRemoteCustomEmoji(key)}
              title={isRemoteCustomEmoji(key) ? "このインスタンスに無い絵文字のためリアクションできません" : undefined}
              onclick={() => react(key)}
            >
              {#if key.startsWith(":")}
                {@const e = reactionEmoji(key, emojiMap)}
                <CustomEmoji name={e.name} url={e.url} />
              {:else}
                <UnicodeEmoji char={key} />
              {/if}
              <span class="rcount">{count}</span>
            </button>
          {/each}
        </div>
      {/if}

      {#if !quoted && accountId}
        <footer class="actions">
          <button title="返信" onclick={() => app.openCompose(accountId!, { replyTo: inner })}>
            <Reply size={15} /> {inner.replyCount || ""}
          </button>
          {#if canRenote}
            <button title="Renote" onclick={doRenote}>
              <Repeat2 size={15} /> {inner.renoteCount || ""}
            </button>
            <button title="引用" onclick={() => app.openCompose(accountId!, { quoteOf: inner })}>
              <Quote size={15} />
            </button>
          {/if}
          <div class="react-wrap">
            <button
              bind:this={pickerBtn}
              title="リアクション"
              class:on={showPicker}
              onclick={togglePicker}
            >
              <SmilePlus size={15} /> {inner.reactionCount || ""}
            </button>
            {#if showPicker && pickerPos}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="picker-overlay" use:portal onclick={() => (app.reactPickerNoteId = null)} role="presentation">
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="picker-pop"
                  style={`left:${pickerPos.left}px;top:${pickerPos.top}px`}
                  onclick={(e) => e.stopPropagation()}
                  role="presentation"
                >
                  <ReactionPicker {accountId} onpick={react} />
                </div>
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
    display: inline-flex;
    align-items: center;
    gap: 4px;
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
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
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
    display: inline-flex;
    align-items: center;
    padding: 2px;
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
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
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
    width: 100%;
    padding: 5px 8px;
    border: none;
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
    color: var(--text);
    border-radius: 6px;
    font-size: 0.88rem;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
  }
  .poll-choice:hover:not(:disabled) {
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  }
  .poll-choice:disabled {
    cursor: default;
  }
  .poll-choice.voted {
    outline: 1px solid var(--accent);
  }
  .poll-hint {
    margin: 4px 0 0;
    font-size: 0.78rem;
    color: var(--text-dim);
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
    /* カラムと同じ不透明度を適用(背景画像設定時にカラムだけ透けてリアクションだけ
       不透明のまま浮いて見えるのを防ぐ)。既定100%なら見た目は従来どおり不透明。 */
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
    border: 1px solid var(--border);
    border-radius:  3px;
    font-size: 0.85rem;
    color: var(--text);
    cursor: pointer;
  }
  .reaction:disabled {
    cursor: default;
    opacity: 0.6;
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
    display: inline-flex;
    align-items: center;
    gap: 4px;
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
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
  }
  .react-wrap {
    position: relative;
  }
  .picker-overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
  }
  .picker-pop {
    position: fixed;
  }
</style>
