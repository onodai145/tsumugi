import { convertFileSrc } from "@tauri-apps/api/core";

/// asset:// のファイルを fetch() で取得し、blob: URL にして返す。
/// WebKitGTK 2.54 では、asset:// を <img> や CSS の background-image から直接読む
/// (no-cors のリソースローダー経由)と読み込みに失敗する。fetch() 経由なら取得できるため、
/// 一度 Blob にしてから blob: URL で参照する。返した URL は呼び出し側が revokeObjectURL で解放する。
export async function loadAssetBlobUrl(path: string): Promise<string> {
  const res = await fetch(convertFileSrc(path));
  if (!res.ok) throw new Error(`status ${res.status}`);
  return URL.createObjectURL(await res.blob());
}
