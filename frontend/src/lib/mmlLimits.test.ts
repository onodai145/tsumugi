import { describe, expect, it } from "vitest";
import { MML_LIMITS, NEAR_RATIO } from "./mmlLimits";

describe("MML_LIMITS", () => {
  // src-tauri/src/mml.rs の MAX_TEXT_CHARS / MAX_NOTES / MAX_TRACKS / MAX_TOTAL_SEC と同じ値。
  // Rust 側のテスト(limits_match_frontend_table)も同じ表を持つ。片方だけ変えると、どちらかのテストが落ちる。
  it("Rust のパーサーの上限と同じ値", () => {
    expect(MML_LIMITS).toEqual({ chars: 5000, notes: 3000, tracks: 4, sec: 90 });
  });

  it("上限の 9 割から「近い」とみなす", () => {
    expect(NEAR_RATIO).toBe(0.9);
  });
});
