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

/// 解析・整形に渡す入力の最大文字数(コードポイント)。読み上げは `MAX_SPEECH_CHARS` で切るので、その数倍あれば十分で、
/// 数万文字の本文(長い空白の連続など)で、整形の正規表現と `parse` が UI スレッドを止めないための上限でもある。
const SOURCE_CAP = MAX_SPEECH_CHARS * 8;

/// 入力を `SOURCE_CAP` コードポイントまでに切る。`cut` は、実際に切ったか。O(上限)で、巨大な入力を全部は読まない。
function capSource(s: string): { text: string; cut: boolean } {
  if (s.length <= SOURCE_CAP) return { text: s, cut: false };
  let i = 0;
  let n = 0;
  while (i < s.length && n < SOURCE_CAP) {
    i += (s.codePointAt(i) ?? 0) > 0xffff ? 2 : 1;
    n++;
  }
  return i < s.length ? { text: s.slice(0, i), cut: true } : { text: s, cut: false };
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
  const raw = hasCw ? note.cw : note.text;
  if (!raw) return null;
  const { text: source, cut } = capSource(raw);
  const nodes = hasCw ? parseSimple(source) : parse(source);
  const text = normalize(plain(nodes));
  if (!text) return null;
  const out = truncate(text);
  // 入力を上限で切ったときは、整形後が短くなっても、続きがあることを伝える。
  return cut && !out.endsWith(ELLIPSIS) ? out + ELLIPSIS : out;
}

/// 投稿者の読み名。表示名のカスタム絵文字を除き、空なら username。
function authorName(user: User): string {
  const name = user.name ? normalize(plain(parseSimple(capSource(user.name).text))) : "";
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
