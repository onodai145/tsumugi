// 読み上げ(Issue #625)の文の整形。ノートから「読み上げる文字列」を作る純粋な関数だけを持つ。
// 合成・キュー・再生は Rust(src-tauri/src/tts/)の担当で、ここは MFM をプレーンな文にするところまで。
import { parse, parseSimple, type MfmNode } from "mfm-js";
import type { Note, User } from "../bindings/tauri.gen";
import { DEFAULT_COLUMN_SPEECH, type ColumnSpeechValue } from "./speechDefaults";

export { DEFAULT_COLUMN_SPEECH };

/// 1件の読み上げ本文の最大文字数(コードポイント)。超えたら切って「以下略」を付ける。
export const MAX_SPEECH_CHARS = 120;
const ELLIPSIS = "、以下略";

/// MFM のノードを、読み上げ用のプレーンな文字列にする。読めないもの(絵文字・数式・検索)は空にする。
function nodeText(n: MfmNode): string {
  switch (n.type) {
    case "text":
      return n.props.text;
    case "bold":
    case "italic":
    case "small":
    case "strike":
    case "center":
    case "quote":
    case "plain":
    case "fn":
      return plain(n.children);
    case "mention":
      return n.props.username;
    case "hashtag":
      return n.props.hashtag;
    case "url":
      return "URL";
    case "link": {
      const label = plain(n.children).trim();
      return label || "URL";
    }
    case "inlineCode":
    case "blockCode":
      return "コード";
    default:
      // emojiCode / unicodeEmoji / mathInline / mathBlock / search
      return "";
  }
}

function plain(nodes: MfmNode[]): string {
  return nodes.map(nodeText).join("");
}

/// 空白・改行を読みやすく整える。改行は読点にする(句読点の直後の改行は読点を足さない)。
function normalize(s: string): string {
  return s
    .replace(/([。．.!?！？、,，])?[ \t　]*\n[\s　]*/g, (_m, p: string | undefined) => p ?? "、")
    .replace(/[ \t　]+/g, " ")
    .replace(/、{2,}/g, "、")
    .replace(/^[\s、]+|[\s、]+$/g, "");
}

/// 上限(コードポイント単位)を超えたら切って「以下略」を付ける。サロゲートペアを割らない。
function truncate(s: string): string {
  const chars = Array.from(s);
  if (chars.length <= MAX_SPEECH_CHARS) return s;
  return chars.slice(0, MAX_SPEECH_CHARS).join("") + ELLIPSIS;
}

/// 読み上げの本文。純粋なリノートや、読む中身がないノートは null。
function bodyText(note: Note): string | null {
  // 純粋なリノート(本文も CW もない)は、元のノートが別に流れてくるので読まない。
  if (note.renoteId && !note.text && !note.cw) return null;
  // CW があれば CW だけ(本文は隠す意図があるので、読み上げて裏切らない)。
  const hasCw = !!note.cw && note.cw.trim() !== "";
  const source = hasCw ? note.cw : note.text;
  if (!source) return null;
  const nodes = hasCw ? parseSimple(source) : parse(source);
  const text = normalize(plain(nodes));
  return text ? truncate(text) : null;
}

/// 投稿者の読み名。表示名のカスタム絵文字を除き、空なら username。
function authorName(user: User): string {
  const name = user.name ? normalize(plain(parseSimple(user.name))) : "";
  return name || user.username;
}

/// ノート1件の読み上げ文。読まないときは null。`speakAuthor` がオンなら `{表示名}さん、` を前置きする。
export function buildSpeechText(note: Note, opts: { speakAuthor: boolean }): string | null {
  const body = bodyText(note);
  if (body === null) return null;
  return opts.speakAuthor ? `${authorName(note.user)}さん、${body}` : body;
}

/// 新着ノートを読み上げるか判定し、読むなら読み上げ文を返す(読まないなら null)。
/// 自分のノート・全体オフ・カラムオフのときは読まない。重複排除(`speak:<noteId>`)は呼び出し側(store)が行う。
export function speechTextForColumnNote(
  note: Note,
  ctx: { isOwn: boolean; globalEnabled: boolean; column: ColumnSpeechValue },
): string | null {
  if (ctx.isOwn || !ctx.globalEnabled || !ctx.column.enabled) return null;
  return buildSpeechText(note, { speakAuthor: ctx.column.speakAuthor });
}
