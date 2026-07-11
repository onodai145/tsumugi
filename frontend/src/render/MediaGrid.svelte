<script lang="ts">
  import { onMount } from "svelte";
  import Viewer from "viewerjs";
  import "viewerjs/dist/viewer.css";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { commands, unwrap } from "../lib/ipc";
  import { app } from "../lib/store.svelte";
  import type { DriveFile } from "../bindings/tauri.gen";
  let { files }: { files: DriveFile[] } = $props();

  let revealed = $state<Record<string, boolean>>({});
  const isImage = (f: DriveFile) => f.mimeType.startsWith("image/");
  const isVideo = (f: DriveFile) => f.mimeType.startsWith("video/");
  const fileName = (f: DriveFile) => f.name || f.mimeType || "file";

  let gridEl = $state<HTMLDivElement | undefined>();
  let viewer: Viewer | undefined;

  async function saveToDisk(url: string, suggestedName: string) {
    try {
      const path = await saveDialog({ defaultPath: suggestedName });
      if (!path) return;
      await unwrap(commands.saveUrlToFile(url, path));
    } catch (e) {
      app.reportError(e);
    }
  }

  // 画像のクリック→拡大表示(ズーム/ドラッグ/ホイールズーム含む)は自前実装せず
  // viewerjs(https://github.com/fengyuanchen/viewerjs)に委譲する。コンテナ内の
  // <img> を自動検出するので、閲覧注意で隠している間はそもそも <img> を描画しない
  // ことで対象から除外し、表示切替(revealed変更)時は update() で再スキャンさせる。
  onMount(() => {
    if (gridEl) {
      viewer = new Viewer(gridEl, {
        url: "data-original",
        toolbar: {
          zoomIn: true,
          zoomOut: true,
          oneToOne: true,
          reset: true,
          prev: true,
          play: true,
          next: true,
          rotateLeft: true,
          rotateRight: true,
          flipHorizontal: true,
          flipVertical: true,
          // viewerjs 組み込みキーではないカスタムボタン(公式の custom-toolbar 例と同じ作法)。
          // `.image` は型定義に無いランタイムプロパティ(現在表示中の<img>のクローン)なのでキャストする。
          download: () => {
            const img = (viewer as unknown as { image?: HTMLImageElement } | undefined)?.image;
            if (img) void saveToDisk(img.src, img.alt || "image");
          },
        },
      });
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
          <button class="video-save" onclick={() => saveToDisk(f.url, fileName(f))} aria-label="保存">
            💾
          </button>
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
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
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
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
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
  .video-save {
    position: absolute;
    top: 6px;
    right: 6px;
    border: none;
    background: rgba(0, 0, 0, 0.5);
    color: #fff;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    font-size: 0.85rem;
    line-height: 1;
    cursor: pointer;
  }
  :global(.viewer-download::before) {
    content: "⬇";
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    font-size: 12px;
    line-height: 1;
    height: 100%;
    margin: 0 !important;
  }
</style>
