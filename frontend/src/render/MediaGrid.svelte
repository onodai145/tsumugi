<script lang="ts">
  import "vidstack/player/styles/base.css";
  import "vidstack/player";
  import "vidstack/player/ui";
  import { Download, Maximize2, Pause, Play, Volume2, VolumeX } from "@lucide/svelte";
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

{#snippet mediaControls(f: DriveFile)}
  <media-controls>
    <media-controls-group class="media-ctrl-group">
      <media-play-button class="media-ctrl-btn" aria-label="再生/一時停止">
        <Play size={14} class="media-icon-play" />
        <Pause size={14} class="media-icon-pause" />
      </media-play-button>
      <media-mute-button class="media-ctrl-btn" aria-label="ミュート切替">
        <Volume2 size={14} class="media-icon-volume" />
        <VolumeX size={14} class="media-icon-mute" />
      </media-mute-button>
      <button
        class="media-ctrl-btn"
        onclick={() => (viewerOpenIndex = deriveViewItems(files).findIndex((x) => x.id === f.id))}
        aria-label="拡大表示"
      >
        <Maximize2 size={14} />
      </button>
      <button
        class="media-ctrl-btn"
        onclick={() => saveMediaToDisk(f.url, fileName(f), (e) => app.reportError(e))}
        aria-label="保存"
      >
        <Download size={14} />
      </button>
    </media-controls-group>
  </media-controls>
{/snippet}

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
          <media-player src={f.url} viewType="video" playsinline preload="metadata" class="h-full w-full">
            <media-provider></media-provider>
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="media-ctrl-bar" onclick={(e) => e.stopPropagation()}>
              {@render mediaControls(f)}
            </div>
          </media-player>
        {:else if isAudio(f)}
          <!-- svelte-ignore a11y_media_has_caption -->
          <media-player src={f.url} viewType="audio" preload="metadata" class="w-[calc(100%-16px)]">
            <media-provider></media-provider>
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="media-ctrl-bar media-ctrl-bar-audio" onclick={(e) => e.stopPropagation()}>
              {@render mediaControls(f)}
            </div>
          </media-player>
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

  /* Vidstackの<media-player>はbase.cssの既定でdisplay:inline-flex; width:100%までしか
     持たないため、グリッドセル(.media-cell)を埋めるレイアウトはここで補う。
     動画はセルの高さいっぱいに敷き詰め(object-fit: coverで従来の<video controls>と同じ見た目)、
     音声はプレイヤー本体を隠して(data-view-type="audio")コントロールバーだけ表示する。 */
  :global(.media-cell media-player) {
    position: relative;
    display: flex;
    height: 100%;
    border-radius: inherit;
  }
  :global(.media-cell media-player[data-view-type="video"] video) {
    height: 100%;
    object-fit: cover;
  }
  :global(.media-cell media-player[data-view-type="audio"]) {
    height: auto;
  }
  /* base.css/theme.css既定の[data-media-player][data-view-type='video'][data-started]:not([data-controls])
     はcursor: noneを付与する(全画面プレイヤーでの再生中マウスカーソル自動非表示用)。
     グリッドサムネイルではネイティブcontrols属性を使わない(=[data-controls]が常に外れた状態になる)ため、
     再生中は常にカーソルが消えてしまう。サムネイル上では不要な挙動なので元に戻す。 */
  :global(.media-cell [data-media-player][data-view-type="video"][data-started]:not([data-controls])) {
    cursor: auto;
  }

  /* 動画は右下にオーバーレイ、音声はセル内の通常フローに配置する
     (音声はネイティブcontrols相当の表示領域自体を持たないため)。 */
  .media-ctrl-bar {
    position: absolute;
    right: 0.25rem;
    bottom: 0.25rem;
    left: 0.25rem;
    z-index: 1;
    display: flex;
    justify-content: flex-end;
  }
  .media-ctrl-bar-audio {
    position: static;
    justify-content: center;
  }
  :global(.media-ctrl-group) {
    display: flex;
    gap: 0.25rem;
  }
  :global(.media-ctrl-btn) {
    display: flex;
    height: 1.75rem;
    width: 1.75rem;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 9999px;
    background: rgb(0 0 0 / 50%);
    color: white;
    font-size: 0.875rem;
    line-height: 1;
    cursor: pointer;
  }

  /* 再生中/一時停止中で表示アイコンを出し分ける。Vidstackが<media-play-button>本体に
     付与するdata-paused属性(素のCSS属性セレクタ、Tailwindプラグイン不使用)で切り替える。 */
  :global(media-play-button:not([data-paused]) .media-icon-play) {
    display: none;
  }
  :global(media-play-button[data-paused] .media-icon-pause) {
    display: none;
  }

  /* ミュート中/非ミュート中で表示アイコンを出し分ける。<media-mute-button>本体の
     data-muted属性で切り替える。 */
  :global(media-mute-button[data-muted] .media-icon-volume) {
    display: none;
  }
  :global(media-mute-button:not([data-muted]) .media-icon-mute) {
    display: none;
  }
</style>
