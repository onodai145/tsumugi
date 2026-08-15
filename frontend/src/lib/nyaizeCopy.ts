// nyaize()で変換された文字列から、変換前の元の文字列を復元するためのユーティリティ。
// nyaize()は原文字の削除・並べ替えを行わない（1文字->複数文字展開、末尾への文字追加のみ）ため、
// 編集距離DP（挿入コストのみ低い非対称コスト）で十分実用的な対応表が得られる。

const MAX_DP_MIDDLE_LEN = 2000;

/**
 * nyaized の各文字が original の何文字目に対応するかを表す配列を構築する。
 * 戻り値の長さは nyaized.length。値は単調非減少。
 */
export function buildNyaizeCharMap(original: string, nyaized: string): number[] {
  const n = original.length;
  const m = nyaized.length;

  if (m === 0) return [];
  if (n === 0) return new Array(m).fill(0);

  // 共通の接頭辞・接尾辞をトリムし、DPの対象を実際に変化した中間部分だけに絞る
  // （nyaize()の変化は通常ごく一部のみなので、この最適化で実用上のコストを大幅に下げられる）。
  let prefix = 0;
  const maxPrefix = Math.min(n, m);
  while (prefix < maxPrefix && original[prefix] === nyaized[prefix]) prefix++;

  let suffix = 0;
  const maxSuffix = Math.min(n, m) - prefix;
  while (suffix < maxSuffix && original[n - 1 - suffix] === nyaized[m - 1 - suffix]) suffix++;

  const origMidStart = prefix;
  const origMidEnd = n - suffix;
  const nyaMidStart = prefix;
  const nyaMidEnd = m - suffix;

  const origMid = original.slice(origMidStart, origMidEnd);
  const nyaMid = nyaized.slice(nyaMidStart, nyaMidEnd);

  let midMap: number[];
  if (origMid.length > MAX_DP_MIDDLE_LEN || nyaMid.length > MAX_DP_MIDDLE_LEN) {
    // 極端に長い差分領域（実運用では通常発生しない）に対する安全弁。
    // 精密な整列を諦め、比例配分で近似する。
    midMap = new Array(nyaMid.length);
    for (let j = 0; j < nyaMid.length; j++) {
      const ratio = nyaMid.length <= 1 ? 0 : j / (nyaMid.length - 1);
      let idx = Math.round(ratio * Math.max(0, origMid.length - 1));
      if (idx < 0) idx = 0;
      if (idx > origMid.length - 1) idx = Math.max(0, origMid.length - 1);
      midMap[j] = idx;
    }
  } else {
    midMap = buildNyaizeCharMapDp(origMid, nyaMid);
  }

  const map = new Array<number>(m);
  for (let j = 0; j < prefix; j++) map[j] = j;
  for (let j = 0; j < midMap.length; j++) map[prefix + j] = origMidStart + midMap[j];
  for (let j = 0; j < suffix; j++) map[m - suffix + j] = n - suffix + j;
  return map;
}

/**
 * トリム前提のない編集距離DP本体。
 * dp[i][j] = original[0..i) と nyaized[0..j) を対応付けるための最小コスト。
 * 操作: match/substitute(original[i-1] <-> nyaized[j-1], コスト0 or 1) /
 *       insert(nyaized[j-1]だけ消費, original側を消費しない, コスト1) /
 *       delete(original[i-1]だけ消費, コスト2 = 極力避ける)
 */
function buildNyaizeCharMapDp(original: string, nyaized: string): number[] {
  const n = original.length;
  const m = nyaized.length;

  if (m === 0) return [];
  if (n === 0) return new Array(m).fill(0);

  const INSERT_COST = 1;
  const DELETE_COST = 2;
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
  for (let i = 1; i <= n; i++) dp[i][0] = i * DELETE_COST;
  for (let j = 1; j <= m; j++) dp[0][j] = j * INSERT_COST;

  for (let i = 1; i <= n; i++) {
    for (let j = 1; j <= m; j++) {
      const subCost = original[i - 1] === nyaized[j - 1] ? 0 : 1;
      dp[i][j] = Math.min(
        dp[i - 1][j - 1] + subCost, // match/substitute
        dp[i][j - 1] + INSERT_COST, // insert (originalを消費しない)
        dp[i - 1][j] + DELETE_COST, // delete (nyaizedを消費しない)
      );
    }
  }

  // バックトレースして、各 nyaized インデックスが対応する original インデックスを求める。
  const map = new Array<number>(m);
  let i = n;
  let j = m;
  while (j > 0) {
    const subCost = i > 0 && original[i - 1] === nyaized[j - 1] ? 0 : 1;
    if (dp[i][j] === dp[i][j - 1] + INSERT_COST) {
      j -= 1;
      // 挿入された文字は直前(まだ消費していない)の original インデックスに対応付ける。
      map[j] = i > 0 ? i - 1 : 0;
    } else if (i > 0 && dp[i][j] === dp[i - 1][j - 1] + subCost) {
      i -= 1;
      j -= 1;
      map[j] = i;
    } else {
      i -= 1;
    }
  }

  // 単調性の保証（挿入が先頭に来た場合など、負値/未設定を0側に丸める）。
  for (let k = 0; k < m; k++) {
    if (map[k] === undefined) map[k] = 0;
    if (map[k] < 0) map[k] = 0;
  }
  return map;
}

