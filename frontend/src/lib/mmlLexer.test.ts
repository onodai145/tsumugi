import { describe, expect, it } from "vitest";
import { lexMml } from "./mmlLexer";

/** [トークンの文字列, 種別] の列にして比べやすくする */
function lex(text: string): [string, string][] {
  return lexMml(text).map((t) => [text.slice(t.from, t.to), t.kind]);
}

describe("lexMml", () => {
  it("コマンドと数字", () => {
    expect(lex("t120 o5 l8")).toEqual([
      ["t", "command"],
      ["120", "number"],
      ["o", "command"],
      ["5", "number"],
      ["l", "command"],
      ["8", "number"],
    ]);
  });

  it("音符は音名と半音記号まで、長さと付点は number", () => {
    expect(lex("c#4. d-8 e+")).toEqual([
      ["c#", "note"],
      ["4.", "number"],
      ["d-", "note"],
      ["8", "number"],
      ["e+", "note"],
    ]);
  });

  it("休符とタイは rest", () => {
    expect(lex("r4&c8")).toEqual([
      ["r", "rest"],
      ["4", "number"],
      ["&", "rest"],
      ["c", "note"],
      ["8", "number"],
    ]);
  });

  it("オクターブの上げ下げは command", () => {
    expect(lex(">c<")).toEqual([
      [">", "command"],
      ["c", "note"],
      ["<", "command"],
    ]);
  });

  it("音色は @番号 と @名前", () => {
    expect(lex("@3c")).toEqual([
      ["@3", "timbre"],
      ["c", "note"],
    ]);
    expect(lex("@bell c")).toEqual([
      ["@bell", "timbre"],
      ["c", "note"],
    ]);
  });

  it("音色名は最長一致で読む(@epiano は e ではなく音色)", () => {
    expect(lex("@epiano")).toEqual([["@epiano", "timbre"]]);
  });

  it("繰り返しは [ と、] とその直後の回数", () => {
    expect(lex("[cde]3")).toEqual([
      ["[", "repeat"],
      ["c", "note"],
      ["d", "note"],
      ["e", "note"],
      ["]3", "repeat"],
    ]);
  });

  it("トラック区切り", () => {
    expect(lex("c;e")).toEqual([
      ["c", "note"],
      [";", "separator"],
      ["e", "note"],
    ]);
  });

  it("構文が誤っていても例外を出さず、最後まで分類する(解析は parseMml の仕事)", () => {
    expect(lex("c4 x")).toEqual([
      ["c", "note"],
      ["4", "number"],
      ["x", "unknown"],
    ]);
    expect(lex("@")).toEqual([["@", "timbre"]]);
    expect(lexMml("@zzz").map((t) => t.kind)).toEqual(["timbre", "unknown", "unknown", "unknown"]);
    expect(lexMml("]]").map((t) => t.kind)).toEqual(["repeat", "repeat"]);
  });

  it("大文字・空白・改行があっても、パーサーと同じく読む(空白は除き、小文字化する)", () => {
    expect(lex("C4\nD4")).toEqual([
      ["C", "note"],
      ["4", "number"],
      ["D", "note"],
      ["4", "number"],
    ]);
    expect(lex("T 120")).toEqual([
      ["T", "command"],
      ["120", "number"],
    ]);
    expect(lex("@BELLc")).toEqual([
      ["@BELL", "timbre"],
      ["c", "note"],
    ]);
  });

  it("空白をまたぐトークンは、空白を含む範囲になる", () => {
    const [t] = lexMml("@ bell");
    expect(t).toEqual({ from: 0, to: 6, kind: "timbre" });
  });

  it("サロゲートペアは 1 つの unknown で、後続の位置は UTF-16 の添字", () => {
    expect(lexMml("😀c")).toEqual([
      { from: 0, to: 2, kind: "unknown" },
      { from: 2, to: 3, kind: "note" },
    ]);
  });

  it("空文字列は空", () => {
    expect(lexMml("")).toEqual([]);
    expect(lexMml("  \n ")).toEqual([]);
  });

  it("位置は昇順で重ならない", () => {
    const tokens = lexMml("t120 o5 l8 [c#4.d-8]3 r4&e @bell g; >a<b @3 x");
    for (let i = 1; i < tokens.length; i++) expect(tokens[i].from).toBeGreaterThanOrEqual(tokens[i - 1].to);
  });
});
