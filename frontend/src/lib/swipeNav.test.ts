import { describe, expect, it } from "vitest";
import type { PaneNode } from "../bindings/tauri.gen";
import { canMoveAdjacentColumn, pageOrder, swapAdjacentColumns, topLevelLeafGroupIds } from "./swipeNav";

function leaf(id: string, groupId: string): PaneNode {
  return { type: "leaf", id, groupId };
}

function row(id: string, children: PaneNode[]): PaneNode {
  return { type: "split", id, direction: "row", children: children.map((node) => ({ node, size: null, auto: true })) };
}

describe("topLevelLeafGroupIds", () => {
  it("最上位rowの直下のleafのgroupIdを出現順で返す", () => {
    const root = row("root", [leaf("l1", "g1"), leaf("l2", "g2"), leaf("l3", "g3")]);
    expect(topLevelLeafGroupIds(root)).toEqual(["g1", "g2", "g3"]);
  });

  it("ネストしたsplit配下のleafは含めない", () => {
    const nested = row("nested", [leaf("l2", "g2"), leaf("l3", "g3")]);
    const root = row("root", [leaf("l1", "g1"), nested]);
    expect(topLevelLeafGroupIds(root)).toEqual(["g1"]);
  });

  it("rootがleaf単体の場合はそのgroupIdのみ返す", () => {
    expect(topLevelLeafGroupIds(leaf("l1", "g1"))).toEqual(["g1"]);
  });
});

describe("pageOrder", () => {
  it("leafのみのrootではtopLevelLeafGroupIdsと同じ配列になる", () => {
    const root = row("root", [leaf("l1", "g1"), leaf("l2", "g2"), leaf("l3", "g3")]);
    expect(pageOrder(root)).toEqual(["g1", "g2", "g3"]);
  });

  it("leafの間にネストしたsplitがある場合、splitの位置はnullになる", () => {
    const nested = row("nested", [leaf("l2", "g2"), leaf("l3", "g3")]);
    const root = row("root", [leaf("l1", "g1"), nested, leaf("l4", "g4")]);
    expect(pageOrder(root)).toEqual(["g1", null, "g4"]);
  });

  it("rootがleaf単体の場合はそのgroupIdのみ返す", () => {
    expect(pageOrder(leaf("l1", "g1"))).toEqual(["g1"]);
  });
});

describe("canMoveAdjacentColumn", () => {
  const root = row("root", [leaf("l1", "g1"), leaf("l2", "g2"), leaf("l3", "g3")]);

  it("先頭カラムはprevへ移動できない", () => {
    expect(canMoveAdjacentColumn(root, "g1", "prev")).toBe(false);
  });
  it("末尾カラムはnextへ移動できない", () => {
    expect(canMoveAdjacentColumn(root, "g3", "next")).toBe(false);
  });
  it("中間カラムはどちらへも移動できる", () => {
    expect(canMoveAdjacentColumn(root, "g2", "prev")).toBe(true);
    expect(canMoveAdjacentColumn(root, "g2", "next")).toBe(true);
  });
  it("対象外のgroupIdはfalseを返す", () => {
    expect(canMoveAdjacentColumn(root, "missing", "next")).toBe(false);
  });
});

describe("swapAdjacentColumns", () => {
  const root = row("root", [leaf("l1", "g1"), leaf("l2", "g2"), leaf("l3", "g3")]);
  const groups = [{ id: "g1" }, { id: "g2" }, { id: "g3" }];

  it("nextで隣のカラムと入れ替わる", () => {
    const result = swapAdjacentColumns(groups, root, "g1", "next");
    expect(result?.map((g) => g.id)).toEqual(["g2", "g1", "g3"]);
  });

  it("prevで隣のカラムと入れ替わる", () => {
    const result = swapAdjacentColumns(groups, root, "g3", "prev");
    expect(result?.map((g) => g.id)).toEqual(["g1", "g3", "g2"]);
  });

  it("先頭カラムでprevを指定するとnullを返す", () => {
    expect(swapAdjacentColumns(groups, root, "g1", "prev")).toBeNull();
  });

  it("末尾カラムでnextを指定するとnullを返す", () => {
    expect(swapAdjacentColumns(groups, root, "g3", "next")).toBeNull();
  });

  it("ネストしたsplit配下のカラムはnullを返す", () => {
    const nested = row("nested", [leaf("l4", "g4"), leaf("l5", "g5")]);
    const rootWithNested = row("root", [leaf("l1", "g1"), nested]);
    const groupsWithNested = [{ id: "g1" }, { id: "g4" }, { id: "g5" }];
    expect(swapAdjacentColumns(groupsWithNested, rootWithNested, "g4", "next")).toBeNull();
  });
});
