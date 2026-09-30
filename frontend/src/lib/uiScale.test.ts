import { beforeEach, describe, expect, it, vi } from "vitest";

// platform.ts は import 時に @tauri-apps/plugin-os の platform() を呼ぶため、
// Tauri ランタイム外で import が失敗しないよう、isMobilePlatform を差し替え可能にしてスタブする。
const platformMock = vi.hoisted(() => ({ isMobilePlatform: false }));
vi.mock("./platform", () => platformMock);

const setZoomMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ setZoom: setZoomMock }),
}));

const { normalizeUiScale, applyUiScale, UI_SCALE_MIN, UI_SCALE_MAX, UI_SCALE_DEFAULT } = await import("./uiScale");

beforeEach(() => {
  setZoomMock.mockReset();
  setZoomMock.mockResolvedValue(undefined);
  platformMock.isMobilePlatform = false;
});

describe("normalizeUiScale", () => {
  it("範囲内の整数はそのまま返す", () => {
    expect(normalizeUiScale(100)).toBe(100);
    expect(normalizeUiScale(130)).toBe(130);
    expect(normalizeUiScale(UI_SCALE_MIN)).toBe(50);
    expect(normalizeUiScale(UI_SCALE_MAX)).toBe(200);
  });

  it("範囲外はクランプする", () => {
    expect(normalizeUiScale(0)).toBe(50);
    expect(normalizeUiScale(-30)).toBe(50);
    expect(normalizeUiScale(49)).toBe(50);
    expect(normalizeUiScale(201)).toBe(200);
    expect(normalizeUiScale(1000)).toBe(200);
  });

  it("小数は四捨五入して整数%にする", () => {
    expect(normalizeUiScale(125.4)).toBe(125);
    expect(normalizeUiScale(125.5)).toBe(126);
  });

  it("null/undefined/NaN/Infinity は既定の100にする", () => {
    expect(normalizeUiScale(undefined)).toBe(UI_SCALE_DEFAULT);
    expect(normalizeUiScale(null)).toBe(UI_SCALE_DEFAULT);
    expect(normalizeUiScale(Number.NaN)).toBe(UI_SCALE_DEFAULT);
    expect(normalizeUiScale(Number.POSITIVE_INFINITY)).toBe(UI_SCALE_DEFAULT);
    expect(normalizeUiScale(Number.NEGATIVE_INFINITY)).toBe(UI_SCALE_DEFAULT);
  });
});

describe("applyUiScale", () => {
  it("正規化した倍率(%÷100)で setZoom を呼ぶ", async () => {
    await applyUiScale(150);
    expect(setZoomMock).toHaveBeenCalledTimes(1);
    expect(setZoomMock).toHaveBeenCalledWith(1.5);
  });

  it("範囲外の値でも正規化後の倍率で呼ぶ", async () => {
    await applyUiScale(1000);
    expect(setZoomMock).toHaveBeenCalledWith(2);
    await applyUiScale(undefined);
    expect(setZoomMock).toHaveBeenLastCalledWith(1);
  });

  it("モバイルでは setZoom を呼ばない", async () => {
    platformMock.isMobilePlatform = true;
    await applyUiScale(150);
    expect(setZoomMock).not.toHaveBeenCalled();
  });

  it("setZoom が失敗したら例外をそのまま投げる", async () => {
    setZoomMock.mockRejectedValue(new Error("unsupported"));
    await expect(applyUiScale(150)).rejects.toThrow("unsupported");
  });
});
