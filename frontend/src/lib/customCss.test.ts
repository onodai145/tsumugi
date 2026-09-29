import { afterEach, describe, expect, it } from "vitest";
import { applyCustomCss, customCssBytes, CUSTOM_CSS_MAX_BYTES } from "./customCss";

const STYLE_ID = "tsumugi-custom-css";
const styleEl = () => document.getElementById(STYLE_ID);

afterEach(() => {
  styleEl()?.remove();
  document.head.querySelectorAll("style[data-test]").forEach((e) => e.remove());
});

describe("applyCustomCss", () => {
  it("CSSを <style id> として <head> に作成する", () => {
    applyCustomCss(".note { margin: 0 }", false);
    const el = styleEl();
    expect(el).not.toBeNull();
    expect(el?.tagName).toBe("STYLE");
    expect(el?.parentElement).toBe(document.head);
    expect(el?.textContent).toBe(".note { margin: 0 }");
  });

  it("再適用しても要素は1つのままで内容だけ差し替わる", () => {
    applyCustomCss("a { color: red }", false);
    applyCustomCss("a { color: blue }", false);
    expect(document.querySelectorAll(`#${STYLE_ID}`)).toHaveLength(1);
    expect(styleEl()?.textContent).toBe("a { color: blue }");
  });

  it("空文字なら内容を空にする", () => {
    applyCustomCss("a { color: red }", false);
    applyCustomCss("", false);
    expect(styleEl()?.textContent).toBe("");
  });

  it("セーフモードなら適用せず、解除されれば復元できる", () => {
    applyCustomCss("a { color: red }", true);
    expect(styleEl()?.textContent ?? "").toBe("");
    applyCustomCss("a { color: red }", false);
    expect(styleEl()?.textContent).toBe("a { color: red }");
  });

  it("後から追加された <style> があっても、再適用で <head> の末尾に来る", () => {
    applyCustomCss("a { color: red }", false);
    const other = document.createElement("style");
    other.dataset.test = "1";
    document.head.appendChild(other);
    applyCustomCss("a { color: blue }", false);
    expect(document.head.lastElementChild).toBe(styleEl());
  });

  it("</style> や <script> を含んでも <style> の外へ出ない", () => {
    const css = "/* </style><script>window.__pwned = 1</script> */ a { color: red }";
    const before = document.head.children.length;
    applyCustomCss(css, false);
    expect(styleEl()?.textContent).toBe(css);
    // 追加された要素は <style> 1つだけ
    expect(document.head.children.length).toBe(before + 1);
    expect(document.head.querySelector("script")).toBeNull();
  });
});

describe("customCssBytes", () => {
  it("UTF-8 のバイト数で数える（マルチバイトは文字数より大きい）", () => {
    expect(customCssBytes("abc")).toBe(3);
    expect(customCssBytes("あ")).toBe(3);
    expect(customCssBytes("")).toBe(0);
  });

  it("上限ちょうどは超過せず、1バイト超えると超過する", () => {
    expect(customCssBytes("a".repeat(CUSTOM_CSS_MAX_BYTES))).toBe(CUSTOM_CSS_MAX_BYTES);
    expect(customCssBytes("a".repeat(CUSTOM_CSS_MAX_BYTES + 1))).toBeGreaterThan(CUSTOM_CSS_MAX_BYTES);
  });
});
