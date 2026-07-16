<script lang="ts">
  import { app } from "../lib/store.svelte";
  import AccountSelect from "./AccountSelect.svelte";
  import VisibilitySelect from "./VisibilitySelect.svelte";
  import Dropdown from "./Dropdown.svelte";
  import { commands, unwrap } from "../lib/ipc";
  import { open } from "@tauri-apps/plugin-dialog";
  import { ImagePlus, X } from "@lucide/svelte";
  import type {
    NoteDraft_Deserialize as NoteDraft,
    VisibilityInput,
    DriveFile,
    Note,
  } from "../bindings/tauri.gen";

  // expanded: モバイルの投稿モーダルなど、常に複数行分の入力欄を確保したい文脈向け
  // (コンパクト表示への収縮を無効化する)。
  let { onPosted, expanded = false }: { onPosted?: () => void; expanded?: boolean } = $props();

  let accountId = $state(app.defaultAccountId());
  // ユーザが手動でアカウントを切り替えたら、以後は設定→アカウントの既定変更に追従しない
  let accountTouched = $state(false);
  let text = $state("");
  let cw = $state("");
  let useCw = $state(false);
  let visibility = $state<VisibilityInput>("public");
  let localOnly = $state(false);
  const MAX_POLL_CHOICES = 10;
  let usePoll = $state(false);
  let pollChoices = $state<string[]>(["", ""]);
  let pollMultiple = $state(false);
  type PollExpiryMode = "none" | "at" | "after";
  let pollExpiryMode = $state<PollExpiryMode>("none");
  let pollExpiresAt = $state(""); // datetime-local文字列(mode="at"用)
  let pollAfterAmount = $state(1); // mode="after"用の数量
  type PollAfterUnit = "minute" | "hour" | "day";
  let pollAfterUnit = $state<PollAfterUnit>("hour");
  const POLL_AFTER_UNIT_MS: Record<PollAfterUnit, number> = {
    minute: 60_000,
    hour: 3_600_000,
    day: 86_400_000,
  };
  const pollExpiryModes: { value: PollExpiryMode; label: string }[] = [
    { value: "none", label: "無期限" },
    { value: "at", label: "日時を指定" },
    { value: "after", label: "期間を指定" },
  ];
  const pollAfterUnits: { value: PollAfterUnit; label: string }[] = [
    { value: "minute", label: "分後" },
    { value: "hour", label: "時間後" },
    { value: "day", label: "日後" },
  ];
  let attached = $state<DriveFile[]>([]);
  let uploading = $state(false);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let replyTo = $state<Note | undefined>(undefined);
  let quoteOf = $state<Note | undefined>(undefined);
  let textarea = $state<HTMLTextAreaElement | undefined>(undefined);
  let focused = $state(false);
  // フォーカスが無く、かつ何も入力/添付/展開していない時だけコンパクト表示にする
  // (未送信の内容がある間は縮めない)。
  const compact = $derived(
    !expanded &&
      !focused &&
      !text.trim() &&
      !cw.trim() &&
      attached.length === 0 &&
      !usePoll &&
      !replyTo &&
      !quoteOf,
  );

  // アカウントが後から読まれた場合／既定アカウントが変更された場合の追従（手動選択後は止める）
  $effect(() => {
    if (!accountTouched) accountId = app.defaultAccountId();
  });

  // 返信/引用/新規投稿ショートカット・ボタンからの「開く」要求を消費してこのバーへ反映する。
  // app.compose は一過性のシグナルとして扱い、消費後すぐ null に戻す（次の要求も同じ形で届くため）。
  $effect(() => {
    const c = app.compose;
    if (!c) return;
    // 返信/引用は対象ノートのアカウントに固定。素の新規投稿(ショートカットNなど)は
    // 既定アカウント追従を維持したいので accountTouched は立てない。
    if (c.replyTo || c.quoteOf) {
      accountId = c.accountId;
      accountTouched = true;
    }
    replyTo = c.replyTo;
    quoteOf = c.quoteOf;
    // 返信先の @acct を本文へ自動挿入する（本家Misskeyクライアント準拠）。
    // 未入力の時だけ差し込み、既に何か書きかけている場合は上書きしない。
    if (c.replyTo && !text.trim()) {
      text = `${acctOf(c.replyTo.user)} `;
    }
    app.compose = null;
    textarea?.focus();
  });

  function acctOf(u: Note["user"]): string {
    return u.host ? `@${u.username}@${u.host}` : `@${u.username}`;
  }

  function cancelContext() {
    replyTo = undefined;
    quoteOf = undefined;
  }

  async function pickFiles() {
    err = null;
    const picked = await open({
      multiple: true,
      filters: [{ name: "画像/動画", extensions: ["png", "jpg", "jpeg", "gif", "webp", "mp4", "webm"] }],
    });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    uploading = true;
    try {
      for (const p of paths) {
        attached = [...attached, await unwrap(commands.uploadFile(accountId, p))];
      }
    } catch (e) {
      err = String(e);
    } finally {
      uploading = false;
    }
  }

  function removeAttached(id: string) {
    attached = attached.filter((f) => f.id !== id);
  }

  async function submit() {
    err = null;
    if (!accountId) {
      err = "アカウントを選択してください";
      return;
    }
    const choices = pollChoices.map((s) => s.trim()).filter(Boolean);
    if (!text.trim() && !quoteOf && choices.length === 0 && attached.length === 0) return;
    let expiresAt: number | null = null;
    if (pollExpiryMode === "at" && pollExpiresAt) {
      expiresAt = new Date(pollExpiresAt).getTime();
    } else if (pollExpiryMode === "after") {
      expiresAt = Date.now() + pollAfterAmount * POLL_AFTER_UNIT_MS[pollAfterUnit];
    }
    const draft: NoteDraft = {
      text: text.trim() || null,
      cw: useCw && cw.trim() ? cw.trim() : null,
      visibility,
      fileIds: attached.map((f) => f.id),
      poll: usePoll && choices.length >= 2 ? { choices, multiple: pollMultiple, expiresAt } : null,
      replyId: replyTo?.id ?? null,
      renoteId: quoteOf?.id ?? null,
      localOnly,
    };
    busy = true;
    try {
      await app.postNote(accountId, draft);
      text = "";
      cw = "";
      useCw = false;
      usePoll = false;
      pollChoices = ["", ""];
      pollMultiple = false;
      pollExpiryMode = "none";
      pollExpiresAt = "";
      pollAfterAmount = 1;
      pollAfterUnit = "hour";
      localOnly = false;
      attached = [];
      replyTo = undefined;
      quoteOf = undefined;
      onPosted?.();
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      submit();
    } else if (e.key === "Escape" && (replyTo || quoteOf)) {
      e.preventDefault();
      cancelContext();
    }
  }
