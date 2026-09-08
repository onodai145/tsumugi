<script lang="ts">
  import type { Snippet } from "svelte";
  import { extractAvgColorFromBlurhash } from "../lib/blurhash";

  let {
    isCat = false,
    avatarBlurhash = null,
    class: className = "",
    children,
  }: {
    isCat?: boolean;
    avatarBlurhash?: string | null;
    class?: string;
    children: Snippet;
  } = $props();

  const earColor = $derived(extractAvgColorFromBlurhash(avatarBlurhash) ?? "var(--border)");
</script>

<span class="avatar-frame relative inline-block {className}">
  <!-- 本家Misskey(MkAvatar.vue)は .inner(アバター画像側)に z-index:1 を明示することで、
       .ears(position:absoluteだがz-index未指定=auto)より確実に手前に描画されるようにしている。
       このラッパーが無いと、position:absoluteな.earsは「非positioned要素は常にpositioned要素より
       背面」というCSSのスタッキング規則により、通常配置(position:static)のアバター画像より
       手前に描画されてしまい、耳が顔を覆ってしまう(Issue #41フォローアップで判明)。 -->
  <span class="avatar-content relative z-[1] block h-full w-full">
    {@render children()}
  </span>
  {#if isCat}
    <span class="ears" style="color: {earColor}" aria-hidden="true">
      <span class="ear-left"></span>
      <span class="ear-right"></span>
    </span>
  {/if}
</span>

<style>
  /* Misskey本家 MkAvatar.vue の .cat > .ears 相当を移植。%ベースなので
     avatar-frame のサイズ(呼び出し側の class で決まる)に自動追従する。 */
  .ears {
    /* Tailwindのpreflightがグローバルに box-sizing: border-box を敷いているため、
       border-box のままだと width/height:100% + padding:50% でpaddingがボックス内に
       食い込み、content areaが0になって子要素(.ear-left/.ear-right)の% サイズ指定が
       すべて0pxに潰れる(本家Misskeyはこのトリックを content-box 前提で書いている)。
       明示的に content-box へ戻して耳を表示させる。 */
    box-sizing: content-box;
    contain: strict;
    position: absolute;
    top: -50%;
    left: -50%;
    width: 100%;
    height: 100%;
    padding: 50%;
    pointer-events: none;
    /* .ear-left/.ear-right はマークアップ上の改行に由来する空白テキストノードを挟むため、
       display:inline-block のまま横並びさせようとすると、その空白の幅ぶんだけ
       50%+50%のちょうど100%を超えて折り返され、耳が縦に積み重なって表示されてしまう
       (向きが90度回転して見えるバグの原因)。flexレイアウトにすると空白テキストノードは
       レイアウトに影響しないため、常に確実に横並びになる。 */
    display: flex;
  }
  .ear-left,
  .ear-right {
    contain: strict;
    display: inline-block;
    height: 50%;
    width: 50%;
    background: currentColor;
  }
  .ear-left::after,
  .ear-right::after {
    content: "";
    display: block;
    width: 60%;
    height: 60%;
    margin: 20%;
    background: #df548f;
  }
  .ear-left {
    transform: rotate(37.5deg) skew(30deg);
  }
  .ear-left,
  .ear-left::after {
    border-radius: 25% 75% 75%;
  }
  .ear-right {
    transform: rotate(-37.5deg) skew(-30deg);
  }
  .ear-right,
  .ear-right::after {
    border-radius: 75% 25% 75% 75%;
  }

  @keyframes earwiggleleft {
    from,
    to {
      transform: rotate(37.6deg) skew(30deg);
    }
    25% {
      transform: rotate(10deg) skew(30deg);
    }
    50% {
      transform: rotate(20deg) skew(30deg);
    }
    75% {
      transform: rotate(0deg) skew(30deg);
    }
  }
  @keyframes earwiggleright {
    from,
    to {
      transform: rotate(-37.6deg) skew(-30deg);
    }
    30% {
      transform: rotate(-10deg) skew(-30deg);
    }
    55% {
      transform: rotate(-20deg) skew(-30deg);
    }
    75% {
      transform: rotate(0deg) skew(-30deg);
    }
  }
  @media (prefers-reduced-motion: no-preference) {
    .avatar-frame:hover .ear-left {
      animation: earwiggleleft 1s infinite;
    }
    .avatar-frame:hover .ear-right {
      animation: earwiggleright 1s infinite;
    }
  }
</style>
