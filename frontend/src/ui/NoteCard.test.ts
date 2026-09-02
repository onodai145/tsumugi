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
  const originalClipboardDescriptor = Object.getOwnPropertyDescriptor(navigator, "clipboard");
  afterEach(async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.length = 0;
    deleteSpy?.mockRestore();
    deleteSpy = null;
    if (originalClipboardDescriptor) {
      Object.defineProperty(navigator, "clipboard", originalClipboardDescriptor);
    } else {
      // @ts-expect-error テスト用スタブで追加したプロパティを取り除く(元は存在しなかった)
      delete navigator.clipboard;
    }
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

  it("本文がある投稿では「内容をコピー」項目を表示し、クリックでクリップボードにコピーしてメニューを閉じる", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    const note = makeNote({ text: "**bold** です" });
    const { getByLabelText, getByText, queryByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();
    await getByText("内容をコピー").click();

    expect(writeText).toHaveBeenCalledWith("**bold** です");
    expect(queryByText("内容をコピー")).toBeNull();
  });

  it("本文がnullのノートでは「内容をコピー」項目を表示しない", async () => {
    const note = makeNote({ text: null });
    const { getByLabelText, queryByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();

    expect(queryByText("内容をコピー")).toBeNull();
  });

  it("本文が空文字のノートでは「内容をコピー」項目を表示しない", async () => {
    const note = makeNote({ text: "" });
    const { getByLabelText, queryByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();

    expect(queryByText("内容をコピー")).toBeNull();
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

  // Instance Ticker(Issue #103)実装前にキャッシュされたノートはpayloadにuser.instanceが
  // 無いため(search機能はキャッシュのみを読み、再取得による最新化が起きない)、リモート
  // ユーザーと分かっていてもtickerが一切出ない問題があった。host名だけの簡素な表示に
  // フォールバックする。
  it("falls back to a plain host-only ticker for a remote author with no instance info", async () => {
    const { getByText } = render(NoteCard, {
      note: makeNote({ user: makeUser({ host: "old.example", instance: undefined }) }),
    });
    expect(getByText("old.example")).toBeTruthy();
    expect(document.querySelector("[data-testid='note-instance-ticker']")).toBeTruthy();
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
    // 不正な値そのものはstyle属性に一切反映されないが、見た目はthemeColor不在時と
    // 同じグラデーション（CSS変数var(--color-muted)使用）+ text-muted-foregroundになる。
    const style = ticker!.getAttribute("style") ?? "";
    expect(style).not.toContain("red;position:fixed");
    expect(style).toContain("var(--color-muted)");
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

describe("投稿時刻の自動更新", () => {
  it("app.nowが進むと相対時刻の表示が更新される", async () => {
    vi.useFakeTimers();
    try {
      const nowSec = Math.floor(Date.now() / 1000);
      const note = makeNote({ createdAt: nowSec - 30 }); // 30秒前
      const { getByTitle } = render(NoteCard, { props: { note } });

      const timeEl = getByTitle(new Date(note.createdAt * 1000).toLocaleString());
      expect(timeEl.textContent?.trim()).toBe("30s");

      // 90秒進める（分単位表示に切り替わるはず）
      vi.setSystemTime(Date.now() + 90_000);
      app.now = Date.now();
      await vi.advanceTimersByTimeAsync(0);

      expect(timeEl.textContent?.trim()).toBe("2m");
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("本文の折りたたみ", () => {
  it("301文字では初期状態で「もっと見る」ボタンを表示し、本文コンテナに折りたたみクラスが付く", () => {
    const note = makeNote({ text: "あ".repeat(301) });
    const { container, getByTestId } = render(NoteCard, { props: { note } });

    expect(getByTestId("note-text-expand")).toBeTruthy();
    const textEl = getByTestId("note-text");
    expect(textEl.classList.contains("note-text-collapsed")).toBe(true);
    void container;
  });

  it("300文字ちょうどでは「もっと見る」ボタンを表示しない", () => {
    const note = makeNote({ text: "あ".repeat(300) });
    const { queryByTestId, getByTestId } = render(NoteCard, { props: { note } });

    expect(queryByTestId("note-text-expand")).toBeNull();
    expect(getByTestId("note-text").classList.contains("note-text-collapsed")).toBe(false);
  });

  it("サロゲートペア(絵文字)151文字は.lengthでは302だが実文字数は300文字未満のため折りたたまない", () => {
    const note = makeNote({ text: "🐱".repeat(151) });
    const { queryByTestId, getByTestId } = render(NoteCard, { props: { note } });

    expect(queryByTestId("note-text-expand")).toBeNull();
    expect(getByTestId("note-text").classList.contains("note-text-collapsed")).toBe(false);
  });

  it("「もっと見る」をクリックすると全文表示になりボタンが消える", async () => {
    const note = makeNote({ text: "あ".repeat(301) });
    const { getByTestId, queryByTestId } = render(NoteCard, { props: { note } });

    await getByTestId("note-text-expand").click();

    expect(queryByTestId("note-text-expand")).toBeNull();
    expect(getByTestId("note-text").classList.contains("note-text-collapsed")).toBe(false);
  });

  it("CWを開いた結果の本文が長文の場合も折りたたみが効く", async () => {
    const note = makeNote({ cw: "注意書き", text: "あ".repeat(301) });
    const { getByText, getByTestId, queryByTestId } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    expect(queryByTestId("note-text-expand")).toBeNull(); // CWが閉じている間は本文自体が無い

    await getByText("続きを見る").click();

    expect(getByTestId("note-text-expand")).toBeTruthy();
  });

  it("引用Renoteのネスト表示でも長文の折りたたみが効く", () => {
    const note = makeNote({
      text: "見て",
      renote: makeNote({ id: "n-quoted", text: "い".repeat(301) }),
    });
    const { container } = render(NoteCard, { props: { note } });

    expect(container.querySelectorAll('[data-testid="note-text-expand"]').length).toBe(1);
  });
});
