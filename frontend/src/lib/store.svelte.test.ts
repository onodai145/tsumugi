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
// applyUiScale は実 WebView を触るため差し替える。boot() を呼ぶ既存テストでも例外にならないよう、
// 既定で resolve する Promise を返す。
const applyUiScaleMock = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
vi.mock("./uiScale", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./uiScale")>()),
  applyUiScale: applyUiScaleMock,
}));

const { app, applyOwnReactionEvent } = await import("./store.svelte");

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
    replyUserId: null,
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
    selectionMoveSeq: 0,
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

  it("noteを直接渡せば、どのタブにも通知にも無くてもリアクションを送る", async () => {
    app.groups = [makeGroup([makeNotificationOnlyTab(makeNote({ id: "unrelated" }))])];
    const note = makeNote({ id: "search-only-note" });

    await app.toggleReaction(ACCOUNT_ID, note.id, "👍", note);

    expect(invokeMock).toHaveBeenCalledWith(
      "react",
      expect.objectContaining({ accountId: ACCOUNT_ID, noteId: note.id, reaction: "👍" }),
    );
    expect(note.myReaction).toBe("👍");
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
    selectionMoveSeq: 0,
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
    selectionMoveSeq: 0,
    ...overrides,
  };
}

describe("app.loadMore (Issue #239)", () => {
  it("通常の上スクロールはキャッシュ優先を使う(bypassCache=false で fetch_backfill を呼ぶ, Issue #427)", async () => {
    const tab = makeNoteTab([makeNote({ id: "n0002", createdAt: 2 })]);
    app.groups = [makeGroup([tab])];

    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "fetch_backfill") {
        expect(args).toMatchObject({ columnId: tab.id, untilId: "n0002", bypassCache: false });
        return [];
      }
      if (cmd === "capture_notes") return null;
      throw new Error(`unexpected command: ${cmd}`);
    });

    await app.loadMore(tab.id);

    expect(invokeMock).toHaveBeenCalledWith("fetch_backfill", expect.objectContaining({ bypassCache: false }));
  });

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

  it("MAX_NOTES超過後も先頭(最新ノート)を切り捨てないこと(遡った後に一番上へ戻ると最新に見えない不具合)", async () => {
    const notes = Array.from({ length: 300 }, (_, i) =>
      makeNote({ id: `n${String(300 - i).padStart(4, "0")}`, createdAt: 300 - i }),
    );
    const tab = makeNoteTab(notes);
    app.groups = [makeGroup([tab])];
    const newestId = notes[0].id; // "n0300"

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "fetch_backfill") return [makeNote({ id: "n0000", createdAt: 0 })];
      if (cmd === "capture_notes") return null;
      throw new Error(`unexpected command: ${cmd}`);
    });

    await app.loadMore(tab.id);

    const live = app.groups[0].tabs[0];
    // 300件を超えても先頭は元々の最新ノートのままであること
    // (slice(-MAX_NOTES) で先頭=最新側を切り捨てると、一番上へスクロールしても
    // 最新のノートが表示されなくなる)
    expect(live.notes[0].id).toBe(newestId);
    expect(live.notes.length).toBe(301);
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

  it("MAX_NOTES(300件)到達後も取得した古いノートを切り捨てず、マーカーの基準ノートが一覧に残る(Issue #433)", async () => {
    // 300件ちょうど。gapMarker の基準(boundaryId)は表示中の最古ノート。
    const notes = Array.from({ length: 300 }, (_, i) =>
      makeNote({ id: `n${String(1300 - i).padStart(4, "0")}`, createdAt: 1300 - i }),
    );
    const tab = makeNoteTab(notes, { gapMarker: { boundaryId: "n1001", targetId: "a0" } });
    app.groups = [makeGroup([tab])];

    let call = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "fetch_backfill") {
        call += 1;
        // 1ページ目だけ古いノートを返し、2ページ目で空を返してループを止める
        return call === 1 ? [makeNote({ id: "n0900", createdAt: 900 })] : [];
      }
      if (cmd === "capture_notes") return null;
      throw new Error(`unexpected command: ${cmd}`);
    });

    await app.fillRemainingGap(tab.id);

    const live = app.groups[0].tabs[0];
    // slice(0, MAX_NOTES) で古い側を切り捨てると、更新後の boundaryId("n0900")が
    // 一覧から消え、Column.svelte がマーカーを描画できなくなる。
    expect(live.notes.map((n) => n.id)).toContain("n0900");
    expect(live.notes).toHaveLength(301);
    expect(live.gapMarker).toEqual({ boundaryId: "n0900", targetId: "a0" });
    expect(live.notes.some((n) => n.id === live.gapMarker?.boundaryId)).toBe(true);
  });

  it("全ページで bypassCache=true を指定してキャッシュHitを避ける(Issue #427)", async () => {
    const tab = makeNoteTab(
      [makeNote({ id: "n5", createdAt: 50 })],
      { gapMarker: { boundaryId: "n4", targetId: "a0" } },
    );
    app.groups = [makeGroup([tab])];

    const calls: unknown[] = [];
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "fetch_backfill") {
        calls.push(args);
        // 2ページ目で targetId("a0") に到達させる
        return calls.length === 1
          ? [makeNote({ id: "n3", createdAt: 30 })]
          : [makeNote({ id: "a0", createdAt: 1 })];
      }
      if (cmd === "capture_notes") return null;
      throw new Error(`unexpected command: ${cmd}`);
    });

    await app.fillRemainingGap(tab.id);

    expect(calls).toHaveLength(2);
    for (const args of calls) {
      expect(args).toMatchObject({ columnId: tab.id, bypassCache: true });
    }
    expect(calls[0]).toMatchObject({ untilId: "n4" });
    expect(calls[1]).toMatchObject({ untilId: "n3" });
  });
});

