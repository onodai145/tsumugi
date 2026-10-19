import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const getCaretCoordinatesMock = vi.hoisted(() => vi.fn());
vi.mock("./caretPosition", () => ({ getCaretCoordinates: getCaretCoordinatesMock }));

import { caretRect, isImeTarget } from "./caretRect";

function domRect(left: number, top: number, width: number, height: number): DOMRect {
  return {
    left, top, width, height,
    right: left + width, bottom: top + height, x: left, y: top,
    toJSON: () => ({}),
  } as DOMRect;
}

function stubBox(el: Element, left: number, top: number, width: number, height: number) {
  vi.spyOn(el, "getBoundingClientRect").mockReturnValue(domRect(left, top, width, height));
}

function makeTextarea(value: string): HTMLTextAreaElement {
  const el = document.createElement("textarea");
  el.value = value;
  document.body.appendChild(el);
  return el;
}

function makeInput(type: string, value: string): HTMLInputElement {
  const el = document.createElement("input");
  el.type = type;
  el.value = value;
  document.body.appendChild(el);
  return el;
}

beforeEach(() => {
  getCaretCoordinatesMock.mockReset();
  getCaretCoordinatesMock.mockReturnValue({ left: 5, top: 7, height: 18 });
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = "";
});

describe("isImeTarget", () => {
  it("accepts textarea and text-like inputs", () => {
    expect(isImeTarget(makeTextarea(""))).toBe(true);
    for (const type of ["text", "search", "url", "email", "password", "tel", "number"]) {
      expect(isImeTarget(makeInput(type, "")), type).toBe(true);
    }
  });

  it("rejects non-text inputs and other elements", () => {
    expect(isImeTarget(makeInput("checkbox", ""))).toBe(false);
    expect(isImeTarget(makeInput("range", ""))).toBe(false);
    expect(isImeTarget(document.createElement("button"))).toBe(false);
    expect(isImeTarget(document.createElement("div"))).toBe(false);
    expect(isImeTarget(null)).toBe(false);
  });

  it("accepts a contenteditable element", () => {
    const div = document.createElement("div");
    div.setAttribute("contenteditable", "true");
    document.body.appendChild(div);
    expect(isImeTarget(div)).toBe(true);
  });
});

describe("caretRect (textarea)", () => {
  it("adds the element origin to the caret coordinates", () => {
    const el = makeTextarea("abcdef");
    el.setSelectionRange(3, 3);
    stubBox(el, 10, 20, 200, 100);
    expect(caretRect(el)).toEqual({ x: 15, y: 27, width: 1, height: 18 });
    expect(getCaretCoordinatesMock).toHaveBeenCalledWith(el, 3);
  });

  it("prefers a numeric anchor over the current selection", () => {
    const el = makeTextarea("abcdef");
    el.setSelectionRange(5, 5);
    stubBox(el, 0, 0, 200, 100);
    caretRect(el, 1);
    expect(getCaretCoordinatesMock).toHaveBeenCalledWith(el, 1);
  });

  it("clamps an anchor beyond the text length", () => {
    const el = makeTextarea("abcdef");
    stubBox(el, 0, 0, 200, 100);
    caretRect(el, 99);
    expect(getCaretCoordinatesMock).toHaveBeenCalledWith(el, 6);
  });

  // Review Focus 1: 空の入力欄
  it("returns a rect for an empty textarea (caret at 0)", () => {
    const el = makeTextarea("");
    stubBox(el, 10, 20, 200, 100);
    expect(caretRect(el)).toEqual({ x: 15, y: 27, width: 1, height: 18 });
    expect(getCaretCoordinatesMock).toHaveBeenCalledWith(el, 0);
  });

  it("passes UTF-16 indices through unchanged for surrogate pairs", () => {
    const el = makeTextarea("😀あ");
    el.setSelectionRange(2, 2);
    stubBox(el, 0, 0, 200, 100);
    caretRect(el);
    expect(getCaretCoordinatesMock).toHaveBeenCalledWith(el, 2);
  });

  it("returns null when the coordinates are not finite", () => {
    const el = makeTextarea("abc");
    stubBox(el, 0, 0, 200, 100);
    getCaretCoordinatesMock.mockReturnValue({ left: Number.NaN, top: 0, height: 18 });
    expect(caretRect(el)).toBeNull();
  });

  it("rounds fractional coordinates to integers", () => {
    const el = makeTextarea("abc");
    stubBox(el, 10.4, 20.6, 200, 100);
    getCaretCoordinatesMock.mockReturnValue({ left: 0.2, top: 0.2, height: 17.6 });
    expect(caretRect(el)).toEqual({ x: 11, y: 21, width: 1, height: 18 });
  });
});

describe("caretRect (input)", () => {
  it("uses the caret x and the full height of the field", () => {
    const el = makeInput("text", "abc");
    stubBox(el, 10, 20, 200, 30);
    expect(caretRect(el)).toEqual({ x: 15, y: 20, width: 1, height: 30 });
  });

  // Review Focus 2: selectionEnd が null の type
  it("falls back to the bottom-left of the field when the type has no selection API", () => {
    for (const type of ["email", "number"]) {
      const el = makeInput(type, "a@b");
      stubBox(el, 10, 20, 200, 30);
      expect(caretRect(el), type).toEqual({ x: 10, y: 49, width: 1, height: 1 });
    }
    expect(getCaretCoordinatesMock).not.toHaveBeenCalled();
  });
});

describe("caretRect (contenteditable)", () => {
  function makeEditable(text: string): { el: HTMLElement; node: Text } {
    const el = document.createElement("div");
    el.setAttribute("contenteditable", "true");
    const node = document.createTextNode(text);
    el.appendChild(node);
    document.body.appendChild(el);
    return { el, node };
  }

  function rangeWithRects(node: Text, offset: number, rects: Partial<DOMRect>[]): Range {
    const range = document.createRange();
    range.setStart(node, offset);
    range.collapse(true);
    (range as unknown as { getClientRects: () => unknown }).getClientRects = () => rects;
    return range;
  }

  it("uses the first client rect of the anchor range", () => {
    const { el, node } = makeEditable("hello");
    stubBox(el, 0, 0, 300, 100);
    const range = rangeWithRects(node, 2, [{ left: 30, top: 40, width: 0, height: 18 }]);
    expect(caretRect(el, range)).toEqual({ x: 30, y: 40, width: 1, height: 18 });
  });

  it("falls back to the bottom-left of the element when the range has no rect", () => {
    const { el, node } = makeEditable("hello");
    stubBox(el, 10, 20, 300, 100);
    const range = rangeWithRects(node, 2, [{ left: 0, top: 0, width: 0, height: 0 }]);
    expect(caretRect(el, range)).toEqual({ x: 10, y: 119, width: 1, height: 1 });
  });

  it("falls back to the bottom-left of the element when there is no selection", () => {
    const { el } = makeEditable("hello");
    stubBox(el, 10, 20, 300, 100);
    document.getSelection()?.removeAllRanges();
    expect(caretRect(el)).toEqual({ x: 10, y: 119, width: 1, height: 1 });
  });

  it("ignores an anchor range that lies outside the element", () => {
    const { el } = makeEditable("hello");
    stubBox(el, 10, 20, 300, 100);
    const other = document.createElement("p");
    const otherNode = document.createTextNode("elsewhere");
    other.appendChild(otherNode);
    document.body.appendChild(other);
    const range = rangeWithRects(otherNode, 1, [{ left: 99, top: 99, width: 0, height: 18 }]);
    expect(caretRect(el, range)).toEqual({ x: 10, y: 119, width: 1, height: 1 });
  });
});
