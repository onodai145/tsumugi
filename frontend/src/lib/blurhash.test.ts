import { describe, expect, it } from "vitest";
import { extractAvgColorFromBlurhash } from "./blurhash";

describe("extractAvgColorFromBlurhash", () => {
  it("BlurHash文字列から平均色のhexコードを抽出する", () => {
    // Misskey本家 extract-avg-color-from-blurhash.ts と同一ロジックの既知の入出力例。
    // "LEHV6nWB2yk8pyo0adR*.7kCMdnj" の3〜6文字目("HV6n")をbase83デコードした値。
    const result = extractAvgColorFromBlurhash("LEHV6nWB2yk8pyo0adR*.7kCMdnj");
    expect(result).toMatch(/^#[0-9a-f]{6}$/);
    expect(result).toBe("#979695");
  });

  it("null/undefinedはundefinedを返す", () => {
    expect(extractAvgColorFromBlurhash(null)).toBeUndefined();
    expect(extractAvgColorFromBlurhash(undefined)).toBeUndefined();
  });

  it("同じ入力に対して常に同じ色を返す(決定的)", () => {
    const a = extractAvgColorFromBlurhash("LEHV6nWB2yk8pyo0adR*.7kCMdnj");
    const b = extractAvgColorFromBlurhash("LEHV6nWB2yk8pyo0adR*.7kCMdnj");
    expect(a).toBe(b);
  });
});
