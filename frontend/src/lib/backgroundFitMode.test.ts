import { describe, expect, it } from "vitest";
import {
  BACKGROUND_FIT_MODE_CSS,
  BACKGROUND_FIT_MODE_OPTIONS,
  BACKGROUND_FIT_MODE_OBJECT_FIT,
  BACKGROUND_FIT_MODE_OPTIONS_FOR_VIDEO,
} from "./backgroundFitMode";

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

describe("backgroundFitMode", () => {
  it("maps cover/contain/fill to matching object-fit keywords", () => {
    expect(BACKGROUND_FIT_MODE_OBJECT_FIT.cover).toBe("cover");
    expect(BACKGROUND_FIT_MODE_OBJECT_FIT.contain).toBe("contain");
    expect(BACKGROUND_FIT_MODE_OBJECT_FIT.fill).toBe("fill");
  });

  it("excludes tile from the video options list", () => {
    expect(BACKGROUND_FIT_MODE_OPTIONS_FOR_VIDEO.map((o) => o.value)).toEqual(["cover", "contain", "fill"]);
    // 元のリストにはtileが含まれ続けること(画像用は変更しない)
    expect(BACKGROUND_FIT_MODE_OPTIONS.map((o) => o.value)).toContain("tile");
  });
});
