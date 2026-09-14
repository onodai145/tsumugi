import { describe, expect, it } from "vitest";
import { COLUMN_DRAG_HINT_THRESHOLD_PX, resolveColumnDragHint } from "./columnDragHint";

describe("resolveColumnDragHint", () => {
  it("閾値未満ではnullを返す", () => {
    expect(resolveColumnDragHint(COLUMN_DRAG_HINT_THRESHOLD_PX - 1, true, true)).toBeNull();
    expect(resolveColumnDragHint(-(COLUMN_DRAG_HINT_THRESHOLD_PX - 1), true, true)).toBeNull();
  });

  it("右方向へ閾値を超えるとnextを返す", () => {
    expect(resolveColumnDragHint(COLUMN_DRAG_HINT_THRESHOLD_PX, true, true)).toBe("next");
  });

  it("左方向へ閾値を超えるとprevを返す", () => {
    expect(resolveColumnDragHint(-COLUMN_DRAG_HINT_THRESHOLD_PX, true, true)).toBe("prev");
  });

  it("末尾カラムではnextを許可しない", () => {
    expect(resolveColumnDragHint(COLUMN_DRAG_HINT_THRESHOLD_PX, true, false)).toBeNull();
  });

  it("先頭カラムではprevを許可しない", () => {
    expect(resolveColumnDragHint(-COLUMN_DRAG_HINT_THRESHOLD_PX, false, true)).toBeNull();
  });
});
