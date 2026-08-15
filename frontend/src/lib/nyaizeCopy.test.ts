import { afterEach, describe, expect, it } from "vitest";
import { fireEvent } from "@testing-library/svelte";
import { buildNyaizeCharMap, handleNyaizeCopy, mapNyaizedRangeToOriginal } from "./nyaizeCopy";
import { nyaize } from "./nyaize";

describe("buildNyaizeCharMap / mapNyaizedRangeToOriginal", () => {
  it("maps a 1-to-2 char expansion (な -> にゃ) back to the original", () => {
    const original = "こんな感じ";
    const nyaized = nyaize(original); // "こんにゃ感じ"
    const map = buildNyaizeCharMap(original, nyaized);

    // 全選択は元テキスト全体に戻る
    expect(mapNyaizedRangeToOriginal(map, original, 0, nyaized.length)).toBe(original);

    // "にゃ" だけを選択しても、対応する元の1文字 "な" を含む区間に戻る
    const nyaIndex = nyaized.indexOf("にゃ");
    const partial = mapNyaizedRangeToOriginal(map, original, nyaIndex, nyaIndex + 2);
    expect(partial).toBe("な");
  });

  it("maps unchanged text 1:1", () => {
    const original = "hello world";
    const nyaized = nyaize(original); // 変化なし
    const map = buildNyaizeCharMap(original, nyaized);
    expect(mapNyaizedRangeToOriginal(map, original, 0, 5)).toBe("hello");
  });

  it("maps a trailing insertion (다 -> 다냥) back to the original char", () => {
    const original = "간다";
    const nyaized = nyaize(original); // "간다냥"
    const map = buildNyaizeCharMap(original, nyaized);
    expect(mapNyaizedRangeToOriginal(map, original, 0, nyaized.length)).toBe(original);
  });

  it("returns an empty string for an empty or inverted range", () => {
    const original = "test";
    const nyaized = nyaize(original);
    const map = buildNyaizeCharMap(original, nyaized);
    expect(mapNyaizedRangeToOriginal(map, original, 2, 2)).toBe("");
    expect(mapNyaizedRangeToOriginal(map, original, 3, 1)).toBe("");
  });

  it("handles empty strings", () => {
    const map = buildNyaizeCharMap("", "");
    expect(mapNyaizedRangeToOriginal(map, "", 0, 0)).toBe("");
  });

  it("completes quickly and stays consistent on a long (3000+ char) string", () => {
    // 大部分が共通の接頭辞/接尾辞になるよう、先頭と末尾は固定文字列、中間だけnyaize対象の文字を挟む。
    const middle = "な".repeat(3000);
    const original = `prefix-${middle}-suffix`;
    const start = Date.now();
    const nyaized = nyaize(original);
    const map = buildNyaizeCharMap(original, nyaized);
    const elapsed = Date.now() - start;

    expect(elapsed).toBeLessThan(500);
    expect(map.length).toBe(nyaized.length);
    for (let i = 1; i < map.length; i++) {
      expect(map[i]).toBeGreaterThanOrEqual(map[i - 1]);
    }
  });
});

