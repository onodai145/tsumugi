// MML の寛容な字句解析(Issue #500)。色分けのためだけに使い、構文は検証しない(検証は Rust の parse_mml)。
// パーサーと同じく、空白を除いて ASCII だけ小文字化して読む。範囲は元のテキストの添字(UTF-16)。
// コメント(// と /* */)は mmlComments.ts が範囲を求め、種別 comment のトークンにする。
import { TIMBRE_LIST } from "./mmlTimbres";
import { blankComments, findComments } from "./mmlComments";

export type MmlTokenKind =
  | "note"
  | "rest"
  | "command"
  | "timbre"
  | "number"
  | "repeat"
  | "separator"
  | "comment"
  | "unknown";

export interface MmlToken {
  from: number;
  to: number;
  kind: MmlTokenKind;
}

interface Sym {
  ch: string;
  from: number;
  to: number;
}

const NOTES = new Set(["a", "b", "c", "d", "e", "f", "g"]);
const NUMBERED_COMMANDS = new Set(["o", "l", "t", "v", "q"]);
const TIMBRE_NAMES: string[] = TIMBRE_LIST.map((p) => p.name);

function asciiLower(ch: string): string {
  return ch >= "A" && ch <= "Z" ? ch.toLowerCase() : ch;
}

function isDigit(ch: string): boolean {
  return ch.length === 1 && ch >= "0" && ch <= "9";
}

function isAccidental(ch: string): boolean {
  return ch === "#" || ch === "+" || ch === "-";
}

/** 空白を除いた (小文字化した文字, 元の範囲) の列 */
function symbols(text: string): Sym[] {
  const out: Sym[] = [];
  let at = 0;
  for (const ch of text) {
    const from = at;
    at += ch.length;
    if (!/\s/u.test(ch)) out.push({ ch: asciiLower(ch), from, to: at });
  }
  return out;
}

export function lexMml(text: string): MmlToken[] {
  // コメントは空白にして読み飛ばす(添字は変わらない)。コメントそのものは最後にトークンとして足す
  const syms = symbols(blankComments(text));
  const out: MmlToken[] = [];
  const at = (i: number): string => (i < syms.length ? syms[i].ch : "");
  const push = (start: number, end: number, kind: MmlTokenKind): void => {
    out.push({ from: syms[start].from, to: syms[end - 1].to, kind });
  };
  const digitsEnd = (i: number): number => {
    let j = i;
    while (isDigit(at(j))) j++;
    return j;
  };
  /** 音符・休符の後ろの長さと付点を number として積み、次の位置を返す */
  const lengthAndDots = (i: number): number => {
    let j = digitsEnd(i);
    while (at(j) === ".") j++;
    if (j > i) push(i, j, "number");
    return j;
  };
  /** i から始まる音色名の長さ(記号数)。無ければ 0。最長一致 */
  const timbreNameLen = (i: number): number => {
    let best = 0;
    for (const name of TIMBRE_NAMES) {
      if (name.length > best && i + name.length <= syms.length && [...name].every((c, k) => syms[i + k].ch === c)) {
        best = name.length;
      }
    }
    return best;
  };

  let i = 0;
  while (i < syms.length) {
    const ch = syms[i].ch;
    if (NOTES.has(ch)) {
      let j = i + 1;
      if (isAccidental(at(j))) j++;
      push(i, j, "note");
      i = lengthAndDots(j);
    } else if (ch === "r") {
      push(i, i + 1, "rest");
      i = lengthAndDots(i + 1);
    } else if (ch === "&") {
      push(i, i + 1, "rest");
      i++;
    } else if (ch === "^") {
      push(i, i + 1, "rest");
      i = lengthAndDots(i + 1);
    } else if (NUMBERED_COMMANDS.has(ch)) {
      // `v+2` `v-2` は、符号までを 1 つのコマンドにする
      const cmdEnd = i + (ch === "v" && (at(i + 1) === "+" || at(i + 1) === "-") ? 2 : 1);
      push(i, cmdEnd, "command");
      const j = digitsEnd(cmdEnd);
      if (j > cmdEnd) push(cmdEnd, j, "number");
      i = j;
    } else if (ch === "<" || ch === ">") {
      push(i, i + 1, "command");
      i++;
    } else if (ch === "@") {
      let j = digitsEnd(i + 1);
      if (j === i + 1) j = i + 1 + timbreNameLen(i + 1);
      push(i, j, "timbre");
      i = j;
    } else if (ch === "[" || ch === "{" || ch === ":") {
      push(i, i + 1, "repeat");
      i++;
    } else if (ch === "]") {
      const j = digitsEnd(i + 1);
      push(i, j, "repeat");
      i = j;
    } else if (ch === "}") {
      // `}` の後の長さと付点まで、1 つのトークンにする
      let j = digitsEnd(i + 1);
      while (at(j) === ".") j++;
      push(i, j, "repeat");
      i = j;
    } else if (ch === ";") {
      push(i, i + 1, "separator");
      i++;
    } else {
      push(i, i + 1, "unknown");
      i++;
    }
  }
  for (const { from, to } of findComments(text)) out.push({ from, to, kind: "comment" });
  return out.sort((a, b) => a.from - b.from);
}
