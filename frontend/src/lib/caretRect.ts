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

function rangeRect(range: Range): CaretRect | null {
  if (typeof range.getClientRects !== "function") return null;
  const rects = range.getClientRects();
  const first =
    rects.length > 0
      ? rects[0]
      : typeof range.getBoundingClientRect === "function"
        ? range.getBoundingClientRect()
        : null;
  if (!first || (first.width === 0 && first.height === 0)) return null;
  return toRect(first.left, first.top, 1, first.height);
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
    const rect = rangeRect(range);
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
