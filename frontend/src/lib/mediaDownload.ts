import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { commands, unwrap } from "./ipc";

/// メディアURLをファイルダイアログで選んだ保存先に書き出す。
/// MediaGrid（グリッド内の💾ボタン）とMediaViewer（拡大ビューワーのダウンロードボタン）の両方から使う。
export async function saveMediaToDisk(
  url: string,
  suggestedName: string,
  onError: (e: unknown) => void,
): Promise<void> {
  try {
    const path = await saveDialog({ defaultPath: suggestedName });
    if (!path) return;
    await unwrap(commands.saveUrlToFile(url, path));
  } catch (e) {
    onError(e);
  }
}
