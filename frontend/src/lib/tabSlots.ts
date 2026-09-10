// モバイル版のタブ横スワイプ(Scroll Snap)で「どのタブをスロットとして描画するか」を
// 解決する純粋関数(Issue #296)。前/アクティブ/次の最大3スロットを組み立てる。

export interface TabSlot<T extends { id: string }> {
  tab: T;
  role: "prev" | "active" | "next";
}

const PREVIEW_NOTE_LIMIT = 50;

/// タブ配列とアクティブなタブIDから、描画すべきスロット(最大3つ、出現順)を返す。
/// アクティブタブが見つからない場合は空配列を返す。
export function computeTabSlots<T extends { id: string }>(tabs: T[], activeTabId: string): TabSlot<T>[] {
  const index = tabs.findIndex((t) => t.id === activeTabId);
  if (index < 0) return [];
  const slots: TabSlot<T>[] = [];
  if (index > 0) slots.push({ tab: tabs[index - 1], role: "prev" });
  slots.push({ tab: tabs[index], role: "active" });
  if (index < tabs.length - 1) slots.push({ tab: tabs[index + 1], role: "next" });
  return slots;
}

/// computeTabSlotsの結果からアクティブスロットのインデックス(0始まり)を返す。
export function activeSlotIndex<T extends { id: string }>(slots: TabSlot<T>[]): number {
  return slots.findIndex((s) => s.role === "active");
}

/// 非アクティブ(prev/next)スロットは、フルマウントのコストを抑えるため先頭
/// PREVIEW_NOTE_LIMIT件だけ描画する。アクティブスロットは全件そのまま返す。
export function notesForSlot<N>(notes: N[], role: "prev" | "active" | "next"): N[] {
  if (role === "active") return notes;
  return notes.slice(0, PREVIEW_NOTE_LIMIT);
}
