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
