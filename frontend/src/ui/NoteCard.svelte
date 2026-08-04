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
  import { app } from "../lib/store.svelte";
  import { reactionEmoji, isRemoteCustomEmoji, proxiedEmojiMap } from "../lib/emoji";
  import { isCustomEmojiKey, customEmojiPinKey, parseCustomEmojiPinKey } from "../lib/emojiKey";
  import { Reply, Repeat2, Quote, SmilePlus, Globe, House, Lock, Mail, MoreHorizontal } from "@lucide/svelte";

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
  {#if isPureRenote && !hideActionBanner}
    <div class="renote-banner">
      <Repeat2 size={13} /> <Mfm
        text={displayName(note.user)}
        emojis={proxiedEmojiMap(note.user.emojis, instanceHost)}
        simple
      /> がRenote
    </div>
  {/if}
  {#if inner.replyId && !hideActionBanner}
    <div class="reply-banner">
      <Reply size={13} /> 返信
    </div>
  {/if}

  <div class="row">
    {#if inner.user.avatarUrl}
      <img class="avatar" src={inner.user.avatarUrl} alt="" loading="lazy" />
    {:else}
      <div class="avatar placeholder"></div>
    {/if}
    <div class="body">
      <header class="head">
        <span class="name"><Mfm
          text={displayName(inner.user)}
          emojis={proxiedEmojiMap(inner.user.emojis, instanceHost)}
          simple
        /></span>
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
          <Self note={inner.renote} quoted={true} hideReactions emojiAccountId={emojiAcct} />
        {/if}
      {/if}

      {#if !hideReactions && reactionList.length > 0}
        <div class="reactions">
          {#each reactionList as [key, count]}
            <!-- disabled な button は mouseenter/mouseleave を発火しないため(WebKitGTK含む)、
                 hover検知はラッパーの span 側に付与する。クリック不可の挙動自体はbuttonのdisabledのまま維持。 -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span
              class="reaction-wrap"
              onmouseenter={(e) => enterHover({ kind: "reaction", key }, e.currentTarget as HTMLElement)}
              onmouseleave={leaveHover}
            >
              <button
                class="reaction"
                class:mine={inner.myReaction === key}
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
                <span class="rcount">{count}</span>
              </button>
            </span>
          {/each}
        </div>
      {/if}

      {#if effectiveShowActions && accountId}
        <footer class="actions">
          <button aria-label="返信" onclick={() => app.openCompose(accountId!, { replyTo: inner })}>
            <Reply size={15} /> {inner.replyCount || ""}
          </button>
          {#if canRenote}
            <button
              aria-label="Renote"
              class:busy={renoteBusy}
              onclick={doRenote}
              onmouseenter={(e) => enterHover({ kind: "renote" }, e.currentTarget as HTMLElement)}
              onmouseleave={leaveHover}
            >
              <Repeat2 size={15} />
              {#if inner.renoteCount > 0}
                <span>{inner.renoteCount}</span>
              {/if}
            </button>
            <button aria-label="引用" onclick={() => app.openCompose(accountId!, { quoteOf: inner })}>
              <Quote size={15} />
            </button>
          {/if}
          <div class="react-wrap">
            <button
              bind:this={pickerBtn}
              aria-label="リアクション"
              class:on={showPicker}
              onclick={togglePicker}
            >
              <SmilePlus size={15} /> {inner.reactionCount || ""}
            </button>
            {#if showPicker && pickerPos}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="picker-overlay" use:portal onclick={() => (app.reactPicker = null)} role="presentation">
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
          <div class="menu-wrap">
            <button
              bind:this={noteMenuBtn}
              aria-label="その他"
              class:on={noteMenuOpen}
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
              <div class="picker-overlay" use:portal onclick={() => (noteMenuOpen = false)} role="presentation">
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="picker-pop"
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
  .note {
    padding: 6px 9px;
    border-bottom: 1px solid var(--border);
    /* 仮想化-lite: 画面外は描画スキップ */
    content-visibility: auto;
    contain-intrinsic-size: auto 92px;
    /* ドラッグ選択で本文以外(ユーザー名/時刻/ボタン等)まで巻き込まれないよう既定で不可に。
       WebKitGTK(Linuxのwebview)は無印字プロパティを反映しないため -webkit- 併記が必須。 */
    -webkit-user-select: none;
    user-select: none;
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
    color: var(--success);
    margin-bottom: 2px;
  }
  .reply-banner {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 0.74rem;
    color: var(--info);
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
    -webkit-user-select: text;
    user-select: text;
  }
  .cw {
    margin-top: 2px;
  }
  .cw-text {
    font-size: 0.9rem;
    -webkit-user-select: text;
    user-select: text;
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
  .reaction-wrap {
    display: inline-flex;
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
  .actions button.busy {
    opacity: 0.5;
  }
  .react-wrap {
    position: relative;
  }
  .menu-wrap {
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
