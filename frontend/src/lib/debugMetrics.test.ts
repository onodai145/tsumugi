import { describe, expect, it } from "vitest";
import { formatHitRate } from "./debugMetrics";

describe("formatHitRate", () => {
  it("試行が0件のときはダッシュを返す", () => {
    expect(formatHitRate(0, 0, 0)).toBe("—");
  });

  it("hitとfallbackの合計に対するhitの割合を四捨五入したパーセントで返す", () => {
    expect(formatHitRate(3, 1, 0)).toBe("75%");
    expect(formatHitRate(1, 1, 1)).toBe("33%");
    expect(formatHitRate(0, 1, 0)).toBe("0%");
  });
});
