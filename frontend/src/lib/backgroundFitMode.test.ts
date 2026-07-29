import { describe, expect, it } from "vitest";
import { BACKGROUND_FIT_MODE_CSS, BACKGROUND_FIT_MODE_OPTIONS } from "./backgroundFitMode";

describe("BACKGROUND_FIT_MODE_CSS", () => {
  it("maps cover to background-size cover / no-repeat", () => {
    expect(BACKGROUND_FIT_MODE_CSS.cover).toEqual(["cover", "no-repeat"]);
  });

  it("maps fill to 100% 100% / no-repeat", () => {
    expect(BACKGROUND_FIT_MODE_CSS.fill).toEqual(["100% 100%", "no-repeat"]);
  });

  it("maps tile to auto / repeat", () => {
    expect(BACKGROUND_FIT_MODE_CSS.tile).toEqual(["auto", "repeat"]);
  });

  it("has a CSS entry for every option value", () => {
    for (const { value } of BACKGROUND_FIT_MODE_OPTIONS) {
      expect(BACKGROUND_FIT_MODE_CSS[value]).toBeDefined();
    }
  });
});

describe("BACKGROUND_FIT_MODE_OPTIONS", () => {
  it("has exactly 4 options", () => {
    expect(BACKGROUND_FIT_MODE_OPTIONS).toHaveLength(4);
  });

  it("has unique values", () => {
    const values = BACKGROUND_FIT_MODE_OPTIONS.map((o) => o.value);
    expect(new Set(values).size).toBe(values.length);
  });
});
