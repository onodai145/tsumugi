import { describe, expect, it } from "vitest";
import { readableTextColor } from "./color";

describe("readableTextColor", () => {
  it("returns white text for a dark background", () => {
    expect(readableTextColor("#000000")).toBe("#ffffff");
  });

  it("returns black text for a light background", () => {
    expect(readableTextColor("#ffffff")).toBe("#000000");
  });

  it("returns black text for a saturated red accent color", () => {
    // WCAG相対輝度は緑チャンネルの重みが大きいため(0.7152)、純赤(#ff0000)の輝度は0.2126で
    // しきい値0.179を上回り、黒文字の方がコントラスト比が高くなる(黒5.25 vs 白4.00)。
    expect(readableTextColor("#ff0000")).toBe("#000000");
  });

  it("returns black text for a pale accent color", () => {
    expect(readableTextColor("#ffff00")).toBe("#000000");
  });

  it("falls back to white text for an invalid hex value", () => {
    expect(readableTextColor("not-a-color")).toBe("#ffffff");
    expect(readableTextColor("")).toBe("#ffffff");
  });
});
