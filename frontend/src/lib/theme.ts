// カスタムテーマ機能。app.css の7個のCSS変数(--surface-1/2/3, --border, --text,
// --text-dim, --accent)に対応する配色セット(ThemeColors, bindings/tauri.gen.ts参照)を
// プリセット or ユーザー定義(UiPrefs.customThemes)から選び、<html> に inline style として反映する。
import type { ThemeColors } from "../bindings/tauri.gen";

export interface ThemePreset {
  id: string;
  name: string;
  colors: ThemeColors;
}

// CSS変数名 <-> ThemeColors のフィールド名対応。
export const THEME_VAR_KEYS: { css: string; key: keyof ThemeColors }[] = [
  { css: "--surface-1", key: "surface1" },
  { css: "--surface-2", key: "surface2" },
  { css: "--surface-3", key: "surface3" },
  { css: "--border", key: "border" },
  { css: "--text", key: "text" },
  { css: "--text-dim", key: "textDim" },
  { css: "--accent", key: "accent" },
  { css: "--success", key: "success" },
  { css: "--info", key: "info" },
];

// 著名なエディタ/ターミナル配色を移植したプリセット(各テーマの公式パレットを参考に7トークンへ写像)。
// 注記(不確かな/独自解釈の部分):
// - Nordは公式には4つの色グループ(Polar Night=背景階調, Snow Storm=明背景/文字階調,
//   Frost=寒色アクセント, Aurora=暖色アクセント)であり、それぞれが独立した「テーマ」ではない。
//   ここでは依頼に合わせて「背景はPolar Night基調のまま、どの公式グループをアクセントに
//   使うか」で4種に分けた(Polar Night=Frost cyan, Snow Storm=明背景版, Frost=Frost blue,
//   Aurora=Aurora purple)。これは非公式の独自解釈。
// - Solarizedは公式8色(base03〜base3)に3段階目の背景トーンや境界線色の定義が無いため、
//   surface3は補間した近似色(コメントで明記)。
export const PRESETS: ThemePreset[] = [
  {
    id: "tokyo-night",
    name: "Tokyo Night",
    colors: {
      surface1: "#1a1b26",
      surface2: "#1f2335",
      surface3: "#292e42",
      border: "#3b4261",
      text: "#c0caf5",
      textDim: "#565f89",
      accent: "#7aa2f7",
      success: "#9ece6a",
      info: "#7dcfff",
    },
  },
  {
    id: "tokyo-night-storm",
    name: "Tokyo Night Storm",
    colors: {
      surface1: "#24283b",
      surface2: "#292e42",
      surface3: "#414868",
      border: "#3b4261",
      text: "#c0caf5",
      textDim: "#565f89",
      accent: "#7aa2f7",
      success: "#9ece6a",
      info: "#7dcfff",
    },
  },
  {
    id: "tokyo-night-light",
    name: "Tokyo Night Light",
    colors: {
      surface1: "#e1e2e7",
      surface2: "#d5d6db",
      surface3: "#c4c8da",
      border: "#a1a6c5",
      text: "#3760bf",
      textDim: "#848cb5",
      accent: "#2e7de9",
      success: "#587539",
      info: "#007197",
    },
  },
  {
    id: "dracula",
    name: "Dracula",
    colors: {
      surface1: "#282a36",
      surface2: "#2f313f",
      surface3: "#44475a",
      border: "#6272a4",
      text: "#f8f8f2",
      textDim: "#6272a4",
      accent: "#bd93f9",
      success: "#50fa7b",
      info: "#8be9fd",
    },
  },
  {
    id: "alucard",
    name: "Alucard (Dracula Light)",
    colors: {
      surface1: "#f8f8f2",
      surface2: "#f0f0ec",
      surface3: "#e6e6e6",
      border: "#d9d9d9",
      text: "#14192b",
      textDim: "#8a8a8a",
      accent: "#644ac9",
      success: "#1a8f4e",
      info: "#1c8db0",
    },
  },
  {
    id: "nord-polar-night",
    name: "Nord Polar Night",
    colors: {
      surface1: "#2e3440",
      surface2: "#3b4252",
      surface3: "#434c5e",
      border: "#4c566a",
      text: "#eceff4",
      textDim: "#d8dee9",
      accent: "#88c0d0",
      success: "#a3be8c",
      info: "#81a1c1",
    },
  },
  {
    id: "nord-snow-storm",
    name: "Nord Snow Storm",
    colors: {
      surface1: "#eceff4",
      surface2: "#e5e9f0",
      surface3: "#d8dee9",
      border: "#c3cad9",
      text: "#2e3440",
      textDim: "#4c566a",
      accent: "#5e81ac",
      success: "#4c7031",
      info: "#5e81ac",
    },
  },
  {
    id: "nord-frost",
    name: "Nord Frost",
    colors: {
      surface1: "#2e3440",
      surface2: "#3b4252",
      surface3: "#434c5e",
      border: "#4c566a",
      text: "#eceff4",
      textDim: "#d8dee9",
      accent: "#5e81ac",
      success: "#a3be8c",
      info: "#81a1c1",
    },
  },
  {
    id: "nord-aurora",
    name: "Nord Aurora",
    colors: {
      surface1: "#2e3440",
      surface2: "#3b4252",
      surface3: "#434c5e",
      border: "#4c566a",
      text: "#eceff4",
      textDim: "#d8dee9",
      accent: "#b48ead",
      success: "#a3be8c",
      info: "#81a1c1",
    },
  },
  {
    id: "catppuccin-mocha",
    name: "Catppuccin Mocha",
    colors: {
      surface1: "#1e1e2e",
      surface2: "#313244",
      surface3: "#45475a",
      border: "#585b70",
      text: "#cdd6f4",
      textDim: "#a6adc8",
      accent: "#cba6f7",
      success: "#a6e3a1",
      info: "#89b4fa",
    },
  },
  {
    id: "catppuccin-latte",
    name: "Catppuccin Latte",
    colors: {
      surface1: "#eff1f5",
      surface2: "#ccd0da",
      surface3: "#bcc0cc",
      border: "#acb0be",
      text: "#4c4f69",
      textDim: "#6c6f85",
      accent: "#8839ef",
      success: "#40a02b",
      info: "#1e66f5",
    },
  },
  {
    id: "catppuccin-frappe",
    name: "Catppuccin Frappé",
    colors: {
      surface1: "#303446",
      surface2: "#414559",
      surface3: "#51576d",
      border: "#626880",
      text: "#c6d0f5",
      textDim: "#a5adce",
      accent: "#ca9ee6",
      success: "#a6d189",
      info: "#8caaee",
    },
  },
  {
    id: "catppuccin-macchiato",
    name: "Catppuccin Macchiato",
    colors: {
      surface1: "#24273a",
      surface2: "#363a4f",
      surface3: "#494d64",
      border: "#5b6078",
      text: "#cad3f5",
      textDim: "#a5adcb",
      accent: "#c6a0f6",
      success: "#a6da95",
      info: "#8aadf4",
    },
  },
  {
    id: "solarized-dark",
    name: "Solarized Dark",
    colors: {
      surface1: "#002b36",
      surface2: "#073642",
      surface3: "#0a4552", // 補間(公式定義なし)
      border: "#586e75",
      text: "#839496",
      textDim: "#93a1a1",
      accent: "#268bd2",
      success: "#859900",
      info: "#2aa198",
    },
  },
  {
    id: "solarized-light",
    name: "Solarized Light",
    colors: {
      surface1: "#fdf6e3",
      surface2: "#eee8d5",
      surface3: "#e4ddc7", // 補間(公式定義なし)
      border: "#93a1a1",
      text: "#657b83",
      textDim: "#586e75",
      accent: "#268bd2",
      success: "#859900",
      info: "#2aa198",
    },
  },
];

export function findPreset(id: string): ThemePreset | undefined {
  return PRESETS.find((p) => p.id === id);
}

/// theme値("preset:<id>"/"custom:<id>")からIDを取り出す。該当形式でなければ null。
export function parseThemeRef(theme: string, prefix: "preset:" | "custom:"): string | null {
  return theme.startsWith(prefix) ? theme.slice(prefix.length) : null;
}

/// <html> に配色を反映する。null なら inline指定を全解除し app.css の既定(auto/light/dark)に戻す。
export function applyThemeColors(colors: ThemeColors | null) {
  const root = document.documentElement;
  for (const { css, key } of THEME_VAR_KEYS) {
    const value = colors?.[key];
    // success/info は追加前のプリセット/カスタムテーマでは未定義のことがあるため、
    // その場合は inline指定を外して app.css の既定色(--success/--info)へフォールバックさせる。
    if (value) root.style.setProperty(css, value);
    else root.style.removeProperty(css);
  }
}
