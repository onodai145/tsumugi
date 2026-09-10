// モバイル版のカラムScroll Snapで「最上位rowの直下leafの並び」を解決する純粋関数(Issue #296)。
import type { PaneNode } from "../bindings/tauri.gen";

/// 最上位rowの直下の子のうちtype:"leaf"のgroupIdだけを、出現順で返す。
/// ネストしたsplit配下のカラムはカラム間Scroll Snapの対象外とする(Issue #296のスコープ決定)。
export function topLevelLeafGroupIds(paneRoot: PaneNode): string[] {
  if (paneRoot.type === "leaf") return [paneRoot.groupId];
  return paneRoot.children.filter((c) => c.node.type === "leaf").map((c) => (c.node as { groupId: string }).groupId);
}

/// 最上位rowのDOM子1つにつき1エントリの「ページ順序」を返す(Issue #296 Finding 2)。
/// leafの位置にはgroupId、ネストしたsplit(非leaf)の位置にはnullを入れる。
/// topLevelLeafGroupIdsと違い、ネストしたsplitのラッパーdivも実DOM上は他のカラムと
/// 同じ1ページ分の幅を占有する(Task 6)ため、スクロール位置⇔インデックス変換には
/// このleaves-onlyでない配列を使う必要がある。
export function pageOrder(paneRoot: PaneNode): (string | null)[] {
  if (paneRoot.type === "leaf") return [paneRoot.groupId];
  return paneRoot.children.map((c) => (c.node.type === "leaf" ? (c.node as { groupId: string }).groupId : null));
}
