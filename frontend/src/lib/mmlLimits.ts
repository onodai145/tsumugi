// MML の上限。src-tauri/src/mml.rs の MAX_TEXT_CHARS / MAX_NOTES / MAX_TRACKS / MAX_TOTAL_SEC と同じ値にする。
// 両側のテストが同じ表を持つ(mmlLimits.test.ts と mml.rs の limits_match_frontend_table)。
export const MML_LIMITS = { chars: 5000, notes: 3000, tracks: 4, sec: 90 } as const;

/** 上限のこの割合以上になったら「近い」と知らせる */
export const NEAR_RATIO = 0.9;

/** 再生時間の浮動小数点誤差の許容幅。Rust の TIME_EPS と同じ */
export const SEC_EPS = 1e-6;