describe("handleNyaizeCopy", () => {
  function setDataAttrSpan(text: string, original: string): HTMLSpanElement {
    const span = document.createElement("span");
    span.dataset.originalText = original;
    span.textContent = text;
    return span;
  }

  afterEach(() => {
    document.body.innerHTML = "";
    window.getSelection()?.removeAllRanges();
  });

  function selectRange(selectStart: Node, selectEnd: Node): Range {
    const range = document.createRange();
    range.setStart(selectStart, 0);
    range.setEnd(selectEnd, (selectEnd.textContent ?? "").length);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
    return range;
  }

  function makeClipboardData() {
    let captured: string | undefined;
    let setDataCalled = false;
    return {
      setData: (_type: string, value: string) => {
        setDataCalled = true;
        captured = value;
      },
      get captured() {
        return captured;
      },
      get setDataCalled() {
        return setDataCalled;
      },
    };
  }

  function fireCopyAndCapture(container: HTMLElement, selectStart: Node, selectEnd: Node): string {
    selectRange(selectStart, selectEnd);
    const clipboardData = makeClipboardData();
    const event = new Event("copy", { bubbles: true, cancelable: true }) as ClipboardEvent;
    Object.defineProperty(event, "clipboardData", { value: clipboardData });
    Object.defineProperty(event, "currentTarget", { value: container });
    fireEvent(container, event);
    return clipboardData.captured ?? "";
  }

  it("replaces the copied text with the original (pre-nyaize) text", () => {
    const container = document.createElement("div");
    const span = setDataAttrSpan("こんにゃ感じ", "こんな感じ");
    container.appendChild(span);
    document.body.appendChild(container);

    container.addEventListener("copy", handleNyaizeCopy);
    const textNode = span.firstChild!;
    const captured = fireCopyAndCapture(container, textNode, textNode);

    expect(captured).toBe("こんな感じ");
  });

  it("converts <br> elements crossed by the selection into newlines", () => {
    const container = document.createElement("div");
    const span1 = setDataAttrSpan("こんにゃ", "こんな");
    container.appendChild(span1);
    container.appendChild(document.createElement("br"));
    const span2 = setDataAttrSpan("感じ", "感じ");
    container.appendChild(span2);
    document.body.appendChild(container);

    container.addEventListener("copy", handleNyaizeCopy);
    const start = span1.firstChild!;
    const end = span2.firstChild!;
    const captured = fireCopyAndCapture(container, start, end);

    expect(captured).toBe("こんな\n感じ");
  });

  it("does not touch the clipboard or preventDefault when the selection has no nyaized content", () => {
    const container = document.createElement("div");
    const span = document.createElement("span");
    span.textContent = "plain text";
    container.appendChild(span);
    document.body.appendChild(container);

    container.addEventListener("copy", handleNyaizeCopy);
    const textNode = span.firstChild!;
    selectRange(textNode, textNode);

    const clipboardData = makeClipboardData();
    const event = new Event("copy", { bubbles: true, cancelable: true }) as ClipboardEvent;
    Object.defineProperty(event, "clipboardData", { value: clipboardData });
    Object.defineProperty(event, "currentTarget", { value: container });
    fireEvent(container, event);

    expect(clipboardData.setDataCalled).toBe(false);
    expect(event.defaultPrevented).toBe(false);
  });

  it("includes <img alt> text at the right position within nyaized content", () => {
    const container = document.createElement("div");
    const span1 = setDataAttrSpan("こんにゃ", "こんな");
    container.appendChild(span1);
    const img = document.createElement("img");
    img.setAttribute("alt", "😀");
    container.appendChild(img);
    const span2 = setDataAttrSpan("感じ", "感じ");
    container.appendChild(span2);
    document.body.appendChild(container);

    container.addEventListener("copy", handleNyaizeCopy);
    const start = span1.firstChild!;
    const end = span2.firstChild!;
    const captured = fireCopyAndCapture(container, start, end);

    expect(captured).toBe("こんな😀感じ");
  });

  it("inserts a newline at a <blockquote> boundary within nyaized content", () => {
    const container = document.createElement("div");
    const span1 = setDataAttrSpan("こんにゃ", "こんな");
    container.appendChild(span1);
    const blockquote = document.createElement("blockquote");
    const span2 = setDataAttrSpan("引用にゃ", "引用な");
    blockquote.appendChild(span2);
    container.appendChild(blockquote);
    const span3 = setDataAttrSpan("感じ", "感じ");
    container.appendChild(span3);
    document.body.appendChild(container);

    container.addEventListener("copy", handleNyaizeCopy);
    const start = span1.firstChild!;
    const end = span3.firstChild!;
    const captured = fireCopyAndCapture(container, start, end);

    expect(captured).toBe("こんな\n引用な\n感じ");
  });

  it("does not preventDefault when event.clipboardData is absent, even with nyaized content selected", () => {
    const container = document.createElement("div");
    const span = setDataAttrSpan("こんにゃ感じ", "こんな感じ");
    container.appendChild(span);
    document.body.appendChild(container);

    container.addEventListener("copy", handleNyaizeCopy);
    const textNode = span.firstChild!;
    selectRange(textNode, textNode);

    const event = new Event("copy", { bubbles: true, cancelable: true }) as ClipboardEvent;
    Object.defineProperty(event, "clipboardData", { value: undefined });
    Object.defineProperty(event, "currentTarget", { value: container });
    fireEvent(container, event);

    expect(event.defaultPrevented).toBe(false);
  });

  it("still writes to the clipboard and preventDefaults when clipboardData IS present", () => {
    const container = document.createElement("div");
    const span = setDataAttrSpan("こんにゃ感じ", "こんな感じ");
    container.appendChild(span);
    document.body.appendChild(container);

    container.addEventListener("copy", handleNyaizeCopy);
    const textNode = span.firstChild!;
    selectRange(textNode, textNode);

    const clipboardData = makeClipboardData();
    const event = new Event("copy", { bubbles: true, cancelable: true }) as ClipboardEvent;
    Object.defineProperty(event, "clipboardData", { value: clipboardData });
    Object.defineProperty(event, "currentTarget", { value: container });
    fireEvent(container, event);

    expect(clipboardData.setDataCalled).toBe(true);
    expect(clipboardData.captured).toBe("こんな感じ");
    expect(event.defaultPrevented).toBe(true);
  });
});
