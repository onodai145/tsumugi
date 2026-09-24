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
    fileName,
    initialImageTransform,
    isImage,
    isRevealed,
    isVideo,
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

  let mediaLoadError = $state<Record<string, boolean>>({});

  // Vidstackはmedia-player自身のアスペクト比を実際の動画の内在解像度(videoWidth/videoHeight)
  // から自動算出する仕組みを持たない(state.mediaWidth/mediaHeightはレンダリング後の箱の
  // offsetWidth/offsetHeightであり、動画そのものの解像度ではない)。そのためtsumugi側で
  // 実際の<video>要素のloadedmetadataを購読し、アスペクト比を読み取る必要がある。
  // 読み込み中(まだ取得できていない)は16:9をフォールバックとして使う。
  const DEFAULT_VIDEO_ASPECT_RATIO = 16 / 9;
  let videoAspectRatios = $state<Record<string, number>>({});

  let scrollEl: HTMLDivElement | undefined;
  type CropperImageEl = HTMLElement & {
    $rotate: (a: number) => void;
    $scale: (x: number, y?: number) => void;
    $zoom: (s: number, x?: number, y?: number) => void;
    $resetTransform: () => void;
    $ready: (callback?: (image: HTMLImageElement) => unknown) => Promise<HTMLImageElement>;
  };
  // <cropper-image>へのbind:thisは、かつて`item.id === current.id`の時だけ単一の
  // cropperImageEl変数へ代入するgetter/setterだった。しかしScroll Snap構造上、
  // 全アイテムの<cropper-image>要素はビューワーの生存期間中ずっとマウントされたままで
  // 破棄・再生成されない(非表示アイテムも常にDOM上に残る)。bind:thisのsetterは
  // そのDOM要素自体が生成/破棄されたタイミングでしか発火しないため、あるアイテムの
  // 要素が最初にマウントされた時点でたまたま`current`でなければ、そのアイテムの
  // 要素は二度とcropperImageElに代入されない。結果として、cropperImageElは
  // マウント時に最初からcurrentだったアイテム(通常はstartIndexの画像)に永久に
  // 固定され、以降どの画像に切り替えてもまったく追従しなかった(実機のdebug_bridge
  // 経由の実測で、currentIndexが変わってもcropperImageElが指す要素のdata-item-idが
  // 一切変わらないことを確認・確定済み)。これにより回転/反転をリセットする
  // applyImageTransform()が常に間違った(多くの場合非表示の)要素に対して実行され、
  // 実際に表示中の画像側はCropper.jsが計算した初期フィット変形(scale≒0.8程度+
  // 中心からのtranslate)のまま一切リセットされず、縮小・位置ズレして見えていた
  // (task-4-report.mdの実測記録参照)。
  // 対策として、単一変数ではなくアイテムのid→要素のマップを保持し、全アイテムの
  // <cropper-image>に条件なしでbind:thisする(マウント時に登録、破棄時に削除)。
  // 実際に操作対象とする要素は、current(=$derived)が変わるたびに正しく再計算される
  // ようcurrentCropperImageElとして$derivedで導出する。
  let cropperImageElsByItemId = $state<Record<string, CropperImageEl>>({});
  const currentCropperImageEl = $derived(cropperImageElsByItemId[current.id]);

  // goTo()が発行したプログラム的スクロール(scrollTo)が進行中かどうか。trueの間は
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
  // 状態(=マウント直後、既にscrollLeft: 0)で呼んだ場合のようにscrollTo自体が実際には
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

  // scrollIntoView(要素のgetBoundingClientRect()を見て「もう見えているか」を判定してから
  // 目的地を決める仕組み)ではなく、常にscrollTo({left: index*clientWidth, ...})で目的の
  // スクロール位置を直接計算して指定する。実機のWebKitGTKで、直前のscrollIntoView呼び出し
  // (特にbehavior:"auto"の瞬間ジャンプ)から数十ms程度しか経っていない状態でもう一度
  // scrollIntoViewを呼ぶと、レイアウトがまだ再計算されておらずgetBoundingClientRect()が
  // 古い(スクロール前の)値を返すことがあり、その結果「目的の要素は既に表示範囲内」と
  // 誤判定されてスクロールが一切発生しないまま呼び出しが握りつぶされる事象を、
  // src-tauri/src/debug_bridge.rs経由でscrollLeftの実測値を追跡して確認した
  // (連打ではなく、ビューワーを開いた直後に1回だけ矢印を押すだけでも、開いた際の
  // マウント時スクロールと発生タイミングが近いと再現する)。scrollToは対象要素の
  // 現在のレイアウト状態を経由せず目的のスクロール位置を直接指定するため、この
  // 「古いレイアウト情報による誤判定」自体が起こり得ず、同じ条件で確実に目的位置まで
  // 到達することを同じ計測方法で確認済み。
  function goTo(index: number, behavior: ScrollBehavior = "smooth") {
    currentIndex = index;
    imageTransform = initialImageTransform;
    beginProgrammaticScroll();
    if (!scrollEl) return;
    scrollEl.scrollTo({ left: index * scrollEl.clientWidth, behavior });
  }

  // マウント時、scrollEl(横スクロールコンテナ)の初期表示位置がstartIndexとズレないよう
  // アニメーションなしで即座に同期させる(goTo()はgoNext/goPrev/矢印キーからしか呼ばれず、
  // マウント時には何もスクロールしないため、非ゼロstartIndexだと常に1枚目が表示されてしまう)。
  onMount(() => {
    beginProgrammaticScroll();
    if (!scrollEl) return;
    scrollEl.scrollTo({ left: currentIndex * scrollEl.clientWidth, behavior: "auto" });
  });

  // 末尾→先頭、先頭→末尾のラップアラウンド送りも含め、常にbehavior: "smooth"(goTo()の
  // デフォルト)で統一する。かつてはラップアラウンド時のみbehavior: "auto"(瞬時ジャンプ)に
  // する分岐があった(158c750)。これは「複数のScroll Snapポイントを跨ぐ長距離のsmooth
  // スクロールがWebKitGTK上で途中のスナップ位置で打ち切られる」という仮説に基づく修正
  // だったが、その後の実測調査(69b28a0、実際の原因はscrollIntoView()の古いレイアウト
  // 情報による誤判定だった)でこの仮説自体が誤りだったと判明している。むしろこの
  // 特別扱いこそが「2枚組ノートで前後送りが常にラップアラウンドになり、瞬時にカクッと
  // 切り替わってスムーズに見えない」というユーザー体感上の不具合の直接の原因だった
  // (goTo/beginProgrammaticScroll付近のコメント、およびtask-4-report.mdの実測記録参照)。
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
  // ただしgoTo()によるプログラム的スクロールが進行中(programmaticScrollActive)の間は、
  // アニメーション途中の中間scrollLeftから誤ったindexを拾ってしまわないよう何もしない
  // (beginProgrammaticScroll/onScrollEndのコメント参照)。
  let scrollEndTimer: ReturnType<typeof setTimeout> | undefined;
  // 余白(メディア本体以外)クリックで閉じる。画像のパン/ズームやシークバー操作の終わりにも
  // clickが発火するため、押下位置から一定以上動いていたらドラッグ操作とみなして閉じない。
  const BACKDROP_CLICK_MAX_MOVE_PX = 6;
  let pressStart: { x: number; y: number } | null = null;
  function onPagePointerDown(e: PointerEvent) {
    pressStart = { x: e.clientX, y: e.clientY };
  }
  function onPageClick(e: MouseEvent, item: (typeof viewItems)[number]) {
    if (pressStart && Math.hypot(e.clientX - pressStart.x, e.clientY - pressStart.y) > BACKDROP_CLICK_MAX_MOVE_PX) return;
    const target = e.target as Element;
    if (target.closest("media-player, button, a")) return;
    if (isImage(item)) {
      // <cropper-canvas>はページ全面を覆うため、画像本体の範囲内かどうかは矩形で判定する。
      const rect = cropperImageElsByItemId[item.id]?.getBoundingClientRect();
      if (rect && e.clientX >= rect.left && e.clientX <= rect.right && e.clientY >= rect.top && e.clientY <= rect.bottom) return;
    }
    onclose();
  }

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

  // $resetTransform()は仕様上、変形行列を文字通りの単位行列(scale:1、原点)へ戻すだけで、
  // Cropper.jsが画像読み込み完了時に自動で適用する「コンテナに収まるようフィットさせた上で
  // 中央寄せする」変形(初期表示で見られるscale≒0.8程度+中心オフセットのtranslate)を
  // 再現しない。cropper-image要素は読み込み完了時に一度だけ
  // `this.$center(this.initialCenterSize || this.initialFit)`を内部的に呼んでこの
  // フィット済み変形を計算する(node_modules/cropperjs/dist/cropper.esm.jsで確認済み、
  // 既定のinitialFitは'contain')が、この読み込み完了イベントは画像ごとに1度しか発生しない。
  // そのため、既に読み込み済みの画像に対してresetTransform()を呼んでしまうと、単位行列の
  // まま(フィットも中央寄せもされない、コンテナ左上を基準にした等倍表示)になり、
  // 二度とフィット済みの表示に戻らない(実機のdebug_bridge経由の実測で、既に表示済みの
  // 画像へ前後送りで切り替えた際にこの状態=画像が本来より狭い/左寄りの領域に表示される、
  // が再現することを確認した)。
  //
  // resetTransform()の直後にCropper.js自身の$center()を呼んでフィット済み変形を
  // 再計算させる対策も試したが、scaleは復元できてもtranslate(中央寄せ)の位置計算が
  // 期待値からずれる問題が残り、根本解決に至らなかった。
  //
  // 採用した方針: imageTransformが初期値(回転0、反転なし)ならresetTransform()以降を
  // 呼ばない(=Cropper.js自身が計算したフィット変形に一切触れない)。ただし単純に
  // 「imageTransformが初期値かどうか」だけを見ると別の不具合が生じる: imageTransformは
  // ビューワー全体で共有する単一のstateで、goTo()が前後送りのたびに無条件で初期値へ
  // リセットする。そのため「Aを回転→次へでBへ移動→前へでAに戻る」という操作をすると、
  // Aに戻った時点で(goTo()が既にimageTransformを初期値に戻し終えているため)
  // 「imageTransformが初期値かどうか」という条件だけでは「Aは一度も変形されていない」
  // 状態と区別がつかず、Aの回転がリセットされないまま(実際のDOM上の変形は回転した
  // ままなのに、ツールバーの表示上は未回転を示す)という状態になることを実機で確認した。
  // これを避けるため、「実際にresetTransform()以降を適用した(=変形した)アイテムのid」を
  // dirtiedItemIdsに記録しておき、imageTransformが初期値でも、そのアイテムがdirty
  // (前回何らかの変形を適用済みで、まだ後始末していない)なら引き続きresetTransform()
  // 以降を呼ぶ(=単位行列へ戻す。前述の通りフィットの再現はできないが、コーディネーター
  // の判断により「実際に変形操作をした画像に対する妥当な代償」として許容する)。
  // 後始末が済んだ(imageTransformが初期値の状態でresetTransform()を適用し終えた)時点で
  // dirtiedItemIdsから除去し、以降は「単に画像を切り替えただけ」の経路に戻る。
  const dirtiedCropperItemIds = new Set<string>();
  function applyImageTransform() {
    const el = currentCropperImageEl;
    if (!el) return;
    const isDefaultTransform =
      imageTransform.rotation === initialImageTransform.rotation &&
      imageTransform.flipH === initialImageTransform.flipH &&
      imageTransform.flipV === initialImageTransform.flipV;
    if (isDefaultTransform && !dirtiedCropperItemIds.has(current.id)) {
      return;
    }
    el.$resetTransform();
    el.$rotate(imageTransform.rotation);
    el.$scale(imageTransform.flipH ? -1 : 1, imageTransform.flipV ? -1 : 1);
    if (isDefaultTransform) {
      dirtiedCropperItemIds.delete(current.id);
    } else {
      dirtiedCropperItemIds.add(current.id);
    }
  }

  // imageTransform(rotation/flipH/flipV)自体に加え、currentCropperImageEl(=current.idが
  // 変わるたびに導出し直される、実際に表示中のアイテムのcropper-image要素)も依存に含める。
  // これにより、前後送りで表示中のアイテムが切り替わったタイミングでも、常に「実際に
  // 表示されている方の」要素に対してリセット/回転/反転が適用される。
  $effect(() => {
    void imageTransform;
    void currentCropperImageEl;
    applyImageTransform();
  });

  // ズーム時のみパンを有効化する。translatable属性は既定でoffにしておき、
  // Cropper.jsのtransformイベント(detail.matrix、[a,b,c,d,e,f]のCSS行列でaがX方向スケール)を
  // 見てscale>1の間だけonにする(等倍時にドラッグがスワイプナビゲーションと競合しないように)。
  // ontransformは各<cropper-image>要素自身に個別にバインドされているため、
  // e.currentTargetは常にイベントを発火させた(=操作されている)その要素そのものであり、
  // currentCropperImageElのような別変数の取り違えは原理上起こらない。
  function onCropperImageTransform(e: Event) {
    const detail = (e as CustomEvent<{ matrix: number[] }>).detail;
    const scale = detail.matrix[0];
    (e.currentTarget as CropperImageEl).toggleAttribute("translatable", scale > 1 + 1e-6);
  }

  // <cropper-image>のonerror属性は原理上発火しない: Cropper.jsは実際の<img>要素を自身の
  // shadow root配下にappendしており、そのimgのerrorイベントはbubbles: false/composed: false
  // (DOM標準のErrorEvent仕様どおり)のためshadow rootの外(ホスト要素である<cropper-image>
  // 自身)まで届かない。代わりにCropper.jsが公開する$ready()(画像の読み込み完了を待つ
  // Promiseを返し、読み込み失敗時はrejectする仕様、node_modules/cropperjs/dist/
  // cropper.esm.jsで確認済み)をフックする。
  function hookImageLoadError(el: CropperImageEl, itemId: string) {
    el.$ready().catch(() => {
      mediaLoadError = { ...mediaLoadError, [itemId]: true };
    });
  }

  // 動画はズーム時のみパンを有効化する(画像のCropper.jsと同じ方針)。videoPanzoomアクションは
  // Scroll Snapの各ページごとに要素が個別マウントされる(#each item.id)ため、アイテム切替時に
  // 別インスタンスとして再生成され、倍率リセットのための追加処理は不要。
  //
  // @panzoom/panzoomは初期化時、elem(node、<media-provider>)とelem.parentNode(=<media-player>)
  // の両方へ、渡したtouchActionオプションの値(既定は'none')をインラインstyleとして設定する
  // (node_modules/@panzoom/panzoom/dist/panzoom.jsで確認済み)。touchAction: 'none'のままだと、
  // 等倍(パン無効)時でもブラウザのネイティブな横スワイプ(Scroll Snapによる前後送り)が
  // タッチデバイス上で機能しなくなる。setOptions()にtouchActionを渡すと同じ2要素へ
  // 動的に再設定される仕様(同ファイルのsetOptions()実装で確認済み)なので、下の
  // syncDisablePan()でdisablePanと連動させ、パン無効(等倍)時のみtouchAction: 'pan-x'
  // (X方向はブラウザに渡し、Y方向は渡さない)にする。パン有効(ズーム済み)時は
  // touchAction: 'none'に切り替え、Panzoom自身にジェスチャーを処理させる。ここで
  // 'none'のままブラウザにも横方向のネイティブpan処理を渡してしまうと、JS側の
  // ジェスチャー処理とブラウザのネイティブpan処理が競合してpointercancelで機能しなく
  // なるIssue #296と同じ失敗パターンに陥るため、パン有効時は確実にPanzoom専有にする。
  function videoPanzoom(node: HTMLElement) {
    const pz: PanzoomObject = Panzoom(node, {
      maxScale: 4,
      disablePan: true,
      touchAction: "pan-x",
      cursor: "default",
    });
    const onWheel = (e: WheelEvent) => pz.zoomWithWheel(e);
    node.addEventListener("wheel", onWheel);

    // Panzoomは初期化時にelem(node、<media-provider>)へ無条件でcursor: 'move'を設定する。
    // しかし等倍(disablePan: true、実際にはドラッグでパンできない)状態でもこのカーソルが
    // 出続けるのは誤解を招くため、disablePanの切り替えに連動してcursorも同期する
    // (パン可能な時だけmove、そうでない時はブラウザ既定に戻す)。touchActionも同様に、
    // パン無効時はpan-x(X方向をブラウザに渡す)、パン有効時はnone(ブラウザには渡さず
    // Panzoom自身が処理する)へ連動させる(上のコメント参照)。
    function syncDisablePan() {
      const disablePan = pz.getScale() <= 1 + 1e-6;
      pz.setOptions({
        disablePan,
        cursor: disablePan ? "default" : "move",
        touchAction: disablePan ? "pan-x" : "none",
      });
    }
    node.addEventListener("panzoomzoom", syncDisablePan);
    syncDisablePan();

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
    onpointerdowncapture={onPagePointerDown}
    class="flex flex-1 snap-x snap-mandatory overflow-x-auto overflow-y-hidden [scrollbar-width:none]"
  >
    {#each viewItems as item (item.id)}
      <div
        class="flex h-full w-full flex-none snap-start items-center justify-center"
        data-testid="media-page"
        onclick={(e) => onPageClick(e, item)}
        role="presentation"
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
                  () => cropperImageElsByItemId[item.id],
                  (el) => {
                    if (el) {
                      cropperImageElsByItemId[item.id] = el;
                      hookImageLoadError(el, item.id);
                    } else {
                      delete cropperImageElsByItemId[item.id];
                    }
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
                <!-- MediaGridのサムネイルと同じ自前コントロールバー(MediaControlBar)を使う。
                     フルスクリーン表示なのでsize="large"、MediaViewer自体が既に拡大表示なので
                     onExpandは渡さない(拡大表示ボタンは出さない)。
                     旧実装はここに`panzoom-exclude`クラス付きのラッパーdivを持っていたが、
                     Panzoomの`isExcluded()`判定は対象要素(<media-provider>)からDOM祖先方向にのみ
                     遡る仕組みで、その兄弟であるこのdivをラップしても判定に一切関与しないため
                     (node_modules/@panzoom/panzoom確認済み)、実効性のないコードとして削除した。 -->
                <MediaControlBar file={item} variant="video" size="large" showFullscreenButton />
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
        class="inline-flex size-9 items-center justify-center rounded-full border border-transparent bg-clip-padding bg-black/40 text-white opacity-0 transition-opacity group-hover:opacity-100 focus-visible:border-ring focus-visible:opacity-100 focus-visible:ring-3 focus-visible:ring-ring/50"
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
        class="inline-flex size-9 items-center justify-center rounded-full border border-transparent bg-clip-padding bg-black/40 text-white opacity-0 transition-opacity group-hover:opacity-100 focus-visible:border-ring focus-visible:opacity-100 focus-visible:ring-3 focus-visible:ring-ring/50"
        onclick={(e) => { e.stopPropagation(); goNext(); }}
        aria-label="次へ"
      >
        <ChevronRight size={20} />
      </button>
    </div>
  {/if}

  {#if isRevealed(revealed, current) && isImage(current)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <!-- role="toolbar"はツールバー内の矢印キーによるフォーカス移動(roving tabindex)を
         期待する属性だが、このコンポーネントは各ボタンを個別にTab移動可能な素の
         <Button>として並べているだけで、それを実装していない。加えて矢印キーは
         svelte:window onkeydown(onKeydown、前後送り)がグローバルに処理しているため、
         role="toolbar"のまま実装すると両者が意味的に競合する。実装コストの低い方として
         role="toolbar"自体を外し、単なるボタングループ(role="group")として扱う。 -->
    <div
      class="relative z-10 flex flex-none items-center justify-center gap-1 p-[0.5rem_0.5rem_max(0.5rem,env(safe-area-inset-bottom))]"
      onclick={(e) => e.stopPropagation()}
      role="group"
      aria-label="画像ツールバー"
    >
      <Button variant="ghost" size="icon" class="text-white viewer-icon-btn hover:text-white" onclick={() => currentCropperImageEl?.$zoom(0.1)} aria-label="ズームイン"><ZoomIn size={16} /></Button>
      <Button variant="ghost" size="icon" class="text-white viewer-icon-btn hover:text-white" onclick={() => currentCropperImageEl?.$zoom(-0.1)} aria-label="ズームアウト"><ZoomOut size={16} /></Button>
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

  /* <media-gesture>(映像クリックで再生/一時停止)の判定領域サイズ・位置。MediaGrid.svelteと
     同じ理由(デフォルトテーマ未使用のためbase.cssにサイズ指定が無い)で、<media-provider>
     いっぱいに広げる。pointer-events: noneはVidstack本体がonAttachで付与済み。 */
  :global([data-media-gesture]) {
    position: absolute;
    inset: 0;
  }
</style>
