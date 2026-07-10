<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { DriveFile } from "../bindings/tauri.gen";
  let { files }: { files: DriveFile[] } = $props();

  let revealed = $state<Record<string, boolean>>({});
  let lightbox = $state<DriveFile | null>(null);
  const isImage = (f: DriveFile) => f.mimeType.startsWith("image/");
  const isVideo = (f: DriveFile) => f.mimeType.startsWith("video/");
  const fileName = (f: DriveFile) => f.name || f.mimeType || "file";

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  function openLightbox(f: DriveFile) {
    lightbox = f;
  }
  function closeLightbox() {
    lightbox = null;
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") closeLightbox();
  }
</script>

{#if files.length > 0}
  <div class="media-grid" class:single={files.length === 1}>
    {#each files as f (f.id)}
      <div class="media-cell">
        {#if f.isSensitive && !revealed[f.id]}
          <button class="sensitive-cover" onclick={() => (revealed = { ...revealed, [f.id]: true })}>
            閲覧注意（クリックで表示）
          </button>
        {:else if isImage(f)}
          <button class="img-btn" onclick={() => openLightbox(f)} aria-label="拡大表示">
            <img src={f.thumbnailUrl ?? f.url} alt={fileName(f)} loading="lazy" />
          </button>
        {:else if isVideo(f)}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video src={f.url} controls preload="metadata"></video>
        {:else}
          <button class="file-link" onclick={() => openUrl(f.url)}>📄 {fileName(f)}</button>
        {/if}
      </div>
    {/each}
  </div>
{/if}

{#if lightbox}
  <div
    class="lightbox-overlay"
    use:portal
    role="button"
    tabindex="0"
    onclick={closeLightbox}
    onkeydown={onKeydown}
  >
    <img class="lightbox-img" src={lightbox.url} alt={fileName(lightbox)} />
    <button
      class="lightbox-download"
      onclick={(e) => {
        e.stopPropagation();
        openUrl(lightbox!.url);
      }}
    >
      ブラウザで開く（保存はブラウザ側で）
    </button>
  </div>
{/if}

<style>
  .media-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
    margin-top: 8px;
    border-radius:  5px;
    overflow: hidden;
  }
  .media-grid.single {
    grid-template-columns: 1fr;
  }
  .media-cell {
    position: relative;
    aspect-ratio: 16 / 10;
    background: var(--surface-2);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .media-cell img,
  .media-cell video {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .sensitive-cover {
    width: 100%;
    height: 100%;
    border: none;
    background: var(--surface-3);
    color: var(--text-dim);
    cursor: pointer;
    font-size: 0.85rem;
  }
  .file-link {
    font-size: 0.85rem;
    padding: 8px;
    color: var(--accent);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
    border: none;
    background: none;
    cursor: pointer;
    font-family: inherit;
  }
  .img-btn {
    width: 100%;
    height: 100%;
    padding: 0;
    border: none;
    background: none;
    cursor: zoom-in;
    display: block;
  }
  .lightbox-overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
    background: rgba(0, 0, 0, 0.85);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    gap: 12px;
    cursor: zoom-out;
  }
  .lightbox-img {
    max-width: 90vw;
    max-height: 85vh;
    object-fit: contain;
  }
  .lightbox-download {
    color: #fff;
    background: var(--accent);
    padding: 6px 16px;
    border-radius: 6px;
    border: none;
    font-size: 0.85rem;
    font-family: inherit;
    cursor: pointer;
  }
</style>
