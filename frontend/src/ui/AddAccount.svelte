<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../lib/store.svelte";
  import { X } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";

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

  // reauthAccount は再認証フロー開始時に設定され、その後の変更は想定していない。初期値参照でよい。
  // svelte-ignore state_referenced_locally
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

<div class="mx-auto my-12 max-w-[420px] rounded-[14px] border border-border bg-background p-6">
  <div class="mb-2 flex items-center justify-between">
    <h2 class="m-0 text-[1.1rem]">
      {reauthAccount
        ? `再認証: @${reauthAccount.username}@${reauthAccount.host}`
        : "アカウントを追加"}
    </h2>
    {#if onclose}
      <Button type="button" variant="ghost" size="icon-xs" onclick={onclose} title="戻る"><X size={16} /></Button>
    {/if}
  </div>
  {#if reauthAccount && !sessionId}
    <p class="mb-3.5 text-[0.86rem] text-muted-foreground">
      {busy ? "認可ページを開いています…" : "認可ページを開けませんでした。"}
    </p>
    {#if !busy}
      <Button type="button" variant="ghost" size="sm" class="text-muted-foreground" onclick={start}
        >もう一度試す</Button
      >
    {/if}
  {:else if !sessionId}
    <p class="mb-3.5 text-[0.86rem] text-muted-foreground">
      Misskeyインスタンスのホスト名を入力してください（例: misskey.example）
    </p>
    <div class="flex gap-2">
      <input
        class="flex-1 rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
        placeholder="misskey.example"
        bind:value={host}
        onkeydown={(e) => e.key === "Enter" && host.trim() && start()}
      />
      <Button type="button" variant="default" size="sm" disabled={busy || !host.trim()} onclick={start}>
        {busy ? "…" : "認可ページを開く"}
      </Button>
    </div>
  {:else}
    <p class="mb-3.5 text-[0.86rem] text-muted-foreground">
      {reauthAccount
        ? "スコープが更新されたトークンを取得します。ブラウザで認可を完了したら、下のボタンを押してください。"
        : "ブラウザで認可を完了したら、下のボタンを押してください。"}
    </p>
    <Button type="button" variant="default" size="sm" class="mt-1.5" disabled={busy} onclick={complete}>
      {busy ? "確認中…" : "認可を完了した"}
    </Button>
    <Button
      type="button"
      variant="ghost"
      size="sm"
      class="ml-2 text-muted-foreground"
      onclick={() => (reauthAccount ? start() : (sessionId = null))}
    >
      やり直す
    </Button>
  {/if}
  {#if err}<p class="mt-3 text-[0.85rem] break-words text-destructive">{err}</p>{/if}
</div>
