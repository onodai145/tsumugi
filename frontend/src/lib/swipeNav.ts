// モバイル版の左右スワイプで「次/前のタブ・カラム」を解決する純粋関数(Issue #296)。
// store.svelte.tsとの循環importを避けるため、GroupViewそのものではなく最小限の
// 構造(SwipeGroup)を受け取る。GroupViewは構造的にこれを満たすのでそのまま渡せる。
import type { PaneNode } from "../bindings/tauri.gen";

export type SwipeDirection = "prev" | "next";

export interface SwipeGroup {
  id: string;
  tabs: { id: string }[];
  activeTabId: string;
}

export type SwipeTarget = { kind: "tab"; groupId: string; tabId: string } | { kind: "group"; groupId: string } | null;

/// 最上位rowの直下の子のうちtype:"leaf"のgroupIdだけを、出現順で返す。
/// ネストしたsplit配下のカラムはカラム間スワイプの対象外とする(Issue #296のスコープ決定)。
export function topLevelLeafGroupIds(paneRoot: PaneNode): string[] {
  if (paneRoot.type === "leaf") return [paneRoot.groupId];
  return paneRoot.children.filter((c) => c.node.type === "leaf").map((c) => (c.node as { groupId: string }).groupId);
}

/// group内でのタブ送りを試み、タブが1つしかない/端に達している場合は
/// 最上位rowの直下leafの並びから次/前のカラムを解決する。移動先が無ければnull。
export function resolveSwipeTarget(
  groups: SwipeGroup[],
  paneRoot: PaneNode,
  groupId: string,
  direction: SwipeDirection,
): SwipeTarget {
  const group = groups.find((g) => g.id === groupId);
  if (!group || group.tabs.length === 0) return null;

  const currentIndex = Math.max(0, group.tabs.findIndex((t) => t.id === group.activeTabId));

  if (direction === "next" && currentIndex < group.tabs.length - 1) {
    return { kind: "tab", groupId, tabId: group.tabs[currentIndex + 1].id };
  }
  if (direction === "prev" && currentIndex > 0) {
    return { kind: "tab", groupId, tabId: group.tabs[currentIndex - 1].id };
  }

  const order = topLevelLeafGroupIds(paneRoot);
  const groupIndex = order.indexOf(groupId);
  if (groupIndex < 0) return null;
  const targetIndex = direction === "next" ? groupIndex + 1 : groupIndex - 1;
  if (targetIndex < 0 || targetIndex >= order.length) return null;
  return { kind: "group", groupId: order[targetIndex] };
}
