import { describe, expect, it } from "vitest";
import { computeUsage, countChars, countTracks, formatUsage, usageLevel } from "./mmlUsage";

describe("countChars", () => {
  it("Rust の chars().count() と同じく、サロゲートペアを 1 文字と数える", () => {
    expect(countChars("c4")).toBe(2);
    expect(countChars("😀c")).toBe(2);
    expect(countChars("")).toBe(0);
  });

  it("空白と改行も数える(パーサーは空白を含めた文字数で上限を見る)", () => {
    expect(countChars("c4 d4\ne4")).toBe(8);
  });
});

describe("countTracks", () => {
  it("; で分けた、空白以外を含む区間の数", () => {
    expect(countTracks("c4;d4")).toBe(2);
    expect(countTracks("c4;;d4;")).toBe(2);
    expect(countTracks(" ; \n ;")).toBe(0);
    expect(countTracks("")).toBe(0);
    expect(countTracks("c4")).toBe(1);
  });

  it("コメントの中の ; は数えない(パーサーはコメントを除いてからトラックに分ける)", () => {
    expect(countTracks("c4 // ; d4\ne4")).toBe(1);
    expect(countTracks("c4 /* ; */ e4")).toBe(1);
    expect(countTracks("c4 /* x */ ; e4")).toBe(2);
    expect(countTracks("// 全部コメント ;")).toBe(0);
  });
});

describe("usageLevel", () => {
  it("9 割未満は normal、9 割以上で上限ちょうどまでは near、超えたら over", () => {
    expect(usageLevel(4499, 5000)).toBe("normal");
    expect(usageLevel(4500, 5000)).toBe("near");
    expect(usageLevel(5000, 5000)).toBe("near");
    expect(usageLevel(5001, 5000)).toBe("over");
  });

  it("値が無い(null)なら normal", () => {
    expect(usageLevel(null, 5000)).toBe("normal");
  });

  it("再生時間がちょうど 90 秒で、浮動小数点の誤差で超えても over にしない(パーサーは 1e-6 の許容幅で通す)", () => {
    expect(usageLevel(90.0000004, 90)).toBe("near");
    expect(usageLevel(90.00001, 90)).toBe("over");
  });
});

describe("computeUsage", () => {
  it("成功した解析があれば、音数と再生時間も入る", () => {
    const items = computeUsage("c4;d4", { noteCount: 2700, totalSec: 12.34 });
    expect(items.map((i) => [i.key, i.value, i.limit, i.level])).toEqual([
      ["chars", 5, 5000, "normal"],
      ["notes", 2700, 3000, "near"],
      ["tracks", 2, 4, "normal"],
      ["sec", 12.34, 90, "normal"],
    ]);
  });

  it("解析の結果が無ければ、音数と再生時間は null", () => {
    const items = computeUsage("c4", null);
    expect(items.find((i) => i.key === "notes")?.value).toBeNull();
    expect(items.find((i) => i.key === "sec")?.value).toBeNull();
    expect(items.find((i) => i.key === "chars")?.value).toBe(2);
  });
});

describe("formatUsage", () => {
  it("項目ごとの表記にする", () => {
    const items = computeUsage("c4;d4", { noteCount: 120, totalSec: 12.34 });
    expect(items.map(formatUsage)).toEqual(["文字 5/5000", "音 120/3000", "トラック 2/4", "再生 12.3/90 秒"]);
  });

  it("値が無いときは — を出す", () => {
    const items = computeUsage("c4", null);
    expect(items.map(formatUsage)).toEqual(["文字 2/5000", "音 —/3000", "トラック 1/4", "再生 —/90 秒"]);
  });
});
