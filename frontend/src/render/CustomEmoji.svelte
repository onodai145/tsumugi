<script lang="ts">
  import { app } from "../lib/store.svelte";

  // カスタム絵文字。url があれば画像、無ければ :name: をテキスト表示（Phase 2: 絵文字辞書は未取得）
  // showTitle=false でネイティブツールチップを抑制（例: リアクションバッジは独自のホバー一覧を持つため不要）
  // 低帯域幅モード(Issue #591)では、url があっても画像を取得せず :name: をテキスト表示する。
  let { name, url, showTitle = true }: { name: string; url?: string; showTitle?: boolean } = $props();
</script>

{#if url && !app.ui.lowBandwidth}
  <img class="custom-emoji" src={url} alt={`:${name}:`} title={showTitle ? `:${name}:` : undefined} loading="lazy" />
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
