// IME の変換候補ウィンドウの位置補正(Issue #502)に使う、キャレット矩形の算出。
// 座標は viewport 座標(CSS px)。DOM を読むだけで、何も書き換えない。
import { getCaretCoordinates } from "./caretPosition";

export interface CaretRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/// 位置の基準。数値は text 系入力欄の文字位置、Range は contenteditable の位置、null は現在のキャレット。
export type CaretAnchor = number | Range | null;

const TEXT_INPUT_TYPES = new Set(["text", "search", "url", "email", "password", "tel", "number"]);

function isContentEditable(el: HTMLElement): boolean {
  if (el.isContentEditable) return true;
  // jsdom など isContentEditable を実装しない環境向けに属性でも見る。
  const attr = el.getAttribute("contenteditable");
  return attr !== null && attr !== "false";
}

/// IME の候補位置を補正する対象の入力欄か。
export function isImeTarget(el: Element | null): el is HTMLElement {
  if (!(el instanceof HTMLElement)) return false;
  if (el instanceof HTMLTextAreaElement) return true;
  if (el instanceof HTMLInputElement) return TEXT_INPUT_TYPES.has(el.type);
  return isContentEditable(el);
}

function toRect(x: number, y: number, width: number, height: number): CaretRect | null {
  if (![x, y, width, height].every(Number.isFinite)) return null;
  return {
    x: Math.round(x),
    y: Math.round(y),
    width: Math.max(1, Math.round(width)),
    height: Math.max(1, Math.round(height)),
  };
}

/// キャレットが求められないときの代わり。入力欄の左下(候補は欄のすぐ下に出る)。
function fallbackRect(el: HTMLElement): CaretRect | null {
  const box = el.getBoundingClientRect();
  return toRect(box.left, box.bottom - 1, 1, 1);
}

function textFieldRect(
  el: HTMLInputElement | HTMLTextAreaElement,
  anchor: CaretAnchor,
): CaretRect | null {
  let position: number | null;
  try {
    // type=email / number などは selectionEnd が null になる。
    position = typeof anchor === "number" ? anchor : el.selectionEnd;
  } catch {
    position = null;
  }
  if (position === null) return fallbackRect(el);

  const box = el.getBoundingClientRect();
  const caret = getCaretCoordinates(el, Math.min(position, el.value.length));
  if (el instanceof HTMLInputElement) {
    // 1 行入力は縦方向を欄いっぱいに取る(候補は欄の下端のすぐ下に出る)。
    return toRect(box.left + caret.left, box.top, 1, box.height);
  }
  return toRect(box.left + caret.left, box.top + caret.top, 1, caret.height);
}

function firstRect(range: Range): DOMRect | null {
  if (typeof range.getClientRects !== "function") return null;
  const rects = range.getClientRects();
  const first =
    rects.length > 0
      ? rects[0]
      : typeof range.getBoundingClientRect === "function"
        ? range.getBoundingClientRect()
        : null;
  if (!first || (first.width === 0 && first.height === 0)) return null;
  return first;
}

/// WebKit は行頭・行末・空行などの折りたたまれた Range で空の矩形列を返すことがある。
/// テキストノード内なら、隣の 1 文字を測って代わりにする(次の文字の左端、なければ前の文字の右端)。
function adjacentCharRect(range: Range): CaretRect | null {
  const node = range.startContainer;
  if (node.nodeType !== Node.TEXT_NODE) return null;
  const length = (node as Text).length;
  const offset = range.startOffset;
  const probe = range.cloneRange();
  if (offset < length) {
    probe.setEnd(node, offset + 1);
    const r = firstRect(probe);
    return r ? toRect(r.left, r.top, 1, r.height) : null;
  }
  if (offset > 0) {
    probe.setStart(node, offset - 1);
    probe.setEnd(node, offset);
    const r = firstRect(probe);
    return r ? toRect(r.right, r.top, 1, r.height) : null;
  }
  return null;
}

/// 矩形が全く取れないとき、キャレットのある行(入力欄の根より内側の最も近い要素)を代わりにする。
/// 複数行の入力欄で、候補が欄全体の下端まで離れて出るのを避ける。
function enclosingLineRect(range: Range, root: HTMLElement): CaretRect | null {
  const node = range.startContainer;
  const element = node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement;
  if (!element || element === root || !root.contains(element)) return null;
  const box = element.getBoundingClientRect();
  return toRect(box.left, box.top, 1, box.height);
}

function rangeRect(range: Range, root: HTMLElement): CaretRect | null {
  const direct = firstRect(range);
  if (direct) return toRect(direct.left, direct.top, 1, direct.height);
  return adjacentCharRect(range) ?? enclosingLineRect(range, root);
}

function editableRect(el: HTMLElement, anchor: CaretAnchor): CaretRect | null {
  let range: Range | null = anchor instanceof Range ? anchor : null;
  if (!range) {
    const selection = el.ownerDocument.getSelection();
    if (selection && selection.rangeCount > 0) {
      range = selection.getRangeAt(0).cloneRange();
      range.collapse(false);
    }
  }
  if (range && el.contains(range.startContainer)) {
    const rect = rangeRect(range, el);
    if (rect) return rect;
  }
  return fallbackRect(el);
}

/// アクティブな入力欄のキャレット矩形(viewport 座標)を返す。求められなければ null。
export function caretRect(el: HTMLElement, anchor: CaretAnchor = null): CaretRect | null {
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    return textFieldRect(el, anchor);
  }
  return editableRect(el, anchor);
}
