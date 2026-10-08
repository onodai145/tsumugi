import { describe, expect, it } from "vitest";
import {
  epochSecToLocalInput,
  localInputToEpochSec,
  scheduleErrorMessage,
  validateSchedule,
} from "./schedule";

describe("localInputToEpochSec / epochSecToLocalInput", () => {
  it("datetime-local 文字列をローカルタイムゾーンの epoch 秒に変換する", () => {
    const sec = localInputToEpochSec("2026-10-08T09:30");
    expect(sec).toBe(Math.floor(new Date(2026, 9, 8, 9, 30).getTime() / 1000));
  });

  it("空文字・不正な文字列は null", () => {
    expect(localInputToEpochSec("")).toBeNull();
    expect(localInputToEpochSec("not a date")).toBeNull();
  });

  it("epoch 秒からローカル成分の datetime-local 文字列に往復できる", () => {
    const sec = Math.floor(new Date(2026, 0, 2, 3, 4).getTime() / 1000);
    expect(epochSecToLocalInput(sec)).toBe("2026-01-02T03:04");
    expect(localInputToEpochSec(epochSecToLocalInput(sec))).toBe(sec);
  });
});

describe("validateSchedule", () => {
  const now = Date.UTC(2026, 9, 7, 0, 0, 0);
  const sec = (offsetMs: number) => Math.floor((now + offsetMs) / 1000);

  it("未来の日時は ok", () => {
    expect(validateSchedule(sec(60_000), now, null)).toEqual({ ok: true });
  });

  // Review Focus 3: 入力後に時間が経って過去になった場合
  it("現在以前の日時は拒否する", () => {
    const r = validateSchedule(sec(-1000), now, null);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.message).toContain("現在より後");
    expect(validateSchedule(sec(0), now, null).ok).toBe(false);
  });

  it("投票の締切が予約日時以前なら拒否する", () => {
    const at = sec(3_600_000);
    const r = validateSchedule(at, now, at * 1000);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.message).toContain("投票の締切");
    expect(validateSchedule(at, now, at * 1000 - 1).ok).toBe(false);
    expect(validateSchedule(at, now, at * 1000 + 1).ok).toBe(true);
  });
});

describe("scheduleErrorMessage", () => {
  it("本家のエラーコードを日本語にする", () => {
    expect(
      scheduleErrorMessage("Error: api: notes/drafts/create: TOO_MANY_SCHEDULED_NOTES You cannot create scheduled notes any more."),
    ).toContain("上限");
    expect(scheduleErrorMessage("api: notes/drafts/create: SCHEDULED_AT_MUST_BE_IN_FUTURE x")).toContain("現在より後");
    expect(scheduleErrorMessage("api: notes/drafts/create: SCHEDULED_AT_REQUIRED x")).toContain("日時");
  });

  it("未知のエラーはそのまま返す", () => {
    expect(scheduleErrorMessage("Error: network error: boom")).toBe("Error: network error: boom");
  });
});
