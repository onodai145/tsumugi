import { describe, expect, it } from "vitest";
import { buildSearchUrl, DEFAULT_SEARCH_ENGINE_URL, SEARCH_ENGINE_PRESETS } from "./searchEngine";

describe("buildSearchUrl", () => {
  it("replaces {query} with the URL-encoded query", () => {
    expect(buildSearchUrl("https://example.com/search?q={query}", "しなちく システム")).toBe(
      `https://example.com/search?q=${encodeURIComponent("しなちく システム")}`,
    );
  });

  it("falls back to Google when the template is undefined", () => {
    expect(buildSearchUrl(undefined, "cat")).toBe(
      DEFAULT_SEARCH_ENGINE_URL.replace("{query}", "cat"),
    );
  });

  it("falls back to Google when the template is empty", () => {
    expect(buildSearchUrl("", "cat")).toBe(DEFAULT_SEARCH_ENGINE_URL.replace("{query}", "cat"));
  });

  it("falls back to Google when the template has no {query} placeholder", () => {
    expect(buildSearchUrl("https://example.com/search", "cat")).toBe(
      DEFAULT_SEARCH_ENGINE_URL.replace("{query}", "cat"),
    );
  });
});

describe("SEARCH_ENGINE_PRESETS", () => {
  it("every preset URL contains a {query} placeholder", () => {
    for (const { url } of SEARCH_ENGINE_PRESETS) {
      expect(url).toContain("{query}");
    }
  });

  it("has unique URLs", () => {
    const urls = SEARCH_ENGINE_PRESETS.map((p) => p.url);
    expect(new Set(urls).size).toBe(urls.length);
  });
});
