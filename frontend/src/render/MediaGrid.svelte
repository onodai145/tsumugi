<script lang="ts">
  import type { DriveFile } from "../bindings/tauri.gen";
  let { files }: { files: DriveFile[] } = $props();

  let revealed = $state<Record<string, boolean>>({});
  const isImage = (f: DriveFile) => f.mimeType.startsWith("image/");
  const isVideo = (f: DriveFile) => f.mimeType.startsWith("video/");
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
          <img src={f.thumbnailUrl ?? f.url} alt="attachment" loading="lazy" />
        {:else if isVideo(f)}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video src={f.url} controls preload="metadata"></video>
        {:else}
          <a class="file-link" href={f.url} target="_blank" rel="noreferrer noopener">{f.mimeType}</a>
        {/if}
      </div>
    {/each}
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
  }
</style>
