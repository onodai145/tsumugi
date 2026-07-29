import { describe, expect, it } from "vitest";
import { nyaize } from "./nyaize";

describe("nyaize", () => {
  it("converts ja-JP な to にゃ", () => {
    expect(nyaize("こんな感じ")).toBe("こんにゃ感じ");
  });

  it("converts katakana ナ to ニャ", () => {
    expect(nyaize("バナナ")).toBe("バニャニャ");
  });

  it("converts lowercase 'na' preceded by n to 'nya'", () => {
    expect(nyaize("banana")).toBe("banyanya");
  });

  it("preserves case when converting 'NA' to 'NYA'", () => {
    expect(nyaize("BANANA")).toBe("BANYANYA");
  });

  it("converts 'morning' to 'mornyan'", () => {
    expect(nyaize("morning")).toBe("mornyan");
  });

  it("converts 'everyone' to 'everynyan'", () => {
    expect(nyaize("everyone")).toBe("everynyan");
  });

  it("returns the input unchanged when nothing matches", () => {
    expect(nyaize("hello world")).toBe("hello world");
  });

  it("returns an empty string for empty input", () => {
    expect(nyaize("")).toBe("");
  });
});
