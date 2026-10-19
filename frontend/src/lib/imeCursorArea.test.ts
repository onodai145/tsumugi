import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const caretRectMock = vi.hoisted(() => vi.fn());
vi.mock("./caretRect", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./caretRect")>()),
  caretRect: caretRectMock,
}));
vi.mock("./ipc", () => ({ setImeCursorArea: vi.fn() }));

import { installImeCursorArea } from "./imeCursorArea";

const RECT = { x: 1, y: 2, width: 1, height: 3 };

const send = vi.fn();
let uninstall: () => void;

function fire(target: EventTarget, type: string) {
  target.dispatchEvent(new Event(type, { bubbles: true }));
}

/// requestAnimationFrame(≈16ms)と送信の遅延(30ms)が過ぎるまで進める。
function settle() {
  vi.advanceTimersByTime(100);
}

function focusTextarea(value = ""): HTMLTextAreaElement {
  const el = document.createElement("textarea");
  el.value = value;
  document.body.appendChild(el);
  el.focus();
  settle(); // focus 自体が起こすイベントの分を流す
  send.mockClear();
  caretRectMock.mockClear();
  return el;
}

beforeEach(() => {
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "requestAnimationFrame", "cancelAnimationFrame"],
  });
  caretRectMock.mockReset();
  caretRectMock.mockReturnValue(RECT);
  send.mockReset();
  uninstall = installImeCursorArea({ send });
});

afterEach(() => {
  uninstall();
  document.body.innerHTML = "";
  vi.useRealTimers();
});

describe("installImeCursorArea", () => {
  it("sends the caret rect after a delay, not immediately", () => {
    const el = focusTextarea("abc");
    fire(el, "input");
    expect(send).not.toHaveBeenCalled();
    settle();
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith(RECT);
    expect(caretRectMock).toHaveBeenLastCalledWith(el, null);
  });

  it("coalesces events arriving while a send is pending", () => {
    const el = focusTextarea("abc");
    fire(el, "input");
    fire(el, "keyup");
    fire(document, "selectionchange");
    settle();
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("does not postpone the send when events keep arriving", () => {
    const el = focusTextarea("abc");
    fire(el, "input");
    vi.advanceTimersByTime(30);
    fire(el, "input");
    vi.advanceTimersByTime(30);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("sends again even when the rect is unchanged", () => {
    const el = focusTextarea("abc");
    fire(el, "input");
    settle();
    fire(el, "input");
    settle();
    expect(send).toHaveBeenCalledTimes(2);
  });

  it("does nothing when the focused element is not a text field", () => {
    const button = document.createElement("button");
    document.body.appendChild(button);
    button.focus();
    settle();
    send.mockClear();
    fire(button, "input");
    settle();
    expect(send).not.toHaveBeenCalled();
  });

  it("does not send when no rect can be computed", () => {
    const el = focusTextarea("abc");
    caretRectMock.mockReturnValue(null);
    fire(el, "input");
    settle();
    expect(send).not.toHaveBeenCalled();
  });

  it("reacts to window resize and to scroll anywhere in the document", () => {
    const el = focusTextarea("abc");
    fire(window, "resize");
    settle();
    expect(send).toHaveBeenCalledTimes(1);

    const scroller = document.createElement("div");
    document.body.appendChild(scroller);
    scroller.dispatchEvent(new Event("scroll")); // scroll は bubble しない
    settle();
    expect(send).toHaveBeenCalledTimes(2);
    expect(caretRectMock).toHaveBeenLastCalledWith(el, null);
  });

  it("stops listening after uninstall", () => {
    const el = focusTextarea("abc");
    uninstall();
    fire(el, "input");
    settle();
    expect(send).not.toHaveBeenCalled();
  });

  // Review Focus 3: 変換中は開始位置に固定される
  it("keeps the anchor at the composition start while the caret moves", () => {
    const el = focusTextarea("abcdef");
    el.setSelectionRange(2, 2);
    fire(el, "compositionstart");
    el.value = "abXYcdef";
    el.setSelectionRange(4, 4);
    fire(el, "compositionupdate");
    settle();
    expect(caretRectMock).toHaveBeenLastCalledWith(el, 2);

    fire(el, "compositionend");
    settle();
    expect(caretRectMock).toHaveBeenLastCalledWith(el, null);
  });

  // Review Focus 4: 選択範囲の置き換え
  it("anchors at the start of the selection when composing over a selection", () => {
    const el = focusTextarea("abcdef");
    el.setSelectionRange(1, 4);
    fire(el, "compositionstart");
    settle();
    expect(caretRectMock).toHaveBeenLastCalledWith(el, 1);
  });

  it("anchors a contenteditable composition with a collapsed Range at the selection start", () => {
    const el = document.createElement("div");
    el.setAttribute("contenteditable", "true");
    el.tabIndex = 0;
    const node = document.createTextNode("hello");
    el.appendChild(node);
    document.body.appendChild(el);
    el.focus();
    settle();
    send.mockClear();
    caretRectMock.mockClear();

    document.getSelection()?.setBaseAndExtent(node, 1, node, 4);
    fire(el, "compositionstart");
    settle();

    const anchor = caretRectMock.mock.lastCall?.[1] as Range;
    expect(anchor).toBeInstanceOf(Range);
    expect(anchor.collapsed).toBe(true);
    expect(anchor.startContainer).toBe(node);
    expect(anchor.startOffset).toBe(1);
  });

  // レビュー指摘: compositionend が来ないまま別の欄へ移っても、古いアンカーを引きずらない
  it("drops a stale composition anchor when focus moves without compositionend", () => {
    const first = focusTextarea("abcdef");
    first.setSelectionRange(2, 2);
    fire(first, "compositionstart");
    settle();

    const second = focusTextarea("xyz");
    fire(second, "input");
    settle();
    expect(caretRectMock).toHaveBeenLastCalledWith(second, null);
  });

  // Review Focus 5: 送信失敗がタイピングを妨げない
  it("does not let a failing send escape the listener", () => {
    const el = focusTextarea("abc");
    send.mockImplementation(() => {
      throw new Error("ipc down");
    });
    fire(el, "input");
    expect(() => settle()).not.toThrow();
    expect(send).toHaveBeenCalledTimes(1);

    send.mockReset();
    fire(el, "input");
    settle();
    expect(send).toHaveBeenCalledTimes(1); // 失敗のあとも動き続ける
  });
});
