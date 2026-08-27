<script lang="ts">
  import type { UrlPreview } from "../bindings/tauri.gen";
  import { cachedUrlPreview, fetchUrlPreview, isSafeUrl } from "../lib/urlPreview";
  import { proxiedImageUrl } from "../lib/emoji";

  let { url, instanceHost }: { url: string; instanceHost: string | undefined } = $props();

  let preview = $state<UrlPreview | null | undefined>(cachedUrlPreview(url));
  let revealed = $state(false);
  let playing = $state(false);

  $effect(() => {
    if (preview !== undefined) return;
    let cancelled = false;
    fetchUrlPreview(url).then((p) => {
      if (!cancelled) preview = p;
    });
    return () => {
      cancelled = true;
    };
  });
</script>

{#snippet cardContent(preview: UrlPreview)}
  {#if preview.sitename}<div class="truncate text-xs text-muted-foreground">{preview.sitename}</div>{/if}
  {#if preview.title}<div class="line-clamp-1 font-semibold">{preview.title}</div>{/if}
  {#if preview.description}<div class="line-clamp-2 text-xs text-muted-foreground">{preview.description}</div>{/if}
{/snippet}

{#snippet linkOrDiv(preview: UrlPreview, classes: string)}
  {#if isSafeUrl(preview.url)}
    <a class={classes} href={preview.url} target="_blank" rel="noreferrer noopener">
      {@render cardContent(preview)}
    </a>
  {:else}
    <div class={classes}>
      {@render cardContent(preview)}
    </div>
  {/if}
{/snippet}

{#if preview}
  {@const hasPlayer = preview.player && isSafeUrl(preview.player.url)}
  <div class="url-preview-card mt-2 w-full max-w-[480px] overflow-hidden rounded-md border border-border text-sm">
    {#if playing && hasPlayer && preview.player}
      <!-- 再生中: 縦長レイアウトに展開し、大きいiframeで再生する。summalyのplayer.width/heightが
           あれば実際の比率で、無ければ動画の一般的な比率(16:9)にフォールバックする
           （固定比率だと実際のプレイヤーと縦横比が合わず引き伸ばされて見えるため）。 -->
      <div
        class="preview-media relative w-full"
        style="aspect-ratio: {preview.player.width && preview.player.height
          ? `${preview.player.width} / ${preview.player.height}`
          : '16 / 9'}"
      >
        <iframe
          src={preview.player.url}
          title={preview.title ?? preview.url}
          sandbox="allow-scripts allow-same-origin"
          class="h-full w-full border-0"
        ></iframe>
      </div>
      {@render linkOrDiv(preview, "block px-2 py-1.5 text-foreground no-underline")}
    {:else}
      <!-- 通常時: 横長レイアウト。TLの縦方向の占有を抑えるため、サムネイルは小さい正方形に固定する -->
      <div class="flex items-stretch">
        {#if preview.thumbnail || hasPlayer}
          <div class="preview-thumb relative aspect-square h-20 w-20 shrink-0">
            {#if preview.sensitive && !revealed}
              <button
                type="button"
                class="sensitive-cover h-full w-full border-0 text-xs text-muted-foreground"
                onclick={() => (revealed = true)}
              >
                閲覧注意
              </button>
            {:else}
              {#if preview.thumbnail}
                <img
                  src={instanceHost ? proxiedImageUrl(preview.thumbnail, instanceHost) : preview.thumbnail}
                  alt=""
                  loading="lazy"
                  class="h-full w-full object-cover"
                />
              {/if}
              {#if hasPlayer}
                <button
                  type="button"
                  class="play-button absolute inset-0 flex items-center justify-center border-0 bg-black/30 text-lg text-white"
                  onclick={() => (playing = true)}
                  aria-label="再生"
                >
                  ▶
                </button>
              {/if}
            {/if}
          </div>
        {/if}
        {@render linkOrDiv(preview, "min-w-0 flex-1 px-2 py-1.5 text-foreground no-underline")}
      </div>
    {/if}
  </div>
{/if}

<style>
  /* 幅広カラムでaspect-ratioのまま伸び続けないよう、MediaGrid（Issue #8）と同じ
     --media-thumbnail-height（設定→表示で調整可能、既定200px）で高さの上限を揃える。
     プレイヤー再生中（縦長展開時）のみ使う。 */
  .preview-media {
    max-height: var(--media-thumbnail-height, 200px);
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
  }
  .preview-thumb {
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
  }
  .sensitive-cover {
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  }
</style>
