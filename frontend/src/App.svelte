<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/store.svelte";
  import type { TabView } from "./lib/store.svelte";
  import Column from "./ui/Column.svelte";
  import AddAccount from "./ui/AddAccount.svelte";
  import AddColumnModal from "./ui/AddColumnModal.svelte";
  import ColumnSettings from "./ui/ColumnSettings.svelte";
  import ComposeBar from "./ui/ComposeBar.svelte";
  import Settings from "./ui/Settings.svelte";
  import Backstage from "./ui/Backstage.svelte";
  import { buildKeymap, eventToChord } from "./lib/keymap";
  import { Settings as SettingsIcon, Pencil } from "@lucide/svelte";

  // ユーザのキー上書きを反映した実効キーマップ（設定変更で即反映）
  const keymap = $derived(buildKeymap(app.ui.keymap ?? {}));
  // モバイル版UIかPC版UIか(設定→表示で上書き可能、既定はOS判定。Issue #51)
  const useMobileUi = $derived(app.useMobileUi());

  let showAdd = $state(false);
  let showAddColumn = $state(false);
  let editTab = $state<TabView | null>(null);
  type SettingsSection = "accounts" | "display" | "notify" | "mute" | "keys";
  let showSettings = $state(false);
  let settingsInitial = $state<SettingsSection>("notify");
  let addTabGroupId = $state<string | null>(null);
  let columnSettingsGroupId = $state<string | null>(null);

  function openSettings(section: SettingsSection) {
    settingsInitial = section;
    showSettings = true;
  }
  function addAccountFromSettings() {
    showSettings = false;
    showAdd = true;
  }

  function openAddColumn() {
    addTabGroupId = null; // 新しい視覚カラム
    editTab = null;
    showAddColumn = true;
  }
  function openAddTab(groupId: string) {
    addTabGroupId = groupId; // 既存カラムにタブ追加
    editTab = null;
    showAddColumn = true;
  }
  function openEditTab(tab: TabView) {
    editTab = tab; // 既存タブを編集
    addTabGroupId = null;
    showAddColumn = true;
  }
  function openColumnSettings(groupId: string) {
    columnSettingsGroupId = groupId; // カラム(視覚カラム)自体の設定
  }

  function onGlobalKey(e: KeyboardEvent) {
    const el = document.activeElement as HTMLElement | null;
    // 入力中はキーバインドを無効化（タイプを妨げない）
    if (
      el &&
      (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable)
    ) {
      return;
    }
    // Esc: 開いているリアクションピッカー/投稿モーダルを閉じる
    if (e.key === "Escape") {
      if (app.reactPicker) {
        app.reactPicker = null;
        e.preventDefault();
      } else if (app.showComposeModal) {
        app.showComposeModal = false;
        e.preventDefault();
      }
      return;
    }
    // モーダル表示中はキーバインド無効（各モーダルの Esc 等に委ねる）
    if (showAdd || showAddColumn || showSettings || app.showComposeModal) return;
    const action = keymap.get(eventToChord(e));
    if (!action) return;
    e.preventDefault();
    app.runKeyAction(action);
  }

  onMount(() => {
    app.boot();
    window.addEventListener("keydown", onGlobalKey);
    return () => window.removeEventListener("keydown", onGlobalKey);
  });
</script>

<div class="app">
  <!-- 上部: 投稿バー(スマホでは非表示) + カラム/アカウント追加 -->
  <header class="appbar">
    {#if app.accounts.length > 0 && !useMobileUi}
      <ComposeBar />
    {:else}
      <div class="spacer"></div>
    {/if}
    {#if app.accounts.length > 0}
      <button class="bar-btn" onclick={openAddColumn}>＋カラム</button>
      <button class="bar-btn" onclick={() => openSettings("accounts")} title="設定">
        <SettingsIcon size={14} /> 設定
      </button>
    {/if}
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
      <AddAccount onclose={app.accounts.length > 0 ? () => (showAdd = false) : undefined} />
    {:else if app.groups.length === 0}
      <div class="center-msg">
        「＋カラム」からソースとフィルタを選んでカラムを追加してください。
      </div>
    {:else}
      <div class="columns">
        {#each app.groups as group (group.id)}
          <Column {group} onAddTab={openAddTab} onEditTab={openEditTab} onEditGroup={openColumnSettings} />
        {/each}
      </div>
    {/if}
  </main>

  {#if app.accounts.length > 0 && !app.booting}
    <Backstage />
  {/if}

  {#if useMobileUi && app.accounts.length > 0 && !app.booting}
    <button class="compose-fab" onclick={() => app.openCompose(app.defaultAccountId())} title="投稿">
      <Pencil size={20} />
    </button>
  {/if}

  {#if app.showComposeModal}
    <div
      class="compose-overlay"
      onclick={() => (app.showComposeModal = false)}
      onkeydown={(e) => e.key === "Escape" && (app.showComposeModal = false)}
      role="presentation"
    >
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="compose-modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="-1">
        <ComposeBar onPosted={() => (app.showComposeModal = false)} expanded />
      </div>
    </div>
  {/if}

  {#if showAddColumn}
    <AddColumnModal
      groupId={addTabGroupId}
      editTab={editTab ?? undefined}
      onclose={() => (showAddColumn = false)}
    />
  {/if}
  {#if columnSettingsGroupId}
    <ColumnSettings groupId={columnSettingsGroupId} onclose={() => (columnSettingsGroupId = null)} />
  {/if}
  {#if showSettings}
    <Settings
      initial={settingsInitial}
      onAddAccount={addAccountFromSettings}
      onclose={() => (showSettings = false)}
    />
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
    align-items: flex-start;
    gap: 10px;
    padding: max(6px, env(safe-area-inset-top)) max(10px, env(safe-area-inset-right)) 6px
      max(10px, env(safe-area-inset-left));
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .spacer {
    flex: 1;
  }
  .bar-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
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
  .compose-fab {
    display: grid;
    place-items: center;
    position: fixed;
    right: calc(20px + env(safe-area-inset-right));
    bottom: calc(20px + env(safe-area-inset-bottom));
    width: 56px;
    height: 56px;
    border: none;
    border-radius: 50%;
    background: var(--accent);
    color: #fff;
    box-shadow: 0 3px 10px rgba(0, 0, 0, 0.3);
    cursor: pointer;
    z-index: 40;
  }
  .compose-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: grid;
    align-items: start;
    justify-items: stretch;
    padding-top: max(6vh, env(safe-area-inset-top));
    z-index: 50;
  }
  .compose-modal {
    width: 100%;
    max-height: 80vh;
    overflow-y: auto;
    box-sizing: border-box;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 12px;
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
