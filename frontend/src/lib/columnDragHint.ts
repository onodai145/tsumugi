// モバイル版のカラム長押しドラッグで、指のX移動量から「前へ/次へ」どちらのヒントを
// ハイライトするかを解決する純粋関数(Issue #354)。閾値を超えるまではnull(トグル式、
// 連続した複数ステップ移動はしない)。
// 閾値は当初40pxだったが、実機確認で「長押し成立直後の自然な指の動きだけですぐに確定
// してしまい、ヒントを確認する余裕がない」との報告があり80pxに緩和した。
export const COLUMN_DRAG_HINT_THRESHOLD_PX = 80;

export type ColumnDragDirection = "prev" | "next";

export function resolveColumnDragHint(
  deltaX: number,
  canGoPrev: boolean,
  canGoNext: boolean,
): ColumnDragDirection | null {
  if (deltaX <= -COLUMN_DRAG_HINT_THRESHOLD_PX) return canGoPrev ? "prev" : null;
  if (deltaX >= COLUMN_DRAG_HINT_THRESHOLD_PX) return canGoNext ? "next" : null;
  return null;
}
