import type { Note } from "../bindings/tauri.gen";

export type ReplyPreviewBody =
  | { kind: "cw" | "text"; text: string }
  | { kind: "label"; label: string };

/**
 * 返信先カード(複数行表示)に出す本文を決める。優先順位は CW → 本文 → ファイル件数 → Renote。
 * 複数行表示のため、改行・連続した空白は畳まない(前後の trim のみ)。
 * 何も出せなければ null(呼び出し側は本文行を出さない)。
 */
export function replyCardBody(reply: Note): ReplyPreviewBody | null {
  if (reply.cw?.trim()) return { kind: "cw", text: reply.cw.trim() };
  if (reply.text?.trim()) return { kind: "text", text: reply.text.trim() };
  const n = reply.files.length;
  if (n > 0) return { kind: "label", label: n === 1 ? "(1つのファイル)" : `(${n}個のファイル)` };
  if (reply.renote) return { kind: "label", label: "(Renote)" };
  return null;
}
