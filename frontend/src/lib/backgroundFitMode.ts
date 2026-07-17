// 背景画像の配置方法（Issue #45）。Rust 側 UiPrefs.backgroundFitMode の文字列値と対応する。
export type BackgroundFitMode = "cover" | "contain" | "fill" | "tile";

export const BACKGROUND_FIT_MODE_CSS: Record<string, [size: string, repeat: string]> = {
  cover: ["cover", "no-repeat"],
  contain: ["contain", "no-repeat"],
  fill: ["100% 100%", "no-repeat"],
  tile: ["auto", "repeat"],
};

export const BACKGROUND_FIT_MODE_OPTIONS: { value: BackgroundFitMode; label: string }[] = [
  { value: "cover", label: "Cover（切り抜いて全面表示）" },
  { value: "contain", label: "Fit（全体を収める）" },
  { value: "fill", label: "Fill（縦横比を無視して引き伸ばし）" },
  { value: "tile", label: "Tile（並べて繰り返し）" },
];
