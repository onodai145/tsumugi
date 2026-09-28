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

// object-fit は background-size と構文が異なる("fill"の縦横比無視表現がキーワード自体)ため、
// 動画の<video>要素向けに別途マッピングする。
export const BACKGROUND_FIT_MODE_OBJECT_FIT: Record<"cover" | "contain" | "fill", "cover" | "contain" | "fill"> = {
  cover: "cover",
  contain: "contain",
  fill: "fill",
};

// Tile(並べて繰り返し)は動画では意味を持たないため、動画選択時のUIからは除外する。
export const BACKGROUND_FIT_MODE_OPTIONS_FOR_VIDEO = BACKGROUND_FIT_MODE_OPTIONS.filter(
  (o) => o.value !== "tile",
);
