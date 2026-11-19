import { describe, expect, it } from "vitest";
import { RecentIds } from "./recentIds";

describe("RecentIds", () => {
  it("初めての ID は true、同じ ID は false", () => {
    const r = new RecentIds(3);
    expect(r.mark("a")).toBe(true);
    expect(r.mark("a")).toBe(false);
    expect(r.mark("b")).toBe(true);
  });

  it("上限を超えたら、古い ID から忘れる(忘れた ID は、また true になる)", () => {
    const r = new RecentIds(2);
    r.mark("a");
    r.mark("b");
    r.mark("c"); // a を忘れる
    expect(r.mark("b")).toBe(false);
    expect(r.mark("c")).toBe(false);
    expect(r.mark("a")).toBe(true);
  });

  it("インスタンスどうしは独立している(通知と読み上げが、互いの枠を食わない)", () => {
    const notified = new RecentIds(2);
    const spoken = new RecentIds(2);
    notified.mark("n1");
    notified.mark("n2");
    notified.mark("n3"); // notified だけが n1 を忘れる
    expect(spoken.mark("n1")).toBe(true);
    expect(spoken.mark("n1")).toBe(false);
  });
});
