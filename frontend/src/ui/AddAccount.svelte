<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../lib/store.svelte";
  import { X } from "@lucide/svelte";

  // onclose があれば「戻る」導線を出す（ログイン済みで設定経由で開いた場合）。
  // 初回（アカウント0件）は onclose 未指定で戻る先が無いため非表示。
  // reauthAccount があれば host入力を省略し、そのアカウントの再認証フローになる。
  let {
    onclose,
    reauthAccount,
  }: {
    onclose?: () => void;
    reauthAccount?: { id: string; host: string; username: string };
  } = $props();

  let host = $state(reauthAccount?.host ?? "");
  let sessionId = $state<string | null>(null);
  let busy = $state(false);
  let err = $state<string | null>(null);

  onMount(() => {
    if (reauthAccount) void start();
  });

  async function start() {
    err = null;
    busy = true;
    try {
      sessionId = await app.addAccount(host.trim());
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  async function complete() {
    if (!sessionId) return;
    err = null;
    busy = true;
    try {
      await app.completeAccount(sessionId);
      sessionId = null;
      host = "";
      onclose?.(); // 完了できたらカラム表示/設定へ戻る
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="add-account">
  <div class="head">
    <h2>
      {reauthAccount
        ? `再認証: @${reauthAccount.username}@${reauthAccount.host}`
        : "アカウントを追加"}
    </h2>
    {#if onclose}
      <button class="close" onclick={onclose} title="戻る"><X size={16} /></button>
    {/if}
  </div>
  {#if reauthAccount && !sessionId}
    <p class="hint">{busy ? "認可ページを開いています…" : "認可ページを開けませんでした。"}</p>
    {#if !busy}
      <button class="link" onclick={start}>もう一度試す</button>
    {/if}
  {:else if !sessionId}
    <p class="hint">Misskeyインスタンスのホスト名を入力してください（例: misskey.example）</p>
    <div class="form">
      <input
        placeholder="misskey.example"
        bind:value={host}
        onkeydown={(e) => e.key === "Enter" && host.trim() && start()}
      />
      <button disabled={busy || !host.trim()} onclick={start}>
        {busy ? "…" : "認可ページを開く"}
      </button>
    </div>
  {:else}
    <p class="hint">
      {reauthAccount
        ? "スコープが更新されたトークンを取得します。ブラウザで認可を完了したら、下のボタンを押してください。"
        : "ブラウザで認可を完了したら、下のボタンを押してください。"}
    </p>
    <button class="primary" disabled={busy} onclick={complete}>
      {busy ? "確認中…" : "認可を完了した"}
    </button>
    <button class="link" onclick={() => (reauthAccount ? start() : (sessionId = null))}>
      やり直す
    </button>
  {/if}
  {#if err}<p class="err">{err}</p>{/if}
</div>

<style>
  .add-account {
    max-width: 420px;
    margin: 48px auto;
    padding: 24px;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 14px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  h2 {
    margin: 0;
    font-size: 1.1rem;
  }
  .close {
    display: inline-flex;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    padding: 2px 6px;
    width: auto;
  }
  .hint {
    color: var(--text-dim);
    font-size: 0.86rem;
    margin: 0 0 14px;
  }
  .form {
    display: flex;
    gap: 8px;
  }
  input {
    flex: 1;
    padding: 9px 11px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
  }
  button {
    padding: 9px 14px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--accent);
    color: white;
    cursor: pointer;
    font-weight: 600;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  button.link {
    background: transparent;
    color: var(--text-dim);
    border: none;
    margin-left: 8px;
    font-weight: 400;
  }
  .primary {
    margin-top: 6px;
  }
  .err {
    margin-top: 12px;
    color: #ef4444;
    font-size: 0.85rem;
    word-break: break-word;
  }
</style>
