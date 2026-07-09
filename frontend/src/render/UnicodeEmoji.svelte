<script lang="ts">
  import { app } from "../lib/store.svelte";

  // 設定→表示の絵文字スタイル（native/Twemoji/Fluent Emoji）に従って1文字を描画する。
  let { char }: { char: string } = $props();

  const url = $derived(app.emojiImageUrl(char));
</script>

{#if url}
  <img class="unicode-emoji" src={url} alt={char} title={char} draggable="false" loading="lazy" />
{:else}
  {char}
{/if}

<style>
  .unicode-emoji {
    height: 1.15em;
    width: 1.15em;
    object-fit: contain;
    vertical-align: -0.2em;
  }
</style>
