import { describe, expect, it } from "vitest";
import {
  applyTqlCompletion,
  charOffset,
  currentWordTrigger,
  detectIdArgTrigger,
  idCandidates,
  syntaxCandidates,
} from "./tqlCompletion";

describe("detectIdArgTrigger", () => {
  it("detects an unterminated list( argument string", () => {
    expect(detectIdArgTrigger('from list("ab', 13)).toEqual({
      trigger: { start: 11, end: 13 },
      kind: "list",
      query: "ab",
    });
  });

  it("detects an unterminated antenna( argument string with an empty query", () => {
    expect(detectIdArgTrigger('from antenna("', 14)).toEqual({
      trigger: { start: 14, end: 14 },
      kind: "antenna",
      query: "",
    });
  });

  it("returns null once the string literal is closed", () => {
    expect(detectIdArgTrigger('from list("ab")', 15)).toBeNull();
  });

  it("returns null for sources without id arguments", () => {
    expect(detectIdArgTrigger('from tag("ab', 12)).toBeNull();
  });
});

describe("currentWordTrigger", () => {
  it("captures the identifier being typed", () => {
    expect(currentWordTrigger("from home where has_fi", 22)).toEqual({ start: 16, end: 22 });
  });

  it("returns a zero-length span right after a space", () => {
    expect(currentWordTrigger("from home where ", 16)).toEqual({ start: 16, end: 16 });
  });
});

describe("charOffset", () => {
  it("counts unicode code points, not UTF-16 units", () => {
    // "😀" はUTF-16では2単位(サロゲートペア)だが、コードポイントは1
    expect(charOffset("😀abc", 5)).toBe(4);
  });
});

describe("idCandidates", () => {
  const lists = [
    { id: "l1", name: "Friends" },
    { id: "l2", name: "Work" },
  ];

  it("filters by prefix (case-insensitive) and inserts the id, closing the argument", () => {
    expect(idCandidates("list", "fr", lists, [], [])).toEqual([
      { key: "l1", label: "Friends", insertText: 'l1")' },
    ]);
  });
});

describe("syntaxCandidates", () => {
  it("maps Rust completion items to the CompletionItem shape", () => {
    expect(syntaxCandidates([{ label: "has_files", insert: "has_files ", kind: "field" }])).toEqual([
      { key: "has_files", label: "has_files", insertText: "has_files " },
    ]);
  });
});

describe("applyTqlCompletion", () => {
  it("replaces the trigger span and places the cursor at the end of the inserted text", () => {
    const result = applyTqlCompletion(
      "from home where has_fi",
      { start: 16, end: 22 },
      { key: "has_files", label: "has_files", insertText: "has_files " },
    );
    expect(result).toEqual({ text: "from home where has_files ", cursor: 26 });
  });
});
