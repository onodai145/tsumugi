<script lang="ts">
  import { app } from "../lib/store.svelte";
  import AccountSelect from "./AccountSelect.svelte";
  import VisibilitySelect from "./VisibilitySelect.svelte";
  import { commands, unwrap } from "../lib/ipc";
  import { open } from "@tauri-apps/plugin-dialog";
  import { ImagePlus, X } from "@lucide/svelte";
  import type {
    NoteDraft_Deserialize as NoteDraft,
    VisibilityInput,
    DriveFile,
    Note,
  } from "../bindings/tauri.gen";

  let accountId = $state(app.defaultAccountId());
  // ユーザが手動でアカウントを切り替えたら、以後は設定→アカウントの既定変更に追従しない
  let accountTouched = $state(false);
  let text = $state("");
  let cw = $state("");
  let useCw = $state(false);
  let visibility = $state<VisibilityInput>("public");
  let localOnly = $state(false);
  let usePoll = $state(false);
  let pollChoices = $state<string[]>(["", ""]);
  let pollMultiple = $state(false);
  let attached = $state<DriveFile[]>([]);
  let uploading = $state(false);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let replyTo = $state<Note | undefined>(undefined);
  let quoteOf = $state<Note | undefined>(undefined);
  let textarea = $state<HTMLTextAreaElement | undefined>(undefined);

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
    app.compose = null;
    textarea?.focus();
  });

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
    const draft: NoteDraft = {
      text: text.trim() || null,
      cw: useCw && cw.trim() ? cw.trim() : null,
      visibility,
      fileIds: attached.map((f) => f.id),
      poll: usePoll && choices.length >= 2 ? { choices, multiple: pollMultiple, expiresAt: null } : null,
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
      localOnly = false;
      attached = [];
      replyTo = undefined;
      quoteOf = undefined;
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

<div class="composebar">
  {#if replyTo || quoteOf}
    <div class="context">
      <span class="context-text">
        {replyTo ? "返信: " : "引用: "}@{(replyTo ?? quoteOf)!.user.username} — {(replyTo ?? quoteOf)!.text ?? ""}
      </span>
      <button class="context-x" title="キャンセル" onclick={cancelContext}><X size={12} /></button>
    </div>
  {/if}

  <div class="row">
    <AccountSelect
      bind:value={
        () => accountId,
        (v) => {
          accountId = v;
          accountTouched = true;
        }
      }
      accounts={app.accounts}
    />

    <textarea
      class="text"
      rows="1"
      placeholder="いまどうしてる？（Ctrl+Enter で投稿）"
      bind:value={text}
      bind:this={textarea}
      onkeydown={onKey}
    ></textarea>

    <VisibilitySelect bind:value={visibility} />
    <button class="icon" title="画像を添付" onclick={pickFiles} disabled={uploading}><ImagePlus size={16} /></button>
    <button class="mini" class:active={useCw} onclick={() => (useCw = !useCw)}>CW</button>
    <button class="mini" class:active={usePoll} onclick={() => (usePoll = !usePoll)}>投票</button>
    <label class="lo"><input type="checkbox" bind:checked={localOnly} /> 連合なし</label>
    <button class="post" disabled={busy} onclick={submit}>{busy ? "…" : "投稿"}</button>
    {#if err}<span class="err" title={err}>!</span>{/if}
  </div>

  {#if useCw}
    <input class="cw-input" placeholder="内容警告 (CW)" bind:value={cw} />
  {/if}

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
        <input class="poll-choice" placeholder={`選択肢 ${i + 1}`} bind:value={pollChoices[i]} />
      {/each}
      <div class="poll-actions">
        <button class="mini" onclick={() => (pollChoices = [...pollChoices, ""])}>＋選択肢</button>
        <label><input type="checkbox" bind:checked={pollMultiple} /> 複数選択</label>
      </div>
    </div>
  {/if}
</div>

<style>
  .composebar {
    display: flex;
    flex-direction: column;
    gap: 5px;
    flex: 1;
    min-width: 0;
    padding: 2px 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .context {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.78rem;
    color: var(--text-dim);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px 8px;
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
    flex: 1;
    min-width: 60px;
    resize: none;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    font-size: 0.86rem;
    line-height: 1.3;
    max-height: 84px;
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
  }
  .poll {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .poll-actions {
    display: flex;
    gap: 12px;
    align-items: center;
    font-size: 0.8rem;
    color: var(--text-dim);
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
    border-radius: 4px;
    padding: 5px 14px;
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
