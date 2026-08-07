import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import type { Note, User } from "../bindings/tauri.gen";

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

const { default: ProfileModal } = await import("./ProfileModal.svelte");

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
});

// ブリーフ原文は `[{ id: "n1" } as Note]` という不完全なオブジェクトをキャストのみで
// Note型に見せかけていたが、NoteCard は実行時に note.user.avatarUrl 等へアクセスするため
// レンダリング時にクラッシュする。NoteCard.test.ts の makeNote/makeUser 相当の完全な
// オブジェクトを最小限用意する。
function makeNote(overrides: Partial<Note> = {}): Note {
  return {
    id: "n1",
    createdAt: 0,
    text: "hello",
    cw: null,
    visibility: "public",
    localOnly: false,
    user: {
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
    } as User,
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

function profileResponse(overrides: Record<string, unknown> = {}) {
  return {
    user: {
      id: "u1",
      username: "alice",
      host: null,
      name: "Alice",
      avatarUrl: null,
      isBot: false,
      isCat: false,
      followersCount: 3,
      followingCount: 5,
      notesCount: 10,
      emojis: {},
      bio: "hello",
      bannerUrl: null,
    },
    isFollowing: false,
    isSelf: false,
    ...overrides,
  };
}

// invokeMockは生成コードのtypedError()に渡される前のraw invoke()相当。
// typedError側が{status:"ok",data:...}に包むため、ここでは生の戻り値(コマンドの実際の返り値そのもの)を返す。
// {status:"ok",data:...}でラップして返すと、typedErrorがそれをさらに包んでしまい
// (unwrapAccが1段階しか剥がせず)コンポーネントが受け取る値が壊れるので絶対にやらないこと。
describe("ProfileModal", () => {
  it("マウント時にget_user_profileを呼び、プロフィールを表示する", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_user_profile") return Promise.resolve(profileResponse());
      if (cmd === "get_user_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const { getByText } = render(ProfileModal, {
      props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(getByText("Alice")).toBeTruthy());
    expect(getByText("hello")).toBeTruthy();
  });

  it("自分自身の場合フォローボタンを表示しない", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_user_profile")
        return Promise.resolve(profileResponse({ isSelf: true, isFollowing: null }));
      if (cmd === "get_user_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const { queryByRole } = render(ProfileModal, {
      props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_user_profile", expect.anything()));
    expect(queryByRole("button", { name: /フォロー/ })).toBeNull();
  });

  it("フォローボタンクリックでfollow_userを呼ぶ", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_user_profile") return Promise.resolve(profileResponse());
      if (cmd === "get_user_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const { getByRole } = render(ProfileModal, {
      props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
    });
    const btn = await waitFor(() => getByRole("button", { name: "フォロー" }));
    btn.click();
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "follow_user",
        expect.objectContaining({ accountId: "acc1", userId: "u1" }),
      ),
    );
  });

  it("target propが変わったら前のユーザーのノート一覧を引き継がない", async () => {
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_user_profile") {
        const userId = args?.userId;
        return Promise.resolve(
          profileResponse({ user: { ...profileResponse().user, id: userId, username: userId } }),
        );
      }
      if (cmd === "get_user_notes") return Promise.resolve([makeNote()]);
      return Promise.resolve(null);
    });
    const { rerender, getByText } = render(ProfileModal, {
      props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
    });
    // acct() は常に "@" を前置するため("u1" 単体のテキストノードは存在しない)、実際に
    // 描画されるテキストで初回ロード完了を待つ。
    await waitFor(() => expect(getByText("@u1")).toBeTruthy());
    invokeMock.mockClear();
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_user_profile") {
        const userId = args?.userId;
        return Promise.resolve(
          profileResponse({ user: { ...profileResponse().user, id: userId, username: userId } }),
        );
      }
      if (cmd === "get_user_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    await rerender({ target: { userId: "u2" }, accountId: "acc1", onclose: () => {} });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "get_user_notes",
        expect.objectContaining({ userId: "u2", untilId: null }),
      ),
    );
  });

  // 修正2の回帰テスト: ノート取得失敗直後はスクロール閾値内にいるため、notesErrを見ずに
  // スクロールイベントでloadMoreNotesを再度呼ぶと再試行が連続発火してしまう。
  // 再試行はエラー表示下の再試行ボタン経由のみに限定する。
  it("ノート取得エラー中はスクロールイベントで再取得しない", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_user_profile") return Promise.resolve(profileResponse());
      if (cmd === "get_user_notes") return Promise.reject(new Error("network error"));
      return Promise.resolve(null);
    });
    const { getByText } = render(ProfileModal, {
      props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(getByText("Error: network error")).toBeTruthy());
    invokeMock.mockClear();
    const notesEl = document.querySelector(".notes") as HTMLElement;
    Object.defineProperty(notesEl, "scrollTop", { value: 500, configurable: true });
    Object.defineProperty(notesEl, "clientHeight", { value: 400, configurable: true });
    Object.defineProperty(notesEl, "scrollHeight", { value: 1200, configurable: true });
    await fireEvent.scroll(notesEl);
    await fireEvent.scroll(notesEl);
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
