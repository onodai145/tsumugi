<script lang="ts">
  import type { Note, Clip } from "../bindings/tauri.gen";
  import { app } from "../lib/store.svelte";
  import { Star, Paperclip, ChevronRight, Trash2, Copy } from "@lucide/svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";

  let { accountId, note, onclose }: { accountId: string; note: Note; onclose: () => void } = $props();

  const isOwnNote = $derived(app.accounts.find((a) => a.id === accountId)?.userId === note.user.id);
  let confirmDeleteOpen = $state(false);

  let clipSubmenuOpen = $state(false);
  let clips = $state<Clip[] | null>(null);
  let clipsLoading = $state(false);
  let clipsError = $state(false);
  let creatingClip = $state(false);
  let newClipName = $state("");
  let clipRowEl = $state<HTMLElement | null>(null);
  let submenuSide = $state<"right" | "left">("right");

  function copyText() {
    if (note.text) {
      navigator.clipboard.writeText(note.text).catch((e) => console.error("本文のコピーに失敗しました", e));
    }
    onclose();
  }

  function toggleFavorite() {
    app.toggleFavorite(accountId, note.id);
    onclose();
  }

  function requestDelete() {
    confirmDeleteOpen = true;
  }

  async function confirmDelete() {
    confirmDeleteOpen = false;
    try {
      await app.deleteNote(accountId, note.id);
    } finally {
      onclose();
    }
  }

  function openClipSubmenu() {
    clipSubmenuOpen = true;
    if (clipRowEl) {
      const r = clipRowEl.getBoundingClientRect();
      const SUBMENU_W = 200;
      submenuSide = r.right + SUBMENU_W <= window.innerWidth ? "right" : "left";
    }
    if (clips === null && !clipsLoading) {
      clipsLoading = true;
      clipsError = false;
      app
        .listClips(accountId)
        .then((list) => (clips = list))
        .catch(() => (clipsError = true))
        .finally(() => (clipsLoading = false));
    }
  }

  function pickClip(clip: Clip) {
    app.addNoteToClip(accountId, clip.id, note.id);
    onclose();
  }

  function startCreateClip() {
    creatingClip = true;
    newClipName = "";
  }

  async function confirmCreateClip() {
    const name = newClipName.trim();
    if (!name) return;
    try {
      const clip = await app.createClip(accountId, name);
      await app.addNoteToClip(accountId, clip.id, note.id);
      onclose();
    } catch (e) {
      // creatingClip stays true so the create-row remains visible for retry
      console.error("クリップ作成/追加に失敗しました", e);
    }
  }
</script>

<div class="w-[200px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]">
  {#if note.text}
    <button type="button" class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted" onclick={copyText}>
      <Copy size={16} />
      内容をコピー
    </button>
  {/if}
  <button type="button" class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted" onclick={toggleFavorite}>
    <Star size={16} />
    {note.isFavoritedByMe ? "お気に入り解除" : "お気に入り登録"}
  </button>

  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="relative" role="presentation" bind:this={clipRowEl} onmouseenter={openClipSubmenu}>
    <button type="button" class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted" onclick={openClipSubmenu}>
      <Paperclip size={16} />
      クリップに追加
      <ChevronRight size={16} class="ml-auto" />
    </button>

    {#if clipSubmenuOpen}
      <div
        class={submenuSide === "left"
          ? "absolute right-full top-0 max-h-[280px] w-[200px] overflow-y-auto rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
          : "absolute left-full top-0 max-h-[280px] w-[200px] overflow-y-auto rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"}
      >
        {#if creatingClip}
          <div class="flex gap-1 p-1">
            <input
              class="box-border min-w-0 flex-1 rounded-md border border-border bg-muted px-1.5 py-1 text-sm text-foreground"
              placeholder="クリップ名"
              bind:value={newClipName}
              onkeydown={(e) => e.key === "Enter" && confirmCreateClip()}
            />
            <button type="button" class="rounded-md bg-primary px-2 py-1 text-sm text-primary-foreground disabled:cursor-default disabled:opacity-50" disabled={!newClipName.trim()} onclick={confirmCreateClip}>
              作成
            </button>
          </div>
        {:else}
          {#if clipsLoading}
            <span class="block px-2 py-1.5 text-sm text-muted-foreground">読み込み中…</span>
          {:else if clipsError}
            <span class="block px-2 py-1.5 text-sm text-muted-foreground">読み込みに失敗しました</span>
          {:else if clips && clips.length === 0}
            <span class="block px-2 py-1.5 text-sm text-muted-foreground">クリップがありません</span>
          {:else if clips}
            {#each clips as clip (clip.id)}
              <button type="button" class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted" onclick={() => pickClip(clip)}>{clip.name}</button>
            {/each}
          {/if}
          <button type="button" class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-primary hover:bg-muted" onclick={startCreateClip}>＋ 新規クリップを作成</button>
        {/if}
      </div>
    {/if}
  </div>

  {#if isOwnNote}
    <button
      type="button"
      class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-destructive hover:bg-muted"
      onclick={requestDelete}
    >
      <Trash2 size={16} />
      削除
    </button>
  {/if}
</div>

{#if confirmDeleteOpen}
  <!-- NoteCard.svelte のノートメニュー自身のクリックアウト用backdropがz-[1010]で、
       ConfirmDialogの既定z-1000より上に乗ってクリックを奪ってしまうため、
       それより高いz-1020を明示的に渡す(ConfirmDialog.svelte参照)。 -->
  <ConfirmDialog
    title="投稿の削除"
    message="この投稿を削除します。取り消せません。よろしいですか？"
    confirmLabel="削除する"
    danger
    z={1020}
    onConfirm={confirmDelete}
    onCancel={() => (confirmDeleteOpen = false)}
  />
{/if}
