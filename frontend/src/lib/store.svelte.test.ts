import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GroupView, TabView } from "./store.svelte";
import type { Note, Notification, User } from "../bindings/tauri.gen";

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

function makeNotification(overrides: Partial<Notification> = {}): Notification {
  return {
    id: "notif1",
    createdAt: 0,
    type: "reaction",
    user: makeUser(),
    note: null,
    reaction: null,
    ...overrides,
  };
}

/// 通知カラムのみに note を保持する TabView（どの t.notes にも存在しない状態を再現する）。
function makeNotificationOnlyTab(note: Note): TabView {
  return {
    id: "tab1",
    accountId: ACCOUNT_ID,
    kind: { type: "notifications" },
    title: "通知",
    customTitle: null,
    filter: { kind: "keywords", value: [] },
    notifyDesktop: false,
    notifySound: false,
    notifySoundChoice: "",
    notes: [],
    notifications: [makeNotification({ note })],
    state: "connected",
    loadingMore: false,
    selectedNoteId: null,
  };
}

function makeGroup(tabs: TabView[]): GroupView {
  return {
    id: "group1",
    width: 400,
    auto: false,
    tabs,
    activeTabId: tabs[0]?.id ?? "",
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

describe("notification-only note actions (Issue #50 follow-up)", () => {
  it("toggleReaction reaches a note that only exists inside a notification", async () => {
    const note = makeNote({ id: "note-only-in-notification" });
    app.groups = [makeGroup([makeNotificationOnlyTab(note)])];

    await app.toggleReaction(ACCOUNT_ID, note.id, "👍");

    expect(invokeMock).toHaveBeenCalledWith(
      "react",
      expect.objectContaining({ accountId: ACCOUNT_ID, noteId: note.id, reaction: "👍" }),
    );
  });

  it("toggleReaction also reaches the inner note of a pure-renote notification", async () => {
    const inner = makeNote({ id: "inner-note" });
    const wrapper = makeNote({ id: "wrapper-note", text: null, renote: inner });
    app.groups = [makeGroup([makeNotificationOnlyTab(wrapper)])];

    await app.toggleReaction(ACCOUNT_ID, inner.id, "👍");

    expect(invokeMock).toHaveBeenCalledWith(
      "react",
      expect.objectContaining({ accountId: ACCOUNT_ID, noteId: inner.id, reaction: "👍" }),
    );
  });

  it("toggleFavorite reaches a note that only exists inside a notification", async () => {
    const note = makeNote({ id: "note-only-in-notification-fav" });
    app.groups = [makeGroup([makeNotificationOnlyTab(note)])];

    await app.toggleFavorite(ACCOUNT_ID, note.id);

    expect(invokeMock).toHaveBeenCalledWith(
      "favorite_note",
      expect.objectContaining({ accountId: ACCOUNT_ID, noteId: note.id }),
    );
  });

  it("votePoll reaches a poll note that only exists inside a notification", async () => {
    const note = makeNote({
      id: "note-only-in-notification-poll",
      poll: {
        choices: [
          { text: "A", votes: 0, isVoted: false },
          { text: "B", votes: 0, isVoted: false },
        ],
        multiple: false,
        expiresAt: null,
      },
    });
    app.groups = [makeGroup([makeNotificationOnlyTab(note)])];

    await app.votePoll(ACCOUNT_ID, note.id, 0);

    expect(invokeMock).toHaveBeenCalledWith(
      "vote_poll",
      expect.objectContaining({ accountId: ACCOUNT_ID, noteId: note.id, choice: 0 }),
    );
  });

  it("does nothing (no backend call) when the note isn't in any tab's notes or notifications", async () => {
    app.groups = [makeGroup([makeNotificationOnlyTab(makeNote({ id: "unrelated" }))])];

    await app.toggleReaction(ACCOUNT_ID, "totally-absent-note", "👍");

    expect(invokeMock).not.toHaveBeenCalled();
  });
});

describe("app.getUserProfile / followUser / unfollowUser", () => {
  it("getUserProfileはコマンド結果をそのまま返す", async () => {
    const profile = { user: makeUser(), isFollowing: false, isSelf: false };
    invokeMock.mockResolvedValueOnce(profile);
    const result = await app.getUserProfile(ACCOUNT_ID, "u1");
    expect(result).toEqual(profile);
    expect(invokeMock).toHaveBeenCalledWith(
      "get_user_profile",
      expect.objectContaining({ accountId: ACCOUNT_ID, userId: "u1" }),
    );
  });

  it("followUserはfollow_userコマンドを呼ぶ", async () => {
    invokeMock.mockResolvedValueOnce({ status: "ok", data: null });
    await app.followUser(ACCOUNT_ID, "u1");
    expect(invokeMock).toHaveBeenCalledWith(
      "follow_user",
      expect.objectContaining({ accountId: ACCOUNT_ID, userId: "u1" }),
    );
  });

  it("unfollowUserはunfollow_userコマンドを呼ぶ", async () => {
    invokeMock.mockResolvedValueOnce({ status: "ok", data: null });
    await app.unfollowUser(ACCOUNT_ID, "u1");
    expect(invokeMock).toHaveBeenCalledWith(
      "unfollow_user",
      expect.objectContaining({ accountId: ACCOUNT_ID, userId: "u1" }),
    );
  });

  // ProfileModal/FollowListModal は自前のエラー表示を持つため、これらのメソッドは失敗時に
  // グローバルエラーモーダル(app.errorModal)を出さずBackstageログ(app.logs)にのみ記録する
  // 必要がある(投稿欄の下に重複してエラーが出る、というユーザー報告への修正の回帰テスト)。
  it("getUserProfileが失敗してもapp.errorModalは変化せずBackstageに記録される", async () => {
    app.errorModal = null;
    const logsBefore = app.logs.length;
    invokeMock.mockRejectedValueOnce(new Error("boom"));
    await expect(app.getUserProfile(ACCOUNT_ID, "u1")).rejects.toThrow("boom");
    expect(app.errorModal).toBeNull();
    expect(app.logs.length).toBe(logsBefore + 1);
    // #log は新しいログを先頭に追加するため、最新エントリはlogs[0]。
    expect(app.logs[0].level).toBe("error");
  });
});

// jsdomはmatchMediaを実装していないため、"(prefers-color-scheme: dark)"に対してのみ
// 指定のmatchesを返すスタブを用意する。addEventListener("change", ...)は登録したリスナーを
// 保持し、テストからdispatchChange()で疑似的なOS設定変化を発火できるようにする。
function mockPrefersColorSchemeDark(matches: boolean) {
  const changeListeners: Array<(e: { matches: boolean }) => void> = [];
  const mql = {
    matches,
    media: "(prefers-color-scheme: dark)",
    addEventListener: (type: string, listener: (e: { matches: boolean }) => void) => {
      if (type === "change") changeListeners.push(listener);
    },
    removeEventListener: (type: string, listener: (e: { matches: boolean }) => void) => {
      if (type !== "change") return;
      const i = changeListeners.indexOf(listener);
      if (i !== -1) changeListeners.splice(i, 1);
    },
    dispatchEvent: () => false,
  };
  vi.stubGlobal("matchMedia", (query: string) => {
    if (query === "(prefers-color-scheme: dark)") return mql;
    return { matches: false, media: query, addEventListener: () => {}, removeEventListener: () => {}, dispatchEvent: () => false };
  });
  return {
    dispatchChange: (nextMatches: boolean) => {
      mql.matches = nextMatches;
      for (const listener of changeListeners) listener({ matches: nextMatches });
    },
    listenerCount: () => changeListeners.length,
  };
}

describe("#applyTheme (Issue #170: data-theme属性から.darkクラスへ移行)", () => {
  afterEach(() => {
    document.documentElement.classList.remove("dark", "light");
    vi.unstubAllGlobals();
  });

  it("theme='dark'のとき<html>にdarkクラスが付与される", async () => {
    mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, theme: "dark" });
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.documentElement.classList.contains("light")).toBe(false);
  });

  it("theme='light'のとき<html>にlightクラスが付与される", async () => {
    mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, theme: "light" });
    expect(document.documentElement.classList.contains("light")).toBe(true);
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("theme='auto'のときOSがdark選好なら<html>にdarkクラスが付与される", async () => {
    mockPrefersColorSchemeDark(true);
    await app.setUiPrefs({ ...app.ui, theme: "auto" });
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.documentElement.classList.contains("light")).toBe(false);
  });

  it("theme='auto'のときOSがdark選好でなければ<html>にdark/lightどちらのクラスも付与されない", async () => {
    mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, theme: "dark" });
    await app.setUiPrefs({ ...app.ui, theme: "auto" });
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(document.documentElement.classList.contains("light")).toBe(false);
  });

  it("theme='preset:*'のとき<html>にdark/lightどちらのクラスも付与されない(プリセット/カスタムテーマは既存動作を維持)", async () => {
    mockPrefersColorSchemeDark(true);
    await app.setUiPrefs({ ...app.ui, theme: "preset:tokyo-night" });
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(document.documentElement.classList.contains("light")).toBe(false);
  });

  it("light→darkへの直接遷移でdarkが付与されlightは外れる", async () => {
    mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, theme: "light" });
    await app.setUiPrefs({ ...app.ui, theme: "dark" });
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.documentElement.classList.contains("light")).toBe(false);
  });

  it("theme='auto'適用中にOS設定がライブでdarkに変わると<html>にdarkクラスが付与される", async () => {
    const { dispatchChange } = mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, theme: "auto" });
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    dispatchChange(true);
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("#applyThemeを繰り返し呼んでもmatchMediaのchangeリスナーは多重登録されない", async () => {
    const { listenerCount } = mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, theme: "auto" });
    await app.setUiPrefs({ ...app.ui, theme: "auto" });
    expect(listenerCount()).toBe(1);
  });

  it("theme='auto'から他テーマへ切り替えるとmatchMediaのchangeリスナーが解除される", async () => {
    const { listenerCount } = mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, theme: "auto" });
    expect(listenerCount()).toBe(1);
    await app.setUiPrefs({ ...app.ui, theme: "light" });
    expect(listenerCount()).toBe(0);
  });
});
