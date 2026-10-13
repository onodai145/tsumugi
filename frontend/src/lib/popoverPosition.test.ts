import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { placeVertically, placementStyle, visibleBottom } from "./popoverPosition";

const setViewport = (h: number) => Object.defineProperty(window, "innerHeight", { value: h, configurable: true });
const setKeyboard = (px: number | null) => {
  if (px === null) document.documentElement.style.removeProperty("--keyboard-inset");
  else document.documentElement.style.setProperty("--keyboard-inset", `${px}px`);
};

beforeEach(() => setViewport(800));
afterEach(() => setKeyboard(null));

describe("popoverPosition", () => {
  it("visibleBottomはキーボードが覆う高さを引く(未設定なら画面下端)", () => {
    expect(visibleBottom()).toBe(800);
    setKeyboard(300);
    expect(visibleBottom()).toBe(500);
  });

  it("下に十分な空きがあればアンカーの下に出す", () => {
    expect(placeVertically({ top: 100, bottom: 130 }, 200)).toEqual({ top: 134 });
  });

  it("下に空きが無く上にあるなら、アンカーの上に出す(画面下端からの距離で指定)", () => {
    // アンカー上端700 → 画面下端(800)からの距離100 + 隙間4
    expect(placeVertically({ top: 700, bottom: 730 }, 200)).toEqual({ bottom: 104 });
  });

  it("キーボードで可視領域が縮んだぶんも考慮して上に出す", () => {
    setKeyboard(300); // 可視下端500
    expect(placeVertically({ top: 440, bottom: 470 }, 200)).toEqual({ bottom: 364 });
  });

  it("上下どちらにも収まらないときは広い側(同じなら下)に出す", () => {
    setViewport(300);
    expect(placeVertically({ top: 100, bottom: 130 }, 400)).toEqual({ top: 134 }); // 下170 > 上100
    expect(placeVertically({ top: 180, bottom: 210 }, 400)).toEqual({ bottom: 124 }); // 下90 < 上180
  });

  it("placementStyleはtopまたはbottomの指定だけを含む", () => {
    expect(placementStyle(12, { top: 34 })).toBe("left:12px;top:34px");
    expect(placementStyle(12, { bottom: 56 })).toBe("left:12px;bottom:56px");
  });
});
