// nyaize()で変換された文字列から、変換前の元の文字列を復元するためのユーティリティ。
// nyaize()は原文字の削除・並べ替えを行わない（1文字->複数文字展開、末尾への文字追加のみ）ため、
// 編集距離DP（挿入コストのみ低い非対称コスト）で十分実用的な対応表が得られる。

/**
 * nyaized の各文字が original の何文字目に対応するかを表す配列を構築する。
 * 戻り値の長さは nyaized.length。値は単調非減少。
 */
export function buildNyaizeCharMap(original: string, nyaized: string): number[] {
  const n = original.length;
  const m = nyaized.length;

  if (m === 0) return [];
  if (n === 0) return new Array(m).fill(0);

  // dp[i][j] = original[0..i) と nyaized[0..j) を対応付けるための最小コスト。
  // 操作: match/substitute(original[i-1] <-> nyaized[j-1], コスト0 or 1) /
  //       insert(nyaized[j-1]だけ消費, original側を消費しない, コスト1) /
  //       delete(original[i-1]だけ消費, コスト2 = 極力避ける)
  const INSERT_COST = 1;
  const DELETE_COST = 2;
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
  for (let i = 1; i <= n; i++) dp[i][0] = i * DELETE_COST;
  for (let j = 1; j <= m; j++) dp[0][j] = j * INSERT_COST;

  for (let i = 1; i <= n; i++) {
    for (let j = 1; j <= m; j++) {
      const subCost = original[i - 1] === nyaized[j - 1] ? 0 : 1;
      dp[i][j] = Math.min(
        dp[i - 1][j - 1] + subCost, // match/substitute
        dp[i][j - 1] + INSERT_COST, // insert (originalを消費しない)
        dp[i - 1][j] + DELETE_COST, // delete (nyaizedを消費しない)
      );
    }
  }

  // バックトレースして、各 nyaized インデックスが対応する original インデックスを求める。
  const map = new Array<number>(m);
  let i = n;
  let j = m;
  while (j > 0) {
    const subCost = i > 0 && original[i - 1] === nyaized[j - 1] ? 0 : 1;
    if (dp[i][j] === dp[i][j - 1] + INSERT_COST) {
      j -= 1;
      // 挿入された文字は直前(まだ消費していない)の original インデックスに対応付ける。
      map[j] = i > 0 ? i - 1 : 0;
    } else if (i > 0 && dp[i][j] === dp[i - 1][j - 1] + subCost) {
      i -= 1;
      j -= 1;
      map[j] = i;
    } else {
      i -= 1;
    }
  }

  // 単調性の保証（挿入が先頭に来た場合など、負値/未設定を0側に丸める）。
  for (let k = 0; k < m; k++) {
    if (map[k] === undefined) map[k] = 0;
    if (map[k] < 0) map[k] = 0;
  }
  return map;
}

/**
 * nyaized 側の半開区間 [start, end) を、map 経由で original の対応部分文字列に変換する。
 */
export function mapNyaizedRangeToOriginal(
  map: number[],
  original: string,
  start: number,
  end: number,
): string {
  if (start >= end || start < 0 || end > map.length) return "";
  const origStart = map[start];
  const origEndExclusive = map[end - 1] + 1;
  return original.slice(origStart, Math.max(origStart, origEndExclusive));
}
