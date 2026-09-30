// UI全体のスケール（Issue #40）。Tauri の Webview::set_zoom（ブラウザのズームと同じ仕組み）で
// 文字・余白・アイコン・画像をまとめて拡縮する。set_zoom は Android 非対応のため、モバイルでは何もしない。
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { isMobilePlatform } from "./platform";

export const UI_SCALE_MIN = 50;
export const UI_SCALE_MAX = 200;
export const UI_SCALE_DEFAULT = 100;
export const UI_SCALE_STEP = 10;

/// 保存値を適用可能な整数%（50〜200）にする。壊れた値（null/NaN/Infinity）は既定の100に倒す。
export function normalizeUiScale(value: number | null | undefined): number {
  if (value == null || !Number.isFinite(value)) return UI_SCALE_DEFAULT;
  return Math.min(UI_SCALE_MAX, Math.max(UI_SCALE_MIN, Math.round(value)));
}

/// WebView のズームを反映する。失敗（未対応環境・IPCエラー）は投げるので、呼び出し側でログする。
export async function applyUiScale(value: number | null | undefined): Promise<void> {
  if (isMobilePlatform) return;
  await getCurrentWebview().setZoom(normalizeUiScale(value) / 100);
}
