<script lang="ts">
  import "vidstack/player/styles/base.css";
  import "vidstack/player";
  import "vidstack/player/ui";
  import type { MediaPlayerElement } from "vidstack/elements";
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

  // 再生速度は1x/1.5x/2xを巡回させる自前トグル。Vidstackにはこの用途の既製部品
  // (<media-speed-*>系)は無く、あってもラジオグループ形式でトグルボタンではないため、
  // <media-player>のplaybackRateプロパティを直接読み書きする(公式APIどおり、
  // https://www.vidstack.io/docs/player/api/media-player#playbackrate)。
  const playbackRateSteps = [1, 1.5, 2] as const;
  let playbackRates = $state<Record<string, number>>({});
  const cyclePlaybackRate = (f: DriveFile, e: MouseEvent) => {
    const player = (e.currentTarget as HTMLElement).closest("media-player") as MediaPlayerElement | null;
    const current = playbackRates[f.id] ?? 1;
    const idx = playbackRateSteps.indexOf(current as (typeof playbackRateSteps)[number]);
    const next = playbackRateSteps[(idx + 1) % playbackRateSteps.length];
    playbackRates = { ...playbackRates, [f.id]: next };
    if (player) player.playbackRate = next;
  };
</script>

{#snippet mediaControls(f: DriveFile)}
  <!-- YouTube風に、シークバー(上段の細い帯)とボタン行(下段)を<media-controls>1個に
       まとめて一体のオーバーレイにする。自動非表示はVidstack本体が<media-controls>に
       付与するdata-visible属性(マウスの動き・ホバー・一時停止中かどうかに応じて
       Vidstackが内部的にトグルする。types/core/controls.d.ts参照)をCSS属性セレクタで
       拾う形で実現し、デフォルトテーマ(.vds-controls)は使わない(これまでの方針を踏襲)。 -->
  <media-controls class="media-ctrl-bar">
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
          <Play size={14} class="media-icon-play" />
          <Pause size={14} class="media-icon-pause" />
        </media-play-button>
        <media-mute-button class="media-ctrl-btn" aria-label="ミュート切替">
          <Volume2 size={14} class="media-icon-volume" />
          <VolumeX size={14} class="media-icon-mute" />
        </media-mute-button>
      </div>
      <div class="media-ctrl-group-right">
        <button class="media-ctrl-btn media-ctrl-btn-rate" onclick={(e) => cyclePlaybackRate(f, e)} aria-label="再生速度">
          {playbackRates[f.id] ?? 1}x
        </button>
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
      </div>
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
          <!-- src は文字列ではなく{src, type}オブジェクトで渡す(重要)。MisskeyのドライブファイルURLは
               拡張子を含まない(例: /files/webpublic-<uuid>)ため、Vidstackがsrc文字列だけからMIMEタイプを
               自動推定するinferType()が失敗し(常に"?"=unknown)、プロバイダ(<video>/<audio>要素)が
               一切生成されない不具合があった。typeを明示することで回避する。 -->
          <media-player src={{ src: f.url, type: f.mimeType }} viewType="video" playsinline preload="metadata" class="h-full w-full">
            <media-provider></media-provider>
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="media-ctrl-overlay" onclick={(e) => e.stopPropagation()}>
              {@render mediaControls(f)}
            </div>
          </media-player>
        {:else if isAudio(f)}
          <!-- svelte-ignore a11y_media_has_caption -->
          <media-player src={{ src: f.url, type: f.mimeType }} viewType="audio" preload="metadata" class="w-[calc(100%-16px)]">
            <media-provider></media-provider>
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="media-ctrl-overlay media-ctrl-overlay-audio" onclick={(e) => e.stopPropagation()}>
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

  /* 動画は下端にYouTube風のオーバーレイ、音声はセル内の通常フローに配置する
     (音声はネイティブcontrols相当の表示領域自体を持たないため、常時表示のまま)。 */
  .media-ctrl-overlay {
    position: absolute;
    right: 0;
    bottom: 0;
    left: 0;
    z-index: 1;
  }
  .media-ctrl-overlay-audio {
    position: static;
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
       の.vds-controlsクラスは使わない(既存方針を踏襲)。 */
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.15s ease-in;
  }
  :global(.media-ctrl-bar[data-visible]) {
    opacity: 1;
    pointer-events: auto;
  }
  /* 音声プレイヤーはオーバーレイではなく常時表示領域なので、data-visibleの
     自動非表示ロジックを無効化し常に見える状態にする。 */
  .media-ctrl-overlay-audio :global(.media-ctrl-bar) {
    opacity: 1;
    pointer-events: auto;
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

  /* シークバー。デフォルトテーマ(vds-slider*クラス)は使わず、track/fill/thumbを
     自前のdivで描画する(Vidstackの型定義例どおりの最小構成)。サムネイルの高さ制約
     (--media-thumbnail-height, 既定200px)に合わせて極力薄くしている。 */
  :global(.media-seek) {
    position: relative;
    display: block;
    width: 100%;
    height: 0.75rem;
    cursor: pointer;
    touch-action: none;
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

  /* 再生速度ボタンはアイコンではなく倍率テキスト表示のため、円形ではなく
     横幅可変の角丸ピルにする。 */
  :global(.media-ctrl-btn-rate) {
    width: auto;
    min-width: 1.75rem;
    padding: 0 0.375rem;
    border-radius: 9999px;
    font-size: 0.6875rem;
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
</style>
