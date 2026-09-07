// Misskey本家 packages/frontend-shared/js/extract-avg-color-from-blurhash.ts を移植。
// BlurHash文字列の先頭数文字(DC成分)は画像全体の平均色を符号化しているため、
// フルデコードせずこの計算だけで平均色が求まる。

const BLURHASH_CHARS =
  "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~";

/** BlurHash文字列から画像全体の平均色を `#rrggbb` 形式で抽出する。無効な入力は undefined。 */
export function extractAvgColorFromBlurhash(hash: string | null | undefined): string | undefined {
  if (typeof hash !== "string" || hash.length < 6) return undefined;
  const value = [...hash.slice(2, 6)]
    .map((c) => BLURHASH_CHARS.indexOf(c))
    .reduce((a, c) => a * 83 + c, 0);
  return "#" + value.toString(16).padStart(6, "0");
}
