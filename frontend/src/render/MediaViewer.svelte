<script lang="ts">
  import "cropperjs";
  import "vidstack/player/styles/default/theme.css";
  import "vidstack/player/styles/default/layouts/video.css";
  import "vidstack/player/styles/default/layouts/audio.css";
  import "vidstack/player";
  import "vidstack/player/layouts";
  import "vidstack/player/ui";
  import Panzoom, { type PanzoomObject } from "@panzoom/panzoom";
  import { onMount, untrack } from "svelte";
  import { ChevronLeft, ChevronRight, Download, FlipHorizontal, FlipVertical, RotateCcw, RotateCw, X, ZoomIn, ZoomOut } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import { portal } from "../lib/portal";
  import { saveMediaToDisk } from "../lib/mediaDownload";
  import { app } from "../lib/store.svelte";
  import type { DriveFile } from "../bindings/tauri.gen";
  import {
    deriveViewItems,
    initialImageTransform,
    isRevealed,
    nextIndex,
    prevIndex,
    reveal,
    rotateCCW,
    rotateCW,
    toggleFlipH,
    toggleFlipV,
    type ImageTransform,
  } from "../lib/mediaViewer.svelte";

  let {
    files,
    startIndex,
    revealed = $bindable(),
    onclose,
  }: {
    files: DriveFile[];
    startIndex: number;
    revealed: Record<string, boolean>;
    onclose: () => void;
  } = $props();

  // files/startIndexはビューワーを開いた時点の値を初期化にのみ使い、以降props側が
  // 変化しても追従しない(ビューワーは開くたびに新規マウントされる想定のため)。
  const viewItems = untrack(() => deriveViewItems(files));
  let currentIndex = $state(untrack(() => startIndex));
  let imageTransform = $state<ImageTransform>(initialImageTransform);

  const current = $derived(viewItems[currentIndex]);
  const fileName = (f: DriveFile) => f.name || f.mimeType || "file";
  const isImage = (f: DriveFile) => f.mimeType.startsWith("image/");
  const isVideo = (f: DriveFile) => f.mimeType.startsWith("video/");

  let scrollEl: HTMLDivElement | undefined;
  let cropperImageEl = $state<(HTMLElement & { $rotate: (a: number) => void; $scale: (x: number, y?: number) => void; $zoom: (s: number, x?: number, y?: number) => void; $resetTransform: () => void }) | undefined>(undefined);

  function goTo(index: number, behavior: ScrollBehavior = "smooth") {
    currentIndex = index;
    imageTransform = initialImageTransform;
    scrollEl?.children[index]?.scrollIntoView({ behavior, inline: "start", block: "nearest" });
  }

  // マウント時、scrollEl(横スクロールコンテナ)の初期表示位置がstartIndexとズレないよう
  // アニメーションなしで即座に同期させる(goTo()はgoNext/goPrev/矢印キーからしか呼ばれず、
  // マウント時には何もスクロールしないため、非ゼロstartIndexだと常に1枚目が表示されてしまう)。
  onMount(() => {
    scrollEl?.children[currentIndex]?.scrollIntoView({ behavior: "auto", inline: "start", block: "nearest" });
  });

  function goNext() {
    goTo(nextIndex(currentIndex, viewItems.length));
  }

  function goPrev() {
    goTo(prevIndex(currentIndex, viewItems.length));
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
    else if (e.key === "ArrowRight") goNext();
    else if (e.key === "ArrowLeft") goPrev();
  }

  function onRevealClick() {
    revealed = reveal(revealed, current);
  }

  // Scroll Snapコンテナのスクロールが落ち着いたら、実際に表示されている位置に
  // currentIndexを合わせる(矢印ボタン以外にスワイプでもcurrentIndexが正しく追従するように)。
  let scrollEndTimer: ReturnType<typeof setTimeout> | undefined;
  function onScroll() {
    clearTimeout(scrollEndTimer);
    scrollEndTimer = setTimeout(() => {
      if (!scrollEl) return;
      const width = scrollEl.clientWidth;
      const index = Math.round(scrollEl.scrollLeft / width);
      if (index !== currentIndex && index >= 0 && index < viewItems.length) {
        currentIndex = index;
        imageTransform = initialImageTransform;
      }
    }, 120);
  }

  function applyImageTransform() {
    if (!cropperImageEl) return;
    cropperImageEl.$resetTransform();
    cropperImageEl.$rotate(imageTransform.rotation);
    cropperImageEl.$scale(imageTransform.flipH ? -1 : 1, imageTransform.flipV ? -1 : 1);
  }

  $effect(() => {
    void imageTransform;
    applyImageTransform();
  });

  // ズーム時のみパンを有効化する。translatable属性は既定でoffにしておき、
  // Cropper.jsのtransformイベント(detail.matrix、[a,b,c,d,e,f]のCSS行列でaがX方向スケール)を
  // 見てscale>1の間だけonにする(等倍時にドラッグがスワイプナビゲーションと競合しないように)。
  function onCropperImageTransform(e: Event) {
    const detail = (e as CustomEvent<{ matrix: number[] }>).detail;
    const scale = detail.matrix[0];
    cropperImageEl?.toggleAttribute("translatable", scale > 1 + 1e-6);
  }

  // 動画はズーム時のみパンを有効化する(画像のCropper.jsと同じ方針)。videoPanzoomアクションは
  // Scroll Snapの各ページごとに要素が個別マウントされる(#each item.id)ため、アイテム切替時に
  // 別インスタンスとして再生成され、倍率リセットのための追加処理は不要。
  function videoPanzoom(node: HTMLElement) {
    const pz: PanzoomObject = Panzoom(node, { maxScale: 4, disablePan: true });
    const onWheel = (e: WheelEvent) => pz.zoomWithWheel(e);
    node.addEventListener("wheel", onWheel);

    function syncDisablePan() {
      pz.setOptions({ disablePan: pz.getScale() <= 1 + 1e-6 });
    }
    node.addEventListener("panzoomzoom", syncDisablePan);

    return {
      destroy() {
        node.removeEventListener("wheel", onWheel);
        node.removeEventListener("panzoomzoom", syncDisablePan);
        pz.destroy();
      },
    };
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div
  class="fixed inset-0 z-[1000] flex flex-col bg-black/90"
  use:portal
  onclick={onclose}
  role="presentation"
>
  <div class="relative z-10 flex flex-none items-center justify-end p-[max(0.5rem,env(safe-area-inset-top))_max(0.5rem,env(safe-area-inset-right))_0.5rem_0.5rem]">
    <Button variant="ghost" size="icon" class="text-white viewer-icon-btn hover:text-white" onclick={(e) => { e.stopPropagation(); onclose(); }} aria-label="閉じる">
      <X size={16} />
    </Button>
  </div>

  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    bind:this={scrollEl}
    onscroll={onScroll}
    onclick={(e) => e.stopPropagation()}
    class="flex flex-1 snap-x snap-mandatory overflow-x-auto overflow-y-hidden [scrollbar-width:none]"
  >
    {#each viewItems as item (item.id)}
      <div
        class="flex h-full w-full flex-none snap-start items-center justify-center"
        data-testid="media-page"
        aria-label={isRevealed(revealed, item) ? `${fileName(item)}を表示中` : undefined}
      >
        {#if !isRevealed(revealed, item)}
          <button
            class="border-0 bg-transparent text-base text-white"
            onclick={item.id === current.id ? onRevealClick : undefined}
          >
            閲覧注意（クリックで表示）
          </button>
        {:else if isImage(item)}
          <cropper-canvas class="h-full w-full" background="false">
            <cropper-image
              bind:this={
                () => (item.id === current.id ? cropperImageEl : undefined),
                (el) => {
                  if (item.id === current.id) cropperImageEl = el;
                }
              }
              src={item.url}
              alt={fileName(item)}
              rotatable
              scalable
              class="h-full w-full"
              ontransform={onCropperImageTransform}
            ></cropper-image>
          </cropper-canvas>
        {:else if isVideo(item)}
          <div class="flex h-full w-full items-center justify-center p-4" use:videoPanzoom>
            <media-player src={item.url} title={fileName(item)} playsinline crossorigin class="max-h-full max-w-full">
              <media-provider></media-provider>
              <media-video-layout></media-video-layout>
            </media-player>
          </div>
        {:else}
          <div class="w-full max-w-md px-4">
            <media-player src={item.url} title={fileName(item)} crossorigin>
              <media-provider></media-provider>
              <media-audio-layout></media-audio-layout>
            </media-player>
          </div>
        {/if}
      </div>
    {/each}
  </div>

  {#if viewItems.length > 1}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="group absolute top-0 left-0 flex h-full w-[15%] min-w-10 cursor-w-resize items-center justify-start pl-2"
      onclick={(e) => { e.stopPropagation(); goPrev(); }}
    >
      <button
        class="rounded-full bg-black/40 p-2 text-white opacity-0 transition-opacity group-hover:opacity-100"
        onclick={(e) => { e.stopPropagation(); goPrev(); }}
        aria-label="前へ"
      >
        <ChevronLeft size={20} />
      </button>
    </div>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="group absolute top-0 right-0 flex h-full w-[15%] min-w-10 cursor-e-resize items-center justify-end pr-2"
      onclick={(e) => { e.stopPropagation(); goNext(); }}
    >
      <button
        class="rounded-full bg-black/40 p-2 text-white opacity-0 transition-opacity group-hover:opacity-100"
        onclick={(e) => { e.stopPropagation(); goNext(); }}
        aria-label="次へ"
      >
        <ChevronRight size={20} />
      </button>
    </div>
  {/if}

  {#if isRevealed(revealed, current) && isImage(current)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="relative z-10 flex flex-none items-center justify-center gap-1 p-[0.5rem_0.5rem_max(0.5rem,env(safe-area-inset-bottom))]"
      onclick={(e) => e.stopPropagation()}
      role="toolbar"
      aria-label="画像ツールバー"
      tabindex="-1"
    >
      <Button variant="ghost" size="icon" class="text-white viewer-icon-btn hover:text-white" onclick={() => cropperImageEl?.$zoom(0.1)} aria-label="ズームイン"><ZoomIn size={16} /></Button>
      <Button variant="ghost" size="icon" class="text-white viewer-icon-btn hover:text-white" onclick={() => cropperImageEl?.$zoom(-0.1)} aria-label="ズームアウト"><ZoomOut size={16} /></Button>
      <Button variant="ghost" size="icon" class="text-white viewer-icon-btn hover:text-white" onclick={() => (imageTransform = rotateCCW(imageTransform))} aria-label="左回転"><RotateCcw size={16} /></Button>
      <Button variant="ghost" size="icon" class="text-white viewer-icon-btn hover:text-white" onclick={() => (imageTransform = rotateCW(imageTransform))} aria-label="右回転"><RotateCw size={16} /></Button>
      <Button variant="ghost" size="icon" class="text-white viewer-icon-btn hover:text-white" onclick={() => (imageTransform = toggleFlipH(imageTransform))} aria-label="左右反転"><FlipHorizontal size={16} /></Button>
      <Button variant="ghost" size="icon" class="text-white viewer-icon-btn hover:text-white" onclick={() => (imageTransform = toggleFlipV(imageTransform))} aria-label="上下反転"><FlipVertical size={16} /></Button>
      <Button variant="ghost" size="icon" class="text-white viewer-icon-btn hover:text-white" onclick={() => saveMediaToDisk(current.url, fileName(current), (e) => app.reportError(e))} aria-label="ダウンロード"><Download size={16} /></Button>
    </div>
  {/if}
</div>

<style>
  /* ホバー背景色は生のTailwindカラー(bg-white/10等)ではなく--accentトークン経由にする
     (Dropdown.svelte/ReactionAcceptanceSelect.svelteの.active色と同じパターン)。
     対象はButtonコンポーネント越しの実DOM要素なので:globalで指定し、Buttonのvariant側
     デフォルトhoverクラス(hover:bg-muted等)より確実に優先させるためimportantを付ける。 */
  :global(.viewer-icon-btn:hover) {
    background-color: color-mix(in srgb, var(--accent) 20%, transparent) !important;
  }

  /* VidstackのデフォルトレイアウトのテーマCSS変数をアプリのトークンにマッピングする。
     style-guide.md §2のrounded-md(6px)相当。 */
  :global(media-player) {
    --video-brand: var(--accent);
    --audio-brand: var(--accent);
    --video-focus-ring-color: var(--accent);
    --audio-focus-ring-color: var(--accent);
    --video-border-radius: 6px;
    --audio-border-radius: 6px;
  }
</style>
