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
    gapMarker: null,
    fillingGap: false,
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

function makeNormalTab(overrides: Partial<TabView> = {}): TabView {
  return {
    id: "tab1",
    accountId: ACCOUNT_ID,
    kind: { type: "home" },
    title: "ホーム",
    customTitle: null,
    filter: { kind: "keywords", value: [] },
    notifyDesktop: false,
    notifySound: false,
    notifySoundChoice: "",
    notes: [],
    notifications: [],
    state: "connected",
    loadingMore: false,
    gapMarker: null,
    fillingGap: false,
    selectedNoteId: null,
    ...overrides,
  };
}

describe("ユーザー直接操作の失敗はerrorModalに記録される(Issue #183)", () => {
  beforeEach(() => {
    app.errorModal = null;
  });

  it("renoteが失敗するとerrorModalがセットされる", async () => {
    invokeMock.mockRejectedValueOnce(new Error("renote failed"));
    await app.renote(ACCOUNT_ID, "note1");
    expect(app.errorModal).toContain("renote failed");
  });

  it("toggleReactionが失敗するとerrorModalがセットされる", async () => {
    const note = makeNote({ id: "note-react", myReaction: null });
    app.groups = [makeGroup([makeNormalTab({ notes: [note] })])];
    invokeMock.mockRejectedValueOnce(new Error("react failed"));
    await app.toggleReaction(ACCOUNT_ID, "note-react", "👍");
    expect(app.errorModal).toContain("react failed");
  });

  it("toggleFavoriteが失敗するとerrorModalがセットされる", async () => {
    const note = makeNote({ id: "note-fav", isFavoritedByMe: false });
    app.groups = [makeGroup([makeNormalTab({ notes: [note] })])];
    invokeMock.mockRejectedValueOnce(new Error("favorite failed"));
    await app.toggleFavorite(ACCOUNT_ID, "note-fav");
    expect(app.errorModal).toContain("favorite failed");
  });

  it("votePollが失敗するとerrorModalがセットされる", async () => {
    const note = makeNote({
      id: "note-poll",
      poll: { choices: [{ text: "a", votes: 0, isVoted: false }], multiple: false, expiresAt: null },
    });
    app.groups = [makeGroup([makeNormalTab({ notes: [note] })])];
    invokeMock.mockRejectedValueOnce(new Error("vote failed"));
    await app.votePoll(ACCOUNT_ID, "note-poll", 0);
    expect(app.errorModal).toContain("vote failed");
  });

  it("addNoteToClipが失敗するとerrorModalがセットされる", async () => {
    invokeMock.mockRejectedValueOnce(new Error("clip failed"));
    await app.addNoteToClip(ACCOUNT_ID, "clip1", "note1");
    expect(app.errorModal).toContain("clip failed");
  });

  it("renameTabが失敗するとerrorModalがセットされる", async () => {
    app.groups = [makeGroup([makeNormalTab()])];
    invokeMock.mockRejectedValueOnce(new Error("rename failed"));
    await app.renameTab("tab1", "新しい名前");
    expect(app.errorModal).toContain("rename failed");
  });

  it("setColumnNotifyが失敗するとerrorModalがセットされる", async () => {
    app.groups = [makeGroup([makeNormalTab()])];
    invokeMock.mockRejectedValueOnce(new Error("notify failed"));
    await app.setColumnNotify("tab1", true, true, "default");
    expect(app.errorModal).toContain("notify failed");
  });

  it("persistGroupWidthが失敗してもerrorModalは変化しない(レイアウト操作は対象外)", async () => {
    invokeMock.mockRejectedValueOnce(new Error("width failed"));
    await app.persistGroupWidth("g1", 300);
    expect(app.errorModal).toBeNull();
  });
});

function makeNoteTab(notes: Note[], overrides: Partial<TabView> = {}): TabView {
  return {
    id: "tab1",
    accountId: ACCOUNT_ID,
    kind: { type: "home" },
    title: "ホーム",
    customTitle: null,
    filter: { kind: "keywords", value: [] },
    notifyDesktop: false,
    notifySound: false,
    notifySoundChoice: "",
    notes,
    notifications: [],
    state: "connected",
    loadingMore: false,
    gapMarker: null,
    fillingGap: false,
    selectedNoteId: null,
    ...overrides,
  };
}

