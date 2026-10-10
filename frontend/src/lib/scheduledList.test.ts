import { describe, expect, it } from "vitest";
import { LOCAL_SCHEDULE_NOTICE, localStatusLabel } from "./scheduledList";

describe("localStatusLabel", () => {
  it("待機中以外の状態を日本語にする(待機中は表示しない)", () => {
    expect(localStatusLabel("pending")).toBeNull();
    expect(localStatusLabel("posting")).toBe("投稿中");
    expect(localStatusLabel("expired")).toBe("期限切れ");
    expect(localStatusLabel("failed")).toBe("投稿に失敗");
  });
});

describe("LOCAL_SCHEDULE_NOTICE", () => {
  it("ローカル予約の制約を伝える文言", () => {
    expect(LOCAL_SCHEDULE_NOTICE).toBe("アプリを起動している間だけ投稿されます");
  });
});
