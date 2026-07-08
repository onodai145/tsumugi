<script lang="ts">
  import type { MfmNode } from "mfm-js";
  import Self from "./MfmNode.svelte";
  import CustomEmoji from "./CustomEmoji.svelte";
  import Sparkle from "./Sparkle.svelte";
  import { mfmFn, isKnownFn } from "../lib/mfm";

  let { node, emojis = {} }: { node: MfmNode; emojis?: Record<string, string> } = $props();

  // props はノード種別ごとに異なるため any 経由でアクセス
  const p = $derived((node as any).props ?? {});
  const children = $derived<MfmNode[]>((node as any).children ?? []);
  // fn ノード（$[name.args ...]）の装飾。本家は全 fn に一律 display:inline-block を付与する。
  const fn = $derived(node.type === "fn" ? mfmFn(p.name, p.args ?? {}) : { class: "", style: "" });
  const fnKnown = $derived(node.type !== "fn" || isKnownFn(p.name));

  // $[ruby base reading]。本家準拠: 子が単一テキストなら空白で base/rt を分割、
  // それ以外は末尾テキストを rt、残りを base として描画する。
  function rubyParts(cs: MfmNode[]): { base: MfmNode[]; baseText?: string; rt: string } {
    if (cs.length === 1 && cs[0].type === "text") {
      const t = String((cs[0] as any).props?.text ?? "");
      const sp = t.split(" ");
      return { base: [], baseText: sp[0], rt: sp[1] ?? "" };
    }
    const last = cs[cs.length - 1];
    const rt = last && last.type === "text" ? String((last as any).props?.text ?? "").trim() : "";
    return { base: cs.slice(0, -1), rt };
  }
  const ruby = $derived(
    node.type === "fn" && p.name === "ruby" ? rubyParts(children) : null,
  );

  // $[unixtime <epoch秒>]。子テキストの数値をローカル日時で表示する。
  const unixMs = $derived.by(() => {
    if (node.type !== "fn" || p.name !== "unixtime") return NaN;
    const t = parseInt(String((children[0] as any)?.props?.text ?? ""), 10);
    return Number.isFinite(t) ? t * 1000 : NaN;
  });
  const unixLabel = $derived(Number.isFinite(unixMs) ? new Date(unixMs).toLocaleString() : "");
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
  {#if ruby}
    <ruby>{#if ruby.baseText !== undefined}{ruby.baseText}{:else}{#each ruby.base as c}<Self node={c} {emojis} />{/each}{/if}<rt>{ruby.rt}</rt></ruby>
  {:else if p.name === "unixtime" && unixLabel}
    <span class="mfm-unixtime" title={unixLabel}>🕛 {unixLabel}</span>
  {:else if p.name === "sparkle"}
    <Sparkle>{#each children as c}<Self node={c} {emojis} />{/each}</Sparkle>
  {:else if p.name === "clickable"}
    <!-- プラグイン用イベント。機構が無いので中身のみ描画 -->
    {#each children as c}<Self node={c} {emojis} />{/each}
  {:else if fnKnown}
    <span class={fn.class} style={`display:inline-block;${fn.style}`}>{#each children as c}<Self node={c} {emojis} />{/each}</span>
  {:else}
    <!-- 未対応の MFM 関数: 本家準拠で $[name ...] をそのまま表示 -->
    <span>$[{p.name} {#each children as c}<Self node={c} {emojis} />{/each}]</span>
  {/if}
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
