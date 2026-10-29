// MML の使用量(文字数・音数・トラック数・再生時間)と、上限に対する段階。
import { MML_LIMITS, NEAR_RATIO, SEC_EPS } from "./mmlLimits";
import { blankComments } from "./mmlComments";
import { blankMacroDefinitions } from "./mmlMacros";

export type UsageLevel = "normal" | "near" | "over";
export type UsageKey = "chars" | "notes" | "tracks" | "sec";

export interface UsageItem {
  key: UsageKey;
  label: string;
  /** 分からない(解析が通っていない)ときは null */
  value: number | null;
  limit: number;
  level: UsageLevel;
}

/** `parseMml` が成功したときの値 */
export interface ParsedUsage {
  noteCount: number;
  totalSec: number;
}

/** Rust の `chars().count()` と同じ数え方(サロゲートペアは 1 文字) */
export function countChars(text: string): number {
  return [...text].length;
}

/** パーサーと同じく、コメントとマクロの定義を除いてから `;` で分けた区間のうち、空白以外を含むものの数 */
export function countTracks(text: string): number {
  return blankMacroDefinitions(blankComments(text))
    .split(";")
    .filter((seg) => seg.trim() !== "").length;
}

export function usageLevel(value: number | null, limit: number): UsageLevel {
  if (value === null) return "normal";
  if (value > limit + SEC_EPS) return "over";
  if (value >= limit * NEAR_RATIO) return "near";
  return "normal";
}

export function computeUsage(text: string, parsed: ParsedUsage | null): UsageItem[] {
  const item = (key: UsageKey, label: string, value: number | null): UsageItem => ({
    key,
    label,
    value,
    limit: MML_LIMITS[key],
    level: usageLevel(value, MML_LIMITS[key]),
  });
  return [
    item("chars", "文字", countChars(text)),
    item("notes", "音", parsed ? parsed.noteCount : null),
    item("tracks", "トラック", countTracks(text)),
    item("sec", "再生", parsed ? parsed.totalSec : null),
  ];
}

export function formatUsage(item: UsageItem): string {
  const value = item.value === null ? "—" : item.key === "sec" ? item.value.toFixed(1) : String(item.value);
  const unit = item.key === "sec" ? " 秒" : "";
  return `${item.label} ${value}/${item.limit}${unit}`;
}
