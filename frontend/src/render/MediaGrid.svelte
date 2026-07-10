<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { DriveFile } from "../bindings/tauri.gen";
  let { files }: { files: DriveFile[] } = $props();

  let revealed = $state<Record<string, boolean>>({});
  let lightbox = $state<DriveFile | null>(null);
  const isImage = (f: DriveFile) => f.mimeType.startsWith("image/");
  const isVideo = (f: DriveFile) => f.mimeType.startsWith("video/");
  const fileName = (f: DriveFile) => f.name || f.mimeType || "file";

  const MIN_ZOOM = 1;
  const MAX_ZOOM = 6;
  let zoom = $state(1);
  let panX = $state(0);
  let panY = $state(0);
  let dragging = false;
  let dragStart = { x: 0, y: 0, panX: 0, panY: 0 };

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  function resetZoom() {
    zoom = 1;
    panX = 0;
    panY = 0;
  }

  function openLightbox(f: DriveFile) {
    lightbox = f;
    resetZoom();
  }
  function closeLightbox() {
    lightbox = null;
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") closeLightbox();
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    const oldZoom = zoom;
    const newZoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, oldZoom * (1 - e.deltaY * 0.001)));
    if (newZoom === oldZoom) return;
    // カーソル位置を基準にズームする: transform は translate(pan) scale(zoom) で
    // transform-origin は要素中心(既定)なので、要素の現在の中心(rect中心)から見た
    // カーソルのオフセットは zoom に対して不変(v = (mouse-中心)/zoom)。
    // 同じ v が同じ画面位置に留まるように新しい pan を逆算する。
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    const dx = e.clientX - cx;
    const dy = e.clientY - cy;
    const factor = 1 - newZoom / oldZoom;
    panX = panX + dx * factor;
    panY = panY + dy * factor;
    zoom = newZoom;
    if (zoom === MIN_ZOOM) {
      panX = 0;
      panY = 0;
    }
  }

  function onDblclick() {
    if (zoom > MIN_ZOOM) {
      resetZoom();
    } else {
      zoom = 2.5;
    }
  }

  function onPointerdown(e: PointerEvent) {
    if (zoom <= MIN_ZOOM) return;
    e.preventDefault(); // ブラウザ既定の画像ドラッグ(ゴースト画像)を止める。これが有効だと
    // 自前のポインタベースのパンと衝突し、パンできたりできなかったりする。
    dragging = true;
    dragStart = { x: e.clientX, y: e.clientY, panX, panY };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onPointermove(e: PointerEvent) {
    if (!dragging) return;
    panX = dragStart.panX + (e.clientX - dragStart.x);
    panY = dragStart.panY + (e.clientY - dragStart.y);
  }
  function onPointerup() {
    dragging = false;
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
    <button class="lightbox-close" onclick={closeLightbox} aria-label="閉じる">✕</button>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <img
      class="lightbox-img"
      class:zoomed={zoom > MIN_ZOOM}
      style="transform: translate({panX}px, {panY}px) scale({zoom})"
      src={lightbox.url}
      alt={fileName(lightbox)}
      draggable="false"
      ondragstart={(e) => e.preventDefault()}
      onclick={(e) => e.stopPropagation()}
      onwheel={onWheel}
      ondblclick={onDblclick}
      onpointerdown={onPointerdown}
      onpointermove={onPointermove}
      onpointerup={onPointerup}
      onpointercancel={onPointerup}
    />
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
    cursor: zoom-in;
    touch-action: none;
    -webkit-user-drag: none;
    user-select: none;
  }
  .lightbox-img.zoomed {
    cursor: grab;
  }
  .lightbox-close {
    position: absolute;
    top: 16px;
    right: 20px;
    border: none;
    background: rgba(255, 255, 255, 0.15);
    color: #fff;
    width: 36px;
    height: 36px;
    border-radius: 50%;
    font-size: 1.1rem;
    cursor: pointer;
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
