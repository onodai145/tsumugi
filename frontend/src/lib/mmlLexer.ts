// MML の寛容な字句解析(Issue #500)。色分けのためだけに使い、構文は検証しない(検証は Rust の parse_mml)。
// パーサーと同じく、空白を除いて ASCII だけ小文字化して読む。範囲は元のテキストの添字(UTF-16)。
// コメント(// と /* */)は mmlComments.ts が範囲を求め、種別 comment のトークンにする。
import { TIMBRE_LIST } from "./mmlTimbres";
import { blankComments, findComments } from "./mmlComments";
import { findMacroDefinitions } from "./mmlMacros";

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
const NUMBERED_COMMANDS = new Set(["o", "l", "t", "v", "q", "p"]);
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
  const blanked = blankComments(text);
  const syms = symbols(blanked);
  // マクロの定義の頭(`$`)と閉じ(`}`)の位置。定義の閉じの `}` は、後ろの長さ・付点を取り込まない
  const defs = findMacroDefinitions(blanked);
  const defStarts = new Set(defs.map((d) => d.from));
  const defCloses = new Set(defs.map((d) => d.bodyTo));
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
  // 和音 `'…'` の中にいるか(開きと閉じの判定)。閉じていない `'` は、文末まで和音の中として読む
  let inChord = false;
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
      // `v+2` `v-2` は符号まで、`t=140` は `=` までを 1 つのコマンドにする
      const signed = ch === "v" && (at(i + 1) === "+" || at(i + 1) === "-");
      const shared = ch === "t" && at(i + 1) === "=";
      const cmdEnd = i + (signed || shared ? 2 : 1);
      push(i, cmdEnd, "command");
      const j = digitsEnd(cmdEnd);
      if (j > cmdEnd) push(cmdEnd, j, "number");
      i = j;
    } else if (ch === "y" || ch === "m" || ch === "k") {
      // 音色パラメーター(Issue #499): `y a,d,s,r` `m 速さ,深さ` `k セント`。`k+` `k-` は符号まで、カンマは command にする
      const signed = ch === "k" && (at(i + 1) === "+" || at(i + 1) === "-");
      const cmdEnd = i + (signed ? 2 : 1);
      push(i, cmdEnd, "command");
      let j = digitsEnd(cmdEnd);
      if (j > cmdEnd) push(cmdEnd, j, "number");
      while (at(j) === ",") {
        push(j, j + 1, "command");
        const next = digitsEnd(j + 1);
        if (next > j + 1) push(j + 1, next, "number");
        j = next;
      }
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
      if (defCloses.has(syms[i].from)) {
        // マクロの定義の閉じ。連符の `}` と違い、後ろの長さ・付点は取り込まない
        push(i, i + 1, "repeat");
        i++;
      } else {
        // `}` の後の長さと付点まで、1 つのトークンにする
        let j = digitsEnd(i + 1);
        while (at(j) === ".") j++;
        push(i, j, "repeat");
        i = j;
      }
    } else if (ch === "'") {
      if (!inChord) {
        push(i, i + 1, "repeat");
        inChord = true;
        i++;
      } else {
        // 閉じの `'` は、後ろの長さと付点まで、1 つのトークンにする
        let j = digitsEnd(i + 1);
        while (at(j) === ".") j++;
        push(i, j, "repeat");
        inChord = false;
        i = j;
      }
    } else if (ch === "$") {
      // マクロ。名前は、元のテキストで `$` の直後に途切れなく続く [a-z0-9_]+(空白・コメントで終わる)
      let j = i + 1;
      while (j < syms.length && /^[a-z0-9_]$/.test(syms[j].ch) && syms[j].from === syms[j - 1].to) j++;
      if (j === i + 1) {
        push(i, i + 1, "unknown");
        i++;
      } else if (defStarts.has(syms[i].from)) {
        // 定義の頭 `$名前 =`(空白は syms に無いので、`=` は名前のすぐ後ろ)
        push(i, j + 1, "command");
        i = j + 1;
      } else {
        // 展開の後ろに空白で区切って書いた長さ・付点(`$riff 4`)は、本体の最後の音符の長さになる
        push(i, j, "command");
        i = lengthAndDots(j);
      }
    } else if (ch === ";") {
      push(i, i + 1, "separator");
      inChord = false; // 和音の状態はトラックごと(Rust のパーサーも `;` でトラックを分けてから読む)
      i++;
    } else {
      push(i, i + 1, "unknown");
      i++;
    }
  }
  // コメントをまたぐトークン(`t/*x*/=60`、`v/**/+2`)は、コメントの範囲で切って、コメントと重ならないようにする。
  // `out` は from の昇順なので、コメントを指す添字は後ろへ進めるだけでよい。
  const comments = findComments(text);
  const result: MmlToken[] = [];
  let ci = 0;
  // 切ったあとの断片が空白だけ(コメントの間の空白)なら捨てる
  const pushPiece = (from: number, to: number, kind: MmlTokenKind): void => {
    if (/\S/u.test(text.slice(from, to))) result.push({ from, to, kind });
  };
  for (const tok of out) {
    while (ci < comments.length && comments[ci].to <= tok.from) ci++;
    let from = tok.from;
    for (let k = ci; k < comments.length && comments[k].from < tok.to; k++) {
      if (comments[k].from > from) pushPiece(from, comments[k].from, tok.kind);
      from = Math.max(from, comments[k].to);
    }
    if (from < tok.to) pushPiece(from, tok.to, tok.kind);
  }
  for (const { from, to } of comments) result.push({ from, to, kind: "comment" });
  return result.sort((a, b) => a.from - b.from);
}