describe("AppStore.now (共有tick)", () => {
  beforeEach(() => {
    // boot() は list_accounts の結果を this.accounts に代入し、その直後に
    // #refreshInstanceMeta() を await せず呼ぶ(fire-and-forget)。デフォルトの
    // モック({status:"ok",data:null}を返す)だと typedError の二重ラップにより
    // this.accounts が配列でなくなり、#refreshInstanceMeta 内の
    // this.accounts.map が例外を投げて未捕捉rejectionになる。
    // list_accounts だけ実配列を返すよう上書きする。
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_accounts") return [];
      return { status: "ok", data: null };
    });
  });

  afterEach(() => {
    app.teardown();
    vi.useRealTimers();
  });

  it("boot()後、5秒ごとにnowが更新される", async () => {
    vi.useFakeTimers();
    const before = app.now;
    // boot()はネットワーク呼び出しを含み失敗しうるが、失敗してもfinallyでタイマーは起動される
    const bootPromise = app.boot();
    await vi.advanceTimersByTimeAsync(0);
    await bootPromise.catch(() => {});

    vi.setSystemTime(Date.now() + 5_000);
    await vi.advanceTimersByTimeAsync(5_000);

    expect(app.now).toBeGreaterThan(before);
  });

  it("teardown()後はnowが更新されなくなる", async () => {
    vi.useFakeTimers();
    const bootPromise = app.boot();
    await vi.advanceTimersByTimeAsync(0);
    await bootPromise.catch(() => {});

    app.teardown();
    const after = app.now;
    await vi.advanceTimersByTimeAsync(5_000);

    expect(app.now).toBe(after);
  });
});

