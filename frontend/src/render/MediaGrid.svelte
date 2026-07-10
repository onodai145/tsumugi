<script lang="ts">
  import { onMount } from "svelte";
  import Viewer from "viewerjs";
  import "viewerjs/dist/viewer.css";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { DriveFile } from "../bindings/tauri.gen";
  let { files }: { files: DriveFile[] } = $props();

  let revealed = $state<Record<string, boolean>>({});
  const isImage = (f: DriveFile) => f.mimeType.startsWith("image/");
  const isVideo = (f: DriveFile) => f.mimeType.startsWith("video/");
  const fileName = (f: DriveFile) => f.name || f.mimeType || "file";

  let gridEl = $state<HTMLDivElement | undefined>();
  let viewer: Viewer | undefined;

  // 画像のクリック→拡大表示(ズーム/ドラッグ/ホイールズーム含む)は自前実装せず
  // viewerjs(https://github.com/fengyuanchen/viewerjs)に委譲する。コンテナ内の
  // <img> を自動検出するので、閲覧注意で隠している間はそもそも <img> を描画しない
  // ことで対象から除外し、表示切替(revealed変更)時は update() で再スキャンさせる。
  onMount(() => {
    if (gridEl) {
      viewer = new Viewer(gridEl, { url: "data-original" });
    }
    return () => viewer?.destroy();
  });

  $effect(() => {
    void revealed;
    viewer?.update();
  });
</script>

{#if files.length > 0}
  <div class="media-grid" class:single={files.length === 1} bind:this={gridEl}>
    {#each files as f (f.id)}
      <div class="media-cell">
        {#if f.isSensitive && !revealed[f.id]}
          <button class="sensitive-cover" onclick={() => (revealed = { ...revealed, [f.id]: true })}>
            閲覧注意（クリックで表示）
          </button>
        {:else if isImage(f)}
          <img src={f.thumbnailUrl ?? f.url} data-original={f.url} alt={fileName(f)} loading="lazy" />
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
    cursor: zoom-in;
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
</style>
