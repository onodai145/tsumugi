import { describe, expect, it } from "vitest";
import { DEFAULT_TRANSLATE_LANG, TRANSLATE_LANG_PRESETS, normalizeTranslateLang } from "./translateLang";

describe("translateLang", () => {
  it("既定は ja で、プリセットの先頭にある", () => {
    expect(DEFAULT_TRANSLATE_LANG).toBe("ja");
    expect(TRANSLATE_LANG_PRESETS[0].code).toBe("ja");
  });

  it("プリセットのコードは重複しない", () => {
    const codes = TRANSLATE_LANG_PRESETS.map((p) => p.code);
    expect(new Set(codes).size).toBe(codes.length);
  });

  it("空・空白・未設定は既定へ戻し、それ以外は trim して返す", () => {
    expect(normalizeTranslateLang("")).toBe("ja");
    expect(normalizeTranslateLang("   ")).toBe("ja");
    expect(normalizeTranslateLang(undefined)).toBe("ja");
    expect(normalizeTranslateLang(null)).toBe("ja");
    expect(normalizeTranslateLang(" en ")).toBe("en");
  });
});
