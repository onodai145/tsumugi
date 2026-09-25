import type { DriveFile } from "../bindings/tauri.gen";

export const isImage = (f: DriveFile) => f.mimeType.startsWith("image/");
export const isVideo = (f: DriveFile) => f.mimeType.startsWith("video/");
export const isAudio = (f: DriveFile) => f.mimeType.startsWith("audio/");
export const fileName = (f: DriveFile) => f.name || f.mimeType || "file";

/// MediaViewerで前後送りする対象（画像・動画・音声）だけを残す。
/// その他のファイル（📄表示のもの）はMediaGrid側で従来通りopenUrlするためここでは除外する。
export function deriveViewItems(files: DriveFile[]): DriveFile[] {
  return files.filter((f) => isImage(f) || isVideo(f) || isAudio(f));
}

export function nextIndex(current: number, length: number): number {
  return (current + 1) % length;
}

export function prevIndex(current: number, length: number): number {
  return (current - 1 + length) % length;
}

/// 閲覧注意でなければ常に表示可、閲覧注意なら revealed に登録済みかどうかで判定する。
export function isRevealed(revealed: Record<string, boolean>, file: DriveFile): boolean {
  return !file.isSensitive || !!revealed[file.id];
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
