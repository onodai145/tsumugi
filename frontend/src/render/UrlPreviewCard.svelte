<script lang="ts">
  import type { UrlPreview } from "../bindings/tauri.gen";
  import { cachedUrlPreview, fetchUrlPreview } from "../lib/urlPreview";

  let { url }: { url: string } = $props();

  let preview = $state<UrlPreview | null | undefined>(cachedUrlPreview(url));
  let revealed = $state(false);
  let playing = $state(false);

  function isSafeUrl(url: string): boolean {
    return /^https?:\/\//i.test(url);
  }

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

{#if preview}
  <div class="url-preview-card mt-2 overflow-hidden rounded-md border border-border text-sm">
    {#if preview.thumbnail || (preview.player && isSafeUrl(preview.player.url))}
      <div class="relative aspect-[21/9] w-full">
        {#if preview.sensitive && !revealed}
          <button
            type="button"
            class="sensitive-cover h-full w-full border-0 text-sm text-muted-foreground"
            onclick={() => (revealed = true)}
          >
            閲覧注意（クリックで表示）
          </button>
        {:else if playing && preview.player && isSafeUrl(preview.player.url)}
          <iframe
            src={preview.player.url}
            title={preview.title ?? preview.url}
            sandbox="allow-scripts allow-same-origin"
            class="h-full w-full border-0"
          ></iframe>
        {:else}
          {#if preview.thumbnail}
            <img src={preview.thumbnail} alt="" loading="lazy" class="h-full w-full object-cover" />
          {/if}
          {#if preview.player && isSafeUrl(preview.player.url)}
            <button
              type="button"
              class="play-button absolute inset-0 flex items-center justify-center border-0 bg-black/30 text-2xl text-white"
              onclick={() => (playing = true)}
              aria-label="再生"
            >
              ▶
            </button>
          {/if}
        {/if}
      </div>
    {/if}
    {#if isSafeUrl(preview.url)}
      <a
        class="block px-2 py-1.5 text-foreground no-underline"
        href={preview.url}
        target="_blank"
        rel="noreferrer noopener"
      >
        {@render cardContent(preview)}
      </a>
    {:else}
      <div class="block px-2 py-1.5 text-foreground">
        {@render cardContent(preview)}
      </div>
    {/if}
  </div>
{/if}

<style>
  .sensitive-cover {
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  }
</style>
