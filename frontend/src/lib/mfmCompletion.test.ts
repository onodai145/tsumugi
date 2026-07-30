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

  it("detects a mention trigger at the start of the text", () => {
    expect(detectTrigger("@ali", 4)).toEqual({ kind: "mention", query: "ali", start: 0, end: 4 });
  });

  it("detects a mention trigger with a host part as one trigger", () => {
    expect(detectTrigger("hello @alice@example.com", 24)).toEqual({
      kind: "mention", query: "alice@example.com", start: 6, end: 24,
    });
  });

  it("does not treat an email-address-like '@' as a mention trigger", () => {
    // "user@" の直前が英数字("r")なので境界外(誤検出しない)
    expect(detectTrigger("user@example.com", 16)).toBeNull();
  });

  it("detects a hashtag trigger", () => {
    expect(detectTrigger("hello #misskey", 14)).toEqual({
      kind: "hashtag", query: "misskey", start: 6, end: 14,
    });
  });

  it("does not treat a '#' glued to a word as a hashtag trigger", () => {
    expect(detectTrigger("C#lang", 6)).toBeNull();
  });

  it("still detects a hashtag trigger inside an fn's content (after whitespace)", () => {
    expect(detectTrigger("$[tada hi #tag", 14)).toEqual({
      kind: "hashtag", query: "tag", start: 10, end: 14,
    });
  });

  it("does not trigger on a bare '@' with no query", () => {
    expect(detectTrigger("@", 1)).toBeNull();
  });

  it("does not trigger on a bare '#' with no query", () => {
    expect(detectTrigger("#", 1)).toBeNull();
  });

  it("detects a hashtag trigger with non-ASCII characters (Misskey hashtags allow this)", () => {
    // "hello " (6 UTF-16 code units) + "#" (1) + "日本語" (3, each a single BMP code unit) = 10
    expect(detectTrigger("hello #日本語", 10)).toEqual({
      kind: "hashtag", query: "日本語", start: 6, end: 10,
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
    expect(detectTrigger("$[border.style=so", 17)).toEqual({
      kind: "argValue", fnName: "border", argName: "style", query: "so", start: 15, end: 17,
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
    const customResults = result.filter((r) => r.kind === "custom");
    expect(customResults.map((r) => r.name)).toEqual(["Smile_cat", "smoke"]);
  });

  it("ranks all custom matches ahead of any unicode matches", () => {
    const custom = [emoji("Smile_cat"), emoji("smoke")];
    const result = matchEmojis("sm", custom);
    const firstNonCustomIndex = result.findIndex((r) => r.kind !== "custom");
    const customCount = result.filter((r) => r.kind === "custom").length;
    expect(customCount).toBe(2);
    expect(firstNonCustomIndex === -1 || firstNonCustomIndex >= customCount).toBe(true);
  });

  it("matches custom emoji by alias prefix too", () => {
    const custom = [emoji("neko", ["cat_face"])];
    const result = matchEmojis("cat", custom);
    const customResults = result.filter((r) => r.kind === "custom");
    expect(customResults.map((r) => r.name)).toEqual(["neko"]);
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

import { applyCompletion, buildCompletionItems, type CompletionItem } from "./mfmCompletion";

describe("buildCompletionItems", () => {
  it("builds emoji items with :name: insert text and a thumbnail", () => {
    const custom = [emoji("neko")];
    const trigger = { kind: "emoji", query: "ne", start: 0, end: 3 } as const;
    const items = buildCompletionItems(trigger, custom);
    expect(items[0]).toEqual({ key: "custom:neko", label: "neko", insertText: ":neko:", thumbnail: { type: "custom", url: custom[0].url } });
  });

  it("builds fnName items with the bare name as insert text", () => {
    const trigger = { kind: "fnName", query: "tad", start: 2, end: 5 } as const;
    expect(buildCompletionItems(trigger, [])).toEqual([
      { key: "tada", label: "tada", insertText: "tada" },
    ]);
  });

  it("builds argName items, appending '=' for value args but not for flags", () => {
    const trigger = { kind: "argName", fnName: "spin", query: "", start: 0, end: 0 } as const;
    const items = buildCompletionItems(trigger, []);
    expect(items.find((i) => i.key === "speed")).toEqual({ key: "speed", label: "speed=", insertText: "speed=" });
    expect(items.find((i) => i.key === "x")).toEqual({ key: "x", label: "x", insertText: "x" });
  });

  it("builds argValue items with the bare enum value as insert text", () => {
    const trigger = { kind: "argValue", fnName: "border", argName: "style", query: "so", start: 0, end: 2 } as const;
    expect(buildCompletionItems(trigger, [])).toEqual([
      { key: "solid", label: "solid", insertText: "solid" },
    ]);
  });
});

describe("applyCompletion", () => {
  it("splices the insert text into the trigger's range and places the cursor after it", () => {
    const item: CompletionItem = { key: "neko", label: "neko", insertText: ":neko:" };
    const trigger = { kind: "emoji", query: "ne", start: 6, end: 9 } as const;
    const result = applyCompletion("hello :ne", trigger, item);
    expect(result).toEqual({ text: "hello :neko:", cursor: 12 });
  });

  it("keeps text after the trigger end intact", () => {
    const item: CompletionItem = { key: "tada", label: "tada", insertText: "tada" };
    const trigger = { kind: "fnName", query: "ta", start: 2, end: 4 } as const;
    const result = applyCompletion("$[ta hi]", trigger, item);
    expect(result).toEqual({ text: "$[tada hi]", cursor: 6 });
  });
});
