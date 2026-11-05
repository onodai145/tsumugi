import { describe, expect, it } from "vitest";
import { MAX_EXTERNAL_URL_LEN, validateSearchEngineUrl, validateSummalyProxyUrl } from "./externalUrl";

// Rust側(domain/ui.rs のテスト)と同じ入力例。
describe("validateSearchEngineUrl", () => {
  it.each([
    "https://www.google.com/search?q={query}",
    "http://localhost:8080/s?q={query}",
    "https://example.com/{query}/x",
    "HTTPS://Example.com/?q={query}",
  ])("accepts %s", (ok) => {
    expect(validateSearchEngineUrl(ok)).toBeNull();
  });

  it.each([
    "javascript:alert({query})",
    "JavaScript:alert({query})",
    " javascript:alert({query})",
    "data:text/html,{query}",
    "file:///etc/passwd?{query}",
    "ftp://example.com/{query}",
    "https://example.com/",
    "",
    "https://user:pass@example.com/?q={query}",
    "https://{query}@example.com/",
    "https://{query}.example.com/",
    "https://{query}/",
    "https://example.com:{query}/",
    "https://example.com{query}",
    "not a url {query}",
    "ht\ntps://good.example/?q={query}",
    "https://good.example/?q={query}\n",
    " https://good.example/?q={query}",
    "https://exa\tmple.com/?q={query}",
    "https://example.com/\u0000?q={query}",
  ])("rejects %j", (bad) => {
    expect(validateSearchEngineUrl(bad)).not.toBeNull();
  });

  it("rejects a template longer than the limit", () => {
    expect(validateSearchEngineUrl(`https://example.com/?q={query}${"a".repeat(MAX_EXTERNAL_URL_LEN)}`)).not.toBeNull();
  });
});

describe("validateSummalyProxyUrl", () => {
  it.each(["", "   ", "https://summaly.example.com/", "http://192.168.1.10:3030/url", "http://localhost:3030", "http://[::1]:3030/url"])(
    "accepts %j",
    (ok) => {
      expect(validateSummalyProxyUrl(ok)).toBeNull();
    },
  );

  it.each(["javascript:alert(1)", "data:,x", "ftp://example.com", "file:///x", "https://u:p@example.com/", "example.com/url", "http://", "//example.com", " https://example.com/", "https://example.com/\n", "ht\ntps://example.com/", "https://exa\tmple.com/"])(
    "rejects %j",
    (bad) => {
      expect(validateSummalyProxyUrl(bad)).not.toBeNull();
    },
  );
});
