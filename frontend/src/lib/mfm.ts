// MFM 関数 `$[name.args ...]` を inline style / class に落とす。
// 値・キーフレームは Misskey 本家 packages/frontend/src/components/global/MkMfm.ts
// および style.scss の @keyframes に準拠する（mfm-js はパースのみで描画は関知しないため）。
// keyframes は app.css（グローバル）側に定義。

type Args = Record<string, string | boolean>;

const HEX = /^[0-9a-fA-F]{3,6}$/;
function validColor(v: unknown): string | null {
  return typeof v === "string" && HEX.test(v) ? v : null;
}
function safeFloat(v: unknown): number | null {
  if (typeof v !== "string" || v === "") return null;
  const n = parseFloat(v);
  return Number.isFinite(n) ? n : null;
}
// 本家と同じ検証: 符号 + 数字 + 単位 "s"（例 "1.5s"）のみ許容。
function validTime(v: unknown): string | null {
  return typeof v === "string" && /^-?[0-9.]+s$/.test(v) ? v : null;
}

// inline style は CSS の prefers-reduced-motion メディアクエリで上書きできないため、
// ここで判定してアニメーション自体を付与しない（静的な装飾は残す）。
function prefersReducedMotion(): boolean {
  return typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export interface MfmStyle {
  class: string;
  style: string;
}

const KNOWN_FN = new Set([
  "tada",
  "jelly",
  "twitch",
  "shake",
  "spin",
  "jump",
  "bounce",
  "rainbow",
  "flip",
  "x2",
  "x3",
  "x4",
  "font",
  "blur",
  "rotate",
  "position",
  "scale",
  "fg",
  "bg",
  "border",
]);

/// 未対応の関数名か（本家準拠: 未対応なら `$[name ...]` をそのまま表示する）。
export function isKnownFn(name: string): boolean {
  return KNOWN_FN.has(name);
}

const FONTS: Record<string, string> = {
  serif: "serif",
  monospace: "ui-monospace, monospace",
  cursive: "cursive",
  fantasy: "fantasy",
  emoji: "emoji",
  math: "math",
};

const BORDER_STYLES = new Set([
  "hidden",
  "dotted",
  "dashed",
  "solid",
  "double",
  "groove",
  "ridge",
  "inset",
  "outset",
]);

/// fn ノードの name/args を描画用の class/style へ。未対応は空（子要素だけ描画される）。
/// 呼び出し側（MfmNode.svelte）が全ケース共通で `display:inline-block` を付与する。
export function mfmFn(name: string, args: Args = {}): MfmStyle {
  let style = "";
  let cls = "";
  const reduced = prefersReducedMotion();

  switch (name) {
    case "tada": {
      const speed = validTime(args.speed) ?? "1s";
      const delay = validTime(args.delay) ?? "0s";
      style = `font-size:150%;${
        reduced ? "" : `animation:mfm-tada ${speed} linear infinite both;animation-delay:${delay}`
      }`;
      break;
    }
    case "jelly": {
      if (reduced) break;
      const speed = validTime(args.speed) ?? "1s";
      const delay = validTime(args.delay) ?? "0s";
      style = `animation:mfm-rubberBand ${speed} linear infinite both;animation-delay:${delay}`;
      break;
    }
    case "twitch": {
      if (reduced) break;
      const speed = validTime(args.speed) ?? "0.5s";
      const delay = validTime(args.delay) ?? "0s";
      style = `animation:mfm-twitch ${speed} ease infinite;animation-delay:${delay}`;
      break;
    }
    case "shake": {
      if (reduced) break;
      const speed = validTime(args.speed) ?? "0.5s";
      const delay = validTime(args.delay) ?? "0s";
      style = `animation:mfm-shake ${speed} ease infinite;animation-delay:${delay}`;
      break;
    }
    case "spin": {
      if (reduced) break;
      const direction = args.left ? "reverse" : args.alternate ? "alternate" : "normal";
      const anim = args.x ? "mfm-spinX" : args.y ? "mfm-spinY" : "mfm-spin";
      const speed = validTime(args.speed) ?? "1.5s";
      const delay = validTime(args.delay) ?? "0s";
      style = `animation:${anim} ${speed} linear infinite;animation-direction:${direction};animation-delay:${delay}`;
      break;
    }
    case "jump": {
      if (reduced) break;
      const speed = validTime(args.speed) ?? "0.75s";
      const delay = validTime(args.delay) ?? "0s";
      style = `animation:mfm-jump ${speed} linear infinite;animation-delay:${delay}`;
      break;
    }
    case "bounce": {
      if (reduced) break;
      const speed = validTime(args.speed) ?? "0.75s";
      const delay = validTime(args.delay) ?? "0s";
      style = `animation:mfm-bounce ${speed} linear infinite;transform-origin:center bottom;animation-delay:${delay}`;
      break;
    }
    case "rainbow": {
      if (reduced) break;
      const speed = validTime(args.speed) ?? "1s";
      const delay = validTime(args.delay) ?? "0s";
      style = `animation:mfm-rainbow ${speed} linear infinite;animation-delay:${delay}`;
      break;
    }
    case "flip": {
      const t = args.h && args.v ? "scale(-1,-1)" : args.v ? "scaleY(-1)" : "scaleX(-1)";
      style = `transform:${t}`;
      break;
    }
    case "x2":
      style = "font-size:2em";
      break;
    case "x3":
      style = "font-size:3em";
      break;
    case "x4":
      style = "font-size:4em";
      break;
    case "font": {
      const key = Object.keys(FONTS).find((k) => args[k]);
      if (key) style = `font-family:${FONTS[key]}`;
      break;
    }
    case "blur":
      cls = "mfm-blur";
      break;
    case "rotate": {
      const deg = safeFloat(args.deg) ?? 90;
      style = `transform:rotate(${deg}deg);transform-origin:center center`;
      break;
    }
    case "position": {
      const x = safeFloat(args.x) ?? 0;
      const y = safeFloat(args.y) ?? 0;
      style = `transform:translateX(${x}em) translateY(${y}em)`;
      break;
    }
    case "scale": {
      const x = Math.min(safeFloat(args.x) ?? 1, 5);
      const y = Math.min(safeFloat(args.y) ?? 1, 5);
      style = `transform:scale(${x},${y})`;
      break;
    }
    case "fg": {
      const color = validColor(args.color) ?? "f00";
      style = `color:#${color};overflow-wrap:anywhere`;
      break;
    }
    case "bg": {
      const color = validColor(args.color) ?? "f00";
      style = `background-color:#${color};overflow-wrap:anywhere`;
      break;
    }
    case "border": {
      const color = validColor(args.color);
      const c = color ? `#${color}` : "var(--accent)";
      const bStyle =
        typeof args.style === "string" && BORDER_STYLES.has(args.style) ? args.style : "solid";
      const width = safeFloat(args.width) ?? 1;
      const radius = safeFloat(args.radius) ?? 0;
      style = `border:${width}px ${bStyle} ${c};border-radius:${radius}px;${
        args.noclip ? "" : "overflow:clip;"
      }`;
      break;
    }
    default:
      break;
  }

  return { class: cls, style };
}
