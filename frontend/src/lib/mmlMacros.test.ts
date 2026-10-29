import { describe, expect, it } from "vitest";
import { blankMacroDefinitions, findMacroDefinitions } from "./mmlMacros";

describe("findMacroDefinitions", () => {
  it("定義の範囲(`$` から閉じの `}` の次まで)と、本体の範囲を返す", () => {
    expect(findMacroDefinitions("$a={c}")).toEqual([{ name: "a", from: 0, to: 6, bodyFrom: 4, bodyTo: 5 }]);
  });

  it("本体の中の波括弧(連符)は対で数える", () => {
    expect(findMacroDefinitions("$u={{c d}4} x")).toEqual([{ name: "u", from: 0, to: 11, bodyFrom: 4, bodyTo: 10 }]);
  });

  it("`=` と `{` の前後の空白、大文字(名前は小文字にする)", () => {
    expect(findMacroDefinitions("$Riff = { c }")).toEqual([{ name: "riff", from: 0, to: 13, bodyFrom: 9, bodyTo: 12 }]);
  });

  it("定義でないもの: 展開、`$a t=60`、`$at=60`、`=` の次が `{` でない、名前が無い", () => {
    expect(findMacroDefinitions("$a")).toEqual([]);
    expect(findMacroDefinitions("$a t=60")).toEqual([]);
    expect(findMacroDefinitions("$at=60")).toEqual([]);
    expect(findMacroDefinitions("$a=c")).toEqual([]);
    expect(findMacroDefinitions("$ ={c}")).toEqual([]);
  });

  it("閉じていない定義は、文末まで", () => {
    expect(findMacroDefinitions("$a={c d")).toEqual([{ name: "a", from: 0, to: 7, bodyFrom: 4, bodyTo: 7 }]);
  });

  it("複数の定義と、本体の中の展開(定義と誤認しない)", () => {
    const defs = findMacroDefinitions("$a={$b $b} $b={c}");
    expect(defs.map((d) => d.name)).toEqual(["a", "b"]);
  });

  it("位置は UTF-16 の添字(サロゲートペアは 2)", () => {
    expect(findMacroDefinitions("😀 $a={c}")[0].from).toBe(3);
  });
});

describe("blankMacroDefinitions", () => {
  it("定義を空白にし、長さと、定義以外の添字を変えない", () => {
    const text = "$a={c} d";
    const out = blankMacroDefinitions(text);
    expect(out).toHaveLength(text.length);
    expect(out.trim()).toBe("d");
    expect(out[text.length - 1]).toBe("d");
  });

  it("定義が無ければ同じ文字列", () => {
    expect(blankMacroDefinitions("c4 $a d4")).toBe("c4 $a d4");
  });

  it("本体の中の `;` も空白になる", () => {
    expect(blankMacroDefinitions("c $a={x;y} d")).not.toContain(";");
  });
});
