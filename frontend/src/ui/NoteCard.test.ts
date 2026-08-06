import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import type { Note, User } from "../bindings/tauri.gen";

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
vi.mock("../lib/profileModal.svelte", () => ({ openProfile: vi.fn() }));

const { default: NoteCard } = await import("./NoteCard.svelte");
const { openProfile } = await import("../lib/profileModal.svelte");

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
    user: makeUser(),
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

describe("NoteCard action banner", () => {
  it("shows a renote banner for a pure renote by default", () => {
    const note = makeNote({
      text: null,
      user: makeUser({ id: "u2", name: "Bob" }),
      renote: makeNote({ id: "n0", user: makeUser({ id: "u1", name: "Alice" }) }),
    });
    const { getByText } = render(NoteCard, { props: { note } });
    expect(getByText(/がRenote/)).toBeTruthy();
  });

  it("hides the renote banner when hideActionBanner is set", () => {
    const note = makeNote({
      text: null,
      user: makeUser({ id: "u2", name: "Bob" }),
      renote: makeNote({ id: "n0", user: makeUser({ id: "u1", name: "Alice" }) }),
    });
    const { queryByText } = render(NoteCard, { props: { note, hideActionBanner: true } });
    expect(queryByText(/がRenote/)).toBeNull();
  });

  it("shows a reply banner for a reply by default", () => {
    const note = makeNote({ replyId: "parent1" });
    const { getByText } = render(NoteCard, { props: { note } });
    expect(getByText("返信")).toBeTruthy();
  });

  it("hides the reply banner when hideActionBanner is set", () => {
    const note = makeNote({ replyId: "parent1" });
    const { queryByText } = render(NoteCard, { props: { note, hideActionBanner: true } });
    expect(queryByText("返信")).toBeNull();
  });

  it("does not show the quoted note's own reactions", () => {
    const note = makeNote({
      text: "見て",
      renote: makeNote({ id: "n0", reactions: { "👍": 3 }, reactionCount: 3 }),
    });
    const { container } = render(NoteCard, { props: { note } });
    expect(container.querySelector(".reaction-wrap")).toBeNull();
  });
});

describe("NoteCard showActions", () => {
  it("hides the action footer for a quoted note by default even with accountId", () => {
    const note = makeNote({ id: "n1" });
    const { queryByLabelText } = render(NoteCard, {
      props: { note, quoted: true, accountId: "a1" },
    });
    expect(queryByLabelText("返信")).toBeNull();
  });

  it("shows the action footer for a quoted note when showActions is set", () => {
    const note = makeNote({ id: "n1" });
    const { getByLabelText } = render(NoteCard, {
      props: { note, quoted: true, accountId: "a1", showActions: true },
    });
    expect(getByLabelText("返信")).toBeTruthy();
  });

  it("still shows the action footer for a non-quoted note with accountId (unchanged default)", () => {
    const note = makeNote({ id: "n1" });
    const { getByLabelText } = render(NoteCard, {
      props: { note, accountId: "a1" },
    });
    expect(getByLabelText("返信")).toBeTruthy();
  });
});

describe("プロフィール導線", () => {
  it("アバタークリックでopenProfileが呼ばれる", () => {
    const note = makeNote();
    const { container } = render(NoteCard, { props: { note, accountId: "acc1" } });
    const avatar = container.querySelector(".avatar") as HTMLElement;
    avatar.click();
    expect(openProfile).toHaveBeenCalledWith({ userId: note.user.id }, "acc1");
  });
});
