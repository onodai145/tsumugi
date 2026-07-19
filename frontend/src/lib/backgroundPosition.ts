// 背景画像の基準点（background-position）。9点グリッドから選択（Issue #76）。
// Rust 側 UiPrefs.backgroundPosition の文字列値と対応する。
export type BackgroundPosition =
  | "top-left"
  | "top"
  | "top-right"
  | "left"
  | "center"
  | "right"
  | "bottom-left"
  | "bottom"
  | "bottom-right";

export const BACKGROUND_POSITION_CSS: Record<string, string> = {
  "top-left": "left top",
  top: "center top",
  "top-right": "right top",
  left: "left center",
  center: "center center",
  right: "right center",
  "bottom-left": "left bottom",
  bottom: "center bottom",
  "bottom-right": "right bottom",
};

// 3x3グリッドUIの描画順（row-major: 左上から右下へ）。
export const BACKGROUND_POSITION_GRID: BackgroundPosition[] = [
  "top-left",
  "top",
  "top-right",
  "left",
  "center",
  "right",
  "bottom-left",
  "bottom",
  "bottom-right",
];
