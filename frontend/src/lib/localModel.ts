import type { LocalModelStatus, NetworkStatus } from "./ipc";

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

/// 回線の状態を取得できなかったとき（IPC の失敗）の値。「不明」として、安全側の注記つきの確認になる。
export const UNKNOWN_NETWORK: NetworkStatus = { transport: "unknown", metered: "unknown" };

/// ダウンロード前の確認の文面。確認が要らないとき（非従量制の Wi-Fi など）は null。
/// モバイル通信・従量制は警告、判定できないとき（回線なし・VPN・デスクトップ）は、安全側に倒して注記つきの確認にする。
export function downloadWarning(net: NetworkStatus, sizeBytes: number): string | null {
  const size = formatBytes(sizeBytes);
  if (net.transport === "cellular") return `モバイル通信を使用中です。約 ${size} を取得します。続けますか?`;
  if (net.metered === "metered") return `従量制の接続です。約 ${size} を取得します。続けますか?`;
  if (net.metered === "unmetered") return null;
  if (net.transport === "wifi" || net.transport === "ethernet") return null;
  return `回線の種類を判定できませんでした。約 ${size} を取得します。モバイル通信の場合はご注意ください。続けますか?`;
}
