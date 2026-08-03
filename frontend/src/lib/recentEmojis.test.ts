import { describe, expect, it } from "vitest";
import { RECENT_EMOJIS_MAX, withRecentEmojiUsage } from "./recentEmojis";

describe("withRecentEmojiUsage", () => {
  it("prepends a new key to an empty list", () => {
    expect(withRecentEmojiUsage([], "👍")).toEqual(["👍"]);
  });

  it("moves an existing key to the front instead of duplicating it", () => {
    expect(withRecentEmojiUsage(["😆", "👍", "🎉"], "👍")).toEqual(["👍", "😆", "🎉"]);
  });

  it("truncates to RECENT_EMOJIS_MAX entries", () => {
    const list = Array.from({ length: RECENT_EMOJIS_MAX }, (_, i) => `emoji-${i}`);
    const result = withRecentEmojiUsage(list, "new-emoji");
    expect(result).toHaveLength(RECENT_EMOJIS_MAX);
    expect(result[0]).toBe("new-emoji");
    expect(result).not.toContain(`emoji-${RECENT_EMOJIS_MAX - 1}`);
  });

  it("re-adding the same key keeps the list length stable", () => {
    const list = ["👍", "😆"];
    expect(withRecentEmojiUsage(list, "👍")).toEqual(["👍", "😆"]);
  });
});
