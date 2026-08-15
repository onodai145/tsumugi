import { describe, expect, it } from "vitest";
import { buildNyaizeCharMap, mapNyaizedRangeToOriginal } from "./nyaizeCopy";
import { nyaize } from "./nyaize";

describe("buildNyaizeCharMap / mapNyaizedRangeToOriginal", () => {
  it("maps a 1-to-2 char expansion (な -> にゃ) back to the original", () => {
    const original = "こんな感じ";
    const nyaized = nyaize(original); // "こんにゃ感じ"
    const map = buildNyaizeCharMap(original, nyaized);

    // 全選択は元テキスト全体に戻る
    expect(mapNyaizedRangeToOriginal(map, original, 0, nyaized.length)).toBe(original);

    // "にゃ" だけを選択しても、対応する元の1文字 "な" を含む区間に戻る
    const nyaIndex = nyaized.indexOf("にゃ");
    const partial = mapNyaizedRangeToOriginal(map, original, nyaIndex, nyaIndex + 2);
    expect(partial).toBe("な");
  });

  it("maps unchanged text 1:1", () => {
    const original = "hello world";
    const nyaized = nyaize(original); // 変化なし
    const map = buildNyaizeCharMap(original, nyaized);
    expect(mapNyaizedRangeToOriginal(map, original, 0, 5)).toBe("hello");
  });

  it("maps a trailing insertion (다 -> 다냥) back to the original char", () => {
    const original = "간다";
    const nyaized = nyaize(original); // "간다냥"
    const map = buildNyaizeCharMap(original, nyaized);
    expect(mapNyaizedRangeToOriginal(map, original, 0, nyaized.length)).toBe(original);
  });

  it("returns an empty string for an empty or inverted range", () => {
    const original = "test";
    const nyaized = nyaize(original);
    const map = buildNyaizeCharMap(original, nyaized);
    expect(mapNyaizedRangeToOriginal(map, original, 2, 2)).toBe("");
    expect(mapNyaizedRangeToOriginal(map, original, 3, 1)).toBe("");
  });

  it("handles empty strings", () => {
    const map = buildNyaizeCharMap("", "");
    expect(mapNyaizedRangeToOriginal(map, "", 0, 0)).toBe("");
  });
});
