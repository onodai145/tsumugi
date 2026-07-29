import { describe, expect, it } from "vitest";
import {
  ACTIONS,
  buildKeymap,
  defaultKeymap,
  effectiveChord,
  eventToChord,
  isModifierOnly,
  prettyChord,
} from "./keymap";

function key(
  k: string,
  mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean }> = {},
): KeyboardEvent {
  return new KeyboardEvent("keydown", { key: k, ...mods });
}

describe("eventToChord", () => {
  it("returns a lowercase single key with no modifiers", () => {
    expect(eventToChord(key("J"))).toBe("j");
  });

  it("prefixes modifiers in ctrl/meta/alt/shift order", () => {
    expect(eventToChord(key("Enter", { ctrlKey: true, shiftKey: true }))).toBe("ctrl+shift+Enter");
  });

  it("normalizes space to 'space'", () => {
    expect(eventToChord(key(" "))).toBe("space");
  });

  it("keeps multi-character key names as-is", () => {
    expect(eventToChord(key("ArrowUp"))).toBe("ArrowUp");
  });
});

describe("defaultKeymap", () => {
  it("maps every default chord to its action", () => {
    const m = defaultKeymap();
    expect(m.get("j")).toBe("note.next");
    expect(m.get("n")).toBe("compose.new");
    expect(m.size).toBe(ACTIONS.length);
  });
});

describe("effectiveChord", () => {
  it("returns the default chord when there is no override", () => {
    expect(effectiveChord("note.next", {})).toBe("j");
  });

  it("returns the overridden chord when present", () => {
    expect(effectiveChord("note.next", { "note.next": "shift+j" })).toBe("shift+j");
  });
});

describe("buildKeymap", () => {
  it("applies overrides to the resulting chord map", () => {
    const m = buildKeymap({ "note.next": "shift+j" });
    expect(m.get("shift+j")).toBe("note.next");
    expect(m.get("j")).toBeUndefined();
  });

  it("falls back to defaults for actions without overrides", () => {
    const m = buildKeymap({ "note.next": "shift+j" });
    expect(m.get("k")).toBe("note.prev");
  });
});

describe("prettyChord", () => {
  it("formats a single letter chord", () => {
    expect(prettyChord("j")).toBe("J");
  });

  it("formats modifiers with their display labels", () => {
    expect(prettyChord("ctrl+shift+Enter")).toBe("Ctrl + Shift + Enter");
  });

  it("formats meta as the command symbol", () => {
    expect(prettyChord("meta+k")).toBe("⌘ + K");
  });

  it("formats space as the word 'Space'", () => {
    expect(prettyChord("space")).toBe("Space");
  });
});

describe("isModifierOnly", () => {
  it("returns true when only Shift is pressed", () => {
    expect(isModifierOnly(key("Shift"))).toBe(true);
  });

  it("returns true when only Control is pressed", () => {
    expect(isModifierOnly(key("Control"))).toBe(true);
  });

  it("returns false for a regular key", () => {
    expect(isModifierOnly(key("j"))).toBe(false);
  });
});
