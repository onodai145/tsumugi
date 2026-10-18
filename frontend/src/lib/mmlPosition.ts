// `parseMml` のエラー位置を、エディタの文字列添字にする。
// Rust の `position` は Unicode スカラ値の添字。JS の文字列は UTF-16 なので、サロゲートペアがあるとずれる。

/** Rust の文字位置を、`text` の添字(UTF-16)にする。文字数以上なら `text.length` */
export function charPosToIndex(text: string, pos: number): number {
  let index = 0;
  let n = 0;
  for (const ch of text) {
    if (n >= pos) break;
    index += ch.length;
    n++;
  }
  return index;
}

/** エラー下線の範囲。空文書は null。位置が文字数以上なら最後の 1 文字 */
export function errorRange(text: string, pos: number): { from: number; to: number } | null {
  const chars = [...text];
  if (chars.length === 0) return null;
  const i = Math.max(0, Math.min(pos, chars.length - 1));
  const from = charPosToIndex(text, i);
  return { from, to: from + chars[i].length };
}
