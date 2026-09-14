// モバイル版のカラムScroll Snapで「最上位rowの直下leafの並び」を解決する純粋関数(Issue #296)。
import type { PaneNode } from "../bindings/tauri.gen";

/// 最上位rowの直下の子のうちtype:"leaf"のgroupIdだけを、出現順で返す。
/// ネストしたsplit配下のカラムはカラム間Scroll Snapの対象外とする(Issue #296のスコープ決定)。
/// rootがdirection:"column"のsplit(「下に分割」で作られた縦並び)の場合は空配列を返す。
/// この関数の契約は「横並びの最上位row」限定であり、縦並びには左右の隣接カラムという
/// 概念自体が無い。ここでleafを返してしまうと、movePane(Edge::Left/Right = Row方向)を
/// 縦分割の木に対して実行して縦分割を勝手に横分割へ作り変えてしまう(Issue #354)。
export function topLevelLeafGroupIds(paneRoot: PaneNode): string[] {
  if (paneRoot.type === "leaf") return [paneRoot.groupId];
  if (paneRoot.direction !== "row") return [];
  return paneRoot.children.filter((c) => c.node.type === "leaf").map((c) => (c.node as { groupId: string }).groupId);
}

/// 最上位rowのDOM子1つにつき1エントリの「ページ順序」を返す(Issue #296 Finding 2)。
/// leafの位置にはgroupId、ネストしたsplit(非leaf)の位置にはnullを入れる。
/// topLevelLeafGroupIdsと違い、ネストしたsplitのラッパーdivも実DOM上は他のカラムと
/// 同じ1ページ分の幅を占有する(Task 6)ため、スクロール位置⇔インデックス変換には
/// このleaves-onlyでない配列を使う必要がある。
/// rootがdirection:"column"のsplitの場合は空配列を返す。Pane.svelteが横スクロールの
/// Scroll Snapコンテナを作るのはdirection==="row"の描画時だけなので、縦並びrootには
/// そもそも「横1ページ分の子」が1つも存在しない(nullを並べるとページ数が実DOMと
/// 食い違う)。呼び出し側はlength===0 / indexOf<0で既に早期returnする。
export function pageOrder(paneRoot: PaneNode): (string | null)[] {
  if (paneRoot.type === "leaf") return [paneRoot.groupId];
  if (paneRoot.direction !== "row") return [];
  return paneRoot.children.map((c) => (c.node.type === "leaf" ? (c.node as { groupId: string }).groupId : null));
}

export type AdjacentDirection = "prev" | "next";

/// groupIdの、topLevelLeafGroupIds上での隣(前/次)のtop-level leafカラムのgroupIdを返す。
/// 端・対象外のgroupId・ネストしたsplit配下のカラムの場合はnull。
export function adjacentColumnId(paneRoot: PaneNode, groupId: string, direction: AdjacentDirection): string | null {
  const order = topLevelLeafGroupIds(paneRoot);
  const i = order.indexOf(groupId);
  if (i < 0) return null;
  const j = direction === "prev" ? i - 1 : i + 1;
  if (j < 0 || j >= order.length) return null;
  return order[j];
}

/// groupIdが最上位rowのleafの並びの中で、前/次に隣接カラムへ移動できるかどうか。
export function canMoveAdjacentColumn(paneRoot: PaneNode, groupId: string, direction: AdjacentDirection): boolean {
  return adjacentColumnId(paneRoot, groupId, direction) !== null;
}
