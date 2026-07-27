// 設定UIのカード一覧に出す shiki 同梱テーマの一覧（表示名 + shikiのテーマID + プレビュー用スワッチ）。
// swatch は設定UI選定時、実際にインストール済みの shiki 4.3.1 から
// h.getTheme(id) で background/foreground/keyword.control のトークン色を抽出した実測値
// （bg=editor.background, fg=editor.foreground, accent=keyword系トークンの最初の一致）。
export const BUNDLED_SHIKI_THEMES: { id: string; label: string; swatch: { bg: string; fg: string; accent: string } }[] = [
  { id: "github-dark", label: "GitHub Dark", swatch: { bg: "#24292e", fg: "#e1e4e8", accent: "#f97583" } },
  { id: "github-light", label: "GitHub Light", swatch: { bg: "#ffffff", fg: "#24292e", accent: "#d73a49" } },
  { id: "dracula", label: "Dracula", swatch: { bg: "#282a36", fg: "#f8f8f2", accent: "#ff79c6" } },
  { id: "nord", label: "Nord", swatch: { bg: "#2e3440", fg: "#d8dee9", accent: "#81a1c1" } },
  { id: "one-dark-pro", label: "One Dark Pro", swatch: { bg: "#282c34", fg: "#abb2bf", accent: "#c678dd" } },
  { id: "monokai", label: "Monokai", swatch: { bg: "#272822", fg: "#f8f8f2", accent: "#f92672" } },
  { id: "solarized-dark", label: "Solarized Dark", swatch: { bg: "#002b36", fg: "#839496", accent: "#859900" } },
  { id: "solarized-light", label: "Solarized Light", swatch: { bg: "#fdf6e3", fg: "#657b83", accent: "#859900" } },
  { id: "tokyo-night", label: "Tokyo Night", swatch: { bg: "#1a1b26", fg: "#a9b1d6", accent: "#bb9af7" } },
  { id: "catppuccin-mocha", label: "Catppuccin Mocha", swatch: { bg: "#1e1e2e", fg: "#cdd6f4", accent: "#cba6f7" } },
  { id: "catppuccin-latte", label: "Catppuccin Latte", swatch: { bg: "#eff1f5", fg: "#4c4f69", accent: "#8839ef" } },
  { id: "min-dark", label: "Min Dark", swatch: { bg: "#1f1f1f", fg: "#b392f0", accent: "#f97583" } },
  { id: "min-light", label: "Min Light", swatch: { bg: "#ffffff", fg: "#24292e", accent: "#d32f2f" } },
];
