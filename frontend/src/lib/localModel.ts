import type { LocalModelStatus } from "./ipc";

/// MB（1MB = 1,000,000 バイト）で小数1桁。モデルのサイズ表示用。
export function formatBytes(n: number): string {
  return `${(n / 1_000_000).toFixed(1)} MB`;
}

/// 0〜100 の整数。total が 0 以下なら 0。
export function progressPercent(downloaded: number, total: number): number {
  if (total <= 0) return 0;
  return Math.max(0, Math.min(100, Math.floor((downloaded / total) * 100)));
}

export function statusText(s: LocalModelStatus): string {
  switch (s.type) {
    case "notInstalled":
      return "未取得";
    case "ready":
      return "取得済み";
    case "verifying":
      return "検証中…";
    case "downloading":
      return `ダウンロード中 ${(s.downloaded / 1_000_000).toFixed(1)} / ${formatBytes(s.total)}（${progressPercent(s.downloaded, s.total)}%）`;
    case "failed":
      return `失敗: ${s.reason}`;
  }
}
