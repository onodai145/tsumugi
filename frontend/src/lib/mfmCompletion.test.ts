import { describe, expect, it } from "vitest";
import { detectTrigger } from "./mfmCompletion";

describe("detectTrigger", () => {
  it("returns null when there is no trigger character", () => {
    expect(detectTrigger("hello world", 11)).toBeNull();
  });

  it("detects an emoji trigger at the start of the text", () => {
    expect(detectTrigger(":sm", 3)).toEqual({ kind: "emoji", query: "sm", start: 0, end: 3 });
  });

  it("detects an emoji trigger after whitespace", () => {
    expect(detectTrigger("hello :sm", 9)).toEqual({ kind: "emoji", query: "sm", start: 6, end: 9 });
  });

  it("does not treat a colon glued to a word as an emoji trigger", () => {
    // "http:" のように直前が英数字の ":" は、その時点ではトリガーにしない
    // (直前が行頭/空白/開き括弧類の ":" だけをトリガーとみなす)
    expect(detectTrigger("http:", 5)).toBeNull();
  });

  it("detects an fn name trigger right after $[", () => {
    expect(detectTrigger("$[ta", 4)).toEqual({ kind: "fnName", query: "ta", start: 2, end: 4 });
  });

  it("detects an fn name trigger with an empty query", () => {
    expect(detectTrigger("$[", 2)).toEqual({ kind: "fnName", query: "", start: 2, end: 2 });
  });

  it("does not detect an fn trigger once a $[...] has already been closed", () => {
    expect(detectTrigger("$[tada hi] world:", 17)).toBeNull();
  });

  it("stops fn-name detection once whitespace has been typed, falling back to no trigger", () => {
    expect(detectTrigger("$[tada hi", 9)).toBeNull();
  });

  it("still detects an emoji trigger inside an fn's content (after whitespace)", () => {
    expect(detectTrigger("$[tada hi :sm", 13)).toEqual({
      kind: "emoji", query: "sm", start: 10, end: 13,
    });
  });

  it("detects an arg-name trigger right after the dot", () => {
    expect(detectTrigger("$[tada.spee", 11)).toEqual({
      kind: "argName", fnName: "tada", query: "spee", start: 7, end: 11,
    });
  });

  it("detects an arg-name trigger for the second argument after a comma", () => {
    expect(detectTrigger("$[tada.speed=1s,de", 18)).toEqual({
      kind: "argName", fnName: "tada", query: "de", start: 16, end: 18,
    });
  });

  it("detects an arg-value trigger for border.style", () => {
    expect(detectTrigger("$[border.style=so", 18)).toEqual({
      kind: "argValue", fnName: "border", argName: "style", query: "so", start: 15, end: 18,
    });
  });

  it("does not detect an arg-value trigger for an arg without an enum (e.g. border.color)", () => {
    expect(detectTrigger("$[border.color=f", 17)).toBeNull();
  });

  it("does not detect an arg-value trigger for an unknown fn name", () => {
    expect(detectTrigger("$[nonexistent.style=so", 23)).toBeNull();
  });
});

import { matchArgNames, matchArgValues, matchEmojis, matchFnNames } from "./mfmCompletion";
import type { EmojiDef } from "../bindings/tauri.gen";

function emoji(name: string, aliases: string[] = []): EmojiDef {
  return { name, host: null, url: `https://example.com/${name}.png`, category: null, aliases };
}

describe("matchEmojis", () => {
  it("matches custom emoji by name prefix, case-insensitively", () => {
    const custom = [emoji("Smile_cat"), emoji("smoke"), emoji("wave")];
    const result = matchEmojis("sm", custom);
    expect(result.map((r) => r.name)).toEqual(["Smile_cat", "smoke"]);
    expect(result.every((r) => r.kind === "custom")).toBe(true);
  });

  it("matches custom emoji by alias prefix too", () => {
    const custom = [emoji("neko", ["cat_face"])];
    expect(matchEmojis("cat", custom).map((r) => r.name)).toEqual(["neko"]);
  });

  it("ranks custom emoji ahead of unicode emoji for the same query", () => {
    const custom = [emoji("smile")];
    const result = matchEmojis("smi", custom);
    expect(result[0]).toEqual({ key: "custom:smile", kind: "custom", name: "smile", url: custom[0].url });
  });

  it("falls back to unicode emoji shortcodes when no custom emoji matches", () => {
    const result = matchEmojis("grin", []);
    expect(result.length).toBeGreaterThan(0);
    expect(result.every((r) => r.kind === "unicode")).toBe(true);
    expect(result[0].char).toBeTruthy();
  });

  it("caps the total at 10 matches", () => {
    const custom = Array.from({ length: 20 }, (_, i) => emoji(`smile_${i}`));
    expect(matchEmojis("smile", custom)).toHaveLength(10);
  });

  it("returns everything up to the limit for an empty query", () => {
    const custom = [emoji("a"), emoji("b")];
    expect(matchEmojis("", custom).length).toBeGreaterThan(0);
  });
});

describe("matchFnNames", () => {
  it("matches known fn names by prefix, sorted", () => {
    expect(matchFnNames("s")).toEqual(["scale", "shake", "spin"]);
  });

  it("returns an empty array for no match", () => {
    expect(matchFnNames("zzz")).toEqual([]);
  });
});

describe("matchArgNames", () => {
  it("matches an fn's arg specs by name prefix", () => {
    expect(matchArgNames("border", "s")).toEqual([
      {
        name: "style", hasValue: true,
        enum: ["hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset", "outset"],
      },
    ]);
  });

  it("returns an empty array for an unknown fn", () => {
    expect(matchArgNames("nonexistent", "s")).toEqual([]);
  });
});

describe("matchArgValues", () => {
  it("matches border.style's enum by prefix", () => {
    expect(matchArgValues("border", "style", "d")).toEqual(["dotted", "dashed", "double"]);
  });

  it("returns an empty array for an arg with no enum", () => {
    expect(matchArgValues("border", "color", "f")).toEqual([]);
  });
});
