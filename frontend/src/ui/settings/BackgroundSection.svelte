<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import {
    BACKGROUND_FIT_MODE_OPTIONS,
    BACKGROUND_FIT_MODE_OPTIONS_FOR_VIDEO,
    type BackgroundFitMode,
  } from "../../lib/backgroundFitMode";
  import { BACKGROUND_POSITION_GRID, type BackgroundPosition } from "../../lib/backgroundPosition";
  import { Button } from "$lib/components/ui/button";
  import { convertFileSrc } from "@tauri-apps/api/core";

  let backgroundKind = $state<"image" | "video" | null>(app.ui.backgroundKind ?? null);
  let backgroundPath = $state<string | null>(app.ui.backgroundPath ?? null);
  let backgroundDim = $state(app.ui.backgroundDim ?? 0);
  let backgroundBlur = $state(app.ui.backgroundBlur ?? 0);
  let columnOpacity = $state(app.ui.columnOpacity ?? 100);
  let backgroundFitMode = $state<BackgroundFitMode>(
    (app.ui.backgroundFitMode as BackgroundFitMode) ?? "cover",
  );
  let backgroundPosition = $state<BackgroundPosition>(
    (app.ui.backgroundPosition as BackgroundPosition) ?? "center",
  );
  let pickingImage = $state(false);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  const previewUrl = $derived(backgroundPath ? convertFileSrc(backgroundPath) : "");
  const fitModeOptions = $derived(
    backgroundKind === "video" ? BACKGROUND_FIT_MODE_OPTIONS_FOR_VIDEO : BACKGROUND_FIT_MODE_OPTIONS,
  );

  // 動画プレビュー用のblob: URL。WebKitGTK(GStreamerバックエンド)の<video>要素は
  // Tauriのasset://カスタムURIスキームを直接読めない(store.svelte.tsの#applyBackground /
  // #loadBackgroundVideoBlobと同じ理由)ため、fetch()で取得したBlobから作ったblob: URLを使う。
  let videoPreviewUrl = $state("");
  $effect(() => {
    if (backgroundKind !== "video" || !backgroundPath) {
      videoPreviewUrl = "";
      return;
    }
    let blobUrl = "";
    let cancelled = false;
    fetch(convertFileSrc(backgroundPath))
      .then((r) => r.blob())
      .then((blob) => {
        if (cancelled) return;
        blobUrl = URL.createObjectURL(blob);
        videoPreviewUrl = blobUrl;
      })
      .catch(() => {
        if (!cancelled) videoPreviewUrl = "";
      });
    // backgroundPath が変わった/このコンポーネントが破棄された際に、前のblob: URLを解放する。
    return () => {
      cancelled = true;
      if (blobUrl) URL.revokeObjectURL(blobUrl);
    };
  });

  // 背景メディアの基準点（9点グリッド、Issue #76）。position→アクセシブルラベル。
  const positionLabels: Record<BackgroundPosition, string> = {
    "top-left": "左上",
    top: "上",
    "top-right": "右上",
    left: "左",
    center: "中央",
    right: "右",
    "bottom-left": "左下",
    bottom: "下",
    "bottom-right": "右下",
  };

  async function pickMedia() {
    err = null;
    pickingImage = true;
    try {
      const media = await app.pickBackgroundMedia();
      if (media) {
        backgroundKind = media.kind;
        backgroundPath = media.absolutePath;
        // 動画選択時、直前がTileだった場合はUIから選択肢が消えるためcoverへ揃える。
        if (media.kind === "video" && backgroundFitMode === "tile") backgroundFitMode = "cover";
      }
    } catch (e) {
      err = String(e);
    } finally {
      pickingImage = false;
    }
  }

  function clearMedia() {
    backgroundKind = null;
    backgroundPath = null;
  }

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      // このセクションが編集しないフィールド(表示・テーマ等)を保存で消さないよう、
      // 現在の app.ui をベースに編集項目だけ上書きする。
      await app.setUiPrefs({
        ...app.ui,
        backgroundKind,
        backgroundPath,
        backgroundDim,
        backgroundBlur,
        columnOpacity,
        backgroundFitMode,
        backgroundPosition,
      });
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h3 class="mb-3.5 mt-0 text-base font-semibold">背景</h3>