</script>

<div class="composewrap">
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

  <div class="composebox">
  {#if replyTo || quoteOf}
    <div class="context">
      <span class="context-text">
        {replyTo ? "返信: " : "引用: "}@{(replyTo ?? quoteOf)!.user.username} — {(replyTo ?? quoteOf)!.text ?? ""}
      </span>
      <button class="context-x" title="キャンセル" onclick={cancelContext}><X size={12} /></button>
    </div>
  {/if}

  {#if useCw}
    <input class="cw-input" placeholder="内容警告 (CW)" bind:value={cw} />
  {/if}

  <textarea
    class="text"
    class:compact
    class:expanded
    rows={expanded ? 4 : 1}
    placeholder="いまどうしてる？（Ctrl+Enter で投稿）"
    bind:value={text}
    bind:this={textarea}
    onkeydown={onKey}
    onfocus={() => (focused = true)}
    onblur={() => (focused = false)}
  ></textarea>

  {#if attached.length > 0 || uploading}
    <div class="thumbs">
      {#each attached as f (f.id)}
        <div class="thumb-wrap">
          {#if f.mimeType.startsWith("image/")}
            <img class="thumb" src={f.thumbnailUrl ?? f.url} alt="" />
          {:else}
            <span class="thumb badge">{f.mimeType.split("/")[0]}</span>
          {/if}
          <button class="thumb-x" title="削除" onclick={() => removeAttached(f.id)}><X size={10} /></button>
        </div>
      {/each}
      {#if uploading}<span class="thumb badge">…</span>{/if}
    </div>
  {/if}

  {#if usePoll}
    <div class="poll">
      {#each pollChoices as _, i}
        <div class="poll-choice-row">
          <input class="poll-choice" placeholder={`選択肢 ${i + 1}`} bind:value={pollChoices[i]} />
          <button
            class="poll-choice-x"
            title="この選択肢を削除"
            disabled={pollChoices.length <= 2}
            onclick={() => (pollChoices = pollChoices.filter((_, j) => j !== i))}
          >
            <X size={12} />
          </button>
        </div>
      {/each}
      <div class="poll-actions">
        <button
          class="mini"
          disabled={pollChoices.length >= MAX_POLL_CHOICES}
          onclick={() => (pollChoices = [...pollChoices, ""])}
        >
          ＋選択肢
        </button>
        <label><input type="checkbox" bind:checked={pollMultiple} /> 複数選択</label>
      </div>
      <div class="poll-expiry">
        <span class="expiry-label">期限:</span>
        {#each pollExpiryModes as m (m.value)}
          <button
            class="mini"
            class:active={pollExpiryMode === m.value}
            onclick={() => (pollExpiryMode = m.value)}
          >
            {m.label}
          </button>
        {/each}
        {#if pollExpiryMode === "at"}
          <input type="datetime-local" bind:value={pollExpiresAt} class="poll-expires" />
        {:else if pollExpiryMode === "after"}
          <input
            type="number"
            min="1"
            class="poll-after-amount"
            bind:value={pollAfterAmount}
          />
          <div class="poll-after-unit">
            <Dropdown bind:value={pollAfterUnit} options={pollAfterUnits} />
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <div class="toolbar">
    <div class="tools left">
      <VisibilitySelect bind:value={visibility} />
      <button class="icon" title="画像を添付" onclick={pickFiles} disabled={uploading}><ImagePlus size={16} /></button>
      <button class="mini" class:active={useCw} onclick={() => (useCw = !useCw)}>CW</button>
      <button class="mini" class:active={usePoll} onclick={() => (usePoll = !usePoll)}>投票</button>
      <label class="lo"><input type="checkbox" bind:checked={localOnly} /> 連合なし</label>
      {#if err}<span class="err" title={err}>!</span>{/if}
    </div>
    <div class="tools right">
      <button class="post" disabled={busy} onclick={submit}>{busy ? "…" : "投稿"}</button>
    </div>
  </div>
  </div>
</div>

<style>
  .composewrap {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }
  .composebox {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
    min-width: 0;
  }
  .context {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.78rem;
    color: var(--text-dim);
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 3px 6px;
  }
  .context-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .context-x {
    display: inline-flex;
    flex: none;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
  .text {
    width: 100%;
    resize: vertical;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    font-size: 0.86rem;
    line-height: 1.4;
    min-height: 80px;
    box-sizing: border-box;
    transition: min-height 0.12s ease;
  }
  /* フォーカスが無く未入力の時はコンパクトに(フォーカス/入力があれば通常サイズへ戻す) */
  .text.compact {
    min-height: 34px;
    resize: none;
  }
  /* モバイル投稿モーダルなど: 常に4行分の高さを確保する */
  .text.expanded {
    min-height: 96px;
  }
  .cw-input,
  .poll-choice {
    width: 100%;
    padding: 6px 9px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    font-size: 0.84rem;
    box-sizing: border-box;
  }
  .poll {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .poll-choice-row {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .poll-choice-row .poll-choice {
    flex: 1;
  }
  .poll-choice-x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    padding: 4px;
  }
  .poll-choice-x:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .poll-actions {
    display: flex;
    gap: 12px;
    align-items: center;
    flex-wrap: wrap;
    font-size: 0.8rem;
    color: var(--text-dim);
  }
  .poll-actions .mini:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .poll-expiry {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    font-size: 0.8rem;
    color: var(--text-dim);
  }
  .expiry-label {
    flex: none;
  }
  .poll-expires {
    padding: 3px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    font-size: 0.78rem;
  }
  .poll-after-amount {
    width: 60px;
    padding: 3px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    font-size: 0.78rem;
  }
  .poll-after-unit {
    width: 90px;
  }
  .thumbs {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .thumb-wrap {
    position: relative;
    width: 28px;
    height: 28px;
  }
  .thumb {
    width: 28px;
    height: 28px;
    border-radius: 4px;
    object-fit: cover;
  }
  .thumb.badge {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    background: var(--surface-3);
    color: var(--text-dim);
    font-size: 0.6rem;
    border-radius: 4px;
  }
  .thumb-x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    position: absolute;
    top: -4px;
    right: -4px;
    border: none;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    border-radius: 50%;
    width: 14px;
    height: 14px;
    cursor: pointer;
  }
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .tools.left {
    flex: 1;
    min-width: 0;
  }
  .tools.right {
    flex: none;
  }
  .icon {
    display: inline-flex;
    border: 1px solid var(--border);
    background: var(--surface-1);
    color: var(--text);
    border-radius: 4px;
    padding: 4px 7px;
    cursor: pointer;
    flex: none;
  }
  .icon:disabled {
    opacity: 0.5;
  }
  .mini {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface-1);
    color: var(--text);
    cursor: pointer;
    font-size: 0.78rem;
    flex: none;
  }
  .mini.active {
    border-color: var(--accent);
    color: var(--accent);
  }
  .lo {
    font-size: 0.78rem;
    color: var(--text-dim);
    flex: none;
    white-space: nowrap;
  }
  .post {
    border: none;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    border-radius: 6px;
    padding: 7px 20px;
    cursor: pointer;
    flex: none;
  }
  .post:disabled {
    opacity: 0.5;
  }
  .err {
    color: #ef4444;
    font-weight: 700;
    flex: none;
  }
</style>