describe("#applyAvatarRadius (Issue #94: アイコンの丸みカスタマイズ)", () => {
  beforeEach(() => {
    // 直前の "AppStore.now" ブロックが boot() を list_accounts 以外 data:null で
    // 呼び出すため、this.ui.theme が undefined のまま残ることがある(#applyTheme が
    // parseThemeRef(undefined) で例外を投げるのを防ぐため、既知の値に戻しておく)。
    app.ui = { ...app.ui, theme: "auto" };
  });

  afterEach(() => {
    document.documentElement.style.removeProperty("--avatar-radius");
    vi.unstubAllGlobals();
  });

  it("setUiPrefsでavatarRadiusを指定すると--avatar-radius CSS変数に反映される", async () => {
    mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, avatarRadius: 65 });
    expect(document.documentElement.style.getPropertyValue("--avatar-radius")).toBe("65%");
  });

  it("avatarRadiusが範囲外(100超)でも100%にクランプされる", async () => {
    mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, avatarRadius: 150 });
    expect(document.documentElement.style.getPropertyValue("--avatar-radius")).toBe("100%");
  });

  it("avatarRadiusが範囲外(負数)でも0%にクランプされる", async () => {
    mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, avatarRadius: -10 });
    expect(document.documentElement.style.getPropertyValue("--avatar-radius")).toBe("0%");
  });
});

describe("UIスケール(Issue #40)", () => {
  beforeEach(() => {
    applyUiScaleMock.mockReset();
    applyUiScaleMock.mockResolvedValue(undefined);
    app.ui = { ...app.ui, theme: "auto" };
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("setUiPrefsでuiScaleを指定するとapplyUiScaleに渡される", async () => {
    mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, uiScale: 130 });
    expect(applyUiScaleMock).toHaveBeenCalledWith(130);
    expect(app.ui.uiScale).toBe(130);
  });

  it("uiScaleが未設定(旧データ)なら100で適用される", async () => {
    mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, uiScale: undefined });
    expect(applyUiScaleMock).toHaveBeenCalledWith(100);
    expect(app.ui.uiScale).toBe(100);
  });

  it("範囲外のuiScaleは正規化されてapp.uiに保持され、正規化後の値が適用される", async () => {
    mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, uiScale: 1000 });
    expect(app.ui.uiScale).toBe(200);
    expect(applyUiScaleMock).toHaveBeenLastCalledWith(200);
  });

  it("同じ倍率の保存ではapplyUiScaleを繰り返し呼ばない", async () => {
    mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, uiScale: 170 });
    await app.setUiPrefs({ ...app.ui, uiScale: 170 });
    expect(applyUiScaleMock.mock.calls.filter(([v]) => v === 170)).toHaveLength(1);
    await app.setUiPrefs({ ...app.ui, uiScale: 180 });
    expect(applyUiScaleMock).toHaveBeenLastCalledWith(180);
  });

  it("適用に失敗した後、同じ倍率の保存では再試行される", async () => {
    mockPrefersColorSchemeDark(false);
    const warnCount = () => app.logs.filter((l) => l.level === "warn" && l.text.includes("UIスケール")).length;
    const before = warnCount();
    applyUiScaleMock.mockRejectedValueOnce(new Error("unsupported"));
    await app.setUiPrefs({ ...app.ui, uiScale: 190 });
    await vi.waitFor(() => expect(warnCount()).toBe(before + 1));
    await app.setUiPrefs({ ...app.ui, uiScale: 190 });
    expect(applyUiScaleMock.mock.calls.filter(([v]) => v === 190)).toHaveLength(2);
  });

  it("applyUiScaleが失敗しても設定の保存は成功し、警告ログが残る", async () => {
    mockPrefersColorSchemeDark(false);
    applyUiScaleMock.mockRejectedValue(new Error("unsupported"));
    await expect(app.setUiPrefs({ ...app.ui, uiScale: 150 })).resolves.toBeUndefined();
    await vi.waitFor(() => {
      expect(app.logs.some((l) => l.level === "warn" && l.text.includes("UIスケール"))).toBe(true);
    });
  });
});

