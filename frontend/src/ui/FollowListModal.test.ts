import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
const invokeMock = vi.fn().mockResolvedValue(null);
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: FollowListModal } = await import("./FollowListModal.svelte");

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
});

function makeUser(id: string, username: string) {
  return {
    id,
    username,
    host: null,
    name: username,
    avatarUrl: null,
    isBot: false,
    isCat: false,
    followersCount: 0,
    followingCount: 0,
    notesCount: 0,
    emojis: {},
    bio: null,
    bannerUrl: null,
  };
}

// users/followers・users/following はFollowingレコードのidでページングするため、
// レスポンスは {user, cursor} のペア。cursor はユーザーIDとは別のFollowingレコードID。
function makeEntry(userId: string, username: string, cursor: string) {
  return { user: makeUser(userId, username), cursor };
}

// invokeMockは生成コードのtypedError()に渡される前のraw invoke()相当。
// typedError側が{status:"ok",data:...}に包むため、ここでは生の戻り値のみを返す
// ({status:"ok",data:...}でラップして返すと二重ラップになりコンポーネントが壊れた値を受け取る)。
describe("FollowListModal", () => {
  it("kind=followersでget_user_followersを呼び一覧表示する", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_user_followers") return Promise.resolve([makeEntry("u2", "bob", "f2")]);
      return Promise.resolve(null);
    });
    const { getByText } = render(FollowListModal, {
      props: { kind: "followers", userId: "u1", accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(getByText("bob")).toBeTruthy());
    expect(invokeMock).toHaveBeenCalledWith(
      "get_user_followers",
      expect.objectContaining({ accountId: "acc1", userId: "u1" }),
    );
  });

  it("kind=followingでget_user_followingを呼ぶ", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_user_following") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    render(FollowListModal, {
      props: { kind: "following", userId: "u1", accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "get_user_following",
        expect.objectContaining({ accountId: "acc1", userId: "u1" }),
      ),
    );
  });

  // ProfileModal の同一インスタンスが openProfile() 経由の target 切り替えで再利用されうるため、
  // FollowListModal も props(userId 等)の変化だけで再マウントされずに再ロードされる必要がある
  // (kind/userId が変わらない限りloadMore()を再度発火させないと、古いユーザーの一覧が残り続ける)。
  it("userId propが変わると新しいuserIdで再取得し、古い一覧を破棄する", async () => {
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_user_followers" && args?.userId === "u1") {
        return Promise.resolve([makeEntry("u2", "bob", "f2")]);
      }
      if (cmd === "get_user_followers" && args?.userId === "u3") {
        return Promise.resolve([makeEntry("u4", "carol", "f4")]);
      }
      return Promise.resolve(null);
    });
    const { rerender, getByText, queryByText } = render(FollowListModal, {
      props: { kind: "followers", userId: "u1", accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(getByText("bob")).toBeTruthy());
    invokeMock.mockClear();
    await rerender({ kind: "followers", userId: "u3", accountId: "acc1", onclose: () => {} });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "get_user_followers",
        expect.objectContaining({ accountId: "acc1", userId: "u3" }),
      ),
    );
    await waitFor(() => expect(getByText("carol")).toBeTruthy());
    expect(queryByText("bob")).toBeNull();
  });

  // 修正1の回帰テスト: users/followers・users/following はFollowingレコードのidで
  // ページングする仕様のため、2ページ目のuntilIdにはユーザーID(u2)ではなく
  // 1件目のcursor(f2)を送らなければならない。
  it("もっと見るクリック時、untilIdにユーザーIDではなくFollowingレコードのcursorを送る", async () => {
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_user_followers" && args?.untilId == null) {
        return Promise.resolve([makeEntry("u2", "bob", "f2")]);
      }
      if (cmd === "get_user_followers" && args?.untilId === "f2") {
        return Promise.resolve([makeEntry("u5", "dave", "f5")]);
      }
      return Promise.resolve([]);
    });
    const { getByText } = render(FollowListModal, {
      props: { kind: "followers", userId: "u1", accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(getByText("bob")).toBeTruthy());
    await fireEvent.click(getByText("もっと見る"));
    await waitFor(() => expect(getByText("dave")).toBeTruthy());
    expect(invokeMock).toHaveBeenCalledWith(
      "get_user_followers",
      expect.objectContaining({ accountId: "acc1", userId: "u1", untilId: "f2" }),
    );
  });
});
