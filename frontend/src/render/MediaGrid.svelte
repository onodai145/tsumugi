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
  const isAudio = (f: DriveFile) => f.mimeType.startsWith("audio/");
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
  <div
    class={files.length === 1
      ? "mt-2 grid grid-cols-1 gap-1 overflow-hidden rounded-md"
      : "mt-2 grid grid-cols-2 gap-1 overflow-hidden rounded-md"}
    bind:this={gridEl}
  >
    {#each files as f (f.id)}
      <div class="media-cell relative flex aspect-[16/10] items-center justify-center">
        {#if f.isSensitive && !revealed[f.id]}
          <button
            class="sensitive-cover h-full w-full border-0 text-sm text-muted-foreground"
            onclick={() => (revealed = { ...revealed, [f.id]: true })}
          >
            閲覧注意（クリックで表示）
          </button>
        {:else if isImage(f)}
          <img
            src={f.thumbnailUrl ?? f.url}
            data-original={f.url}
            alt={fileName(f)}
            loading="lazy"
            class="h-full w-full cursor-zoom-in object-cover"
          />
        {:else if isVideo(f)}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video src={f.url} controls preload="metadata" class="h-full w-full cursor-zoom-in object-cover"
          ></video>
          <button
            class="absolute top-1.5 right-1.5 flex size-7 items-center justify-center rounded-full bg-black/50 text-sm leading-none text-white"
            onclick={() => saveToDisk(f.url, fileName(f))}
            aria-label="保存"
          >
            💾
          </button>
        {:else if isAudio(f)}
          <!-- svelte-ignore a11y_media_has_caption -->
          <audio src={f.url} controls preload="metadata" class="w-[calc(100%-16px)]"></audio>
          <button
            class="absolute top-1.5 right-1.5 flex size-7 items-center justify-center rounded-full bg-black/50 text-sm leading-none text-white"
            onclick={() => saveToDisk(f.url, fileName(f))}
            aria-label="保存"
          >
            💾
          </button>
        {:else}
          <button
            class="max-w-full overflow-hidden text-ellipsis whitespace-nowrap border-0 bg-none p-2 font-[inherit] text-sm text-primary"
            onclick={() => openUrl(f.url)}
          >
            📄 {fileName(f)}
          </button>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .media-cell {
    /* 幅広カラムでは aspect-ratio のままだと高さも際限なく伸びてしまう
       (Issue #8) ため、高さの上限を設ける。object-fit: cover で見た目は保たれる。
       設定→表示 で調整可能（--media-thumbnail-height, 既定200px）。 */
    max-height: var(--media-thumbnail-height, 200px);
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
  }
  .sensitive-cover {
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
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
