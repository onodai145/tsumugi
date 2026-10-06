<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { highlightCode } from "../lib/shiki";

  let { code, lang }: { code: string; lang: string | null } = $props();

  let html = $state<string | null>(null);
  let el = $state<HTMLDivElement>();

  // WebKitGTK では、overflow:auto な <pre> を含むカードを遅延レイアウトに任せると、スクロール
  // バー分の高さが親に反映されず、フッターがスクロールバーに重なったまま固まる（Issue #166）。
  // DOM挿入の直後（マウント時とハイライト反映時）に同期でレイアウトを強制すると数フレーム後に解消する。
  $effect(() => {
    html;
    void el?.offsetHeight;
  });

  $effect(() => {
    const currentCode = code;
    const currentLang = lang;
    const themeSelection = app.ui.codeHighlightTheme ?? "auto";
    const customSyntaxThemes = app.ui.customSyntaxThemes ?? [];
    let cancelled = false;
    highlightCode(currentCode, currentLang, themeSelection, customSyntaxThemes).then((result) => {
      if (!cancelled) html = result;
    });
    return () => {
      cancelled = true;
    };
  });
</script>

<div class="mfm-codeblock" bind:this={el}>
  {#if html}
    {@html html}
  {:else}
    <pre class="shiki-plain"><code>{code}</code></pre>
  {/if}
</div>
