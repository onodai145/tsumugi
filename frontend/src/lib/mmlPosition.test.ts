import { describe, expect, it } from "vitest";
import { charPosToIndex, errorRange } from "./mmlPosition";

describe("charPosToIndex", () => {
  it("ASCII だけなら、そのまま", () => {
    expect(charPosToIndex("c4 x", 3)).toBe(3);
  });

  it("サロゲートペアは Rust では 1 文字、JS では 2 単位", () => {
    // 「😀」は Rust の 1 文字目。2 文字目(c)は JS の添字では 2
    expect(charPosToIndex("😀cx", 1)).toBe(2);
    expect(charPosToIndex("😀cx", 2)).toBe(3);
  });

  it("文字数以上なら、末尾", () => {
    expect(charPosToIndex("c4", 2)).toBe(2);
    expect(charPosToIndex("c4", 99)).toBe(2);
    expect(charPosToIndex("😀", 5)).toBe(2);
    expect(charPosToIndex("", 0)).toBe(0);
  });
});

describe("errorRange", () => {
  it("その位置の 1 文字", () => {
    expect(errorRange("c4 x", 3)).toEqual({ from: 3, to: 4 });
  });

  it("手前にサロゲートペアがあっても、正しい文字に付く", () => {
    expect(errorRange("😀cx", 2)).toEqual({ from: 3, to: 4 });
  });

  it("エラー位置がサロゲートペアなら、2 単位ぶん", () => {
    expect(errorRange("c😀", 1)).toEqual({ from: 1, to: 3 });
  });

  it("文字数以上(5000 字超過のエラーは position = 文書の長さ)なら、最後の 1 文字", () => {
    expect(errorRange("ab", 2)).toEqual({ from: 1, to: 2 });
    expect(errorRange("ab", 5000)).toEqual({ from: 1, to: 2 });
    expect(errorRange("a😀", 5000)).toEqual({ from: 1, to: 3 });
  });

  it("空文書には引かない", () => {
    expect(errorRange("", 0)).toBeNull();
  });
});
