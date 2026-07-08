<script lang="ts">
  import type { Snippet } from "svelte";

  let { children }: { children: Snippet } = $props();

  const reduced =
    typeof window !== "undefined" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // 本家 MkSparkle 準拠: SVG 星型パーティクルを 500〜1000ms ごとに生成し、
  // 各パーティクルは 1000〜2000ms 生存して回転しながらスケールイン/アウトする。
  const STAR =
    "M29.427,2.011C29.721,0.83 30.782,0 32,0C33.218,0 34.279,0.83 34.573,2.011L39.455,21.646C39.629,22.347 39.991,22.987 40.502,23.498C41.013,24.009 41.653,24.371 42.354,24.545L61.989,29.427C63.17,29.721 64,30.782 64,32C64,33.218 63.17,34.279 61.989,34.573L42.354,39.455C41.653,39.629 41.013,39.991 40.502,40.502C39.991,41.013 39.629,41.653 39.455,42.354L34.573,61.989C34.279,63.17 33.218,64 32,64C30.782,64 29.721,63.17 29.427,61.989L24.545,42.354C24.371,41.653 24.009,41.013 23.498,40.502C22.987,39.991 22.347,39.629 21.646,39.455L2.011,34.573C0.83,34.279 0,33.218 0,32C0,30.782 0.83,29.721 2.011,29.427L21.646,24.545C22.347,24.371 22.987,24.009 23.498,23.498C24.009,22.987 24.371,22.347 24.545,21.646L29.427,2.011Z";
  const COLORS = ["#FF1493", "#00FFFF", "#FFE202", "#FFE202", "#FFE202"];

  interface Particle {
    id: number;
    x: number;
    y: number;
    size: number;
    duration: number;
    color: string;
  }

  let host = $state<HTMLElement | null>(null);
  let particles = $state<Particle[]>([]);
  let seq = 0;

  $effect(() => {
    if (reduced || !host) return;
    let timer: ReturnType<typeof setTimeout>;
    let alive = true;

    const add = () => {
      if (!alive || !host) return;
      const rect = host.getBoundingClientRect();
      const sizeFactor = Math.random();
      const size = 0.2 + (sizeFactor / 10) * 3;
      const duration = 1000 + sizeFactor * 1000;
      const p: Particle = {
        id: ++seq,
        // 文字領域内のランダム点（箱の中心をこの点に合わせる → margin で -32px 補正）
        x: Math.random() * rect.width,
        y: Math.random() * rect.height,
        size,
        duration,
        color: COLORS[Math.floor(Math.random() * COLORS.length)],
      };
      particles = [...particles, p];
      setTimeout(() => {
        particles = particles.filter((q) => q.id !== p.id);
      }, duration);
      timer = setTimeout(add, 500 + Math.random() * 500);
    };
    // 最初の生成も非同期に（同期実行すると particles 読取が effect 依存になり
    // 追加のたびに effect が再実行されてクリアされてしまう）。
    timer = setTimeout(add, 300 + Math.random() * 400);

    return () => {
      alive = false;
      clearTimeout(timer);
      particles = [];
    };
  });
</script>

<span class="mfm-sparkle" bind:this={host}>
  {@render children()}
  {#if !reduced}
    <span class="layer" aria-hidden="true">
      {#each particles as p (p.id)}
        <svg
          class="particle"
          viewBox="0 0 64 64"
          style={`left:${p.x}px;top:${p.y}px;--size:${p.size};animation-duration:${p.duration}ms;fill:${p.color}`}
        >
          <path d={STAR} />
        </svg>
      {/each}
    </span>
  {/if}
</span>

<style>
  .mfm-sparkle {
    position: relative;
    display: inline-block;
  }
  .layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
    overflow: visible;
  }
  .particle {
    position: absolute;
    width: 64px;
    height: 64px;
    /* 64px 箱の中心を left/top の座標に合わせる（transform はアニメが使うため margin で補正） */
    margin: -32px 0 0 -32px;
    transform: scale(0);
    animation-name: mfm-sparkle-particle;
    animation-timing-function: linear;
    animation-iteration-count: 1;
  }
  @keyframes mfm-sparkle-particle {
    0% {
      transform: rotate(0deg) scale(0);
    }
    50% {
      transform: rotate(180deg) scale(var(--size));
    }
    100% {
      transform: rotate(360deg) scale(0);
    }
  }
</style>
