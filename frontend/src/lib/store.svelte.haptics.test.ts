import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Note, User } from "../bindings/tauri.gen";

// store.svelte.ts が起動時に @tauri-apps/plugin-os の platform() を呼ぶため、
// Tauri ランタイム外(jsdom/node)で import が失敗しないようスタブする（NoteCard.test.ts と同じ構成）。
vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
const invokeMock = vi.fn().mockResolvedValue({ status: "ok", data: null });
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
// このテストファイルはモバイル向けのハプティクス発火を検証するため、実機OS判定に依らず
// isMobilePlatform を true に固定する(@tauri-apps/plugin-os のモックは "linux" のまま)。
// vi.mock はファイルスコープで効くため、この固定化の影響を他のstore.svelteテストから
// 隔離する目的で本ファイルを独立させている(最終レビュー指摘)。
vi.mock("./platform", () => ({ isMobilePlatform: true }));

const { app } = await import("./store.svelte");

const ACCOUNT_ID = "acc1";

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

beforeEach(() => {
  invokeMock.mockClear();
  invokeMock.mockResolvedValue({ status: "ok", data: null });
  app.groups = [];
});

afterEach(() => {
  app.groups = [];
});

describe("ハプティクス(Issue #26)", () => {
  it("新規リアクション付与時にvibrateコマンドをlightパターンで呼ぶ", async () => {
    const note = makeNote({ id: "note-haptics-add" });
    await app.toggleReaction(ACCOUNT_ID, note.id, "👍", note);

    expect(invokeMock).toHaveBeenCalledWith("vibrate", { pattern: "light" });
  });

  it("リアクション取り消し時はvibrateコマンドを呼ばない", async () => {
    const note = makeNote({ id: "note-haptics-remove", myReaction: "👍" });
    await app.toggleReaction(ACCOUNT_ID, note.id, "👍", note);

    expect(invokeMock).not.toHaveBeenCalledWith("vibrate", expect.anything());
  });
});
