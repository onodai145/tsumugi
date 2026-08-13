<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/store.svelte";
  import type { TabView } from "./lib/store.svelte";
  import type { Account } from "./bindings/tauri.gen";
  import Pane from "./ui/Pane.svelte";
  import AddAccount from "./ui/AddAccount.svelte";
  import AddColumnModal from "./ui/AddColumnModal.svelte";
  import ColumnSettings from "./ui/ColumnSettings.svelte";
  import ComposeBar from "./ui/ComposeBar.svelte";
  import Settings from "./ui/Settings.svelte";
  import Backstage from "./ui/Backstage.svelte";
  import ProfileModal from "./ui/ProfileModal.svelte";
  import Modal from "./ui/Modal.svelte";
  import { currentProfileTarget, currentProfileAccountId, closeProfile } from "./lib/profileModal.svelte";
  import { buildKeymap, eventToChord } from "./lib/keymap";
  import { Settings as SettingsIcon, Pencil } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";

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
  let reauthAccount = $state<Account | null>(null);

  function openSettings(section: SettingsSection) {
    settingsInitial = section;
    showSettings = true;
  }
  function addAccountFromSettings() {
    showSettings = false;
    showAdd = true;
  }
  function startReauth(account: Account) {
    showSettings = false;
    reauthAccount = account;
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

  let pendingSplitGroupId = $state<string | null>(null);

  async function splitDown(groupId: string) {
    const newGroupId = await app.splitPane(groupId, "column");
    if (!newGroupId) return;
    pendingSplitGroupId = newGroupId;
    openAddTab(newGroupId);
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
    if (showAdd || showAddColumn || showSettings || app.showComposeModal || app.errorModal) return;
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

<div class="flex h-screen flex-col overflow-hidden">
  <!-- 上部: 投稿バー(スマホでは非表示) + カラム/アカウント追加 -->
  <header
    class="flex flex-none items-start gap-2.5 border-b border-border bg-muted p-[max(6px,env(safe-area-inset-top))_max(10px,env(safe-area-inset-right))_6px_max(10px,env(safe-area-inset-left))]"
  >
    {#if app.accounts.length > 0 && !useMobileUi}
      <ComposeBar />
    {:else}
      <div class="flex-1"></div>
    {/if}
    {#if app.accounts.length > 0}
      <Button type="button" variant="outline" size="sm" onclick={openAddColumn}>＋カラム</Button>
      <Button type="button" variant="outline" size="sm" onclick={() => openSettings("accounts")} title="設定">
        <SettingsIcon size={14} /> 設定
      </Button>
    {/if}
  </header>

  <main class="min-h-0 min-w-0 flex-1">
    {#if app.booting}
      <div class="grid h-full place-items-center p-6 text-center text-muted-foreground">起動中…</div>
    {:else if showAdd || reauthAccount || app.accounts.length === 0}
      <AddAccount
        reauthAccount={reauthAccount ?? undefined}
        onclose={
          app.accounts.length > 0
            ? () => {
                showAdd = false;
                reauthAccount = null;
              }
            : undefined
        }
      />
    {:else if app.groups.length === 0}
      <div class="grid h-full place-items-center p-6 text-center text-muted-foreground">
        「＋カラム」からソースとフィルタを選んでカラムを追加してください。
      </div>
    {:else}
      <div class="flex h-full overflow-x-auto">
        <Pane node={app.paneRoot} onAddTab={openAddTab} onEditTab={openEditTab} onEditGroup={openColumnSettings} onSplitDown={splitDown} />
      </div>
    {/if}
  </main>

  {#if app.accounts.length > 0 && !app.booting}
    <Backstage
      onReauth={(accountId) => {
        const acc = app.accounts.find((a) => a.id === accountId);
        if (acc) startReauth(acc);
      }}
    />
  {/if}

  {#if useMobileUi && app.accounts.length > 0 && !app.booting}
    <button
      class="fixed right-[calc(20px+env(safe-area-inset-right))] bottom-[calc(20px+env(safe-area-inset-bottom))] z-40 grid size-14 place-items-center rounded-full bg-primary text-white shadow-[0_3px_10px_rgba(0,0,0,0.3)]"
      onclick={() => app.openCompose(app.defaultAccountId())}
      title="投稿"
    >
      <Pencil size={20} />
    </button>
  {/if}

  {#if app.showComposeModal}
    <div
      class="fixed inset-0 z-50 grid content-start justify-items-stretch bg-black/45 pt-[max(6vh,env(safe-area-inset-top))]"
      onclick={() => (app.showComposeModal = false)}
      onkeydown={(e) => e.key === "Escape" && (app.showComposeModal = false)}
      role="presentation"
    >
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div
        class="box-border max-h-[80vh] w-full overflow-y-auto rounded-[14px] border border-border bg-background p-3"
        onclick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        tabindex="-1"
      >
        <ComposeBar onPosted={() => (app.showComposeModal = false)} expanded />
      </div>
    </div>
  {/if}

  {#if showAddColumn}
    <AddColumnModal
      groupId={addTabGroupId}
      editTab={editTab ?? undefined}
      onclose={() => {
        showAddColumn = false;
        if (pendingSplitGroupId) {
          void app.discardEmptyGroup(pendingSplitGroupId);
          pendingSplitGroupId = null;
        }
      }}
    />
  {/if}
  {#if columnSettingsGroupId}
    <ColumnSettings groupId={columnSettingsGroupId} onclose={() => (columnSettingsGroupId = null)} />
  {/if}
  {#if showSettings}
    <Settings
      initial={settingsInitial}
      onAddAccount={addAccountFromSettings}
      onReauth={startReauth}
      onclose={() => (showSettings = false)}
    />
  {/if}
  {#if currentProfileTarget()}
    <ProfileModal
      target={currentProfileTarget()!}
      accountId={currentProfileAccountId() ?? app.defaultAccountId()}
      onclose={closeProfile}
    />
  {/if}
  {#if app.errorModal}
    <Modal title="エラー" onclose={() => (app.errorModal = null)}>
      {#snippet children()}
        <p class="mb-3.5 mt-0 whitespace-pre-wrap break-words text-[0.9rem] text-foreground">{app.errorModal}</p>
        <div class="flex justify-end">
          <Button onclick={() => (app.errorModal = null)}>わかった</Button>
        </div>
      {/snippet}
    </Modal>
  {/if}
</div>
