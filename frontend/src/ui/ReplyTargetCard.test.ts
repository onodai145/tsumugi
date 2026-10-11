import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import type { DriveFile, Note, User } from "../bindings/tauri.gen";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("../lib/profileModal.svelte", () => ({ openProfile: vi.fn() }));

const { default: ReplyTargetCard } = await import("./ReplyTargetCard.svelte");
const { openProfile } = await import("../lib/profileModal.svelte");
const { app } = await import("../lib/store.svelte");

afterEach(() => {
  cleanup();
  vi.mocked(openProfile).mockClear();
});

function makeUser(overrides: Partial<User> = {}): User {
  return {
    id: "u2",
    username: "bob",
    host: null,
    name: "Bob",
    avatarUrl: "https://example.com/bob.png",
    isBot: false,
    isCat: false,
    followersCount: 0,
    followingCount: 0,
    notesCount: 0,
    ...overrides,
  };
}

function file(): DriveFile {
  return { id: "f1", mimeType: "image/png", isSensitive: false, url: "https://example.com/f", thumbnailUrl: null, name: "f", size: 1 };
}

function makeNote(overrides: Partial<Note> = {}): Note {
  return {
    id: "parent1",
    createdAt: 0,
    text: "元のノートの本文",
    cw: null,
    visibility: "public",
    localOnly: false,
    user: makeUser(),
    replyId: null,
    replyUserId: null,
    reply: null,
    renoteId: null,
    renote: null,
    files: [],
    poll: null,
    tags: [],
    mentions: [],
    emojis: {},
    channelId: null,
    via: null,
    lang: null,
    reactions: {},
    reactionCount: 0,
    renoteCount: 0,
    replyCount: 0,
    myReaction: null,
    isRenotedByMe: false,
    isFavoritedByMe: false,
    isPinned: false,
    ...overrides,
  };
}

describe("ReplyTargetCard", () => {
  it("shows the avatar, display name, @acct and relative time", () => {
    const nowSec = Math.floor(Date.now() / 1000);
    const { getByTestId } = render(ReplyTargetCard, {
      props: { note: makeNote({ createdAt: nowSec - 30 }) },
    });
    const card = getByTestId("reply-card");
    const img = card.querySelector("img") as HTMLImageElement;
    expect(img.getAttribute("src")).toBe("https://example.com/bob.png");
    expect(getByTestId("reply-card-name").textContent).toContain("Bob");
    expect(getByTestId("reply-card-acct").textContent).toBe("@bob");
    expect(getByTestId("reply-card-time").textContent?.trim()).toBe("30s");
  });

  it("shows the body with line-clamp-3 and keeps newlines", () => {
    const { getByTestId } = render(ReplyTargetCard, {
      props: { note: makeNote({ text: "一行目\n二行目" }) },
    });
    const body = getByTestId("reply-card-body");
    expect(body.className).toContain("line-clamp-3");
    expect(body.className).toContain("whitespace-pre-wrap");
    expect(body.querySelector("br")).not.toBeNull();
    expect(body.textContent).toContain("一行目");
    expect(body.textContent).toContain("二行目");
  });

  it("shows the cw instead of the body", () => {
    const { getByTestId } = render(ReplyTargetCard, {
      props: { note: makeNote({ cw: "ネタバレ", text: "秘密の本文" }) },
    });
    const text = getByTestId("reply-card").textContent ?? "";
    expect(text).toContain("ネタバレ");
    expect(text).not.toContain("秘密の本文");
  });

  it("shows a file-count label for file-only notes", () => {
    const { getByTestId } = render(ReplyTargetCard, {
      props: { note: makeNote({ text: null, files: [file()] }) },
    });
    expect(getByTestId("reply-card-body").textContent).toContain("(1つのファイル)");
  });

  it("renders no body line when there is nothing to show", () => {
    const { queryByTestId } = render(ReplyTargetCard, {
      props: { note: makeNote({ text: null }) },
    });
    expect(queryByTestId("reply-card-body")).toBeNull();
  });

  it("leaves a mb-2 gap below the card, before the reply note body", () => {
    const { getByTestId } = render(ReplyTargetCard, { props: { note: makeNote() } });
    const cls = getByTestId("reply-card").classList;
    expect(cls.contains("mb-2")).toBe(true);
    expect(cls.contains("mb-0.5")).toBe(false);
  });

  it("has no expand toggle", () => {
    const { getByTestId, container } = render(ReplyTargetCard, { props: { note: makeNote() } });
    expect(getByTestId("reply-card").querySelector("[aria-expanded]")).toBeNull();
    expect(container.querySelector("button")).toBeNull();
  });

  it("opens the profile when the avatar is clicked", async () => {
    const { getByTestId } = render(ReplyTargetCard, { props: { note: makeNote(), accountId: "acc1" } });
    await fireEvent.click(getByTestId("reply-card").querySelector("img") as HTMLElement);
    expect(openProfile).toHaveBeenCalledWith({ userId: "u2" }, "acc1");
  });

  it("opens the profile when the avatar fallback is activated by Enter", async () => {
    const { getByTestId } = render(ReplyTargetCard, {
      props: { note: makeNote({ user: makeUser({ avatarUrl: null }) }), accountId: "acc1" },
    });
    const fallback = getByTestId("reply-card-avatar");
    await fireEvent.keyDown(fallback, { key: "Enter" });
    expect(openProfile).toHaveBeenCalledWith({ userId: "u2" }, "acc1");
  });

  it("opens the profile when the name is clicked", async () => {
    const { getByTestId } = render(ReplyTargetCard, { props: { note: makeNote(), accountId: "acc1" } });
    await fireEvent.click(getByTestId("reply-card-name"));
    expect(openProfile).toHaveBeenCalledWith({ userId: "u2" }, "acc1");
  });

  it("resolves custom emojis in the body from emojiMap", () => {
    const { getByTestId } = render(ReplyTargetCard, {
      props: {
        note: makeNote({ text: "やあ :localmoji:" }),
        emojiMap: { localmoji: "https://local.example/localmoji.png" },
      },
    });
    const img = getByTestId("reply-card-body").querySelector("img.custom-emoji");
    expect(img?.getAttribute("src")).toBe("https://local.example/localmoji.png");
  });

  it("updates the relative time when app.now advances", async () => {
    vi.useFakeTimers();
    try {
      const nowSec = Math.floor(Date.now() / 1000);
      const { getByTestId } = render(ReplyTargetCard, { props: { note: makeNote({ createdAt: nowSec - 30 }) } });
      const t = getByTestId("reply-card-time");
      expect(t.textContent?.trim()).toBe("30s");
      vi.setSystemTime(Date.now() + 90_000);
      app.now = Date.now();
      await vi.advanceTimersByTimeAsync(0);
      expect(t.textContent?.trim()).toBe("2m");
    } finally {
      vi.useRealTimers();
    }
  });
});
