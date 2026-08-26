import { describe, expect, it } from "vitest";
import { extractPreviewUrls } from "./extractPreviewUrls";

describe("extractPreviewUrls", () => {
  it("returns an empty array for text without URLs", () => {
    expect(extractPreviewUrls("hello world")).toEqual([]);
  });

  it("extracts a bare URL", () => {
    expect(extractPreviewUrls("見て https://example.com/a")).toEqual(["https://example.com/a"]);
  });

  it("dedupes repeated URLs", () => {
    expect(extractPreviewUrls("https://example.com/a https://example.com/a")).toEqual([
      "https://example.com/a",
    ]);
  });

  it("finds URLs nested inside a quote block", () => {
    expect(extractPreviewUrls("> https://example.com/a")).toEqual(["https://example.com/a"]);
  });

  it("ignores empty text", () => {
    expect(extractPreviewUrls("")).toEqual([]);
  });
});
