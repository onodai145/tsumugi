import { describe, expect, it } from "vitest";
import { BACKGROUND_POSITION_CSS, BACKGROUND_POSITION_GRID } from "./backgroundPosition";

describe("BACKGROUND_POSITION_CSS", () => {
  it("maps center to center center", () => {
    expect(BACKGROUND_POSITION_CSS.center).toBe("center center");
  });

  it("maps top-left to left top", () => {
    expect(BACKGROUND_POSITION_CSS["top-left"]).toBe("left top");
  });

  it("maps bottom-right to right bottom", () => {
    expect(BACKGROUND_POSITION_CSS["bottom-right"]).toBe("right bottom");
  });

  it("has a CSS value for every grid position", () => {
    for (const pos of BACKGROUND_POSITION_GRID) {
      expect(BACKGROUND_POSITION_CSS[pos]).toBeDefined();
    }
  });
});

describe("BACKGROUND_POSITION_GRID", () => {
  it("has 9 positions in row-major order", () => {
    expect(BACKGROUND_POSITION_GRID).toEqual([
      "top-left",
      "top",
      "top-right",
      "left",
      "center",
      "right",
      "bottom-left",
      "bottom",
      "bottom-right",
    ]);
  });
});