describe("矢印キー選択移動とselectionMoveSeq(Issue #363)", () => {
  beforeEach(() => {
    app.groups = [];
    app.focusedGroupId = "";
  });

  it("runKeyAction(\"note.next\")はselectedNoteIdを進め、selectionMoveSeqをインクリメントする", () => {
    const notes = [makeNote({ id: "n1" }), makeNote({ id: "n2" })];
    app.groups = [makeGroup([makeNoteTab(notes, { selectedNoteId: "n1" })])];
    app.focusedGroupId = "group1";
    const tab = app.groups[0].tabs[0];

    expect(tab.selectionMoveSeq).toBe(0);
    app.runKeyAction("note.next");

    expect(tab.selectedNoteId).toBe("n2");
    expect(tab.selectionMoveSeq).toBe(1);
  });

  it("runKeyAction(\"note.prev\")もselectionMoveSeqをインクリメントする", () => {
    const notes = [makeNote({ id: "n1" }), makeNote({ id: "n2" })];
    app.groups = [makeGroup([makeNoteTab(notes, { selectedNoteId: "n2" })])];
    app.focusedGroupId = "group1";
    const tab = app.groups[0].tabs[0];

    app.runKeyAction("note.prev");

    expect(tab.selectedNoteId).toBe("n1");
    expect(tab.selectionMoveSeq).toBe(1);
  });

  it("selectNote(クリック/タップ選択)ではselectionMoveSeqが変化しない", () => {
    const notes = [makeNote({ id: "n1" }), makeNote({ id: "n2" })];
    app.groups = [makeGroup([makeNoteTab(notes)])];
    app.focusedGroupId = "group1";
    const tab = app.groups[0].tabs[0];

    app.selectNote("tab1", "n2");

    expect(tab.selectedNoteId).toBe("n2");
    expect(tab.selectionMoveSeq).toBe(0);
  });

  it("未選択状態でキーバインド操作(暗黙選択)するとselectionMoveSeqがインクリメントされる", () => {
    const notes = [makeNote({ id: "n1" }), makeNote({ id: "n2" })];
    const tab = makeNoteTab(notes); // selectedNoteId未指定=null
    app.groups = [makeGroup([tab])];
    app.focusedGroupId = "group1";

    app.runKeyAction("note.react");

    const t = app.groups[0].tabs[0];
    expect(t.selectedNoteId).toBe("n1");
    expect(t.selectionMoveSeq).toBe(1);
  });
});

describe("自分のアカウントによるreacted/unreactedイベントの反映(Issue #28)", () => {
  it("他クライアントで付けたリアクションのreactedでmyReactionと件数が反映される", () => {
    const note = makeNote({ reactions: { "❤️": 2 }, reactionCount: 2, myReaction: null });

    applyOwnReactionEvent(note, { type: "reacted", reaction: "👍" });

    expect(note.myReaction).toBe("👍");
    expect(note.reactions).toEqual({ "❤️": 2, "👍": 1 });
    expect(note.reactionCount).toBe(3);
  });

  it("tsumugi自身の操作で反映済み(myReactionが同じ)のreactedは二重加算しない", () => {
    const note = makeNote({ reactions: { "👍": 1 }, reactionCount: 1, myReaction: "👍" });

    applyOwnReactionEvent(note, { type: "reacted", reaction: "👍" });

    expect(note.myReaction).toBe("👍");
    expect(note.reactions).toEqual({ "👍": 1 });
    expect(note.reactionCount).toBe(1);
  });

  it("他クライアントで別の絵文字へ付け替えたreactedは旧リアクションを外して付け直す", () => {
    const note = makeNote({ reactions: { "👍": 1, "❤️": 1 }, reactionCount: 2, myReaction: "👍" });

    applyOwnReactionEvent(note, { type: "reacted", reaction: "😀" });

    expect(note.myReaction).toBe("😀");
    expect(note.reactions).toEqual({ "❤️": 1, "😀": 1 });
    expect(note.reactionCount).toBe(2);
  });

  it("他クライアントで外したリアクションのunreactedでmyReactionと件数が反映される", () => {
    const note = makeNote({ reactions: { "👍": 2 }, reactionCount: 2, myReaction: "👍" });

    applyOwnReactionEvent(note, { type: "unreacted", reaction: "👍" });

    expect(note.myReaction).toBeNull();
    expect(note.reactions).toEqual({ "👍": 1 });
    expect(note.reactionCount).toBe(1);
  });

  it("tsumugi自身の取り消しで反映済み(myReactionがnull)のunreactedは二重減算しない", () => {
    const note = makeNote({ reactions: { "👍": 1 }, reactionCount: 1, myReaction: null });

    applyOwnReactionEvent(note, { type: "unreacted", reaction: "👍" });

    expect(note.myReaction).toBeNull();
    expect(note.reactions).toEqual({ "👍": 1 });
    expect(note.reactionCount).toBe(1);
  });

  it("付け替え後に遅れて届く旧リアクションのunreactedは現在のリアクションに影響しない", () => {
    const note = makeNote({ reactions: { "😀": 1 }, reactionCount: 1, myReaction: "😀" });

    applyOwnReactionEvent(note, { type: "unreacted", reaction: "👍" });

    expect(note.myReaction).toBe("😀");
    expect(note.reactions).toEqual({ "😀": 1 });
    expect(note.reactionCount).toBe(1);
  });
});

