import type { DriveFile } from "../bindings/tauri.gen";

export const isImage = (f: DriveFile) => f.mimeType.startsWith("image/");
export const isVideo = (f: DriveFile) => f.mimeType.startsWith("video/");
export const isAudio = (f: DriveFile) => f.mimeType.startsWith("audio/");

// VidstackのisVideoSrc()は拡張子なしURL(MisskeyのドライブURL)だと VIDEO_TYPES
// (mp4/webm/3gp/ogg/avi/mpeg)に含まれるMIMEしか動画と認識せず、それ以外は<video>プロバイダが
// 生成されない。実際のコンテナ判別はブラウザ側が行うため、未対応MIMEはvideo/mp4として渡す。
const VIDSTACK_VIDEO_TYPES = new Set(["video/mp4", "video/webm", "video/3gp", "video/ogg", "video/avi", "video/mpeg"]);
export const playerSrc = (f: DriveFile) => ({
  src: f.url,
  type: isVideo(f) && !VIDSTACK_VIDEO_TYPES.has(f.mimeType) ? "video/mp4" : f.mimeType,
});
export const fileName = (f: DriveFile) => f.name || f.mimeType || "file";

/// MediaViewerで前後送りする対象（画像・動画・音声）だけを残す。
/// その他のファイル(非メディア)は MediaGrid 側で FileList に分離して表示するためここでは除外する。
export function deriveViewItems(files: DriveFile[]): DriveFile[] {
  return files.filter((f) => isImage(f) || isVideo(f) || isAudio(f));
}

export function nextIndex(current: number, length: number): number {
  return (current + 1) % length;
}

export function prevIndex(current: number, length: number): number {
  return (current - 1 + length) % length;
}

/// 閲覧注意でなく、かつ低帯域幅モードでもなければ常に表示可。それ以外は revealed に登録済みかどうかで判定する
/// (閲覧注意と低帯域幅が重なっても、登録は 1 回で足りる)。
export function isRevealed(
  revealed: Record<string, boolean>,
  file: DriveFile,
  lowBandwidth = false,
): boolean {
  return (!file.isSensitive && !lowBandwidth) || !!revealed[file.id];
}

/// 未読み込みカバーの文言。閲覧注意なら従来の文言を優先する。
export function coverLabel(file: DriveFile): string {
  return file.isSensitive ? "閲覧注意（クリックで表示）" : "タップで読み込む";
}

export function reveal(
  revealed: Record<string, boolean>,
  file: DriveFile,
): Record<string, boolean> {
  return { ...revealed, [file.id]: true };
}

export type ImageTransform = {
  rotation: 0 | 90 | 180 | 270;
  flipH: boolean;
  flipV: boolean;
};

export const initialImageTransform: ImageTransform = { rotation: 0, flipH: false, flipV: false };

const ROTATIONS = [0, 90, 180, 270] as const;

export function rotateCW(t: ImageTransform): ImageTransform {
  const i = ROTATIONS.indexOf(t.rotation);
  return { ...t, rotation: ROTATIONS[(i + 1) % ROTATIONS.length] };
}

export function rotateCCW(t: ImageTransform): ImageTransform {
  const i = ROTATIONS.indexOf(t.rotation);
  return { ...t, rotation: ROTATIONS[(i - 1 + ROTATIONS.length) % ROTATIONS.length] };
}

export function toggleFlipH(t: ImageTransform): ImageTransform {
  return { ...t, flipH: !t.flipH };
}

export function toggleFlipV(t: ImageTransform): ImageTransform {
  return { ...t, flipV: !t.flipV };
}

/// 変形後の画像の外接矩形がコンテナをはみ出している(=パンする意味がある)かどうか。
/// matrixはCropper.jsのtransformイベントのdetail.matrix([a, b, c, d, e, f]のCSS行列)、
/// naturalは<cropper-image>の自然サイズ(画像はこのサイズの箱に行列を掛けて描画される)。
/// 倍率の絶対値(a>1)で見ると、contain フィットで1未満に縮んだ大きい画像はいくら拡大しても
/// 判定が立たず、90度回転でaが0、反転でaが負になる。そのため外接矩形のサイズで判定する。
/// フィット時は一辺がコンテナとちょうど等しくなるので、丸め誤差ぶんのtolerancePxを許す。
export function isImageOverflowing(
  matrix: readonly number[],
  natural: { width: number; height: number },
  container: { width: number; height: number },
  tolerancePx = 1,
): boolean {
  if (natural.width <= 0 || natural.height <= 0) return false;
  const [a, b, c, d] = matrix;
  const boundsWidth = Math.abs(a) * natural.width + Math.abs(c) * natural.height;
  const boundsHeight = Math.abs(b) * natural.width + Math.abs(d) * natural.height;
  return boundsWidth > container.width + tolerancePx || boundsHeight > container.height + tolerancePx;
}
