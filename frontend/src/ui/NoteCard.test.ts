import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import type { Note, User } from "../bindings/tauri.gen";
import { app } from "../lib/store.svelte";

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

afterEach(() => {
  cleanup();
  app.ui = { ...app.ui, instanceTicker: "remote" };
});

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
    expect(container.querySelector('[data-testid="note-reaction-wrap"]')).toBeNull();
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
  it("アバタークリックでopenProfileが呼ばれる（プレースホルダー: avatarUrl未設定）", () => {
    const note = makeNote();
    const { container } = render(NoteCard, { props: { note, accountId: "acc1" } });
    const avatar = container.querySelector('[data-testid="note-avatar"]') as HTMLElement;
    avatar.click();
    expect(openProfile).toHaveBeenCalledWith({ userId: note.user.id }, "acc1");
  });

  it("アバタークリックでopenProfileが呼ばれる（imgタグ: avatarUrl設定あり）", () => {
    const note = makeNote({ user: makeUser({ avatarUrl: "https://example.com/a.png" }) });
    const { container } = render(NoteCard, { props: { note, accountId: "acc1" } });
    const avatar = container.querySelector('img[data-testid="note-avatar"]') as HTMLElement;
    expect(avatar).toBeTruthy();
    avatar.click();
    expect(openProfile).toHaveBeenCalledWith({ userId: note.user.id }, "acc1");
  });

  it("表示名クリックでopenProfileが呼ばれる", () => {
    const note = makeNote();
    const { container } = render(NoteCard, { props: { note, accountId: "acc1" } });
    const name = container.querySelector('[data-testid="note-name"]') as HTMLElement;
    name.click();
    expect(openProfile).toHaveBeenCalledWith({ userId: note.user.id }, "acc1");
  });

  it("acctクリックでopenProfileが呼ばれる", () => {
    const note = makeNote();
    const { container } = render(NoteCard, { props: { note, accountId: "acc1" } });
    const acctEl = container.querySelector('[data-testid="note-acct"]') as HTMLElement;
    acctEl.click();
    expect(openProfile).toHaveBeenCalledWith({ userId: note.user.id }, "acc1");
  });
});

describe("投稿削除メニュー", () => {
  // アサーション失敗時にも確実に実行されるよう、account/spy の後始末は
  // 各 it() 末尾ではなく afterEach に置く(app はファイル全体で共有される
  // モジュール単位シングルトンのため、失敗による後始末漏れが後続テストに漏れる)。
  // vi.restoreAllMocks() は使わない — このファイルの他ブロックが
  // vi.mock(...) のモジュールファクトリで持つ実装(isPermissionGranted等)まで
  // まとめて剥がしてしまうため、このブロックで張った spy だけを個別に restore する。
  let deleteSpy: ReturnType<typeof vi.spyOn> | null = null;
  afterEach(async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.length = 0;
    deleteSpy?.mockRestore();
    deleteSpy = null;
  });

  it("自分の投稿では削除項目を表示する", async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.push({
      id: "acc1",
      host: "misskey.example",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
    });
    const note = makeNote({ user: makeUser({ id: "u1" }) });
    const { baseElement, getByLabelText, getByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();

    expect(getByText("削除")).toBeTruthy();
    expect(baseElement.querySelector("svg.lucide-trash-2")).toBeTruthy();
  });

  it("他人の投稿では削除項目を表示しない", async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.push({
      id: "acc1",
      host: "misskey.example",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
    });
    const note = makeNote({ user: makeUser({ id: "other-user" }) });
    const { getByLabelText, queryByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();

    expect(queryByText("削除")).toBeNull();
  });

  it("削除ボタン→確認ダイアログで確定するとdeleteNoteが呼ばれる", async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.push({
      id: "acc1",
      host: "misskey.example",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
    });
    deleteSpy = vi.spyOn(app, "deleteNote").mockResolvedValue(undefined);
    const note = makeNote({ id: "n-delete-1", user: makeUser({ id: "u1" }) });
    const { baseElement, getByLabelText, getByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();
    await getByText("削除").click();

    // ConfirmDialogがNoteCardのメニュー用backdrop(z-[1010])の下に隠れないよう、
    // NoteMenuからz=1020が渡っていることをstyle経由で確認する回帰ガード
    // (jsdomはヒットテストできないため、実クリックの可否そのものは検証できない)。
    // NoteCard自身のbackdropはTailwindのz-[1010]クラスでインラインstyleを持たないため、
    // style.zIndexが設定されている要素 = ConfirmDialogのoverlayとして特定する。
    const overlays = Array.from(baseElement.querySelectorAll('[role="presentation"]')) as HTMLElement[];
    const confirmOverlay = overlays.find((el) => el.style.zIndex !== "");
    expect(confirmOverlay?.style.zIndex).toBe("1020");

    await getByText("削除する").click();

    expect(deleteSpy).toHaveBeenCalledWith("acc1", "n-delete-1");
  });

  it("削除ボタン→確認ダイアログをキャンセルするとdeleteNoteが呼ばれない", async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.push({
      id: "acc1",
      host: "misskey.example",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
    });
    const deleteSpy = vi.spyOn(app, "deleteNote").mockResolvedValue(undefined);
    const note = makeNote({ id: "n-delete-2", user: makeUser({ id: "u1" }) });
    const { getByLabelText, getByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();
    await getByText("削除").click();
    await getByText("キャンセル").click();

    expect(deleteSpy).not.toHaveBeenCalled();
  });
});

