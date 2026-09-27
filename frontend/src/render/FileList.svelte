<script lang="ts">
  import { Download } from "@lucide/svelte";
  import type { DriveFile } from "../bindings/tauri.gen";
  import { formatFileSize } from "../lib/fileSize";
  import { saveMediaToDisk } from "../lib/mediaDownload";
  import { fileName, isRevealed } from "../lib/mediaViewer.svelte";
  import { app } from "../lib/store.svelte";

  let {
    files,
    revealed,
    onreveal,
  }: {
    files: DriveFile[];
    revealed: Record<string, boolean>;
    onreveal: (f: DriveFile) => void;
  } = $props();

  const focusRing = "focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none";
</script>

{#if files.length > 0}
  <ul class="mt-2 flex flex-col gap-1">
    {#each files as f (f.id)}
      <li>
        {#if !isRevealed(revealed, f)}
          <button
            class="file-row w-full rounded-md border-0 px-2 py-2 text-left text-sm text-muted-foreground {focusRing}"
            onclick={() => onreveal(f)}
          >
            閲覧注意（クリックで表示）
          </button>
        {:else}
          <button
            class="file-row flex w-full items-center gap-2 rounded-md border-0 px-2 py-2 text-left text-sm {focusRing}"
            onclick={() => saveMediaToDisk(f.url, fileName(f), (e) => app.reportError(e))}
            aria-label={`${fileName(f)} を保存`}
          >
            <Download size={16} class="flex-none text-primary" />
            <span class="min-w-0 flex-1 truncate">{fileName(f)}</span>
            {#if formatFileSize(f.size)}
              <span class="flex-none text-xs text-muted-foreground">{formatFileSize(f.size)}</span>
            {/if}
          </button>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .file-row {
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
  }
</style>
