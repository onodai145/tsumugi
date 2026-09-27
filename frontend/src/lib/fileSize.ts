const UNITS = ["B", "KB", "MB", "GB"] as const;

/// バイト数を "1.2 MB" 形式に整形する。サイズ不明・不正値は空文字（呼び出し側で非表示にする）。
export function formatFileSize(bytes: number | null | undefined): string {
  if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return "";
  if (bytes < 1024) return `${bytes} B`;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(1)} ${UNITS[unit]}`;
}
