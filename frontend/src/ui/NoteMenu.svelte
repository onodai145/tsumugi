<script lang="ts">
  import type { Note, Clip } from "../bindings/tauri.gen";
  import { app } from "../lib/store.svelte";
  import { Star, Paperclip, ChevronRight } from "@lucide/svelte";

  let { accountId, note, onclose }: { accountId: string; note: Note; onclose: () => void } = $props();

  let clipSubmenuOpen = $state(false);
  let clips = $state<Clip[] | null>(null);
  let clipsLoading = $state(false);
  let clipsError = $state(false);
  let creatingClip = $state(false);
  let newClipName = $state("");
  let clipRowEl = $state<HTMLElement | null>(null);
  let submenuSide = $state<"right" | "left">("right");

  function toggleFavorite() {
    app.toggleFavorite(accountId, note.id);
    onclose();
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

<div class="menu">
  <button class="item" onclick={toggleFavorite}>
    <Star size={14} />
    {note.isFavoritedByMe ? "お気に入り解除" : "お気に入り登録"}
  </button>

  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="item-wrap" role="presentation" bind:this={clipRowEl} onmouseenter={openClipSubmenu}>
    <button class="item" onclick={openClipSubmenu}>
      <Paperclip size={14} />
      クリップに追加
      <ChevronRight size={14} class="chevron" />
    </button>

    {#if clipSubmenuOpen}
      <div class="submenu" class:submenu-left={submenuSide === "left"}>
        {#if creatingClip}
          <div class="create-row">
            <input
              class="name-input"
              placeholder="クリップ名"
              bind:value={newClipName}
              onkeydown={(e) => e.key === "Enter" && confirmCreateClip()}
            />
            <button class="confirm-btn" disabled={!newClipName.trim()} onclick={confirmCreateClip}>
              作成
            </button>
          </div>
        {:else}
          {#if clipsLoading}
            <span class="hint">読み込み中…</span>
          {:else if clipsError}
            <span class="hint">読み込みに失敗しました</span>
          {:else if clips && clips.length === 0}
            <span class="hint">クリップがありません</span>
          {:else if clips}
            {#each clips as clip (clip.id)}
              <button class="item" onclick={() => pickClip(clip)}>{clip.name}</button>
            {/each}
          {/if}
          <button class="item new-clip" onclick={startCreateClip}>＋ 新規クリップを作成</button>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .menu {
    width: 200px;
    padding: 4px;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
  }
  .item-wrap {
    position: relative;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 8px;
    background: transparent;
    border: none;
    color: var(--text);
    font-size: 0.82rem;
    text-align: left;
    cursor: pointer;
    border-radius: 5px;
    box-sizing: border-box;
  }
  .item:hover {
    background: var(--surface-2);
  }
  .item :global(.chevron) {
    margin-left: auto;
  }
  .submenu {
    position: absolute;
    left: 100%;
    top: 0;
    width: 200px;
    max-height: 280px;
    overflow-y: auto;
    padding: 4px;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
  }
  .submenu.submenu-left {
    left: auto;
    right: 100%;
  }
  .new-clip {
    color: var(--accent);
  }
  .hint {
    display: block;
    padding: 6px 8px;
    font-size: 0.78rem;
    color: var(--text-dim);
  }
  .create-row {
    display: flex;
    gap: 4px;
    padding: 4px;
  }
  .name-input {
    flex: 1;
    min-width: 0;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--surface-2);
    color: var(--text);
    font-size: 0.82rem;
    box-sizing: border-box;
  }
  .confirm-btn {
    padding: 4px 8px;
    border: none;
    border-radius: 5px;
    background: var(--accent);
    color: var(--surface-1);
    font-size: 0.78rem;
    cursor: pointer;
  }
  .confirm-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
