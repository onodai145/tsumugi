import { describe, expect, it } from "vitest";
import { customEmojiKey, customEmojiPinKey, isCustomEmojiKey, parseCustomEmojiPinKey } from "./emojiKey";

describe("isCustomEmojiKey", () => {
  it("returns true for a custom emoji key", () => {
    expect(isCustomEmojiKey(":blob_cat:")).toBe(true);
  });

  it("returns false for a plain unicode emoji", () => {
    expect(isCustomEmojiKey("😺")).toBe(false);
  });

  it("returns false for a lone colon", () => {
    expect(isCustomEmojiKey(":")).toBe(false);
  });

  it("returns false for an empty name between colons", () => {
    expect(isCustomEmojiKey("::")).toBe(false);
  });
});

describe("customEmojiKey", () => {
  it("wraps the name in colons with a local host suffix", () => {
    expect(customEmojiKey("blob_cat")).toBe(":blob_cat@.:");
  });
});

describe("customEmojiPinKey", () => {
  it("wraps name and host in colons with an @ separator", () => {
    expect(customEmojiPinKey("blob_cat", "misskey.io")).toBe(":blob_cat@misskey.io:");
  });
});

describe("parseCustomEmojiPinKey", () => {
  it("splits name and host", () => {
    expect(parseCustomEmojiPinKey(":blob_cat@misskey.io:")).toEqual({
      name: "blob_cat",
      host: "misskey.io",
    });
  });

  it("returns a null host for keys without an @", () => {
    expect(parseCustomEmojiPinKey(":blob_cat:")).toEqual({ name: "blob_cat", host: null });
  });

  it("splits on the last @ when the name itself contains one", () => {
    expect(parseCustomEmojiPinKey(":weird@name@misskey.io:")).toEqual({
      name: "weird@name",
      host: "misskey.io",
    });
  });
});
