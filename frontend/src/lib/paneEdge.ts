import type { Edge } from "../bindings/tauri.gen";

/// 要素内でのポインタ位置(offsetX/Y)から、ドロップ先のエッジ(Left/Right/Top/Bottom)を
/// 判定する。4辺までの距離のうち、幅/高さに対する比率(0〜0.5)が最小のものを採用する。
/// 最小の比率が0.25(25%)を超える=中央寄りすぎる場合はnull(ドロップ対象外=タブ統合等
/// 本Sliceの対象外の中央エリア)を返す。
const EDGE_MARGIN_RATIO = 0.25;

export function edgeFromPointer(offsetX: number, offsetY: number, width: number, height: number): Edge | null {
  if (width <= 0 || height <= 0) return null;
  const nx = Math.min(offsetX, width - offsetX) / width;
  const ny = Math.min(offsetY, height - offsetY) / height;
  const minRatio = Math.min(nx, ny);
  if (minRatio > EDGE_MARGIN_RATIO) return null;
  if (nx <= ny) {
    return offsetX < width / 2 ? "left" : "right";
  }
  return offsetY < height / 2 ? "top" : "bottom";
}
