<script lang="ts">
  import type { MfmNode } from "mfm-js";
  import Self from "./MfmNode.svelte";
  import CustomEmoji from "./CustomEmoji.svelte";

  let { node, emojis = {} }: { node: MfmNode; emojis?: Record<string, string> } = $props();

  // props はノード種別ごとに異なるため any 経由でアクセス
  const p = $derived((node as any).props ?? {});
  const children = $derived<MfmNode[]>((node as any).children ?? []);
</script>

{#if node.type === "text"}
  {#each String(p.text ?? "").split("\n") as line, i}{#if i > 0}<br />{/if}{line}{/each}
{:else if node.type === "bold"}
  <b>{#each children as c}<Self node={c} {emojis} />{/each}</b>
{:else if node.type === "italic"}
  <i>{#each children as c}<Self node={c} {emojis} />{/each}</i>
{:else if node.type === "strike"}
  <s>{#each children as c}<Self node={c} {emojis} />{/each}</s>
{:else if node.type === "small"}
  <small>{#each children as c}<Self node={c} {emojis} />{/each}</small>
{:else if node.type === "center"}
  <div style="text-align:center">{#each children as c}<Self node={c} {emojis} />{/each}</div>
{:else if node.type === "quote"}
  <blockquote class="mfm-quote">{#each children as c}<Self node={c} {emojis} />{/each}</blockquote>
{:else if node.type === "fn"}
  <!-- MFM関数(spin等)は装飾を省略し子要素のみ描画 -->
  <span>{#each children as c}<Self node={c} {emojis} />{/each}</span>
{:else if node.type === "url"}
  <a class="mfm-link" href={p.url} target="_blank" rel="noreferrer noopener">{p.url}</a>
{:else if node.type === "link"}
  <a class="mfm-link" href={p.url} target="_blank" rel="noreferrer noopener">{#each children as c}<Self node={c} {emojis} />{/each}</a>
{:else if node.type === "mention"}
  <span class="mfm-mention">{p.acct}</span>
{:else if node.type === "hashtag"}
  <span class="mfm-hashtag">#{p.hashtag}</span>
{:else if node.type === "emojiCode"}
  <CustomEmoji name={p.name} url={emojis[p.name]} />
{:else if node.type === "unicodeEmoji"}
  {p.emoji}
{:else if node.type === "inlineCode"}
  <code class="mfm-code">{p.code}</code>
{:else if node.type === "blockCode"}
  <pre class="mfm-codeblock"><code>{p.code}</code></pre>
{:else if node.type === "mathInline" || node.type === "mathBlock"}
  <code class="mfm-code">{p.formula}</code>
{:else if node.type === "search"}
  <span>{p.query}</span>
{:else if node.type === "plain"}
  {#each children as c}<Self node={c} {emojis} />{/each}
{:else}
  <!-- 未対応ノード: 子があれば描画 -->
  {#each children as c}<Self node={c} {emojis} />{/each}
{/if}
