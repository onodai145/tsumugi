<script lang="ts">
  import "cropperjs";
  import "vidstack/player/styles/base.css";
  import "vidstack/player";
  import "vidstack/player/ui";
  import Panzoom, { type PanzoomObject } from "@panzoom/panzoom";
  import { onMount, untrack } from "svelte";
  import { ChevronLeft, ChevronRight, Download, FlipHorizontal, FlipVertical, RotateCcw, RotateCw, X, ZoomIn, ZoomOut } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import { portal } from "../lib/portal";
  import { saveMediaToDisk } from "../lib/mediaDownload";
  import { app } from "../lib/store.svelte";
  import type { DriveFile } from "../bindings/tauri.gen";
  import MediaControlBar from "./MediaControlBar.svelte";
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

  let mediaLoadError = $state<Record<string, boolean>>({});

  // Vidstackはmedia-player自身のアスペクト比を実際の動画の内在解像度(videoWidth/videoHeight)
  // から自動算出する仕組みを持たない(state.mediaWidth/mediaHeightはレンダリング後の箱の
  // offsetWidth/offsetHeightであり、動画そのものの解像度ではない)。そのためtsumugi側で
  // 実際の<video>要素のloadedmetadataを購読し、アスペクト比を読み取る必要がある。
  // 読み込み中(まだ取得できていない)は16:9をフォールバックとして使う。
  const DEFAULT_VIDEO_ASPECT_RATIO = 16 / 9;
  let videoAspectRatios = $state<Record<string, number>>({});

  let scrollEl: HTMLDivElement | undefined;
  let cropperImageEl = $state<(HTMLElement & { $rotate: (a: number) => void; $scale: (x: number, y?: number) => void; $zoom: (s: number, x?: number, y?: number) => void; $resetTransform: () => void }) | undefined>(undefined);

  // goTo()が発行したプログラム的スクロール(scrollIntoView)が進行中かどうか。trueの間は
  // onScroll()の受動的なcurrentIndex再同期(手でスワイプした場合の追従用)を止める。
  // 理由: ユーザーが前後送りボタンを連打すると、前のsmoothスクロールアニメーションが
  // 完了しないうちに次のgoTo()が呼ばれることがある(ごく一般的な操作)。この間もscrollLeftは
  // アニメーション途中の中間値を取り続けるため、onScroll()の120msデバウンスがその中間値から
  // Math.roundで誤ったindexを計算し、currentIndexをgoTo()が設定した最終目的地とは違う値に
  // 巻き戻してしまうことがある(実機のWebKitGTKで、next→prev→next→prevを短間隔で連打すると
  // 元のindexに戻らず1つずれた位置に留まることを、src-tauri/src/debug_bridge.rs経由で
  // 実際に再現・確認した)。スクロールが実際に完了するまでこの再同期を止めることで、
  // 中間値をcurrentIndexに取り込むこと自体を防ぐ。
  let programmaticScrollActive = false;
  let programmaticScrollFallbackTimer: ReturnType<typeof setTimeout> | undefined;
  // 'scrollend'(スクロールが実際に静止したタイミングを教えてくれるイベント)が発火すれば
  // それを正として早期にフラグを解除する(下記onScrollEnd参照)。ただし'onscrollend' in window
  // が真であることは「発火することの保証」にはならない — 例えばgoTo(0)をstartIndex: 0の
  // 状態(=マウント直後、既にscrollLeft: 0)で呼んだ場合のようにscrollIntoView自体が実際には
  // 1pxも動かさないケースでは、対応環境でも'scrollend'イベントは発火しない(スクロール位置が
  // 変化していないため)。そのため機能検出(SCROLLEND_SUPPORTED)による分岐はやめ、
  // 常に500ms(1アイテム分のsmoothスクロールが十分収まる余裕を持たせた時間)の
  // フォールバックタイマーもarmする。scrollendが先に発火すればそちらで早期解除され
  // fallbackタイマーはclearされるため、二重解除にはならない。
  function beginProgrammaticScroll() {
    programmaticScrollActive = true;
    clearTimeout(programmaticScrollFallbackTimer);
    programmaticScrollFallbackTimer = setTimeout(() => {
      programmaticScrollActive = false;
    }, 500);
  }

  function onScrollEnd() {
    programmaticScrollActive = false;
    clearTimeout(programmaticScrollFallbackTimer);
  }

  function goTo(index: number, behavior: ScrollBehavior = "smooth") {
    currentIndex = index;
    imageTransform = initialImageTransform;
    beginProgrammaticScroll();
    scrollEl?.children[index]?.scrollIntoView({ behavior, inline: "start", block: "nearest" });
  }

  // マウント時、scrollEl(横スクロールコンテナ)の初期表示位置がstartIndexとズレないよう
  // アニメーションなしで即座に同期させる(goTo()はgoNext/goPrev/矢印キーからしか呼ばれず、
  // マウント時には何もスクロールしないため、非ゼロstartIndexだと常に1枚目が表示されてしまう)。
  onMount(() => {
    beginProgrammaticScroll();
    scrollEl?.children[currentIndex]?.scrollIntoView({ behavior: "auto", inline: "start", block: "nearest" });
  });

  // 末尾→先頭、先頭→末尾のラップアラウンド送りは、Scroll Snapコンテナ上で複数の
  // スナップポイントを一気に跨ぐ長距離のsmoothスクロールになる。WebKitGTK等の一部の
  // ブラウザエンジンでは、scroll-snap-type: mandatoryが有効なコンテナに対する
  // scrollIntoView({behavior:"smooth"})が、目的地に到達する前に途中のスナップ位置で
  // アニメーションを打ち切ってしまうことがある(CSS Scroll SnapとProgrammatic Scrolling
  // の相互作用に関する既知の実装差異)。この場合、goTo()内でcurrentIndexは即座に
  // 目的のindexへ更新済みだが実際のスクロール位置は途中アイテムのまま止まってしまい、
  // その後onScroll()の再同期(スクロール位置からindexを再計算するデバウンス処理)が
  // currentIndexを実際に止まった途中アイテムのindexへ巻き戻してしまう
  // (＝表示アイテムとcurrentIndexは一致するが、ユーザーが意図した「1枚目に戻る」操作が
  // 完了しない)。ラップアラウンドの1ステップ分だけbehavior: "auto"(アニメーションなしの
  // 瞬間スクロール)にすることで、複数スナップポイントを跨ぐsmoothスクロール自体を
  // 発生させず、この競合を回避する。
  function goNext() {
    const wrapping = currentIndex === viewItems.length - 1;
    goTo(nextIndex(currentIndex, viewItems.length), wrapping ? "auto" : "smooth");
  }

  function goPrev() {
    const wrapping = currentIndex === 0;
    goTo(prevIndex(currentIndex, viewItems.length), wrapping ? "auto" : "smooth");
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
  // ただしgoTo()によるプログラム的スクロールが進行中(programmaticScrollActive)の間は、
  // アニメーション途中の中間scrollLeftから誤ったindexを拾ってしまわないよう何もしない
  // (beginProgrammaticScroll/onScrollEndのコメント参照)。
  let scrollEndTimer: ReturnType<typeof setTimeout> | undefined;
  function onScroll() {
    clearTimeout(scrollEndTimer);
    scrollEndTimer = setTimeout(() => {
      if (!scrollEl || programmaticScrollActive) return;
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

  // <media-player>(node)配下に<media-provider>がレンダリングする実際の<video>要素は
  // ロード処理が非同期のためマウント直後にはまだ存在しないことがある。MutationObserverで
  // 出現を待ち、見つかり次第loadedmetadataを購読して実際のアスペクト比(videoWidth/videoHeight)
  // をvideoAspectRatiosへ反映する。既にメタデータ読み込み済み(videoWidth/videoHeightが
  // 取得可能)なケースにも対応するため、購読直後に一度読み取りを試みる。
  function trackVideoAspectRatio(node: HTMLElement, itemId: string) {
    let videoEl: HTMLVideoElement | null = null;

    function onLoadedMetadata() {
      if (videoEl && videoEl.videoWidth > 0 && videoEl.videoHeight > 0) {
        videoAspectRatios = { ...videoAspectRatios, [itemId]: videoEl.videoWidth / videoEl.videoHeight };
      }
    }

    function attach() {
      const found = node.querySelector("video");
      if (!found || found === videoEl) return;
      videoEl?.removeEventListener("loadedmetadata", onLoadedMetadata);
      videoEl = found;
      videoEl.addEventListener("loadedmetadata", onLoadedMetadata);
      onLoadedMetadata();
    }

    attach();
    const observer = new MutationObserver(attach);
    observer.observe(node, { childList: true, subtree: true });

    return {
      destroy() {
        videoEl?.removeEventListener("loadedmetadata", onLoadedMetadata);
        observer.disconnect();
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
    onscrollend={onScrollEnd}
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
          {#if mediaLoadError[item.id]}
            <p class="text-sm text-white">画像を読み込めませんでした</p>
          {:else}
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
                onerror={() => (mediaLoadError = { ...mediaLoadError, [item.id]: true })}
              ></cropper-image>
            </cropper-canvas>
          {/if}
        {:else if isVideo(item)}
          {#if mediaLoadError[item.id]}
            <p class="text-sm text-white">動画を読み込めませんでした</p>
          {:else}
            <div class="flex h-full w-full items-center justify-center p-4">
              <media-player
                src={{ src: item.url, type: item.mimeType }}
                title={fileName(item)}
                playsinline
                crossorigin
                class="max-h-full max-w-full"
                style={`aspect-ratio: ${videoAspectRatios[item.id] ?? DEFAULT_VIDEO_ASPECT_RATIO}`}
                use:trackVideoAspectRatio={item.id}
                onerror={() => (mediaLoadError = { ...mediaLoadError, [item.id]: true })}
              >
                <!-- videoPanzoomは<media-player>全体ではなく<media-provider>(実際の映像が
                     描画される要素)にだけ適用する。Panzoomは常時(操作していない時も)
                     transform: scale(...) translate(...)をelemへインラインstyleで
                     設定しており、transform:none以外の値は仕様上そのelemに新しい
                     スタッキングコンテキストを作る。以前<media-player>ごと(=<media-video-
                     layout>のコントロールバーも含めて)panzoomの対象にしていたときは、
                     その新しいスタッキングコンテキストの中にコントロールバー(z-index:10)
                     ごと閉じ込められてしまい、コンテナ外にある左右送りクリックエリア
                     (前後送り、絶対配置)との重なり順比較にこのz-index:10が反映されず、
                     DOM順で後に来る左右送りクリックエリアが動画+コントロールバーを
                     まとめて覆ってクリックを奪っていた(前回の閉じるボタン問題と同根)。
                     <media-provider>だけをpanzoom対象にすることでコントロールバー
                     (MediaControlBar、.media-ctrl-overlayでz-index:1)をこの
                     スタッキングコンテキストの外に出し、クリックエリアと正しく
                     比較されるようにする。 -->
                <media-provider use:videoPanzoom>
                  <!-- 映像クリックで再生/一時停止をトグルする。MediaGrid.svelteと同じく
                       Vidstack公式の<media-gesture>プリミティブを使う(自前clickハンドラは
                       書かない)。videoPanzoomは<media-provider>にpointerdown/wheelしか
                       listenしない(node_modules/@panzoom/panzoom)のに対し、<media-gesture>は
                       pointerupで購読する(Gesture#attachListener、dev/chunks/
                       vidstack-C7VnVlv2.js参照)ため競合しない。左右送りクリックエリア
                       (画面端15%幅)は<media-player>の外側(モーダル直下)にあり、同一
                       スタッキングコンテキスト内でDOM順が<media-gesture>の対象
                       (<media-provider>)より後に来るため、その範囲では引き続き
                       クリックエリアが優先される(中央70%でのみジェスチャーが機能すれば
                       十分という要件どおり)。 -->
                  <media-gesture event="pointerup" action="toggle:paused"></media-gesture>
                </media-provider>
                <!-- panzoom-exclude: 上記の構造変更によりMediaControlBarは
                     videoPanzoomの対象(<media-provider>)の子孫ではなくなったため、
                     Panzoomのpointerdownハンドラは基本的にもう発火しないはずだが、
                     念のため残しておく(害はなく、将来の構造変更に対する保険として機能する)。
                     MediaGridのサムネイルと同じ自前コントロールバー(MediaControlBar)を
                     使う。フルスクリーン表示なのでsize="large"、MediaViewer自体が既に
                     拡大表示なのでonExpandは渡さない(拡大表示ボタンは出さない)。 -->
                <div class="panzoom-exclude">
                  <MediaControlBar file={item} variant="video" size="large" showFullscreenButton />
                </div>
              </media-player>
            </div>
          {/if}
        {:else}
          {#if mediaLoadError[item.id]}
            <p class="text-sm text-white">音声を読み込めませんでした</p>
          {:else}
            <div class="w-full max-w-md px-4">
              <media-player
                src={{ src: item.url, type: item.mimeType }}
                title={fileName(item)}
                crossorigin
                onerror={() => (mediaLoadError = { ...mediaLoadError, [item.id]: true })}
              >
                <media-provider></media-provider>
                <MediaControlBar file={item} variant="audio" size="large" />
              </media-player>
            </div>
          {/if}
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
        class="inline-flex size-9 items-center justify-center rounded-full bg-black/40 text-white opacity-0 transition-opacity group-hover:opacity-100"
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
        class="inline-flex size-9 items-center justify-center rounded-full bg-black/40 text-white opacity-0 transition-opacity group-hover:opacity-100"
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

  /* Vidstackのmedia-playerは既定でwidth:100%(横幅いっぱいに広がる仕様)。これが
     videoPanzoomのラッパーdiv(高さ100%、横幅ほぼビューワー全体)の横幅をそのまま
     埋めてしまい、画像ビューワー(Cropper.js、余白を持って中央表示)と違って画面端から
     端まで巨大に広がって見えていた(不具合1)。
     aspect-ratio自体は各<media-player>のstyle属性(インラインstyle)で個別に指定する
     ようにした。Vidstackは実際の動画の内在解像度(videoWidth/videoHeight)を自動で
     CSSへ反映する仕組みを持たない(state.mediaWidth/mediaHeightはレンダリング後の箱の
     offsetWidth/offsetHeightであり内在解像度ではない)ため、trackVideoAspectRatio
     アクション(<script>参照)で実際の<video>要素のloadedmetadataを購読し、
     videoAspectRatios[item.id]へ動的に反映している(読み込み中はDEFAULT_VIDEO_ASPECT_RATIO
     =16:9をフォールバックとして使う)。インラインstyleは常にこのstylesheetのルールより
     優先されるため、固定16:9指定はここでは行わない。
     width:auto + height:100% + max-width:100%にすることで、表示領域の高さを基準に
     (インラインstyleで指定された)アスペクト比を保った箱を計算し、横にはみ出す場合のみ
     max-widthでクランプする。はみ出さない場合はラッパーdivのflex(items-center
     justify-center)により左右・上下に余白を持って中央表示される。
     コントロールのグラデーションオーバーレイ(MediaControlBarの.media-ctrl-overlay)は
     media-player自身を基準にposition:absoluteで重ねられているため、この箱のサイズを
     正しく合わせることでオーバーレイのズレ(不具合2)も連動して解消する。 */
  :global(media-player[data-view-type="video"]) {
    width: auto;
    height: 100%;
    max-width: 100%;
  }

  /* 主な修正は<script>側で行った(videoPanzoomの対象を<media-player>全体から
     <media-provider>だけに変更し、コントロールバー(MediaControlBar)をtransformによる
     スタッキングコンテキストの外に出した)。コントロールバー自体はMediaControlBar.svelteの
     .media-ctrl-overlayでz-index:1を持っており、これが左右送りクリックエリア
     (z-index未指定)より確実に手前に来る。 */

  /* <media-gesture>(映像クリックで再生/一時停止)の判定領域サイズ・位置。MediaGrid.svelteと
     同じ理由(デフォルトテーマ未使用のためbase.cssにサイズ指定が無い)で、<media-provider>
     いっぱいに広げる。pointer-events: noneはVidstack本体がonAttachで付与済み。 */
  :global([data-media-gesture]) {
    position: absolute;
    inset: 0;
  }
</style>
