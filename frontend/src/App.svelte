<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/store.svelte";
  import Column from "./ui/Column.svelte";
  import AddAccount from "./ui/AddAccount.svelte";
  import Compose from "./ui/Compose.svelte";

  let showAdd = $state(false);

  onMount(() => {
    app.boot();
  });
</script>

<div class="app">
  <aside class="toolbar">
    <div class="brand">tsumugi</div>
    <div class="accounts">
      {#each app.accounts as acc (acc.id)}
        <div class="acc">
          {#if acc.avatarUrl}
            <img class="acc-avatar" src={acc.avatarUrl} alt="" />
          {:else}
            <div class="acc-avatar placeholder"></div>
          {/if}
          <span class="acc-name" title={`@${acc.username}@${acc.host}`}>@{acc.username}</span>
          <button class="add-col" title="ホームカラムを追加" onclick={() => app.addHomeColumn(acc.id)}>+Home</button>
          <button class="add-col" title="投稿" onclick={() => app.openCompose(acc.id)}>✎</button>
        </div>
      {/each}
    </div>
    <button class="add-account-btn" onclick={() => (showAdd = !showAdd)}>
      {showAdd ? "閉じる" : "＋ アカウント"}
    </button>
    {#if app.error}<div class="global-err" title={app.error}>{app.error}</div>{/if}
  </aside>

  <main class="main">
    {#if app.booting}
      <div class="center-msg">起動中…</div>
    {:else if showAdd || app.accounts.length === 0}
      <AddAccount />
    {:else if app.columns.length === 0}
      <div class="center-msg">
        左のアカウントの「+Home」でホームタイムラインを開いてください。
      </div>
    {:else}
      <div class="columns">
        {#each app.columns as column (column.id)}
          <Column {column} />
        {/each}
      </div>
    {/if}
  </main>

  {#if app.compose}
    <Compose />
  {/if}
</div>

<style>
  .app {
    display: flex;
    height: 100vh;
    overflow: hidden;
  }
  .toolbar {
    width: 200px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px 12px;
    background: var(--surface-2);
    border-right: 1px solid var(--border);
  }
  .brand {
    font-weight: 700;
    font-size: 1.1rem;
    letter-spacing: 0.02em;
  }
  .accounts {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex: 1;
    overflow-y: auto;
  }
  .acc {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.82rem;
  }
  .acc-avatar {
    width: 26px;
    height: 26px;
    border-radius: 7px;
    object-fit: cover;
    flex: none;
  }
  .acc-avatar.placeholder {
    background: var(--surface-3);
  }
  .acc-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
  .add-col {
    font-size: 0.72rem;
    padding: 2px 6px;
    border: 1px solid var(--border);
    background: var(--surface-1);
    color: var(--text);
    border-radius: 6px;
    cursor: pointer;
  }
  .add-account-btn {
    padding: 8px;
    border: 1px solid var(--border);
    background: var(--surface-1);
    color: var(--text);
    border-radius: 8px;
    cursor: pointer;
  }
  .global-err {
    font-size: 0.72rem;
    color: #ef4444;
    max-height: 60px;
    overflow: hidden;
  }
  .main {
    flex: 1;
    min-width: 0;
  }
  .columns {
    display: flex;
    height: 100%;
    overflow-x: auto;
  }
  .center-msg {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--text-dim);
    padding: 24px;
    text-align: center;
  }
</style>
