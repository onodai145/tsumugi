<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import type { Account } from "../../bindings/tauri.gen";
  import { Button } from "$lib/components/ui/button";

  let {
    onAddAccount,
    onReauth,
  }: { onAddAccount: () => void; onReauth: (account: Account) => void } = $props();

  let busyId = $state<string | null>(null);
  let confirmId = $state<string | null>(null);
  let err = $state<string | null>(null);

  async function remove(id: string) {
    err = null;
    busyId = id;
    try {
      await app.removeAccount(id);
      confirmId = null;
    } catch (e) {
      err = String(e);
    } finally {
      busyId = null;
    }
  }

  async function makeDefault(id: string) {
    err = null;
    try {
      await app.setUiPrefs({ ...app.ui, defaultAccountId: id });
    } catch (e) {
      err = String(e);
    }
  }
</script>

<h3 class="mb-3.5 mt-0 text-base font-semibold">アカウント</h3>

{#if app.accounts.length === 0}
  <p class="mb-3.5 mt-0 text-[0.76rem] text-muted-foreground">ログイン中のアカウントはありません。</p>
{:else}
  <ul class="m-0 mb-3 flex list-none flex-col gap-1.5 p-0">
    {#each app.accounts as a (a.id)}
      <li class="flex items-center gap-2.5 rounded-lg border border-border bg-muted p-2">
        {#if a.avatarUrl}
          <img class="h-[34px] w-[34px] flex-none rounded-lg object-cover" src={a.avatarUrl} alt="" />
        {:else}
          <div class="grid h-[34px] w-[34px] flex-none place-items-center rounded-lg bg-accent font-bold text-muted-foreground">{(a.displayName || a.username).charAt(0)}</div>
        {/if}
        <div class="min-w-0 flex-1">
          <div class="overflow-hidden text-ellipsis whitespace-nowrap text-[0.86rem] font-semibold">{a.displayName || a.username}{#if a.id === app.defaultAccountId()}<span class="default-badge ml-1.5 rounded px-1.5 py-px text-[0.68rem] font-semibold text-primary">既定</span>{/if}</div>
          <div class="overflow-hidden text-ellipsis whitespace-nowrap text-[0.76rem] text-muted-foreground">@{a.username}@{a.host}</div>
        </div>
        {#if confirmId === a.id}
          <div class="flex flex-none items-center gap-1.5 text-[0.78rem] text-muted-foreground">
            <span>削除？</span>
            <Button type="button" variant="destructive" size="xs" disabled={busyId === a.id} onclick={() => remove(a.id)}>
              {busyId === a.id ? "…" : "はい"}
            </Button>
            <Button type="button" variant="outline" size="xs" onclick={() => (confirmId = null)}>いいえ</Button>
          </div>
        {:else}
          {#if a.id !== app.defaultAccountId()}
            <Button type="button" variant="outline" size="xs" onclick={() => makeDefault(a.id)}>既定に設定</Button>
          {/if}
          <Button type="button" variant="outline" size="xs" onclick={() => onReauth(a)}>再認証</Button>
          <Button type="button" variant="outline" size="xs" onclick={() => (confirmId = a.id)}>削除</Button>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<p class="mb-3.5 mt-0 text-[0.76rem] text-muted-foreground">
  アカウントを削除すると、そのアカウントのカラム(タブ)も表示されなくなり、保存済みトークンは keyring から破棄されます。
</p>

<div class="flex justify-start">
  <Button type="button" variant="outline" class="border-primary text-primary hover:text-primary" onclick={onAddAccount}>＋ アカウントを追加</Button>
</div>
{#if err}<p class="mt-2 mb-0 text-[0.82rem] text-destructive">{err}</p>{/if}

<style>
  .default-badge {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
</style>
