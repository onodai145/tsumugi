import type { EmojiDef } from "../bindings/tauri.gen";
import { FN_ARGS, KNOWN_FN, type MfmArgSpec } from "./mfm";
import { UNICODE_EMOJIS } from "./unicodeEmojiList";

export type Trigger =
  | { kind: "emoji"; query: string; start: number; end: number }
  | { kind: "fnName"; query: string; start: number; end: number }
  | { kind: "argName"; fnName: string; query: string; start: number; end: number }
  | { kind: "argValue"; fnName: string; argName: string; query: string; start: number; end: number }
  | { kind: "mention"; query: string; start: number; end: number }
  | { kind: "hashtag"; query: string; start: number; end: number };

const IDENT = /^[a-zA-Z0-9_]*$/;
// 直前が行頭または空白/開き括弧類のときだけ ":" を絵文字トリガーの開始とみなす
// (英数字に直接くっついた ":"(例: "http:")を誤検出しないため)。
const EMOJI_TRIGGER = /(?:^|[\s([{"'>])(:[a-zA-Z0-9_+-]*)$/;
const MENTION_TRIGGER = /(?:^|[\s([{"'>])(@[a-zA-Z0-9_-]+(?:@[a-zA-Z0-9_.-]+)?)$/;
const HASHTAG_TRIGGER = /(?:^|[\s([{"'>])(#\S+)$/;

function detectFnTrigger(text: string, cursor: number): Trigger | null {
  const head = text.slice(0, cursor);
  const openIdx = head.lastIndexOf("$[");
  if (openIdx === -1) return null;
  const seg = head.slice(openIdx + 2);
  if (seg.includes("]")) return null; // 直近の $[ はすでに閉じている
  if (/\s/.test(seg)) return null; // 本文コンテンツに入っている(呼び出し側が絵文字トリガーを別途評価する)

  const dotIdx = seg.indexOf(".");
  if (dotIdx === -1) {
    if (!IDENT.test(seg)) return null;
    return { kind: "fnName", query: seg, start: openIdx + 2, end: cursor };
  }

  const fnName = seg.slice(0, dotIdx);
  const argsStr = seg.slice(dotIdx + 1);
  const lastComma = argsStr.lastIndexOf(",");
  const argSeg = lastComma === -1 ? argsStr : argsStr.slice(lastComma + 1);
  const argSegStart = openIdx + 2 + dotIdx + 1 + (lastComma === -1 ? 0 : lastComma + 1);

  const eqIdx = argSeg.indexOf("=");
  if (eqIdx === -1) {
    if (!IDENT.test(argSeg)) return null;
    return { kind: "argName", fnName, query: argSeg, start: argSegStart, end: cursor };
  }

  const argName = argSeg.slice(0, eqIdx);
  const valueQuery = argSeg.slice(eqIdx + 1);
  if (!IDENT.test(valueQuery)) return null;
  const spec = FN_ARGS[fnName]?.find((a) => a.name === argName);
  if (!spec?.enum) return null; // 列挙値を持つ引数のみ値補完する
  const valueStart = argSegStart + eqIdx + 1;
  return { kind: "argValue", fnName, argName, query: valueQuery, start: valueStart, end: cursor };
}

function detectEmojiTrigger(text: string, cursor: number): Trigger | null {
  const head = text.slice(0, cursor);
  const m = head.match(EMOJI_TRIGGER);
  if (!m) return null;
  const matched = m[1]; // ":query" (先頭の境界文字は含まない)
  return { kind: "emoji", query: matched.slice(1), start: cursor - matched.length, end: cursor };
}

function detectMentionTrigger(text: string, cursor: number): Trigger | null {
  const head = text.slice(0, cursor);
  const m = head.match(MENTION_TRIGGER);
  if (!m) return null;
  const matched = m[1]; // "@query"
  return { kind: "mention", query: matched.slice(1), start: cursor - matched.length, end: cursor };
}

function detectHashtagTrigger(text: string, cursor: number): Trigger | null {
  const head = text.slice(0, cursor);
  const m = head.match(HASHTAG_TRIGGER);
  if (!m) return null;
  const matched = m[1]; // "#query"
  return { kind: "hashtag", query: matched.slice(1), start: cursor - matched.length, end: cursor };
}

export function detectTrigger(text: string, cursor: number): Trigger | null {
  return (
    detectFnTrigger(text, cursor) ??
    detectEmojiTrigger(text, cursor) ??
    detectMentionTrigger(text, cursor) ??
    detectHashtagTrigger(text, cursor)
  );
}

const MAX_MATCHES = 10;

function startsWithCI(name: string, query: string): boolean {
  return name.toLowerCase().startsWith(query.toLowerCase());
}

export interface EmojiMatch {
  key: string;
  kind: "custom" | "unicode";
  name: string;
  url?: string;
  char?: string;
}

export function matchEmojis(query: string, customEmojis: EmojiDef[]): EmojiMatch[] {
  const custom: EmojiMatch[] = customEmojis
    .filter((e) => startsWithCI(e.name, query) || e.aliases.some((a) => startsWithCI(a, query)))
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((e) => ({ key: `custom:${e.name}`, kind: "custom", name: e.name, url: e.url }));

  const remaining = MAX_MATCHES - custom.length;
  const unicode: EmojiMatch[] =
    remaining > 0
      ? UNICODE_EMOJIS.filter((e) => startsWithCI(e.name, query))
          .sort((a, b) => a.name.localeCompare(b.name))
          .slice(0, remaining)
          .map((e) => ({ key: `unicode:${e.name}`, kind: "unicode", name: e.name, char: e.char }))
      : [];

  return [...custom.slice(0, MAX_MATCHES), ...unicode];
}

export function matchFnNames(query: string): string[] {
  return [...KNOWN_FN]
    .filter((name) => startsWithCI(name, query))
    .sort((a, b) => a.localeCompare(b))
    .slice(0, MAX_MATCHES);
}

export function matchArgNames(fnName: string, query: string): MfmArgSpec[] {
  const specs = FN_ARGS[fnName] ?? [];
  return specs.filter((s) => startsWithCI(s.name, query)).slice(0, MAX_MATCHES);
}

export function matchArgValues(fnName: string, argName: string, query: string): string[] {
  const spec = (FN_ARGS[fnName] ?? []).find((s) => s.name === argName);
  const values = spec?.enum ?? [];
  return values.filter((v) => startsWithCI(v, query)).slice(0, MAX_MATCHES);
}

export interface CompletionThumbnail {
  type: "custom" | "unicode" | "avatar";
  url?: string;
  char?: string;
}

export interface CompletionItem {
  key: string;
  label: string;
  insertText: string;
  thumbnail?: CompletionThumbnail;
}

export type SyncTrigger = Exclude<Trigger, { kind: "mention" } | { kind: "hashtag" }>;

export function buildCompletionItems(trigger: SyncTrigger, customEmojis: EmojiDef[]): CompletionItem[] {
  switch (trigger.kind) {
    case "emoji":
      return matchEmojis(trigger.query, customEmojis).map((m) => ({
        key: m.key,
        label: m.name,
        insertText: `:${m.name}:`,
        thumbnail:
          m.kind === "custom" ? { type: "custom" as const, url: m.url } : { type: "unicode" as const, char: m.char },
      }));
    case "fnName":
      return matchFnNames(trigger.query).map((name) => ({ key: name, label: name, insertText: name }));
    case "argName":
      return matchArgNames(trigger.fnName, trigger.query).map((spec) => ({
        key: spec.name,
        label: spec.hasValue ? `${spec.name}=` : spec.name,
        insertText: spec.hasValue ? `${spec.name}=` : spec.name,
      }));
    case "argValue":
      return matchArgValues(trigger.fnName, trigger.argName, trigger.query).map((value) => ({
        key: value,
        label: value,
        insertText: value,
      }));
  }
}

export function applyCompletion(
  text: string,
  trigger: Trigger,
  item: CompletionItem,
): { text: string; cursor: number } {
  const next = text.slice(0, trigger.start) + item.insertText + text.slice(trigger.end);
  return { text: next, cursor: trigger.start + item.insertText.length };
}