describe("instance ticker", () => {
  function remoteUser(): User {
    return makeUser({
      host: "remote.example",
      instance: {
        name: "Remote Instance",
        iconUrl: "https://remote.example/icon.png",
        themeColor: "#ff8800",
      },
    });
  }

  it("shows the ticker for a remote author by default (mode=remote)", async () => {
    const { getByText } = render(NoteCard, {
      note: makeNote({ user: remoteUser() }),
    });
    expect(getByText("Remote Instance")).toBeTruthy();
  });

  it("hides the ticker entirely when mode=off", async () => {
    app.ui = { ...app.ui, instanceTicker: "off" };
    const { queryByText } = render(NoteCard, {
      note: makeNote({ user: remoteUser() }),
    });
    expect(queryByText("Remote Instance")).toBeNull();
  });

  it("does not show a ticker for a local author when mode=remote", async () => {
    const { queryByText } = render(NoteCard, {
      note: makeNote({ user: makeUser({ host: null }) }),
    });
    expect(queryByText("Alice")).toBeTruthy(); // 投稿者名は出る
    expect(document.querySelector("[data-testid='note-instance-ticker']")).toBeNull();
  });

  it("ignores a malicious themeColor and falls back to the plain style (no CSS injection)", async () => {
    const maliciousUser = makeUser({
      host: "evil.example",
      instance: {
        name: "Evil Instance",
        iconUrl: null,
        themeColor: "red;position:fixed;top:0;left:0;width:100vw;height:100vh;z-index:99999",
      },
    });
    render(NoteCard, { note: makeNote({ user: maliciousUser }) });

    const ticker = document.querySelector("[data-testid='note-instance-ticker']");
    expect(ticker).toBeTruthy();
    // 不正なthemeColorはstyle属性に一切反映されず、themeColor不在時と同じ
    // フォールバック（style未設定 + bg-muted/text-muted-foreground）になる。
    expect(ticker!.getAttribute("style")).toBeFalsy();
    expect(ticker!.classList.contains("bg-muted")).toBe(true);
    expect(ticker!.classList.contains("text-muted-foreground")).toBe(true);
  });

  it("shows the viewing account's instance for a local author when mode=always", async () => {
    app.ui = { ...app.ui, instanceTicker: "always" };
    const accountsBackup = app.accounts.slice();
    app.accounts.push({
      id: "acc-always-local",
      host: "local.example",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
      instance: { name: "Local Instance", iconUrl: null, themeColor: "#112233" },
    });

    try {
      const { getByText } = render(NoteCard, {
        note: makeNote({ user: makeUser({ host: null }) }),
        accountId: "acc-always-local",
        emojiAccountId: "acc-always-local",
      });
      expect(getByText("Local Instance")).toBeTruthy();
    } finally {
      app.accounts.length = 0;
      app.accounts.push(...accountsBackup);
    }
  });
});