/**
 * nyaized 側の半開区間 [start, end) を、map 経由で original の対応部分文字列に変換する。
 */
export function mapNyaizedRangeToOriginal(
  map: number[],
  original: string,
  start: number,
  end: number,
): string {
  if (start >= end || start < 0 || end > map.length) return "";
  const origStart = map[start];
  const origEndExclusive = map[end - 1] + 1;
  return original.slice(origStart, Math.max(origStart, origEndExclusive));
}

// TreeWalkerが「境界マーカー」として個別に visit する要素タグ。
// BR: 明示的な改行。IMG: カスタム/Unicode絵文字（altテキストをコピー結果に含める）。
// それ以外の要素（BLOCKQUOTE/PRE含む）はFILTER_SKIPされるが、子孫は通常通りwalkされ続ける。
const BOUNDARY_TAGS = new Set(["BR", "IMG"]);

// mfm-jsが引用・コードブロックの前後に literal な "\n" を出力しないため、
// ブロック要素をまたいだ選択でも改行が失われないよう、直近のブロック祖先が
// 変化するたびに "\n" を補う（入る時・出る時の両方で機能する）。
const BLOCK_SELECTOR = "blockquote, pre";

function pushNewlineIfNeeded(pieces: string[]): void {
  const last = pieces.length > 0 ? pieces[pieces.length - 1] : undefined;
  if (last !== undefined && !last.endsWith("\n")) {
    pieces.push("\n");
  }
}

/**
 * range内に data-original-text を持つ要素が1つでも含まれるかどうかを判定する。
 * nyaize対象外の選択（例: cw/text内にnyaize対象が一切ないケース）を早期に見分け、
 * ネイティブコピー動作へフォールバックするために使う。
 */
function rangeTouchesNyaizedContent(container: HTMLElement, range: Range): boolean {
  const candidates = container.querySelectorAll<HTMLElement>("[data-original-text]");
  for (const el of candidates) {
    if (range.intersectsNode(el)) return true;
  }
  return false;
}

function collectRangePieces(container: HTMLElement, range: Range): string[] {
  const pieces: string[] = [];
  const walker = document.createTreeWalker(
    container,
    NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT,
    {
      acceptNode(node) {
        if (node.nodeType === Node.ELEMENT_NODE && !BOUNDARY_TAGS.has((node as Element).tagName)) {
          return NodeFilter.FILTER_SKIP;
        }
        return range.intersectsNode(node) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
      },
    },
  );

  let lastBlockAncestor: Element | null = null;
  let node = walker.nextNode();
  while (node) {
    if (node.nodeType === Node.ELEMENT_NODE) {
      const el = node as Element;
      const blockAncestor = el.closest(BLOCK_SELECTOR);
      if (blockAncestor !== lastBlockAncestor) {
        pushNewlineIfNeeded(pieces);
        lastBlockAncestor = blockAncestor;
      }
      if (el.tagName === "IMG") {
        pieces.push(el.getAttribute("alt") ?? "");
      } else {
        // BR: 直前の出力が改行で終わっていなければ改行を1つ差し込む。
        pushNewlineIfNeeded(pieces);
      }
    } else {
      const textNode = node as Text;
      const blockAncestor = textNode.parentElement?.closest(BLOCK_SELECTOR) ?? null;
      if (blockAncestor !== lastBlockAncestor) {
        pushNewlineIfNeeded(pieces);
        lastBlockAncestor = blockAncestor;
      }

      const full = textNode.textContent ?? "";
      const start = textNode === range.startContainer ? range.startOffset : 0;
      const end = textNode === range.endContainer ? range.endOffset : full.length;

      const originalAncestor = textNode.parentElement?.closest<HTMLElement>("[data-original-text]");
      if (originalAncestor) {
        const original = originalAncestor.dataset.originalText ?? "";
        const map = buildNyaizeCharMap(original, full);
        pieces.push(mapNyaizedRangeToOriginal(map, original, start, end));
      } else {
        pieces.push(full.slice(start, end));
      }
    }
    node = walker.nextNode();
  }
  return pieces;
}

/**
 * copyイベントを横取りし、nyaize済みテキストの選択範囲を元の（nyaize前の）文字列に
 * 差し替えてクリップボードに書き込む。以下の場合は何もせず、ブラウザデフォルトの
 * コピー動作にフォールバックする:
 *   - 選択がコンテナ外にまたがる場合
 *   - 選択範囲が data-original-text を持つ要素を一切含まない場合
 *     （非catノートや、catノートでもnyaize対象外の部分だけの選択など）
 */
export function handleNyaizeCopy(event: ClipboardEvent): void {
  const container = event.currentTarget as HTMLElement | null;
  if (!container) return;

  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return;

  const range = selection.getRangeAt(0);
  if (!container.contains(range.startContainer) || !container.contains(range.endContainer)) {
    return;
  }

  if (!rangeTouchesNyaizedContent(container, range)) return;

  const pieces = collectRangePieces(container, range);
  if (!event.clipboardData) return;
  event.clipboardData.setData("text/plain", pieces.join(""));
  event.preventDefault();
}
