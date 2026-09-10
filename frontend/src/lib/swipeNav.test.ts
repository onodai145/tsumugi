import { describe, expect, it } from "vitest";
import type { PaneNode } from "../bindings/tauri.gen";
import { resolveSwipeTarget, topLevelLeafGroupIds, type SwipeGroup } from "./swipeNav";

function leaf(id: string, groupId: string): PaneNode {
  return { type: "leaf", id, groupId };
}

function row(id: string, children: PaneNode[]): PaneNode {
  return { type: "split", id, direction: "row", children: children.map((node) => ({ node, size: null, auto: true })) };
}

function group(id: string, tabIds: string[], activeTabId: string): SwipeGroup {
  return { id, tabs: tabIds.map((tid) => ({ id: tid })), activeTabId };
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

describe("resolveSwipeTarget", () => {
  it("次のタブがあればタブ送りを返す", () => {
    const groups = [group("g1", ["t1", "t2"], "t1")];
    const root = leaf("l1", "g1");
    expect(resolveSwipeTarget(groups, root, "g1", "next")).toEqual({ kind: "tab", groupId: "g1", tabId: "t2" });
  });

  it("前のタブがあればタブ戻りを返す", () => {
    const groups = [group("g1", ["t1", "t2"], "t2")];
    const root = leaf("l1", "g1");
    expect(resolveSwipeTarget(groups, root, "g1", "prev")).toEqual({ kind: "tab", groupId: "g1", tabId: "t1" });
  });

  it("末尾タブでさらに次へスワイプすると次のカラムを返す", () => {
    const groups = [group("g1", ["t1", "t2"], "t2"), group("g2", ["t3"], "t3")];
    const root = row("root", [leaf("l1", "g1"), leaf("l2", "g2")]);
    expect(resolveSwipeTarget(groups, root, "g1", "next")).toEqual({ kind: "group", groupId: "g2" });
  });

  it("先頭タブでさらに前へスワイプすると前のカラムを返す", () => {
    const groups = [group("g1", ["t1"], "t1"), group("g2", ["t2"], "t2")];
    const root = row("root", [leaf("l1", "g1"), leaf("l2", "g2")]);
    expect(resolveSwipeTarget(groups, root, "g2", "prev")).toEqual({ kind: "group", groupId: "g1" });
  });

  it("タブが1つしかないカラムはスワイプが直接カラム移動になる", () => {
    const groups = [group("g1", ["t1"], "t1"), group("g2", ["t2"], "t2")];
    const root = row("root", [leaf("l1", "g1"), leaf("l2", "g2")]);
    expect(resolveSwipeTarget(groups, root, "g1", "next")).toEqual({ kind: "group", groupId: "g2" });
  });

  it("末尾カラムの末尾タブでさらに次へは移動先なし(null)", () => {
    const groups = [group("g1", ["t1"], "t1")];
    const root = leaf("l1", "g1");
    expect(resolveSwipeTarget(groups, root, "g1", "next")).toBeNull();
  });

  it("ネストしたsplit配下のカラムはカラム間移動の対象外(見つからずnull)", () => {
    const groups = [group("g1", ["t1"], "t1"), group("g2", ["t2"], "t2")];
    const nested = row("nested", [leaf("l2", "g2")]);
    const root = row("root", [leaf("l1", "g1"), nested]);
    // g2はtopLevelLeafGroupIdsに含まれないため、g1からの次カラム移動先は無い
    expect(resolveSwipeTarget(groups, root, "g1", "next")).toBeNull();
  });

  it("groupが空(タブなし)ならnull", () => {
    const groups = [group("g1", [], "")];
    const root = leaf("l1", "g1");
    expect(resolveSwipeTarget(groups, root, "g1", "next")).toBeNull();
  });
});
