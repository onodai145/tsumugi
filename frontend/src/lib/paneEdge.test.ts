import { describe, expect, it } from "vitest";
import { edgeFromPointer } from "./paneEdge";

describe("edgeFromPointer", () => {
  it("returns left when pointer is near the left edge", () => {
    expect(edgeFromPointer(5, 100, 200, 200)).toBe("left");
  });

  it("returns right when pointer is near the right edge", () => {
    expect(edgeFromPointer(195, 100, 200, 200)).toBe("right");
  });

  it("returns top when pointer is near the top edge", () => {
    expect(edgeFromPointer(100, 5, 200, 200)).toBe("top");
  });

  it("returns bottom when pointer is near the bottom edge", () => {
    expect(edgeFromPointer(100, 195, 200, 200)).toBe("bottom");
  });

  it("returns null at the dead center", () => {
    expect(edgeFromPointer(100, 100, 200, 200)).toBeNull();
  });

  it("picks the nearest edge in a corner-ish position on a wide rect", () => {
    // 幅800/高さ100の横長要素。左上寄りでも、上下の余白比率(y=10/100=10%)の方が
    // 左右の余白比率(x=50/800=6.25%)より小さくないので、xの近さ(6.25%<25%)が勝つ。
    expect(edgeFromPointer(50, 10, 800, 100)).toBe("left");
  });

  it("returns null when neither axis is within the 25% margin", () => {
    expect(edgeFromPointer(250, 250, 800, 800)).toBeNull();
  });
});
