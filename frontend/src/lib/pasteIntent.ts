/**
 * `paste` イベントを横取りして Rust 側のクリップボード読み取りに回すべきかを、
 * 同期的に判定する(Issue #117)。
 *
 * `preventDefault()` は paste ハンドラ内で同期的に呼ぶ必要があり、Rust への問い合わせ(非同期)
 * の結果を待ってからでは既定のテキスト貼り付けを止められないため、DOM から同期的に取れる
 * `types` と `text/plain` だけで判定する。WebKitGTK では `text/uri-list` があると `text/plain`
 * が DOM から隠され、`text/plain` のみのときは `types` が空になる(spec の実測表を参照)。
 */
export function shouldInterceptPaste(types: readonly string[], plainText: string): boolean {
  if (types.includes("text/uri-list") || types.includes("Files")) return true;
  return plainText === "";
}
