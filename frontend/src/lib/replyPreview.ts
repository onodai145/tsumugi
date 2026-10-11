import type { Note } from "../bindings/tauri.gen";

export type ReplyPreviewBody =
  | { kind: "cw" | "text"; text: string }
  | { kind: "label"; label: string };

/**
 * 返信先の1行プレビューに出す本文を決める。
 * CW があれば本文より優先し(CW 配下の本文を隠す意図を尊重する)、本文が無ければ
 * ファイル/Renote の種別ラベルにする。何も出せなければ null(呼び出し側は表示名だけにする)。
 */
export function replyPreviewBody(reply: Note): ReplyPreviewBody | null {
  if (reply.cw?.trim()) return { kind: "cw", text: reply.cw };
  if (reply.text?.trim()) return { kind: "text", text: reply.text };
  if (reply.files.length > 0) {
    const allImages = reply.files.every((f) => f.mimeType.startsWith("image/"));
    return { kind: "label", label: allImages ? "(画像)" : "(ファイル)" };
  }
  if (reply.renote) return { kind: "label", label: "(Renote)" };
  return null;
}
