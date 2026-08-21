import { describe, expect, it } from "vitest";
import { highlightCode } from "./shiki";

describe("highlightCode", () => {
  // 言語未指定・非対応言語は shiki 組み込みの特殊言語 "text" にフォールバックし、
  // 独自の plainHtml() （shiki-plain）ではなく shiki 経由のテーマ付き出力になる（Issue #229）。
  it("言語未指定でも shiki-plain ではなく shiki のテーマ付き出力になる", async () => {
    const html = await highlightCode("no lang here", null, "auto", []);
    expect(html).not.toContain("shiki-plain");
    expect(html).toContain("shiki");
  });

  it("BUNDLED_LANGS に無い言語でも shiki-plain ではなく shiki のテーマ付き出力になる", async () => {
    const html = await highlightCode("some code", "brainfuck", "auto", []);
    expect(html).not.toContain("shiki-plain");
    expect(html).toContain("shiki");
  });

  it("対応言語では引き続きシンタックスハイライトされる", async () => {
    const html = await highlightCode("const x = 1;", "javascript", "auto", []);
    expect(html).not.toContain("shiki-plain");
    expect(html).toContain("shiki");
  });
});
