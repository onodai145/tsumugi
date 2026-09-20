<script lang="ts">
  import { Download, Maximize2 } from "@lucide/svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { saveMediaToDisk } from "../lib/mediaDownload";
  import { app } from "../lib/store.svelte";
  import type { DriveFile } from "../bindings/tauri.gen";
  import MediaViewer from "./MediaViewer.svelte";
  import { deriveViewItems } from "../lib/mediaViewer.svelte";
  let { files }: { files: DriveFile[] } = $props();

  let revealed = $state<Record<string, boolean>>({});
  const isImage = (f: DriveFile) => f.mimeType.startsWith("image/");
  const isVideo = (f: DriveFile) => f.mimeType.startsWith("video/");
  const isAudio = (f: DriveFile) => f.mimeType.startsWith("audio/");
  const fileName = (f: DriveFile) => f.name || f.mimeType || "file";

  let viewerOpenIndex = $state<number | null>(null);
</script>

{#if files.length > 0}
  <div
    class={files.length === 1
      ? "mt-2 grid grid-cols-1 gap-1 overflow-hidden rounded-md"
      : "mt-2 grid grid-cols-2 gap-1 overflow-hidden rounded-md"}
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
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <img
            src={f.thumbnailUrl ?? f.url}
            alt={fileName(f)}
            loading="lazy"
            class="h-full w-full cursor-zoom-in object-cover"
            onclick={() => (viewerOpenIndex = deriveViewItems(files).findIndex((x) => x.id === f.id))}
          />
        {:else if isVideo(f)}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video src={f.url} controls preload="metadata" class="h-full w-full object-cover"
          ></video>
          <button
            class="absolute top-1.5 right-10 flex size-7 items-center justify-center rounded-full bg-black/50 text-sm leading-none text-white"
            onclick={() => (viewerOpenIndex = deriveViewItems(files).findIndex((x) => x.id === f.id))}
            aria-label="拡大表示"
          >
            <Maximize2 size={14} />
          </button>
          <button
            class="absolute top-1.5 right-1.5 flex size-7 items-center justify-center rounded-full bg-black/50 text-sm leading-none text-white"
            onclick={() => saveMediaToDisk(f.url, fileName(f), (e) => app.reportError(e))}
            aria-label="保存"
          >
            <Download size={14} />
          </button>
        {:else if isAudio(f)}
          <!-- svelte-ignore a11y_media_has_caption -->
          <audio src={f.url} controls preload="metadata" class="w-[calc(100%-16px)]"></audio>
          <button
            class="absolute top-1.5 right-10 flex size-7 items-center justify-center rounded-full bg-black/50 text-sm leading-none text-white"
            onclick={() => (viewerOpenIndex = deriveViewItems(files).findIndex((x) => x.id === f.id))}
            aria-label="拡大表示"
          >
            <Maximize2 size={14} />
          </button>
          <button
            class="absolute top-1.5 right-1.5 flex size-7 items-center justify-center rounded-full bg-black/50 text-sm leading-none text-white"
            onclick={() => saveMediaToDisk(f.url, fileName(f), (e) => app.reportError(e))}
            aria-label="保存"
          >
            <Download size={14} />
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

{#if viewerOpenIndex !== null}
  <MediaViewer
    {files}
    startIndex={viewerOpenIndex}
    bind:revealed
    onclose={() => (viewerOpenIndex = null)}
  />
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
</style>
