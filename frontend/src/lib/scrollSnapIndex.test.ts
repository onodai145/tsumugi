import { describe, expect, it } from "vitest";
import { resolveSettledIndex } from "./scrollSnapIndex";

describe("resolveSettledIndex", () => {
  it("scrollLeftがちょうどコンテナ幅の倍数ならそのインデックス", () => {
    expect(resolveSettledIndex(0, 300, 3)).toBe(0);
    expect(resolveSettledIndex(300, 300, 3)).toBe(1);
    expect(resolveSettledIndex(600, 300, 3)).toBe(2);
  });

  it("端数は最も近いインデックスに丸める", () => {
    expect(resolveSettledIndex(280, 300, 3)).toBe(1);
    expect(resolveSettledIndex(320, 300, 3)).toBe(1);
  });

  it("オーバースクロールで負の値になっても0にクランプする", () => {
    expect(resolveSettledIndex(-40, 300, 3)).toBe(0);
  });

  it("最後を超えるスクロールでも最大インデックスにクランプする", () => {
    expect(resolveSettledIndex(1000, 300, 3)).toBe(2);
  });

  it("containerWidthが0以下なら0を返す", () => {
    expect(resolveSettledIndex(300, 0, 3)).toBe(0);
  });

  it("slotCountが0以下なら0を返す", () => {
    expect(resolveSettledIndex(300, 300, 0)).toBe(0);
  });
});