<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">背景メディア（画像/動画）</span>
  <div class="flex items-center gap-2.5">
    {#if backgroundKind === "image" && previewUrl}
      <img class="h-9 w-14 rounded-md border border-border object-cover" src={previewUrl} alt="背景プレビュー" />
    {:else if backgroundKind === "video" && videoPreviewUrl}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video class="h-9 w-14 rounded-md border border-border object-cover" src={videoPreviewUrl} muted autoplay loop playsinline></video>
    {/if}
    <Button type="button" variant="outline" size="sm" disabled={pickingImage} onclick={pickMedia}>
      {pickingImage ? "読み込み中…" : backgroundKind ? "メディアを変更" : "画像/動画を選択"}
    </Button>
    {#if backgroundKind}
      <Button type="button" variant="outline" size="sm" onclick={clearMedia}>解除</Button>
    {/if}
  </div>
</div>

{#if backgroundKind}
  <div class="mb-3 flex flex-col gap-1.5 text-sm">
    <span class="text-muted-foreground">背景画像の配置方法</span>
    <div class="inline-flex w-fit overflow-hidden rounded-md border border-border">
      {#each fitModeOptions as m (m.value)}
        <button
          type="button"
          class={backgroundFitMode === m.value
            ? "border-r border-border bg-primary px-3.5 py-1.5 text-sm text-primary-foreground last:border-r-0"
            : "border-r border-border bg-muted px-3.5 py-1.5 text-sm text-foreground last:border-r-0"}
          onclick={() => (backgroundFitMode = m.value)}
        >
          {m.label}
        </button>
      {/each}
    </div>
  </div>
  {#if backgroundFitMode !== "fill"}
    <div class="mb-3 flex flex-col gap-1.5 text-sm">
      <span class="text-muted-foreground">基準点</span>
      <div class="grid w-fit grid-cols-[repeat(3,28px)] grid-rows-[repeat(3,28px)] gap-1">
        {#each BACKGROUND_POSITION_GRID as p (p)}
          <button
            type="button"
            class={backgroundPosition === p
              ? "h-[28px] w-[28px] rounded border border-primary bg-primary p-0"
              : "h-[28px] w-[28px] rounded border border-border bg-muted p-0 hover:border-primary"}
            title={positionLabels[p]}
            aria-label={positionLabels[p]}
            onclick={() => (backgroundPosition = p)}
          ></button>
        {/each}
      </div>
    </div>
  {/if}
  <label class="mb-2.5 flex flex-col gap-1 text-sm">
    <span class="text-muted-foreground">背景の暗さ({backgroundDim}%)</span>
    <input class="w-full max-w-[320px] accent-primary" type="range" min="0" max="100" step="5" bind:value={backgroundDim} />
  </label>
  <label class="mb-2.5 flex flex-col gap-1 text-sm">
    <span class="text-muted-foreground">背景のぼかし({backgroundBlur}px)</span>
    <input class="w-full max-w-[320px] accent-primary" type="range" min="0" max="40" step="2" bind:value={backgroundBlur} />
  </label>
  <label class="mb-2.5 flex flex-col gap-1 text-sm">
    <span class="text-muted-foreground">カラムの不透明度({columnOpacity}%)</span>
    <input class="w-full max-w-[320px] accent-primary" type="range" min="60" max="100" step="5" bind:value={columnOpacity} />
  </label>
  <p class="mb-4 mt-0 text-xs text-muted-foreground">数値が低いほど背景画像が透けて見えます。</p>
{/if}

<div class="flex items-center justify-end gap-3">
  {#if saved}<span class="text-sm text-[var(--success)]">保存しました</span>{/if}
  <Button type="button" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</Button>
</div>
{#if err}<p class="mt-2 mb-0 text-sm text-destructive">{err}</p>{/if}