describe("タブ編集で名前だけ変えた場合はノートを保持する(Issue #59)", () => {
  it("kind/filterが同一ならrename_columnだけ呼びupdate_columnは呼ばない", async () => {
    const note = makeNote({ id: "keep-me" });
    app.groups = [makeGroup([makeNormalTab({ notes: [note], selectedNoteId: "keep-me" })])];

    await app.updateColumn("tab1", { type: "home" }, { kind: "keywords", value: [] }, "新しい名前");

    const commandsCalled = invokeMock.mock.calls.map((c) => c[0]);
    expect(commandsCalled).toEqual(["rename_column"]);
    const tab = app.groups[0].tabs[0];
    expect(tab.customTitle).toBe("新しい名前");
    expect(tab.notes.map((n) => n.id)).toEqual(["keep-me"]);
    expect(tab.selectedNoteId).toBe("keep-me");
    expect(tab.state).toBe("connected");
  });

  it("filterのキー順が違っても同一とみなす", async () => {
    app.groups = [
      makeGroup([
        makeNormalTab({
          kind: { type: "tql" },
          filter: { kind: "tql", value: "from home" },
        }),
      ]),
    ];

    await app.updateColumn(
      "tab1",
      { type: "tql" },
      { value: "from home", kind: "tql" } as never,
      "名前",
    );

    expect(invokeMock.mock.calls.map((c) => c[0])).toEqual(["rename_column"]);
  });

  it("名前を空にすると自動生成名に戻す", async () => {
    app.groups = [makeGroup([makeNormalTab({ customTitle: "旧名" })])];

    await app.updateColumn("tab1", { type: "home" }, { kind: "keywords", value: [] }, "  ");

    expect(invokeMock).toHaveBeenCalledWith("rename_column", { columnId: "tab1", title: null });
    expect(app.groups[0].tabs[0].customTitle).toBeNull();
  });

  it("kindが変わったら従来どおりupdate_columnで再取得する", async () => {
    const opened = {
      column: {
        id: "tab1",
        accountId: ACCOUNT_ID,
        kind: { type: "local" },
        filter: { kind: "keywords", value: [] },
        title: null,
      },
      group: { id: "group1" },
      notes: [],
      notifications: [],
    };
    invokeMock.mockResolvedValueOnce(opened);
    app.groups = [makeGroup([makeNormalTab({ notes: [makeNote({ id: "old" })] })])];

    await app.updateColumn("tab1", { type: "local" }, { kind: "keywords", value: [] }, undefined);

    expect(invokeMock.mock.calls[0][0]).toBe("update_column");
    expect(app.groups[0].tabs[0].notes).toEqual([]);
  });

  it("filterが変わったら従来どおりupdate_columnで再取得する", async () => {
    const opened = {
      column: {
        id: "tab1",
        accountId: ACCOUNT_ID,
        kind: { type: "home" },
        filter: { kind: "keywords", value: ["foo"] },
        title: null,
      },
      group: { id: "group1" },
      notes: [],
      notifications: [],
    };
    invokeMock.mockResolvedValueOnce(opened);
    app.groups = [makeGroup([makeNormalTab({ notes: [makeNote({ id: "old" })] })])];

    await app.updateColumn("tab1", { type: "home" }, { kind: "keywords", value: ["foo"] }, undefined);

    expect(invokeMock.mock.calls[0][0]).toBe("update_column");
  });
});
