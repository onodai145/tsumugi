<script lang="ts">
  // MediaGrid(サムネイル)とMediaViewer(フルスクリーン)の両方で使う、Vidstackの
  // デフォルトレイアウト(<media-video-layout>/<media-audio-layout>)を使わない自前の
  // コントロールバー。両者は「表示サイズが違うだけで構成・挙動は同一」のため、
  // このコンポーネントに共通化する(見た目の大小はsizeプロパティで切り替える)。
  import "vidstack/player/styles/base.css";
  import "vidstack/player";
  import "vidstack/player/ui";
  import type { MediaPlayerElement } from "vidstack/elements";
  import { Download, Maximize, Maximize2, Minimize, Pause, Play, Volume2, VolumeX } from "@lucide/svelte";
  import WaveSurfer from "wavesurfer.js";
  import { commands, unwrap } from "../lib/ipc";
  import { saveMediaToDisk } from "../lib/mediaDownload";
  import { app } from "../lib/store.svelte";
  import type { DriveFile } from "../bindings/tauri.gen";
  import { fileName } from "../lib/mediaViewer.svelte";

  let {
    file,
    variant,
    size = "compact",
    onExpand,
    showFullscreenButton = false,
  }: {
    file: DriveFile;
    /** video: 映像の下端に重ねるオーバーレイ。audio: 常時表示の通常フロー配置。 */
    variant: "video" | "audio";
    /** compact: MediaGridのサムネイル用。large: MediaViewerのフルスクリーン用。 */
    size?: "compact" | "large";
    /** 指定時のみ拡大表示ボタンを出す(MediaGrid専用。MediaViewer自体が拡大表示のため渡さない)。 */
    onExpand?: () => void;
    /** OSレベルのFullscreen API切替ボタン(<media-fullscreen-button>)を出すかどうか。
        MediaViewer(フルスクリーンビューワー)の動画にのみ渡す想定
        (MediaGridのサムネイルには既存の拡大表示ボタンがあるため不要、
        音声はフルスクリーンの概念が薄いため対象外)。 */
    showFullscreenButton?: boolean;
  } = $props();

  // style-guide.md §6: アイコンサイズはsize={12}(sm)/size={16}(default)/size={20}(例外的に大きめ)
  // の3段階のみで、13/14/15pxのような1px刻みの中間値は使わない。
  const iconSize = $derived(size === "large" ? 20 : 12);

  // 再生速度は1x/1.5x/2xを巡回させる自前トグル。Vidstackにはこの用途の既製部品
  // (<media-speed-*>系)は無く、あってもラジオグループ形式でトグルボタンではないため、
  // <media-player>のplaybackRateプロパティを直接読み書きする(公式APIどおり、
  // https://www.vidstack.io/docs/player/api/media-player#playbackrate)。
  const playbackRateSteps = [1, 1.5, 2] as const;
  let playbackRate = $state<number>(1);
  const cyclePlaybackRate = (e: MouseEvent) => {
    const player = (e.currentTarget as HTMLElement).closest("media-player") as MediaPlayerElement | null;
    // 対象の<media-player>が見つからない(マークアップ変更等で親子関係が崩れた)場合は、
    // 表示用stateだけ更新して実際の再生速度とズレるのを避けるため、何もせず抜ける。
    if (!player) return;
    const idx = playbackRateSteps.indexOf(playbackRate as (typeof playbackRateSteps)[number]);
    const next = playbackRateSteps[(idx + 1) % playbackRateSteps.length];
    player.playbackRate = next;
    playbackRate = next;
  };

  // <media-fullscreen-button>はFullscreen API呼び出し失敗(環境によって拒否される
  // ケースがある)を自前で捕捉し、ネイティブのunhandled rejectionにはせず<media-player>上で
  // "fullscreen-error"イベント(detail: Error、node_modules/vidstack/types/
  // vidstack-hVlf6lRD.d.ts の FullscreenErrorEvent)として通知する(vidstack本体の
  // FullscreenController#enter/PlayerCore#["media-enter-fullscreen-request"]参照)。
  // 設計ドキュメント(2026-09-19-media-viewer-migration-design.md §5)の方針どおり、
  // 例外を握りつぶしUIをクラッシュさせず、app.reportErrorでログするに留める。
  function reportFullscreenError(node: HTMLElement) {
    const player = node.closest("media-player") as MediaPlayerElement | null;
    if (!player) return {};
    const onFullscreenError = (e: Event) => {
      const detail = (e as CustomEvent<unknown>).detail;
      app.reportError(detail instanceof Error ? detail : new Error(String(detail)));
    };
    player.addEventListener("fullscreen-error", onFullscreenError);
    return {
      destroy() {
        player.removeEventListener("fullscreen-error", onFullscreenError);
      },
    };
  }

  // SoundCloud風の波形表示(音声のみ)。wavesurfer.jsを、Vidstackが内部生成する
  // 実際の<audio>要素(<media-player>の子孫の<media-provider>配下、trackVideoAspectRatio
  // アクションと同じ理由で非同期にレンダリングされうる)にmediaオプションでアタッチする
  // ことで、波形の描画のみを担当させつつ再生制御自体は既存のMediaControlBar/Vidstack側に
  // 委ねる(WaveSurfer側は別途独自の<audio>を生成しない)。
  //
  // WaveSurferは波形描画のため自前でfetch()して音声データをデコードするが、Misskeyの
  // ドライブファイル配信はAccess-Control-Allow-Originを返さないため、file.urlをそのまま
  // urlオプションに渡すと常にCORSで失敗する(<audio>要素自体の再生はCORS制約を受けない
  // ため、再生だけは正常に動いてしまい、波形だけ描画されないという形で気づきにくい)。
  // Rust側(fetch_url_as_base64、src-tauri/src/commands/note.rs)はブラウザのCORSに
  // 縛られないので、そちらでバイト列を取得してBlob化し、blob: URLとしてurlオプションに
  // 渡すことでCORSを回避する(blob: URLへのfetchはCORS対象外)。
  function attachWaveform(node: HTMLElement, mediaFile: DriveFile) {
    let audioEl: HTMLAudioElement | null = null;
    let ws: WaveSurfer | null = null;
    let blobUrl: string | null = null;
    let generation = 0;

    async function createWaveSurfer(el: HTMLAudioElement) {
      const myGeneration = ++generation;
      let waveformUrl = mediaFile.url;
      try {
        const base64 = await unwrap(commands.fetchUrlAsBase64(mediaFile.url));
        const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
        // BlobのtypeにmediaFile.mimeType(例: "audio/mpeg")を明示的に指定すると、
        // WebKitGTKでこのBlobを<audio>要素のsrcとして再生しようとした際に
        // MediaError(code=4, SRC_NOT_SUPPORTED)になる既知の癖がある(audio/mp3や
        // type未指定なら正常に再生できることを実機で確認済み)。型推定を
        // ブラウザに任せるためtypeを指定しない。
        const newBlobUrl = URL.createObjectURL(new Blob([bytes]));
        if (myGeneration !== generation) {
          // 取得中に別のcreateWaveSurfer呼び出しへ切り替わっていたら破棄する
          URL.revokeObjectURL(newBlobUrl);
          return;
        }
        if (blobUrl) URL.revokeObjectURL(blobUrl);
        blobUrl = newBlobUrl;
        waveformUrl = newBlobUrl;
      } catch (e) {
        app.reportError(e);
      }
      if (myGeneration !== generation) return;

      // 波形の色は--accentトークン経由にする(.media-seek-track/.media-seek-fillと
      // 同じ考え方)。ただしcanvasのfillStyleはCSSカスケードの外で解決されるため
      // var(--accent)をそのまま渡しても解決されない。getComputedStyleで実際の値を
      // 読み取ってからcolor-mix()に埋め込む。
      const accentColor = getComputedStyle(node).getPropertyValue("--accent").trim() || "currentColor";
      ws = WaveSurfer.create({
        container: node,
        media: el,
        url: waveformUrl,
        height: size === "large" ? 44 : 24,
        barWidth: 2,
        barGap: 1,
        barRadius: 1,
        cursorWidth: 0,
        waveColor: `color-mix(in srgb, ${accentColor} 30%, transparent)`,
        progressColor: accentColor,
      });
      ws.on("error", (e) => app.reportError(e));
    }

    function attach() {
      const player = node.closest("media-player");
      const found = (player?.querySelector("audio") as HTMLAudioElement | null) ?? null;
      if (!found || found === audioEl) return;
      audioEl = found;
      ws?.destroy();
      void createWaveSurfer(found);
    }

    attach();
    const player = node.closest("media-player");
    const observer = player ? new MutationObserver(attach) : null;
    if (player) observer?.observe(player, { childList: true, subtree: true });

    return {
      destroy() {
        generation++;
        observer?.disconnect();
        ws?.destroy();
        if (blobUrl) URL.revokeObjectURL(blobUrl);
      },
    };
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class={variant === "video" ? "media-ctrl-overlay" : "media-ctrl-overlay-audio"}
  onclick={(e) => e.stopPropagation()}
>
  {#if variant === "audio"}
    <!-- SoundCloud風の波形表示。クリックでのシークはwavesurfer.js標準機能(mediaオプション
         使用時、クリック位置に応じて接続した<audio>要素のcurrentTimeを更新する)をそのまま使う。 -->
    <div class={size === "large" ? "media-waveform media-waveform--lg" : "media-waveform"} use:attachWaveform={file}></div>
  {/if}
  <!-- YouTube風に、シークバー(上段の細い帯)とボタン行(下段)を<media-controls>1個に
       まとめて一体のオーバーレイにする。自動非表示はVidstack本体が<media-controls>に
       付与するdata-visible属性(マウスの動き・ホバー・一時停止中かどうかに応じて
       Vidstackが内部的にトグルする。types/core/controls.d.ts参照)をCSS属性セレクタで
       拾う形で実現し、デフォルトテーマ(.vds-controls)は使わない。 -->
  <media-controls class={size === "large" ? "media-ctrl-bar media-ctrl-bar--lg" : "media-ctrl-bar"}>
    <!-- media-time-slider は独自要素だが、内部の track/fill/thumb はVidstackの型定義例
         (types/elements/define/sliders/time-slider-element.d.ts)通りの素のdivで、
         位置はVidstack本体がホスト要素に設定する--slider-fill/--slider-progress
         CSS変数で決まる(下記styleブロック参照)。デフォルトテーマ(vds-slider*クラス)は
         使わず、色は--accentトークンを直接使用(style-guide.md準拠)。 -->
    <media-time-slider class="media-seek" aria-label="シーク">
      <div class="media-seek-track"></div>
      <div class="media-seek-fill"></div>
      <div class="media-seek-thumb"></div>
    </media-time-slider>
    <media-controls-group class="media-ctrl-row">
      <div class="media-ctrl-group-left">
        <media-play-button class="media-ctrl-btn" aria-label="再生/一時停止">
          <Play size={iconSize} class="media-icon-play" />
          <Pause size={iconSize} class="media-icon-pause" />
        </media-play-button>
        <!-- YouTubeデスクトップ版のようにミュートボタンにホバーした時だけ隣に音量スライダーが
             現れる形にする(常時横長スライダーを置くには幅の余裕がないため)。 -->
        <div class="media-vol-wrap">
          <media-mute-button class="media-ctrl-btn" aria-label="ミュート切替">
            <Volume2 size={iconSize} class="media-icon-volume" />
            <VolumeX size={iconSize} class="media-icon-mute" />
          </media-mute-button>
          <!-- media-volume-slider も media-time-slider と同じくtrack/track-fill/thumbの
               素のdivで構成する(types/elements/define/sliders/volume-slider-element.d.ts
               のdocコメント例どおり)。位置はホスト要素の--slider-fill CSS変数(0〜100%、
               現在の音量)で決まる。 -->
          <media-volume-slider class="media-vol-slider" aria-label="音量">
            <div class="media-vol-track"></div>
            <div class="media-vol-fill"></div>
            <div class="media-vol-thumb"></div>
          </media-volume-slider>
        </div>
        <!-- 現在時間/合計時間。<media-time>はtype属性で追跡対象を切り替える自己完結の
             表示専用プリミティブで、track/fill/thumb等の子要素は不要(テキストは
             Vidstack本体が自動的にtextContentへ書き込む。types/components/ui/time.d.ts
             のJSDoc例どおり子要素なしの単体タグで使う)。size="compact"(MediaGrid)では
             サムネイルが窮屈になるため非表示にし、size="large"(MediaViewer)でのみ
             表示する(下記CSS参照)。 -->
        <div class="media-time-group" aria-label="再生時間">
          <media-time type="current"></media-time>
          <span aria-hidden="true">/</span>
          <media-time type="duration"></media-time>
        </div>
      </div>
      <div class="media-ctrl-group-right">
        <button class="media-ctrl-btn media-ctrl-btn-rate" onclick={cyclePlaybackRate} aria-label="再生速度">
          {playbackRate}x
        </button>
        {#if showFullscreenButton}
          <!-- OSレベルのFullscreen API(標準の<media-fullscreen-button>プリミティブ)。
               既存の「拡大表示」ボタン(onExpand、MediaGrid→MediaViewerを開くボタン)とは別物。
               data-active(フルスクリーン中か)・data-supported(環境でFullscreen APIが
               使えるか、未サポートなら下記CSSでボタン自体を非表示にする)は
               types/components/ui/buttons/fullscreen-button.d.ts参照。 -->
          <media-fullscreen-button class="media-ctrl-btn" aria-label="フルスクリーン" use:reportFullscreenError>
            <Maximize size={iconSize} class="media-icon-fullscreen-enter" />
            <Minimize size={iconSize} class="media-icon-fullscreen-exit" />
          </media-fullscreen-button>
        {/if}
        {#if onExpand}
          <button class="media-ctrl-btn" onclick={onExpand} aria-label="拡大表示">
            <Maximize2 size={iconSize} />
          </button>
        {/if}
        <button
          class="media-ctrl-btn"
          onclick={() => saveMediaToDisk(file.url, fileName(file), (e) => app.reportError(e))}
          aria-label="保存"
        >
          <Download size={iconSize} />
        </button>
      </div>
    </media-controls-group>
  </media-controls>
</div>

<style>
  /* 動画は下端にYouTube風のオーバーレイ、音声は通常フローに配置する(音声はネイティブ
     controls相当の表示領域自体を持たないため、常時表示のまま)。 */
  .media-ctrl-overlay {
    position: absolute;
    right: 0;
    bottom: 0;
    left: 0;
    z-index: 1;
  }
  .media-ctrl-overlay-audio {
    position: static;
    /* <media-player>はdisplay:flex(既定row方向、MediaGrid.svelte/MediaViewer.svelte側で
       指定)のflexコンテナなので、flex-grow指定のないこの子要素はコンテンツ分の幅
       (ボタン列+波形の実測サイズ)にshrink-to-fitしてしまい、プレイヤー幅いっぱいに
       広がらない(左寄せで右側が余白になる不具合の原因だった)。widthを明示して埋める。 */
    width: 100%;
  }

  /* シークバー(1段目、上部の細い帯)とボタン行(2段目)を縦に並べた一体のオーバーレイ。
     背景に軽いグラデーションを敷き、動画の上でもボタンが視認できるようにする。 */
  :global(.media-ctrl-bar) {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    padding: 0.75rem 0.25rem 0.25rem;
    background: linear-gradient(to top, rgb(0 0 0 / 45%), transparent);
    /* 自動非表示: <media-controls>本体にVidstackが付与するdata-visible属性
       (マウス移動・ホバー・一時停止中かどうかに応じて自動トグルされる。
       node_modules配下のtypes/core/controls.d.tsに"@attr data-visible - Whether
       controls should be visible."とある)をCSS属性セレクタで拾う。デフォルトテーマ
       の.vds-controlsクラスは使わない。 */
    opacity: 0;
    /* Vidstack本体のControls#onAttach(dev/chunks/vidstack-C7VnVlv2.js 122行目付近)が
       <media-controls>要素(=このクラスがついたホスト要素)自身に常時インラインstyleで
       pointer-events: noneを設定するため、非!importantの外部CSSルールでは上書きできない。
       !importantが必須。 */
    pointer-events: none !important;
    transition: opacity 0.15s ease-in;
  }
  :global(.media-ctrl-bar[data-visible]) {
    opacity: 1;
    pointer-events: auto !important;
  }
  /* 音声プレイヤーはオーバーレイではなく常時表示領域なので、常に見える状態にする。
     実際にはVidstackは音声タイルでも同じhide/show監視を行っており「常にdata-visible=trueに
     固定される」わけではないが、ここではdata-visibleの値に関わらずopacity:1/pointer-events:autoを
     強制することで、映像を持たないタイルで自動非表示が発生しないようにしている。 */
  .media-ctrl-overlay-audio :global(.media-ctrl-bar) {
    opacity: 1 !important;
    pointer-events: auto !important;
    background: none;
    padding: 0;
  }
  :global(.media-ctrl-row) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.25rem;
  }
  :global(.media-ctrl-group-left),
  :global(.media-ctrl-group-right) {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }
  .media-ctrl-overlay-audio :global(.media-ctrl-row) {
    justify-content: center;
  }

  /* 現在時間/合計時間。size="compact"(MediaGrid)ではサムネイルが窮屈になるため既定で
     非表示にし、size="large"(MediaViewer、下記.media-ctrl-bar--lgブロック参照)でのみ
     表示する。色は--accentではなく他のコントロール(.media-ctrl-btn)と同じ固定の白
     (視認性確保のための慣習、指示どおり)。 */
  .media-time-group {
    display: none;
    align-items: baseline;
    gap: 0.125rem;
    color: white;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  /* シークバー。デフォルトテーマ(vds-slider*クラス)は使わず、track/fill/thumbを
     自前のdivで描画する(Vidstackの型定義例どおりの最小構成)。 */
  :global(.media-seek) {
    position: relative;
    display: block;
    width: 100%;
    height: 0.75rem;
    cursor: pointer;
    touch-action: none;
    /* <media-time-slider>は<media-controls-group>(ボタン行)の外に置いているため、
       ControlsGroup#onAttach(同dev/chunks/vidstack-C7VnVlv2.js 168行目付近、
       el.style.pointerEventsが未設定ならpointer-events: autoを付与するロジック)による
       上書きの恩恵を受けられず、親<media-controls>のインラインpointer-events: noneを
       そのまま継承してドラッグ/クリック操作が一切効かなくなる。ここで明示的に
       !important付きでauto指定して上書きする。 */
    pointer-events: auto !important;
  }
  :global(.media-seek-track) {
    position: absolute;
    top: 50%;
    left: 0;
    width: 100%;
    height: 3px;
    transform: translateY(-50%);
    border-radius: 9999px;
    /* 生カラーの直書きは避け、--accentトークンベースの半透明色にする
       (ReactionAcceptanceSelect.svelte等の color-mix(in srgb, var(--accent) N%, transparent) と同じパターン)。
       fill(下記、var(--accent)を不透明で使用)と視覚的に区別できるよう薄めの濃度にしている。 */
    background: color-mix(in srgb, var(--accent) 30%, transparent);
  }
  :global(.media-seek-fill) {
    position: absolute;
    top: 50%;
    left: 0;
    height: 3px;
    width: var(--slider-fill, 0%);
    transform: translateY(-50%);
    border-radius: 9999px;
    background: var(--accent);
  }
  :global(.media-seek-thumb) {
    position: absolute;
    top: 50%;
    left: var(--slider-fill, 0%);
    width: 0.5rem;
    height: 0.5rem;
    transform: translate(-50%, -50%);
    border-radius: 9999px;
    background: var(--accent);
    opacity: 0;
    transition: opacity 0.15s ease-in;
  }
  :global(media-time-slider[data-dragging] .media-seek-thumb),
  :global(media-time-slider[data-focus] .media-seek-thumb),
  :global(media-time-slider:hover .media-seek-thumb) {
    opacity: 1;
  }
  /* 生カラー(rgb(0 0 0 / 50%)背景・white文字)の直書きは、映像/音声という多様な背景の上に
     載るオーバーレイでテーマトークン(--accent等)に依存しない一定の視認性を確保する必要が
     あるための意図的な例外(.media-time-groupの同種コメント参照)。 */
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
  /* style-guide.md §7: Buttonプリミティブを経由しない素の<button>(再生速度・拡大表示・
     保存ボタン)には focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50
     相当のキーボードフォーカス視覚表示を明示的に持たせる必要がある。Tailwindクラスが
     使えない:globalなCSSコンテキストのため、同等のCSSプロパティで再現する。 */
  :global(.media-ctrl-btn:focus-visible) {
    outline: none;
    border: 1px solid var(--color-ring);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-ring) 50%, transparent);
  }

  /* 音量スライダー: 普段はwidth:0で折り畳んでおき、.media-vol-wrap(ミュートボタンと
     スライダーの共通の親)へのホバー、またはスライダー自体がドラッグ/フォーカス中の時だけ
     展開する(YouTubeデスクトップ版と同様のスライド表示)。<media-volume-slider>は
     <media-controls-group>(ミュートボタンと同じ.media-ctrl-group-left)の内側に置いているため、
     ControlsGroup#onAttach(vidstack-C7VnVlv2.js 166行目付近)がグループ自身にインラインで
     設定するpointer-events: autoをそのまま(pointer-eventsは継承プロパティのため)引き継ぐ。
     media-time-sliderのようにControlsGroupの外にある場合と異なり、!important上書きは不要。 */
  .media-vol-wrap {
    display: flex;
    align-items: center;
  }
  :global(.media-vol-slider) {
    position: relative;
    display: block;
    width: 0;
    height: 1.75rem;
    margin-left: 0;
    overflow: hidden;
    cursor: pointer;
    touch-action: none;
    opacity: 0;
    transition:
      width 0.15s ease,
      opacity 0.15s ease,
      margin-left 0.15s ease;
  }
  .media-vol-wrap:hover :global(.media-vol-slider),
  :global(.media-vol-slider[data-dragging]),
  :global(.media-vol-slider[data-focus]) {
    width: 3rem;
    opacity: 1;
    margin-left: 0.375rem;
  }
  :global(.media-vol-track) {
    position: absolute;
    top: 50%;
    left: 0;
    width: 100%;
    height: 3px;
    transform: translateY(-50%);
    border-radius: 9999px;
    background: color-mix(in srgb, var(--accent) 30%, transparent);
  }
  :global(.media-vol-fill) {
    position: absolute;
    top: 50%;
    left: 0;
    height: 3px;
    /* 音量の既定値はJSが--slider-fillを設定するまでの間、満音量(100%)を仮定しておく
       (再生位置は0%開始が自然だが、音量は通常フル状態で始まるため)。 */
    width: var(--slider-fill, 100%);
    transform: translateY(-50%);
    border-radius: 9999px;
    background: var(--accent);
  }
  :global(.media-vol-thumb) {
    position: absolute;
    top: 50%;
    left: var(--slider-fill, 100%);
    width: 0.5rem;
    height: 0.5rem;
    transform: translate(-50%, -50%);
    border-radius: 9999px;
    background: var(--accent);
  }

  /* 再生速度ボタンはアイコンではなく倍率テキスト表示のため、円形ではなく
     横幅可変の角丸ピルにする。 */
  :global(.media-ctrl-btn-rate) {
    width: auto;
    min-width: 1.75rem;
    padding: 0 0.375rem;
    border-radius: 9999px;
    /* style-guide.md §5の4段階(xs=0.75rem/sm=0.875rem/base=1rem/lg=1.125rem)に該当する
       値がないため、最も近いxs(0.75rem)に寄せる。 */
    font-size: 0.75rem;
    font-weight: 600;
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

  /* フルスクリーン中/非フルスクリーン中で表示アイコンを出し分ける。<media-fullscreen-button>
     本体のdata-active属性(フルスクリーン中かどうか)で切り替える。 */
  :global(media-fullscreen-button:not([data-active]) .media-icon-fullscreen-exit) {
    display: none;
  }
  :global(media-fullscreen-button[data-active] .media-icon-fullscreen-enter) {
    display: none;
  }
  /* Fullscreen API自体が環境で使えない(data-supported属性が付かない)場合は、
     押しても何も起きないボタンを出さないよう非表示にする。 */
  :global(media-fullscreen-button:not([data-supported])) {
    display: none;
  }

  /* size="large"(MediaViewer用): フルスクリーン表示なのでボタン・シークバー・
     音量スライダーを一回り大きくする。配色・配置パターン・自動非表示挙動は
     compact版(MediaGrid用)と共通のまま、サイズ関連のプロパティだけ上書きする。 */
  :global(.media-ctrl-bar--lg) {
    gap: 0.375rem;
    padding: 1.5rem 0.75rem 0.75rem;
  }
  :global(.media-ctrl-bar--lg .media-ctrl-row) {
    gap: 0.625rem;
  }
  :global(.media-ctrl-bar--lg .media-ctrl-group-left),
  :global(.media-ctrl-bar--lg .media-ctrl-group-right) {
    gap: 0.625rem;
  }
  :global(.media-ctrl-bar--lg .media-seek) {
    height: 1rem;
  }
  :global(.media-ctrl-bar--lg .media-seek-track),
  :global(.media-ctrl-bar--lg .media-seek-fill) {
    height: 4px;
  }
  :global(.media-ctrl-bar--lg .media-seek-thumb) {
    width: 0.75rem;
    height: 0.75rem;
  }
  :global(.media-ctrl-bar--lg .media-ctrl-btn) {
    height: 2.5rem;
    width: 2.5rem;
    font-size: 1rem;
  }
  :global(.media-ctrl-bar--lg .media-ctrl-btn-rate) {
    min-width: 2.5rem;
    padding: 0 0.625rem;
    font-size: 0.875rem;
  }
  :global(.media-ctrl-bar--lg .media-vol-slider) {
    height: 2.5rem;
  }
  :global(.media-ctrl-bar--lg .media-vol-wrap:hover .media-vol-slider),
  :global(.media-ctrl-bar--lg .media-vol-slider[data-dragging]),
  :global(.media-ctrl-bar--lg .media-vol-slider[data-focus]) {
    width: 4.5rem;
    margin-left: 0.5rem;
  }
  :global(.media-ctrl-bar--lg .media-vol-track),
  :global(.media-ctrl-bar--lg .media-vol-fill) {
    height: 4px;
  }
  :global(.media-ctrl-bar--lg .media-vol-thumb) {
    width: 0.75rem;
    height: 0.75rem;
  }
  :global(.media-ctrl-bar--lg) .media-time-group {
    display: flex;
    font-size: 0.875rem;
  }

  /* 波形表示コンテナ。wavesurfer.jsが内部にcanvasを生成する。クリックでのシークは
     wavesurfer.js標準機能。size="compact"(MediaGrid)/size="large"(MediaViewer)で
     高さを切り替える(実際の高さ指定はattachWaveformアクション内のheightオプション、
     ここではレイアウト上の余白のみ)。 */
  .media-waveform {
    width: 100%;
    cursor: pointer;
    margin-bottom: 0.25rem;
  }
  .media-waveform--lg {
    margin-bottom: 0.5rem;
  }
</style>