describe("app.loadMore (Issue #239)", () => {
  it("MAX_NOTES(300件)到達後もbackfillで取得した古いノートを保持し、最古IDが前進すること", async () => {
    // MAX_NOTES は store.svelte.ts からは export されていないため、ここではリテラルの
    // 300 を使う(既存の GAP_CONTINUE_MAX_PAGES=10 のテストと同じ慣習)。
    // id は Misskey の aidx 同様、辞書順が新しい順と一致するよう時系列順の値にする。
    const notes = Array.from({ length: 300 }, (_, i) =>
      makeNote({ id: `n${String(300 - i).padStart(4, "0")}`, createdAt: 300 - i }),
    );
    const tab = makeNoteTab(notes);
    app.groups = [makeGroup([tab])];

    const oldestBefore = notes[notes.length - 1].id; // "n0001"
    const older = makeNote({ id: "n0000", createdAt: 0 });

    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "fetch_backfill") {
        expect(args).toMatchObject({ untilId: oldestBefore });
        return [older];
      }
      if (cmd === "capture_notes") return null;
      throw new Error(`unexpected command: ${cmd}`);
    });

    await app.loadMore(tab.id);

    const live = app.groups[0].tabs[0];
    // 取得した古いノートが切り捨てられず残っており、最古IDが前進していること。
    // (このアサーションは修正前は失敗する: 追加直後に slice(0, MAX_NOTES) で捨てられるため)
    expect(live.notes[live.notes.length - 1].id).toBe("n0000");

    // 2回目の loadMore が同じ until_id を繰り返さないこと(前進した最古IDを使うこと)
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "fetch_backfill") {
        expect(args).toMatchObject({ untilId: "n0000" });
        return [];
      }
      if (cmd === "capture_notes") return null;
      throw new Error(`unexpected command: ${cmd}`);
    });
    await app.loadMore(tab.id);
  });
});

describe("app.fillRemainingGap (Issue #148)", () => {
  it("gapMarkerが無ければ何もしない", async () => {
    const tab = makeNoteTab([makeNote({ id: "n1" })]);
    app.groups = [makeGroup([tab])];

    await app.fillRemainingGap(tab.id);

    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("targetIdに到達したらgapMarkerを消し、取得したノートをマージする", async () => {
    // id は Misskey の aidx 同様、辞書順が新しい順（sort比較は id 文字列比較のため、
    // 本物のidに近い辞書順=時系列順のIDを使う。"target"のような非時系列語は使わない）。
    const existing = [makeNote({ id: "n5", createdAt: 50 }), makeNote({ id: "n1", createdAt: 10 })];
    const tab = makeNoteTab(existing, { gapMarker: { boundaryId: "n4", targetId: "n1" } });
    app.groups = [makeGroup([tab])];

    // invoke は Tauri の Result<T,E> の生の解決値を返す(成功時はTをそのまま解決、
    // typedError側で {status,data} に包む)。ここで {status,data} を返すと二重包装になる。
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "fetch_backfill") {
        expect(args).toMatchObject({ columnId: "tab1", untilId: "n4" });
        return [makeNote({ id: "n3", createdAt: 30 }), makeNote({ id: "n1", createdAt: 10 })];
      }
      if (cmd === "capture_notes") return null;
      throw new Error(`unexpected command: ${cmd}`);
    });

    await app.fillRemainingGap(tab.id);

    // app.groups = [...] で渡した時点で Svelte の $state が渡したオブジェクトを
    // リアクティブプロキシ化するため、以降のミューテーションは元の `tab` 参照ではなく
    // app.groups 経由で読んだプロキシ側に反映される。
    const live = app.groups[0].tabs[0];
    expect(live.gapMarker).toBeNull();
    expect(live.fillingGap).toBe(false);
    expect(live.notes.map((n) => n.id)).toEqual(["n5", "n3", "n1"]);
  });

  it("targetIdに到達しないままページ上限に達したらgapMarkerを新しい境界で更新する", async () => {
    // targetId は全ページIDより辞書順で小さい値にする(=時系列でより古い)。
    // <= 比較で「到達」判定されないようにするため("target"のような非時系列語は使わない)。
    const tab = makeNoteTab(
      [makeNote({ id: "n5", createdAt: 50 })],
      { gapMarker: { boundaryId: "n4", targetId: "a0" } },
    );
    app.groups = [makeGroup([tab])];

    let call = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "fetch_backfill") {
        call += 1;
        return [makeNote({ id: `page${call}`, createdAt: 100 - call })];
      }
      if (cmd === "capture_notes") return null;
      throw new Error(`unexpected command: ${cmd}`);
    });

    await app.fillRemainingGap(tab.id);

    expect(call).toBe(10); // GAP_CONTINUE_MAX_PAGES
    const live = app.groups[0].tabs[0];
    expect(live.gapMarker).toEqual({ boundaryId: "page10", targetId: "a0" });
    expect(live.fillingGap).toBe(false);
  });

  it("APIが失敗したらgapMarkerを維持しfillingGapをfalseに戻し、errorModalをセットする(Issue #183)", async () => {
    const tab = makeNoteTab(
      [makeNote({ id: "n5", createdAt: 50 })],
      { gapMarker: { boundaryId: "n4", targetId: "a0" } },
    );
    app.groups = [makeGroup([tab])];
    app.errorModal = null;
    // invoke は Rust側の Err(Error) を reject として伝える(実行時のTauri invoke挙動)。
    // typedError は plain object の reject を {status:"error",error} に変換する。
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "fetch_backfill") throw { kind: "network", message: "boom" };
      throw new Error(`unexpected command: ${cmd}`);
    });

    await app.fillRemainingGap(tab.id);

    const live = app.groups[0].tabs[0];
    expect(live.gapMarker).toEqual({ boundaryId: "n4", targetId: "a0" });
    expect(live.fillingGap).toBe(false);
    expect(app.errorModal).not.toBeNull();
  });
});
