import { describe, expect, it } from "vitest";
import { blankComments, findComments } from "./mmlComments";

describe("findComments", () => {
  it("行コメントは改行の手前まで(改行は含めない)", () => {
    expect(findComments("c // d\ne")).toEqual([{ from: 2, to: 6 }]);
    expect(findComments("c//x")).toEqual([{ from: 1, to: 4 }]);
    expect(findComments("c // x\r\nd")).toEqual([{ from: 2, to: 7 }]); // \r は行コメントの中
  });

  it("範囲コメントは `*/` まで。改行をまたげる", () => {
    expect(findComments("c/* x */d")).toEqual([{ from: 1, to: 8 }]);
    expect(findComments("c/* x\ny */d")).toEqual([{ from: 1, to: 10 }]);
    expect(findComments("a/**/b")).toEqual([{ from: 1, to: 5 }]);
  });

  it("入れ子にできない: 最初の `*/` で閉じる", () => {
    expect(findComments("/* a /* b */ c */")).toEqual([{ from: 0, to: 12 }]);
  });

  it("閉じていない `/*` は文末まで", () => {
    expect(findComments("c /* d")).toEqual([{ from: 2, to: 6 }]);
    expect(findComments("c/*/")).toEqual([{ from: 1, to: 4 }]); // `/*/` は閉じていない
  });

  it("`/` 単独や、間に空白のある `/ /` はコメントではない", () => {
    expect(findComments("c/d")).toEqual([]);
    expect(findComments("c / / d")).toEqual([]);
  });

  it("コメントの中の `//` `/*` は、外側のコメントに含まれる", () => {
    expect(findComments("// /* \nc /* // */")).toEqual([
      { from: 0, to: 6 },
      { from: 9, to: 17 },
    ]);
  });

  it("位置は UTF-16 の添字(サロゲートペアは 2)", () => {
    expect(findComments("😀//x")).toEqual([{ from: 2, to: 5 }]);
  });

  it("コメントが無ければ空", () => {
    expect(findComments("")).toEqual([]);
    expect(findComments("c4 d4")).toEqual([]);
  });
});

describe("blankComments", () => {
  it("コメントを空白にし、長さと添字を変えない", () => {
    const text = "c // d\ne";
    const out = blankComments(text);
    expect(out).toHaveLength(text.length);
    expect(out.replace(/\s/g, "")).toBe("ce");
    expect(out[0]).toBe("c");
    expect(out[text.length - 1]).toBe("e");
  });

  it("範囲コメントは、改行を含めて空白になる", () => {
    expect(blankComments("c/* x\ny */d")).toBe("c" + " ".repeat(9) + "d");
  });

  it("コメントが無ければ同じ文字列", () => {
    expect(blankComments("c4 d4")).toBe("c4 d4");
  });

  it("コメント内の `;` が残らない", () => {
    expect(blankComments("c // ; d\ne")).not.toContain(";");
  });
});
