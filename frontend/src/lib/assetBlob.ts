import { convertFileSrc } from "@tauri-apps/api/core";

/// asset:// のファイルを fetch() で取得し、blob: URL にして返す。
/// dev ビルドのように http オリジン(http://127.0.0.1:5173)から読み込んだページでは、
/// WebKitGTK(2.54.1で確認)が asset:// を <img> や CSS の background-image から直接読む
/// (no-cors のリソースローダー経由)と読み込みに失敗する。tauri://localhost オリジン
/// (埋め込みフロントのビルド)では読める。fetch() 経由ならどちらでも取得できるため、
/// 一度 Blob にしてから blob: URL で参照する。返した URL は呼び出し側が revokeObjectURL で解放する。
export async function loadAssetBlobUrl(path: string): Promise<string> {
  const res = await fetch(convertFileSrc(path));
  if (!res.ok) throw new Error(`status ${res.status}`);
  return URL.createObjectURL(await res.blob());
}
