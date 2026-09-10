// モバイル版の左右スワイプのジェスチャー判定に使う純粋関数(Issue #296)。
// ノート一覧の縦スクロールと衝突しないよう、横/縦の軸判定と、ドラッグ終了時に
// 確定させるかどうかの判定、移動先が無い端でのラバーバンド量の計算を切り出す。
export type SwipeAxis = "horizontal" | "vertical";

const AXIS_LOCK_THRESHOLD_PX = 10;
const COMMIT_DISTANCE_RATIO = 0.35;
const COMMIT_VELOCITY_PX_MS = 0.5;
const RUBBER_BAND_RESISTANCE = 0.35;

/// ポインタの累積移動量から、ジェスチャーの軸を判定する。
/// dx/dyの絶対値がどちらも閾値未満なら判定を保留してnullを返す。
export function resolveSwipeAxis(dx: number, dy: number, thresholdPx = AXIS_LOCK_THRESHOLD_PX): SwipeAxis | null {
  if (Math.abs(dx) < thresholdPx && Math.abs(dy) < thresholdPx) return null;
  return Math.abs(dx) > Math.abs(dy) ? "horizontal" : "vertical";
}

/// ドラッグ終了時、残りをアニメーションで確定させるかを判定する。
/// コンテナ幅に対する移動量の割合、またはリリース時の速度(px/ms)のいずれかが
/// 閾値以上なら確定とする。
export function shouldCommitSwipe(
  dx: number,
  containerWidth: number,
  velocityPxPerMs: number,
  distanceRatioThreshold = COMMIT_DISTANCE_RATIO,
  velocityThreshold = COMMIT_VELOCITY_PX_MS,
): boolean {
  if (containerWidth <= 0) return false;
  const ratio = Math.abs(dx) / containerWidth;
  return ratio >= distanceRatioThreshold || Math.abs(velocityPxPerMs) >= velocityThreshold;
}

/// 移動先が無い端でのドラッグ量を減衰させ、ゴムのような抵抗感(rubber-band)を出す。
export function applyRubberBand(dx: number, resistance = RUBBER_BAND_RESISTANCE): number {
  return dx * resistance;
}
