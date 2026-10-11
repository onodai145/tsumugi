import type { Note } from "../bindings/tauri.gen";

export type ReplyPreviewBody =
  | { kind: "cw" | "text"; text: string }
  | { kind: "label"; label: string };

/**
 * 返信先の1行プレビューに出す本文を決める。
 * CW があれば本文より優先し(CW 配下の本文を隠す意図を尊重する)、本文が無ければ
 * ファイル/Renote の種別ラベルにする。何も出せなければ null(呼び出し側は表示名だけにする)。
 * 返す text/cw は1行表示のため、改行を含む連続した空白を1つの半角スペースに畳んで trim 済み。
 */
export function replyPreviewBody(reply: Note): ReplyPreviewBody | null {
  if (reply.cw?.trim()) return { kind: "cw", text: reply.cw.replace(/\s+/g, " ").trim() };
  if (reply.text?.trim()) return { kind: "text", text: reply.text.replace(/\s+/g, " ").trim() };
  if (reply.files.length > 0) {
    const allImages = reply.files.every((f) => f.mimeType.startsWith("image/"));
    return { kind: "label", label: allImages ? "(画像)" : "(ファイル)" };
  }
  if (reply.renote) return { kind: "label", label: "(Renote)" };
  return null;
}
