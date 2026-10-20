// MML のコメント(Issue #499)。`// …` は行末(改行の手前)まで、`/* … */` は入れ子にできない範囲。
// 閉じていない `/*` は文末まで。Rust の `mml.rs` の `symbols` と同じ規則(構文の検証は Rust 側の parse_mml)。
// `/` 単独はコメントではない。範囲は元のテキストの添字(UTF-16)。パーサーが数える文字位置
// (Unicode スカラ値)との変換は `mmlPosition.ts` が持つ。

export interface CommentRange {
  from: number;
  to: number;
}

/** コメントの範囲を、昇順で返す。コメントの中の `//` や `/*` は、外側のコメントに含まれる */
export function findComments(text: string): CommentRange[] {
  const out: CommentRange[] = [];
  let i = 0;
  while (i < text.length) {
    if (text[i] === "/" && text[i + 1] === "/") {
      let j = i + 2;
      while (j < text.length && text[j] !== "\n") j++;
      out.push({ from: i, to: j });
      i = j;
    } else if (text[i] === "/" && text[i + 1] === "*") {
      const close = text.indexOf("*/", i + 2);
      const to = close < 0 ? text.length : close + 2;
      out.push({ from: i, to });
      i = to;
    } else {
      i++;
    }
  }
  return out;
}

/** コメントを空白に置き換えた文字列。長さと、コメント以外の各文字の添字は変わらない */
export function blankComments(text: string): string {
  const ranges = findComments(text);
  if (ranges.length === 0) return text;
  let out = "";
  let at = 0;
  for (const { from, to } of ranges) {
    out += text.slice(at, from) + " ".repeat(to - from);
    at = to;
  }
  return out + text.slice(at);
}
