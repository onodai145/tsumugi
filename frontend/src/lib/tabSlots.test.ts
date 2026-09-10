import { describe, expect, it } from "vitest";
import { activeSlotIndex, computeTabSlots, notesForSlot } from "./tabSlots";

function tab(id: string) {
  return { id };
}

describe("computeTabSlots", () => {
  it("中間のタブは前/アクティブ/次の3スロットになる", () => {
    const tabs = [tab("a"), tab("b"), tab("c")];
    expect(computeTabSlots(tabs, "b")).toEqual([
      { tab: tab("a"), role: "prev" },
      { tab: tab("b"), role: "active" },
      { tab: tab("c"), role: "next" },
    ]);
  });

  it("先頭タブはprevスロットが無い(2スロット)", () => {
    const tabs = [tab("a"), tab("b"), tab("c")];
    expect(computeTabSlots(tabs, "a")).toEqual([
      { tab: tab("a"), role: "active" },
      { tab: tab("b"), role: "next" },
    ]);
  });

  it("末尾タブはnextスロットが無い(2スロット)", () => {
    const tabs = [tab("a"), tab("b"), tab("c")];
    expect(computeTabSlots(tabs, "c")).toEqual([
      { tab: tab("b"), role: "prev" },
      { tab: tab("c"), role: "active" },
    ]);
  });

  it("タブが1つしかない場合は1スロットのみ", () => {
    const tabs = [tab("a")];
    expect(computeTabSlots(tabs, "a")).toEqual([{ tab: tab("a"), role: "active" }]);
  });

  it("activeTabIdが見つからない場合は空配列", () => {
    const tabs = [tab("a"), tab("b")];
    expect(computeTabSlots(tabs, "missing")).toEqual([]);
  });
});

describe("activeSlotIndex", () => {
  it("3スロット中のactiveの位置(中央=1)を返す", () => {
    const slots = computeTabSlots([tab("a"), tab("b"), tab("c")], "b");
    expect(activeSlotIndex(slots)).toBe(1);
  });

  it("先頭タブ(2スロット)ではactiveは0番目", () => {
    const slots = computeTabSlots([tab("a"), tab("b")], "a");
    expect(activeSlotIndex(slots)).toBe(0);
  });
});

describe("notesForSlot", () => {
  const notes = Array.from({ length: 80 }, (_, i) => ({ id: `n${i}` }));

  it("activeスロットは全件そのまま返す", () => {
    expect(notesForSlot(notes, "active")).toHaveLength(80);
  });

  it("prev/nextスロットは先頭50件に制限する", () => {
    expect(notesForSlot(notes, "prev")).toHaveLength(50);
    expect(notesForSlot(notes, "next")).toHaveLength(50);
    expect(notesForSlot(notes, "prev")[0]).toEqual({ id: "n0" });
  });

  it("件数が制限未満ならそのまま返す", () => {
    const few = notes.slice(0, 10);
    expect(notesForSlot(few, "next")).toHaveLength(10);
  });
});
