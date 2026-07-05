<script lang="ts">
  // カスタム絵文字。url があれば画像、無ければ :name: をテキスト表示（Phase 2: 絵文字辞書は未取得）
  let { name, url }: { name: string; url?: string } = $props();
</script>

{#if url}
  <img class="custom-emoji" src={url} alt={`:${name}:`} title={`:${name}:`} loading="lazy" />
{:else}
  <span class="custom-emoji-fallback">:{name}:</span>
{/if}

<style>
  .custom-emoji {
    height: 1.4em;
    vertical-align: middle;
    object-fit: contain;
  }
  .custom-emoji-fallback {
    opacity: 0.75;
    font-size: 0.9em;
  }
</style>
