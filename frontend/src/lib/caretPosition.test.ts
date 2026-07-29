import { afterEach, describe, expect, it } from "vitest";
import { getCaretCoordinates } from "./caretPosition";

function makeTextarea(value: string): HTMLTextAreaElement {
  const el = document.createElement("textarea");
  el.value = value;
  document.body.appendChild(el);
  return el;
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("getCaretCoordinates", () => {
  it("returns numeric left/top/height without throwing", () => {
    const el = makeTextarea("hello world");
    const coords = getCaretCoordinates(el, 5);
    expect(typeof coords.left).toBe("number");
    expect(typeof coords.top).toBe("number");
    expect(typeof coords.height).toBe("number");
    expect(Number.isNaN(coords.left)).toBe(false);
  });

  it("does not leave a mirror element behind in the DOM", () => {
    const el = makeTextarea("hello world");
    getCaretCoordinates(el, 3);
    expect(document.getElementById("mfm-completion-caret-mirror")).toBeNull();
  });

  it("handles a position at the very end of the text", () => {
    const el = makeTextarea("hi");
    expect(() => getCaretCoordinates(el, 2)).not.toThrow();
  });

  it("handles an empty textarea", () => {
    const el = makeTextarea("");
    expect(() => getCaretCoordinates(el, 0)).not.toThrow();
  });
});
