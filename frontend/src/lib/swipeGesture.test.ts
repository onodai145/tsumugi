import { describe, expect, it } from "vitest";
import { applyRubberBand, resolveSwipeAxis, shouldCommitSwipe } from "./swipeGesture";

describe("resolveSwipeAxis", () => {
  it("移動量が閾値未満ならnull(判定保留)", () => {
    expect(resolveSwipeAxis(3, 2)).toBeNull();
  });

  it("横移動が縦移動より大きければhorizontal", () => {
    expect(resolveSwipeAxis(20, 5)).toBe("horizontal");
  });

  it("縦移動が横移動より大きければvertical", () => {
    expect(resolveSwipeAxis(5, 20)).toBe("vertical");
  });
});

describe("shouldCommitSwipe", () => {
  it("移動量がコンテナ幅の35%以上なら確定", () => {
    expect(shouldCommitSwipe(-140, 300, 0)).toBe(true);
  });

  it("移動量が小さくても速度が閾値以上なら確定", () => {
    expect(shouldCommitSwipe(-20, 300, 0.8)).toBe(true);
  });

  it("移動量も速度も足りなければ確定しない", () => {
    expect(shouldCommitSwipe(-20, 300, 0.1)).toBe(false);
  });

  it("コンテナ幅が0以下なら確定しない", () => {
    expect(shouldCommitSwipe(-140, 0, 0)).toBe(false);
  });
});

describe("applyRubberBand", () => {
  it("移動量を抵抗係数分だけ減衰させる", () => {
    expect(applyRubberBand(100, 0.35)).toBeCloseTo(35);
    expect(applyRubberBand(-100, 0.35)).toBeCloseTo(-35);
  });

  it("デフォルトの抵抗係数(0.35)を使う", () => {
    expect(applyRubberBand(100)).toBeCloseTo(35);
  });
});
