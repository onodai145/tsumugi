// MML のマクロ(Issue #499)の定義 `$名前 = { 本体 }` を見つける。Rust の `mml.rs` の `collect_macros` と同じ規則
// (構文の検証は Rust 側の parse_mml)。範囲は元のテキストの添字(UTF-16)。コメントは、呼ぶ前に `blankComments` で
// 空白にしておく。名前は `$` の直後に途切れなく続く [A-Za-z0-9_]+、次(空白をはさんでよい)が `=` と `{` のときだけ定義。

export interface MacroDefinition {
  /** 小文字にした名前 */
  name: string;
  /** `$` の添字 */
  from: number;
  /** 閉じの `}` の次の添字(閉じていなければ文末) */
  to: number;
  /** 本体の始まり(`{` の次)の添字 */
  bodyFrom: number;
  /** 閉じの `}` の添字(閉じていなければ文末) */
  bodyTo: number;
}

const isNameChar = (ch: string): boolean => /^[A-Za-z0-9_]$/.test(ch);
const isSpace = (ch: string): boolean => /^\s$/u.test(ch);

/** 定義の範囲を、昇順で返す。本体の中の `$名前`(展開)は、定義としては数えない */
export function findMacroDefinitions(text: string): MacroDefinition[] {
  const skip = (from: number): number => {
    let k = from;
    while (k < text.length && isSpace(text[k])) k++;
    return k;
  };
  const out: MacroDefinition[] = [];
  let i = 0;
  while (i < text.length) {
    if (text[i] !== "$") {
      i++;
      continue;
    }
    let j = i + 1;
    while (j < text.length && isNameChar(text[j])) j++;
    if (j === i + 1) {
      i++;
      continue;
    }
    const eq = skip(j);
    if (text[eq] !== "=") {
      i = j;
      continue;
    }
    const open = skip(eq + 1);
    if (text[open] !== "{") {
      i = j;
      continue;
    }
    let depth = 1;
    let k = open + 1;
    while (k < text.length) {
      if (text[k] === "{") depth++;
      else if (text[k] === "}" && --depth === 0) break;
      k++;
    }
    const closed = k < text.length;
    const to = closed ? k + 1 : text.length;
    out.push({ name: text.slice(i + 1, j).toLowerCase(), from: i, to, bodyFrom: open + 1, bodyTo: k });
    i = to;
  }
  return out;
}

/** 定義を空白に置き換えた文字列。長さと、定義以外の各文字の添字は変わらない */
export function blankMacroDefinitions(text: string): string {
  const defs = findMacroDefinitions(text);
  if (defs.length === 0) return text;
  let out = "";
  let at = 0;
  for (const { from, to } of defs) {
    out += text.slice(at, from) + " ".repeat(to - from);
    at = to;
  }
  return out + text.slice(at);
}
