<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/store.svelte";
  import Column from "./ui/Column.svelte";
  import AddAccount from "./ui/AddAccount.svelte";
  import AddColumnModal from "./ui/AddColumnModal.svelte";
  import ComposeBar from "./ui/ComposeBar.svelte";
  import Compose from "./ui/Compose.svelte";
  import Settings from "./ui/Settings.svelte";

  let showAdd = $state(false);
  let showAddColumn = $state(false);
  let showSettings = $state(false);
  let settingsInitial = $state<"notify" | "mute">("notify");
  let addTabGroupId = $state<string | null>(null);

  function openSettings(section: "notify" | "mute") {
    settingsInitial = section;
    showSettings = true;
  }

  function openAddColumn() {
    addTabGroupId = null; // 新しい視覚カラム
    showAddColumn = true;
  }
  function openAddTab(groupId: string) {
    addTabGroupId = groupId; // 既存カラムにタブ追加
    showAddColumn = true;
  }

  onMount(() => {
    app.boot();
  });
</script>

<div class="app">
  <!-- 上部: ブランド + 投稿バー + カラム/アカウント追加 -->
  <header class="appbar">
    <div class="brand">tsumugi</div>
    {#if app.accounts.length > 0}
      <ComposeBar />
    {:else}
      <div class="spacer"></div>
    {/if}
    {#if app.accounts.length > 0}
      <button class="bar-btn" onclick={openAddColumn}>＋カラム</button>
      <button class="bar-btn" onclick={() => openSettings("notify")} title="設定">⚙ 設定</button>
    {/if}
    <button class="bar-btn" onclick={() => (showAdd = !showAdd)}>
      {showAdd ? "閉じる" : "＋アカウント"}
    </button>
  </header>

  {#if app.error}
    <div class="global-err" title={app.error} onclick={() => (app.error = null)} role="presentation">
      {app.error}
    </div>
  {/if}

  <main class="main">
    {#if app.booting}
      <div class="center-msg">起動中…</div>
    {:else if showAdd || app.accounts.length === 0}
      <AddAccount />
    {:else if app.groups.length === 0}
      <div class="center-msg">
        「＋カラム」からソースとフィルタを選んでカラムを追加してください。
      </div>
    {:else}
      <div class="columns">
        {#each app.groups as group (group.id)}
          <Column {group} onAddTab={openAddTab} />
        {/each}
      </div>
    {/if}
  </main>

  {#if app.compose}
    <Compose />
  {/if}
  {#if showAddColumn}
    <AddColumnModal groupId={addTabGroupId} onclose={() => (showAddColumn = false)} />
  {/if}
  {#if showSettings}
    <Settings initial={settingsInitial} onclose={() => (showSettings = false)} />
  {/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }
  .appbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .brand {
    font-weight: 700;
    font-size: 1rem;
    letter-spacing: 0.02em;
    flex: none;
  }
  .spacer {
    flex: 1;
  }
  .bar-btn {
    flex: none;
    padding: 5px 10px;
    border: 1px solid var(--border);
    background: var(--surface-1);
    color: var(--text);
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.8rem;
  }
  .bar-btn:hover {
    border-color: var(--accent);
  }
  .global-err {
    padding: 4px 10px;
    background: color-mix(in srgb, #ef4444 15%, var(--surface-1));
    color: #ef4444;
    font-size: 0.78rem;
    cursor: pointer;
    flex: none;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .main {
    flex: 1;
    min-width: 0;
    min-height: 0;
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
