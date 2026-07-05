// epoch秒 → 相対時刻の簡易表示
export function relativeTime(epochSec: number): string {
  if (!epochSec) return "";
  const diff = Date.now() / 1000 - epochSec;
  if (diff < 60) return `${Math.max(0, Math.floor(diff))}s`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h`;
  if (diff < 86400 * 7) return `${Math.floor(diff / 86400)}d`;
  return new Date(epochSec * 1000).toLocaleDateString();
}
