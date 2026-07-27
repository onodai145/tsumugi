<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { highlightCode } from "../lib/shiki";

  let { code, lang }: { code: string; lang: string | null } = $props();

  let html = $state<string | null>(null);

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

<div class="mfm-codeblock">
  {#if html}
    {@html html}
  {:else}
    <pre class="shiki-plain"><code>{code}</code></pre>
  {/if}
</div>
