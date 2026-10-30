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

  it("^ はタイ(rest)で、後ろの長さと付点は number", () => {
    expect(lex("c4^8.")).toEqual([
      ["c", "note"],
      ["4", "number"],
      ["^", "rest"],
      ["8.", "number"],
    ]);
  });

  it("v+n と v-n は 1 つのコマンド(符号まで)と number、q は数字つきコマンド", () => {
    expect(lex("v+2 v- q50")).toEqual([
      ["v+", "command"],
      ["2", "number"],
      ["v-", "command"],
      ["q", "command"],
      ["50", "number"],
    ]);
    expect(lex("v10")).toEqual([
      ["v", "command"],
      ["10", "number"],
    ]);
  });

  it("t= は 1 つのコマンド(= まで。空白をまたいでよい)と number。t120 は今までどおり", () => {
    expect(lex("t=140 t120")).toEqual([
      ["t=", "command"],
      ["140", "number"],
      ["t", "command"],
      ["120", "number"],
    ]);
    expect(lex("T = 140")).toEqual([
      ["T =", "command"],
      ["140", "number"],
    ]);
  });

  it("和音は開きの ' と、閉じの ' とその直後の長さ・付点(repeat)。中は音符と < >", () => {
    expect(lex("'ceg'4.")).toEqual([
      ["'", "repeat"],
      ["c", "note"],
      ["e", "note"],
      ["g", "note"],
      ["'4.", "repeat"],
    ]);
    expect(lex("'c>c' d")).toEqual([
      ["'", "repeat"],
      ["c", "note"],
      [">", "command"],
      ["c", "note"],
      ["'", "repeat"],
      ["d", "note"],
    ]);
  });

  it("閉じていない ' は、以降を和音の中として読む(開きだけ repeat)", () => {
    expect(lex("'ce")).toEqual([
      ["'", "repeat"],
      ["c", "note"],
      ["e", "note"],
    ]);
  });

  it("和音の状態は ; (トラックの区切り)で戻る: 1 本目の閉じていない ' が、2 本目の開き・閉じを逆にしない", () => {
    expect(lex("'c;'ce'4")).toEqual([
      ["'", "repeat"],
      ["c", "note"],
      [";", "separator"],
      ["'", "repeat"],
      ["c", "note"],
      ["e", "note"],
      ["'4", "repeat"],
    ]);
  });

  it("コメントをまたぐトークンは、コメントの範囲で切って、コメントと重ならない", () => {
    expect(lex("t/*x*/=60")).toEqual([
      ["t", "command"],
      ["/*x*/", "comment"],
      ["=", "command"],
      ["60", "number"],
    ]);
    expect(lex("v/**/+2")).toEqual([
      ["v", "command"],
      ["/**/", "comment"],
      ["+", "command"],
      ["2", "number"],
    ]);
    const tokens = lexMml("t/*x*/=60 @be/*y*/ll [c]/*z*/3 // w\nd");
    for (let i = 1; i < tokens.length; i++) expect(tokens[i].from).toBeGreaterThanOrEqual(tokens[i - 1].to);
  });

  it("コメントの間の空白だけの断片は捨てる。1 つのトークンが複数のコメントをまたいでもよい", () => {
    expect(lex("t/*a*/ /*b*/=60")).toEqual([
      ["t", "command"],
      ["/*a*/", "comment"],
      ["/*b*/", "comment"],
      ["=", "command"],
      ["60", "number"],
    ]);
    expect(lex("t/*a*//*b*/=60")).toEqual([
      ["t", "command"],
      ["/*a*/", "comment"],
      ["/*b*/", "comment"],
      ["=", "command"],
      ["60", "number"],
    ]);
  });

  it("マクロの定義の頭 `$名前=` と展開 `$名前` は command。本体の中身は今までどおりの色", () => {
    expect(lex("$riff={c d}")).toEqual([
      ["$riff=", "command"],
      ["{", "repeat"],
      ["c", "note"],
      ["d", "note"],
      ["}", "repeat"],
    ]);
    expect(lex("$riff d")).toEqual([
      ["$riff", "command"],
      ["d", "note"],
    ]);
  });

  it("マクロの名前は、`$` の直後に途切れなく続く英数字と _(空白で終わる)", () => {
    expect(lex("$a c")).toEqual([
      ["$a", "command"],
      ["c", "note"],
    ]);
    expect(lex("$ac")).toEqual([["$ac", "command"]]);
    expect(lex("$a/**/c")).toEqual([
      ["$a", "command"],
      ["/**/", "comment"],
      ["c", "note"],
    ]);
    expect(lex("c $")).toEqual([
      ["c", "note"],
      ["$", "unknown"],
    ]);
  });

  it("定義の閉じの } は後ろの数字を取り込まない。本体の中の連符の }4 は取り込む", () => {
    expect(lex("$a={c}4")).toEqual([
      ["$a=", "command"],
      ["{", "repeat"],
      ["c", "note"],
      ["}", "repeat"],
      ["4", "unknown"],
    ]);
    expect(lex("$a={{c}4}")).toEqual([
      ["$a=", "command"],
      ["{", "repeat"],
      ["{", "repeat"],
      ["c", "note"],
      ["}4", "repeat"],
      ["}", "repeat"],
    ]);
  });

  it("マクロの展開の後ろに空白で区切った長さ・付点は、number(`$riff 4` は本体の最後の音の長さになる)", () => {
    expect(lex("$a 4.")).toEqual([
      ["$a", "command"],
      ["4.", "number"],
    ]);
    expect(lex("$a/**/8")).toEqual([
      ["$a", "command"],
      ["/**/", "comment"],
      ["8", "number"],
    ]);
  });

  it("音色パラメーター y m k は command、数字は number、カンマは command(Issue #499)", () => {
    expect(lex("y10,200,60,300")).toEqual([
      ["y", "command"],
      ["10", "number"],
      [",", "command"],
      ["200", "number"],
      [",", "command"],
      ["60", "number"],
      [",", "command"],
      ["300", "number"],
    ]);
    expect(lex("m5,30 k-12 k+7 y0")).toEqual([
      ["m", "command"],
      ["5", "number"],
      [",", "command"],
      ["30", "number"],
      ["k-", "command"],
      ["12", "number"],
      ["k+", "command"],
      ["7", "number"],
      ["y", "command"],
      ["0", "number"],
    ]);
  });

  it("y m k の後ろに数字がなくても、読めない文字にはしない。途中のカンマだけ・末尾のカンマも", () => {
    expect(lex("y")).toEqual([["y", "command"]]);
    expect(lex("m5,")).toEqual([
      ["m", "command"],
      ["5", "number"],
      [",", "command"],
    ]);
    expect(lex("k c")).toEqual([
      ["k", "command"],
      ["c", "note"],
    ]);
  });

  it("@drumk0 は、音色 @drum と、デチューン k0 に分かれる。@drumkit は 1 つの音色", () => {
    expect(lex("@drumk0")).toEqual([
      ["@drum", "timbre"],
      ["k", "command"],
      ["0", "number"],
    ]);
    expect(lex("@drumkit")).toEqual([["@drumkit", "timbre"]]);
  });

  it("`$a t=60` は展開と t=、`$at=60` は名前 at(定義ではない)", () => {
    expect(lex("$a t=60")).toEqual([
      ["$a", "command"],
      ["t=", "command"],
      ["60", "number"],
    ]);
    expect(lex("$at=60")[0]).toEqual(["$at", "command"]);
  });

  it("連符は { と、} とその直後の長さ・付点(repeat)", () => {
    expect(lex("{cde}4.")).toEqual([
      ["{", "repeat"],
      ["c", "note"],
      ["d", "note"],
      ["e", "note"],
      ["}4.", "repeat"],
    ]);
  });

  it("ループの脱出 : は repeat", () => {
    expect(lex("[c:d]2")).toEqual([
      ["[", "repeat"],
      ["c", "note"],
      [":", "repeat"],
      ["d", "note"],
      ["]2", "repeat"],
    ]);
  });

  it("行コメントは改行の手前までが comment で、中の記号は色分けしない", () => {
    expect(lex("c // t120 [\ne")).toEqual([
      ["c", "note"],
      ["// t120 [", "comment"],
      ["e", "note"],
    ]);
  });

  it("範囲コメントは複数行にまたがり、閉じていなければ文末まで", () => {
    expect(lex("c/* o5\n; */d")).toEqual([
      ["c", "note"],
      ["/* o5\n; */", "comment"],
      ["d", "note"],
    ]);
    expect(lex("c /* d ; e")).toEqual([
      ["c", "note"],
      ["/* d ; e", "comment"],
    ]);
  });

  it("/ 単独は unknown(コメントではない)", () => {
    expect(lex("c/d")).toEqual([
      ["c", "note"],
      ["/", "unknown"],
      ["d", "note"],
    ]);
  });

  it("コメントを含んでも、トークンは from 昇順", () => {
    const tokens = lexMml("t120 // x\n[c#4.^8 d-]3 /* y */ {e f}4 q80 v+1 ; g");
    for (let i = 1; i < tokens.length; i++) expect(tokens[i].from).toBeGreaterThanOrEqual(tokens[i - 1].from);
  });
});
