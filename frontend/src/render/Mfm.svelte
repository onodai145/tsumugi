<script lang="ts">
  import { parse, parseSimple } from "mfm-js";
  import MfmNode from "./MfmNode.svelte";

  // text: MFM原文 / emojis: カスタム絵文字 name->url（無ければ :name: 表示）
  let { text, emojis = {}, simple = false }: { text: string; emojis?: Record<string, string>; simple?: boolean } = $props();

  let nodes = $derived(text ? (simple ? parseSimple(text) : parse(text)) : []);
</script>

{#each nodes as node}
  <MfmNode {node} {emojis} />
{/each}
