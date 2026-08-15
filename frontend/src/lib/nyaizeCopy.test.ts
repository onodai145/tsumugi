import { describe, expect, it } from "vitest";
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
});

describe("handleNyaizeCopy", () => {
  function setDataAttrSpan(text: string, original: string): HTMLSpanElement {
    const span = document.createElement("span");
    span.dataset.originalText = original;
    span.textContent = text;
    return span;
  }

  function fireCopyAndCapture(container: HTMLElement, selectStart: Node, selectEnd: Node): string {
    const range = document.createRange();
    range.setStart(selectStart, 0);
    range.setEnd(selectEnd, (selectEnd.textContent ?? "").length);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);

    let captured = "";
    const clipboardData = {
      setData: (_type: string, value: string) => {
        captured = value;
      },
    };
    const event = new Event("copy", { bubbles: true, cancelable: true }) as ClipboardEvent;
    Object.defineProperty(event, "clipboardData", { value: clipboardData });
    Object.defineProperty(event, "currentTarget", { value: container });
    fireEvent(container, event);
    return captured;
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
    document.body.removeChild(container);
  });

  it("passes through text that has no data-original-text ancestor (nyaize対象外)", () => {
    const container = document.createElement("div");
    const span = document.createElement("span");
    span.textContent = "plain text";
    container.appendChild(span);
    document.body.appendChild(container);

    container.addEventListener("copy", handleNyaizeCopy);
    const textNode = span.firstChild!;
    const captured = fireCopyAndCapture(container, textNode, textNode);

    expect(captured).toBe("plain text");
    document.body.removeChild(container);
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
    document.body.removeChild(container);
  });
});
