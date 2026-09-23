<script lang="ts">
  import "vidstack/player/styles/base.css";
  import "vidstack/player";
  import "vidstack/player/ui";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { DriveFile } from "../bindings/tauri.gen";
  import MediaControlBar from "./MediaControlBar.svelte";
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
          <!-- src は文字列ではなく{src, type}オブジェクトで渡す(重要)。MisskeyのドライブファイルURLは
               拡張子を含まない(例: /files/webpublic-<uuid>)ため、Vidstackがsrc文字列だけからMIMEタイプを
               自動推定するinferType()が失敗し(常に"?"=unknown)、プロバイダ(<video>/<audio>要素)が
               一切生成されない不具合があった。typeを明示することで回避する。 -->
          <media-player src={{ src: f.url, type: f.mimeType }} viewType="video" playsinline preload="metadata" class="h-full w-full">
            <media-provider></media-provider>
            <MediaControlBar
              file={f}
              variant="video"
              onExpand={() => (viewerOpenIndex = deriveViewItems(files).findIndex((x) => x.id === f.id))}
            />
          </media-player>
        {:else if isAudio(f)}
          <!-- svelte-ignore a11y_media_has_caption -->
          <media-player src={{ src: f.url, type: f.mimeType }} viewType="audio" preload="metadata" class="w-[calc(100%-16px)]">
            <media-provider></media-provider>
            <MediaControlBar
              file={f}
              variant="audio"
              onExpand={() => (viewerOpenIndex = deriveViewItems(files).findIndex((x) => x.id === f.id))}
            />
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

  /* コントロールバー(シークバー・再生/ミュート/音量/再生速度/拡大表示/ダウンロード)の
     マークアップ・スタイルはMediaViewer.svelteと共有のMediaControlBar.svelteに
     切り出した(:global()ルールのためバンドル時にそちらのCSSがそのまま効く)。 */
</style>
