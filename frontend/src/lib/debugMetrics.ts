/** キャッシュhit率を表示用文字列にする。試行が0件なら"—"。Issue #241。 */
export function formatHitRate(hit: number, fallbackA: number, fallbackB: number): string {
  const total = hit + fallbackA + fallbackB;
  if (total === 0) return "—";
  return `${Math.round((hit / total) * 100)}%`;
}
