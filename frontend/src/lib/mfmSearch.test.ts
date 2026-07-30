import { describe, expect, it, vi } from "vitest";

vi.mock("./ipc", () => ({
  commands: {
    searchUsers: vi.fn(),
    searchHashtags: vi.fn(),
  },
  unwrap: async <T>(p: Promise<{ status: "ok"; data: T } | { status: "error"; error: unknown }>) => {
    const r = await p;
    if (r.status === "ok") return r.data;
    throw new Error("unwrap failed in test");
  },
}));

import { commands } from "./ipc";
import { searchHashtagItems, searchMentionItems } from "./mfmSearch";

function user(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    id: "1",
    username: "alice",
    host: null,
    name: "Alice",
    avatarUrl: "https://example.com/a.png",
    isBot: false,
    isCat: false,
    followersCount: 0,
    followingCount: 0,
    notesCount: 0,
    emojis: {},
    ...overrides,
  };
}

describe("searchMentionItems", () => {
  it("maps a local user to @username with an avatar thumbnail", async () => {
    vi.mocked(commands.searchUsers).mockResolvedValue({ status: "ok", data: [user()] } as never);
    const items = await searchMentionItems("acc1", "ali");
    expect(items).toEqual([
      {
        key: "user:1",
        label: "@alice",
        insertText: "@alice",
        thumbnail: { type: "avatar", url: "https://example.com/a.png" },
      },
    ]);
    expect(commands.searchUsers).toHaveBeenCalledWith("acc1", "ali");
  });

  it("maps a remote user to @username@host", async () => {
    vi.mocked(commands.searchUsers).mockResolvedValue({
      status: "ok",
      data: [user({ id: "2", username: "bob", host: "example.com" })],
    } as never);
    const items = await searchMentionItems("acc1", "bob");
    expect(items[0]).toMatchObject({ key: "user:2", label: "@bob@example.com", insertText: "@bob@example.com" });
  });

  it("omits the thumbnail when the user has no avatar", async () => {
    vi.mocked(commands.searchUsers).mockResolvedValue({
      status: "ok",
      data: [user({ avatarUrl: null })],
    } as never);
    const items = await searchMentionItems("acc1", "ali");
    expect(items[0].thumbnail).toBeUndefined();
  });

  it("propagates a rejection when the search command fails", async () => {
    vi.mocked(commands.searchUsers).mockResolvedValue({
      status: "error",
      error: { kind: "network", message: "offline" },
    } as never);
    await expect(searchMentionItems("acc1", "ali")).rejects.toThrow();
  });
});

describe("searchHashtagItems", () => {
  it("maps tag strings to #tag items", async () => {
    vi.mocked(commands.searchHashtags).mockResolvedValue({
      status: "ok",
      data: ["misskey", "tsumugi"],
    } as never);
    const items = await searchHashtagItems("acc1", "mi");
    expect(items).toEqual([
      { key: "tag:misskey", label: "#misskey", insertText: "#misskey" },
      { key: "tag:tsumugi", label: "#tsumugi", insertText: "#tsumugi" },
    ]);
    expect(commands.searchHashtags).toHaveBeenCalledWith("acc1", "mi");
  });

  it("propagates a rejection when the search command fails", async () => {
    vi.mocked(commands.searchHashtags).mockResolvedValue({
      status: "error",
      error: { kind: "network", message: "offline" },
    } as never);
    await expect(searchHashtagItems("acc1", "mi")).rejects.toThrow();
  });
});
