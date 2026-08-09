import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { isKnownFn, mfmFn } from "./mfm";

function mockMatchMedia(matches: boolean) {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches,
    media: query,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  }));
}

describe("isKnownFn", () => {
  it("returns true for a known function name", () => {
    expect(isKnownFn("tada")).toBe(true);
  });

  it("returns false for an unknown function name", () => {
    expect(isKnownFn("nonexistent")).toBe(false);
  });
});

describe("mfmFn", () => {
  beforeEach(() => {
    mockMatchMedia(false);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns empty class/style for an unknown function", () => {
    expect(mfmFn("nonexistent")).toEqual({ class: "", style: "" });
  });

  it("applies font-size scaling for x2/x3/x4", () => {
    expect(mfmFn("x2").style).toBe("font-size:2em");
    expect(mfmFn("x3").style).toBe("font-size:3em");
    expect(mfmFn("x4").style).toBe("font-size:4em");
  });

  it("builds a tada animation with default timing", () => {
    const result = mfmFn("tada");
    expect(result.style).toBe(
      "font-size:150%;animation:mfm-tada 1s linear infinite both;animation-delay:0s",
    );
  });

  it("respects custom speed/delay args for tada", () => {
    const result = mfmFn("tada", { speed: "2s", delay: "0.5s" });
    expect(result.style).toBe(
      "font-size:150%;animation:mfm-tada 2s linear infinite both;animation-delay:0.5s",
    );
  });

  it("ignores invalid time args and falls back to defaults", () => {
    const result = mfmFn("tada", { speed: "not-a-time" });
    expect(result.style).toBe(
      "font-size:150%;animation:mfm-tada 1s linear infinite both;animation-delay:0s",
    );
  });

  it("suppresses the animation when reduced motion is preferred", () => {
    mockMatchMedia(true);
    const result = mfmFn("jelly");
    expect(result.style).toBe("");
  });

  it("still applies static styling under reduced motion", () => {
    mockMatchMedia(true);
    const result = mfmFn("tada");
    expect(result.style).toBe("font-size:150%;");
  });

  it("suppresses the animation when animationsEnabled is false (Issue #175)", () => {
    const result = mfmFn("jelly", {}, false);
    expect(result.style).toBe("");
  });

  it("still applies static styling when animationsEnabled is false", () => {
    const result = mfmFn("tada", {}, false);
    expect(result.style).toBe("font-size:150%;");
  });

  it("applies the animation when animationsEnabled defaults to true and motion is not reduced", () => {
    const result = mfmFn("jelly");
    expect(result.style).toBe("animation:mfm-rubberBand 1s linear infinite both;animation-delay:0s");
  });

  it("validates hex colors for fg, falling back to red", () => {
    expect(mfmFn("fg", { color: "0f0" }).style).toBe("color:#0f0;overflow-wrap:anywhere");
    expect(mfmFn("fg", { color: "not-a-color" }).style).toBe("color:#f00;overflow-wrap:anywhere");
  });

  it("applies blur as a class, not an inline style", () => {
    expect(mfmFn("blur")).toEqual({ class: "mfm-blur", style: "" });
  });
});

import { FN_ARGS, KNOWN_FN } from "./mfm";

describe("FN_ARGS", () => {
  it("has an entry for every known fn name", () => {
    for (const name of KNOWN_FN) {
      expect(FN_ARGS[name]).toBeDefined();
    }
  });

  it("marks tada's speed/delay as value args", () => {
    expect(FN_ARGS.tada).toEqual([
      { name: "speed", hasValue: true },
      { name: "delay", hasValue: true },
    ]);
  });

  it("marks flip's h/v as flag args (no value)", () => {
    expect(FN_ARGS.flip).toEqual([
      { name: "h", hasValue: false },
      { name: "v", hasValue: false },
    ]);
  });

  it("gives border.style a closed enum matching the CSS border-style keywords mfmFn accepts", () => {
    const style = FN_ARGS.border.find((a) => a.name === "style");
    expect(style?.enum).toEqual([
      "hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset", "outset",
    ]);
  });

  it("does not give border.color an enum (free-form hex input)", () => {
    const color = FN_ARGS.border.find((a) => a.name === "color");
    expect(color?.enum).toBeUndefined();
  });

  it("gives x2/x3/x4/blur an empty arg list", () => {
    expect(FN_ARGS.x2).toEqual([]);
    expect(FN_ARGS.x3).toEqual([]);
    expect(FN_ARGS.x4).toEqual([]);
    expect(FN_ARGS.blur).toEqual([]);
  });
});
