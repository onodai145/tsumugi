<script lang="ts">
  import { app } from "../lib/store.svelte";
  import type { NoteDraft_Deserialize as NoteDraft, VisibilityInput } from "../bindings/tauri.gen";

  // compose は非 null 前提（呼び出し側で存在確認）
  const c = $derived(app.compose!);

  let text = $state("");
  let cw = $state("");
  let useCw = $state(false);
  let visibility = $state<VisibilityInput>("public");
  let localOnly = $state(false);
  let usePoll = $state(false);
  let pollChoices = $state<string[]>(["", ""]);
  let pollMultiple = $state(false);
  let busy = $state(false);
  let err = $state<string | null>(null);

  const visibilities: { v: VisibilityInput; label: string }[] = [
    { v: "public", label: "公開" },
    { v: "home", label: "ホーム" },
    { v: "followers", label: "フォロワー" },
    { v: "specified", label: "ダイレクト" },
  ];

  async function submit() {
    err = null;
    const choices = pollChoices.map((s) => s.trim()).filter(Boolean);
    if (!text.trim() && !c.quoteOf && choices.length === 0) {
      err = "本文を入力してください";
      return;
    }
    const draft: NoteDraft = {
      text: text.trim() || null,
      cw: useCw && cw.trim() ? cw.trim() : null,
      visibility,
      fileIds: [],
      poll: usePoll && choices.length >= 2 ? { choices, multiple: pollMultiple, expiresAt: null } : null,
      replyId: c.replyTo?.id ?? null,
      renoteId: c.quoteOf?.id ?? null,
      localOnly,
    };
    busy = true;
    try {
      await app.postNote(c.accountId, draft);
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div
  class="overlay"
  onclick={() => app.closeCompose()}
  onkeydown={(e) => e.key === "Escape" && app.closeCompose()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="modal"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <header class="head">
      <span>{c.replyTo ? "返信" : c.quoteOf ? "引用Renote" : "新規投稿"}</span>
      <button class="x" onclick={() => app.closeCompose()}>✕</button>
    </header>

    {#if c.replyTo}
      <div class="context">To: @{c.replyTo.user.username} — {c.replyTo.text ?? ""}</div>
    {/if}
    {#if c.quoteOf}
      <div class="context">RN: @{c.quoteOf.user.username} — {c.quoteOf.text ?? ""}</div>
    {/if}

    {#if useCw}
      <input class="cw-input" placeholder="内容警告 (CW)" bind:value={cw} />
    {/if}
    <textarea class="text" rows="5" placeholder="いまどうしてる？" bind:value={text}></textarea>

    {#if usePoll}
      <div class="poll">
        {#each pollChoices as _, i}
          <input class="poll-choice" placeholder={`選択肢 ${i + 1}`} bind:value={pollChoices[i]} />
        {/each}
        <div class="poll-actions">
          <button class="mini" onclick={() => (pollChoices = [...pollChoices, ""])}>＋選択肢</button>
          <label><input type="checkbox" bind:checked={pollMultiple} /> 複数選択</label>
        </div>
      </div>
    {/if}

    <div class="toolbar">
      <select bind:value={visibility}>
        {#each visibilities as o}<option value={o.v}>{o.label}</option>{/each}
      </select>
      <button class="mini" class:active={useCw} onclick={() => (useCw = !useCw)}>CW</button>
      <button class="mini" class:active={usePoll} onclick={() => (usePoll = !usePoll)}>投票</button>
      <label class="lo"><input type="checkbox" bind:checked={localOnly} /> 連合なし</label>
      <button class="submit" disabled={busy} onclick={submit}>
        {busy ? "送信中…" : "投稿"}
      </button>
    </div>
    {#if err}<p class="err">{err}</p>{/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: grid;
    place-items: start center;
    padding-top: 8vh;
    z-index: 50;
  }
  .modal {
    width: min(560px, 92vw);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 14px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 600;
    margin-bottom: 8px;
  }
  .x {
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
  .context {
    font-size: 0.8rem;
    color: var(--text-dim);
    background: var(--surface-2);
    border-radius: 8px;
    padding: 6px 8px;
    margin-bottom: 8px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cw-input,
  .text,
  .poll-choice {
    width: 100%;
    padding: 9px 11px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    margin-bottom: 8px;
    font-family: inherit;
    resize: vertical;
  }
  .poll {
    display: flex;
    flex-direction: column;
  }
  .poll-actions {
    display: flex;
    gap: 12px;
    align-items: center;
    margin-bottom: 8px;
    font-size: 0.82rem;
    color: var(--text-dim);
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  select {
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
  }
  .mini {
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-1);
    color: var(--text);
    cursor: pointer;
    font-size: 0.82rem;
  }
  .mini.active {
    border-color: var(--accent);
    color: var(--accent);
  }
  .lo {
    font-size: 0.8rem;
    color: var(--text-dim);
  }
  .submit {
    margin-left: auto;
    padding: 8px 18px;
    border: none;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    cursor: pointer;
  }
  .submit:disabled {
    opacity: 0.5;
  }
  .err {
    color: #ef4444;
    font-size: 0.85rem;
    margin: 8px 0 0;
  }
</style>
