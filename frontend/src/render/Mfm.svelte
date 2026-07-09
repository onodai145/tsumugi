<script lang="ts">
  import { parse, parseSimple } from "mfm-js";
  import MfmNode from "./MfmNode.svelte";

  // text: MFM原文 / emojis: カスタム絵文字 name->url（無ければ :name: 表示）
  // nyaize: 投稿者が isCat のとき本文を にゃん語化する（本家 :nyaize="'respect'" 相当）
  let {
    text,
    emojis = {},
    simple = false,
    nyaize = false,
  }: { text: string; emojis?: Record<string, string>; simple?: boolean; nyaize?: boolean } = $props();

  let nodes = $derived(text ? (simple ? parseSimple(text) : parse(text)) : []);
</script>

{#each nodes as node}
  <MfmNode {node} {emojis} {nyaize} />
{/each}
