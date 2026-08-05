import type { SourceItem, TqlCompletionItem, UserList } from "../bindings/tauri.gen";
import type { CompletionItem } from "./mfmCompletion";

export interface TqlTrigger {
  start: number;
  end: number;
}

export interface TqlIdTrigger {
  trigger: TqlTrigger;
  kind: "list" | "antenna" | "channel";
  query: string;
}

// list("..." / antenna("..." / channel("..." の、閉じられていない文字列リテラルの中を検出する。
// (閉じ引用符が既にある場合はマッチしない = tokenize可能な通常の構文補完へフォールバックする)
const ID_ARG_RE = /(list|antenna|channel)\(\s*"([^"]*)$/;
const WORD_CHAR = /[A-Za-z0-9_]/;

export function detectIdArgTrigger(text: string, cursor: number): TqlIdTrigger | null {
  const head = text.slice(0, cursor);
  const m = ID_ARG_RE.exec(head);
  if (!m) return null;
  const [, kind, query] = m;
  return {
    trigger: { start: cursor - query.length, end: cursor },
    kind: kind as "list" | "antenna" | "channel",
    query,
  };
}

// カーソル直前の識別子(ASCII英数字と'_')の範囲を返す。Rust側 current_word() と同じ規則。
export function currentWordTrigger(text: string, cursor: number): TqlTrigger {
  let start = cursor;
  while (start > 0 && WORD_CHAR.test(text[start - 1])) start--;
  return { start, end: cursor };
}

// JSのカーソル位置(UTF-16コード単位)をUnicodeコードポイント数へ変換する
// (Rust側は cursor_chars をコードポイント数として受け取るため)。
export function charOffset(text: string, cursor: number): number {
  return [...text.slice(0, cursor)].length;
}

function startsWithCI(s: string, query: string): boolean {
  return s.toLowerCase().startsWith(query.toLowerCase());
}

export function idCandidates(
  kind: "list" | "antenna" | "channel",
  query: string,
  lists: UserList[],
  antennas: SourceItem[],
  channels: SourceItem[],
): CompletionItem[] {
  const pool = kind === "list" ? lists : kind === "antenna" ? antennas : channels;
  return pool
    .filter((x) => startsWithCI(x.name || x.id, query))
    .map((x) => ({ key: x.id, label: x.name || x.id, insertText: `${x.id}")` }));
}

export function syntaxCandidates(items: TqlCompletionItem[]): CompletionItem[] {
  return items.map((it) => ({ key: it.label, label: it.label, insertText: it.insert }));
}

export function applyTqlCompletion(
  text: string,
  trigger: TqlTrigger,
  item: CompletionItem,
): { text: string; cursor: number } {
  const next = text.slice(0, trigger.start) + item.insertText + text.slice(trigger.end);
  return { text: next, cursor: trigger.start + item.insertText.length };
}
