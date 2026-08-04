import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import type { Note, Notification, User } from "../bindings/tauri.gen";

// store.svelte.ts が起動時に @tauri-apps/plugin-os の platform() を呼ぶため、
// Tauri ランタイム外(jsdom)で import が失敗しないようスタブする。
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

const { default: NotificationCard } = await import("./NotificationCard.svelte");

afterEach(() => cleanup());

function makeUser(overrides: Partial<User> = {}): User {
  return {
    id: "u1",
    username: "alice",
    host: null,
    name: "Alice",
    avatarUrl: null,
    isBot: false,
    isCat: false,
    followersCount: 0,
    followingCount: 0,
    notesCount: 0,
    ...overrides,
  };
}

function makeNote(overrides: Partial<Note> = {}): Note {
  return {
    id: "n1",
    createdAt: 0,
    text: "hello",
    cw: null,
    visibility: "public",
    localOnly: false,
    user: makeUser({ id: "u2", name: "Bob" }),
    replyId: null,
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

function makeNotification(overrides: Partial<Notification> = {}): Notification {
  return {
    id: "notif1",
    createdAt: 0,
    type: "mention",
    user: makeUser({ id: "u2", name: "Bob" }),
    note: makeNote(),
    reaction: null,
    ...overrides,
  };
}

describe("NotificationCard note actions", () => {
  it("shows the action footer for a mention notification when accountId is given", () => {
    const notification = makeNotification({ type: "mention" });
    const { getByLabelText } = render(NotificationCard, {
      props: { notification, accountId: "a1" },
    });
    expect(getByLabelText("返信")).toBeTruthy();
  });

  it("does not show the action footer without accountId", () => {
    const notification = makeNotification({ type: "mention" });
    const { queryByLabelText } = render(NotificationCard, {
      props: { notification },
    });
    expect(queryByLabelText("返信")).toBeNull();
  });

  it("does not render a note preview for note-less notifications like follow", () => {
    const notification = makeNotification({ type: "follow", note: null });
    const { container } = render(NotificationCard, {
      props: { notification, accountId: "a1" },
    });
    expect(container.querySelector(".note-preview")).toBeNull();
  });
});
