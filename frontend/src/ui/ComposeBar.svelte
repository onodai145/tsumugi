<script lang="ts">
  import { app } from "../lib/store.svelte";
  import AccountSelect from "./AccountSelect.svelte";
  import { commands, unwrap } from "../lib/ipc";
  import { open } from "@tauri-apps/plugin-dialog";
  import type {
    NoteDraft_Deserialize as NoteDraft,
    VisibilityInput,
    DriveFile,
  } from "../bindings/tauri.gen";

  let accountId = $state(app.accounts[0]?.id ?? "");
  let text = $state("");
  let visibility = $state<VisibilityInput>("public");
  let attached = $state<DriveFile[]>([]);
  let uploading = $state(false);
  let busy = $state(false);
  let err = $state<string | null>(null);

  // アカウントが後から読まれた場合の既定選択
  $effect(() => {
    if (!accountId && app.accounts.length > 0) accountId = app.accounts[0].id;
  });

  const visibilities: { v: VisibilityInput; label: string }[] = [
    { v: "public", label: "公開" },
    { v: "home", label: "ホーム" },
    { v: "followers", label: "フォロワー" },
    { v: "specified", label: "ダイレクト" },
  ];

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

  async function submit() {
    err = null;
    if (!accountId) {
      err = "アカウントを選択してください";
      return;
    }
    if (!text.trim() && attached.length === 0) return;
    const draft: NoteDraft = {
      text: text.trim() || null,
      cw: null,
      visibility,
      fileIds: attached.map((f) => f.id),
      poll: null,
      replyId: null,
      renoteId: null,
      localOnly: false,
    };
    busy = true;
    try {
      await app.postNote(accountId, draft);
      text = "";
      attached = [];
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
    }
  }
</script>

<div class="composebar">
  <AccountSelect bind:value={accountId} accounts={app.accounts} />

  <textarea
    class="text"
    rows="1"
    placeholder="いまどうしてる？（Ctrl+Enter で投稿）"
    bind:value={text}
    onkeydown={onKey}
  ></textarea>

  {#if attached.length > 0 || uploading}
    <div class="thumbs">
      {#each attached as f (f.id)}
        {#if f.mimeType.startsWith("image/")}
          <img class="thumb" src={f.thumbnailUrl ?? f.url} alt="" />
        {:else}
          <span class="thumb badge">{f.mimeType.split("/")[0]}</span>
        {/if}
      {/each}
      {#if uploading}<span class="thumb badge">…</span>{/if}
    </div>
  {/if}

  <select class="vis" bind:value={visibility} title="公開範囲">
    {#each visibilities as o}<option value={o.v}>{o.label}</option>{/each}
  </select>
  <button class="icon" title="画像を添付" onclick={pickFiles} disabled={uploading}>📎</button>
  <button class="post" disabled={busy} onclick={submit}>{busy ? "…" : "投稿"}</button>
  {#if err}<span class="err" title={err}>!</span>{/if}
</div>

<style>
  .composebar {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }
  .vis {
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--text);
    font-size: 0.8rem;
    flex: none;
    max-width: 120px;
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
  .thumbs {
    display: flex;
    gap: 3px;
    flex: none;
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
    background: var(--surface-3);
    color: var(--text-dim);
    font-size: 0.6rem;
  }
  .icon {
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
