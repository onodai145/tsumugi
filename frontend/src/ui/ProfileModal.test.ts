import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import type { Note, User } from "../bindings/tauri.gen";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
const openUrlMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: openUrlMock }));
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
const { app } = await import("../lib/store.svelte");

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
  openUrlMock.mockClear();
  app.ui = { ...app.ui, uiMode: "auto" };
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
    replyUserId: null,
    reply: null,
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
      avatarBlurhash: null,
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
    isMuted: false,
    isBlocking: false,
    isRenoteMuted: false,
    profileUrl: null,
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
    const notesEl = document.querySelector('[data-testid="profile-notes-scroll"]') as HTMLElement;
    Object.defineProperty(notesEl, "scrollTop", { value: 500, configurable: true });
    Object.defineProperty(notesEl, "clientHeight", { value: 400, configurable: true });
    Object.defineProperty(notesEl, "scrollHeight", { value: 1200, configurable: true });
    await fireEvent.scroll(notesEl);
    await fireEvent.scroll(notesEl);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("プロフィールのuser.isCatがtrueのとき猫耳(.ears)を描画する", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_user_profile") return Promise.resolve(profileResponse({ user: { ...profileResponse().user, isCat: true } }));
      if (cmd === "get_user_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    render(ProfileModal, {
      props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(document.querySelector(".ears")).not.toBeNull());
  });

  describe("ミュート(Issue #545)", () => {
    function mockProfile(overrides: Record<string, unknown> = {}, onCommand?: (cmd: string) => unknown) {
      invokeMock.mockImplementation((cmd: string) => {
        const custom = onCommand?.(cmd);
        if (custom !== undefined) return custom;
        if (cmd === "get_user_profile") return Promise.resolve(profileResponse(overrides));
        if (cmd === "get_user_notes") return Promise.resolve([]);
        return Promise.resolve(null);
      });
    }
    const renderModal = () =>
      render(ProfileModal, { props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} } });
    const menuButton = (r: ReturnType<typeof renderModal>) =>
      r.findByRole("button", { name: "ユーザーメニュー" });

    it("自分自身のプロフィールでは「…」メニューを出さない", async () => {
      mockProfile({ isSelf: true, isFollowing: null, isMuted: null });
      const r = renderModal();
      await waitFor(() => expect(r.getByText("Alice")).toBeTruthy());

      expect(r.queryByRole("button", { name: "ユーザーメニュー" })).toBeNull();
    });

    it("isMutedを取得できないとき(null)でも「…」メニューを出し、ミュート以外の項目は使える", async () => {
      mockProfile({ isMuted: null });
      const r = renderModal();
      await fireEvent.click(await menuButton(r));

      expect(await r.findByRole("button", { name: "ブロック" })).toBeTruthy();
      expect(r.getByRole("button", { name: "リストに追加" })).toBeTruthy();
      expect(r.getByRole("button", { name: "通報" })).toBeTruthy();
      expect(r.queryByRole("button", { name: "ミュート" })).toBeNull();
      expect(r.queryByRole("button", { name: "ミュート解除" })).toBeNull();
    });

    it("ブロック中は「フォロー」ボタンを押せない", async () => {
      mockProfile({ isBlocking: true, isFollowing: false });
      const r = renderModal();
      await waitFor(() => expect(r.getByText("ブロック中")).toBeTruthy());

      const follow = r.getByRole("button", { name: "フォロー" }) as HTMLButtonElement;
      expect(follow.disabled).toBe(true);
    });

    it("ブロック中でなければ「フォロー」ボタンを押せる", async () => {
      mockProfile({ isBlocking: false, isFollowing: false });
      const r = renderModal();
      await waitFor(() => expect(r.getByText("Alice")).toBeTruthy());

      const follow = r.getByRole("button", { name: "フォロー" }) as HTMLButtonElement;
      expect(follow.disabled).toBe(false);
    });

    it("期間を選ぶとmute_userを呼び、「ミュート中」バッジが出る", async () => {
      mockProfile();
      const r = renderModal();
      await fireEvent.click(await menuButton(r));
      await fireEvent.click(r.getByRole("button", { name: "ミュート" }));

      await fireEvent.click(r.getByRole("button", { name: "1時間" }));

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith(
          "mute_user",
          expect.objectContaining({ accountId: "acc1", userId: "u1", period: "oneHour" }),
        ),
      );
      await waitFor(() => expect(r.getByText("ミュート中")).toBeTruthy());
      expect(r.queryByTestId("user-menu")).toBeNull(); // メニューは閉じる
    });

    it("無期限を選ぶとperiodにindefinitelyを渡す", async () => {
      mockProfile();
      const r = renderModal();
      await fireEvent.click(await menuButton(r));
      await fireEvent.click(r.getByRole("button", { name: "ミュート" }));

      await fireEvent.click(r.getByRole("button", { name: "無期限" }));

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith(
          "mute_user",
          expect.objectContaining({ period: "indefinitely" }),
        ),
      );
    });

    it("ミュート中のプロフィールでは、バッジが出て、「ミュート解除」でunmute_userを呼びバッジが消える", async () => {
      mockProfile({ isMuted: true });
      const r = renderModal();
      await waitFor(() => expect(r.getByText("ミュート中")).toBeTruthy());
      await fireEvent.click(await menuButton(r));

      await fireEvent.click(r.getByRole("button", { name: "ミュート解除" }));

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith(
          "unmute_user",
          expect.objectContaining({ accountId: "acc1", userId: "u1" }),
        ),
      );
      await waitFor(() => expect(r.queryByText("ミュート中")).toBeNull());
    });

    it("mute_userが失敗したら、エラーを表示し、バッジは出さない", async () => {
      mockProfile({}, (cmd) => (cmd === "mute_user" ? Promise.reject(new Error("RATE_LIMIT")) : undefined));
      const r = renderModal();
      await fireEvent.click(await menuButton(r));
      await fireEvent.click(r.getByRole("button", { name: "ミュート" }));

      await fireEvent.click(r.getByRole("button", { name: "1日" }));

      await waitFor(() => expect(r.getByText(/RATE_LIMIT/)).toBeTruthy());
      expect(r.queryByText("ミュート中")).toBeNull();
    });

    it("ALREADY_MUTINGで失敗したら、表示を「ミュート中」に直し、メニューで解除できる", async () => {
      mockProfile({}, (cmd) =>
        cmd === "mute_user"
          ? Promise.reject(new Error("mute/create: ALREADY_MUTING You are already muting that user."))
          : undefined,
      );
      const r = renderModal();
      await fireEvent.click(await menuButton(r));
      await fireEvent.click(r.getByRole("button", { name: "ミュート" }));

      await fireEvent.click(r.getByRole("button", { name: "1日" }));

      await waitFor(() => expect(r.getByText("ミュート中")).toBeTruthy());
      await fireEvent.click(await menuButton(r));
      expect(r.getByRole("button", { name: "ミュート解除" })).toBeTruthy();
    });

    it("NOT_MUTINGで解除に失敗したら、表示を「ミュート中でない」に直す", async () => {
      mockProfile({ isMuted: true }, (cmd) =>
        cmd === "unmute_user" ? Promise.reject(new Error("mute/delete: NOT_MUTING You are not muting that user.")) : undefined,
      );
      const r = renderModal();
      await waitFor(() => expect(r.getByText("ミュート中")).toBeTruthy());
      await fireEvent.click(await menuButton(r));

      await fireEvent.click(r.getByRole("button", { name: "ミュート解除" }));

      await waitFor(() => expect(r.queryByText("ミュート中")).toBeNull());
      await fireEvent.click(await menuButton(r));
      expect(r.getByRole("button", { name: "ミュート" })).toBeTruthy();
    });

    it("メニューを開いているときのEscapeは、メニューだけ閉じ、モーダルは閉じない", async () => {
      mockProfile();
      const onclose = vi.fn();
      const r = render(ProfileModal, { props: { target: { userId: "u1" }, accountId: "acc1", onclose } });
      const trigger = await menuButton(r);
      await fireEvent.click(trigger);

      await fireEvent.keyDown(trigger, { key: "Escape" }); // フォーカスがモーダルの中にあるとき

      expect(r.queryByTestId("user-menu")).toBeNull();
      expect(onclose).not.toHaveBeenCalled();
    });

    it("フォーカスがメニューの中にあるときのEscapeも、メニューだけ閉じる。メニューが閉じた後のEscapeはモーダルを閉じる", async () => {
      mockProfile();
      const onclose = vi.fn();
      const r = render(ProfileModal, { props: { target: { userId: "u1" }, accountId: "acc1", onclose } });
      const trigger = await menuButton(r);
      await fireEvent.click(trigger);

      await fireEvent.keyDown(r.getByRole("button", { name: "ミュート" }), { key: "Escape" });
      expect(r.queryByTestId("user-menu")).toBeNull();
      expect(onclose).not.toHaveBeenCalled();

      await fireEvent.keyDown(trigger, { key: "Escape" });
      expect(onclose).toHaveBeenCalledTimes(1);
    });

    it("mute_userの完了前に別ユーザーへ切り替わったら、新しいプロフィールのバッジを書き換えない", async () => {
      let resolveMute: (v: unknown) => void = () => {};
      const mutePending = new Promise((resolve) => (resolveMute = resolve));
      invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "mute_user") return mutePending;
        if (cmd === "get_user_profile") {
          const userId = args?.userId as string;
          return Promise.resolve(
            profileResponse({ user: { ...profileResponse().user, id: userId, username: userId } }),
          );
        }
        if (cmd === "get_user_notes") return Promise.resolve([]);
        return Promise.resolve(null);
      });
      const r = render(ProfileModal, {
        props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
      });
      await fireEvent.click(await menuButton(r));
      await fireEvent.click(r.getByRole("button", { name: "ミュート" }));
      await fireEvent.click(r.getByRole("button", { name: "1時間" }));
      await r.rerender({ target: { userId: "u2" }, accountId: "acc1", onclose: () => {} });
      await waitFor(() => expect(r.getByText("@u2")).toBeTruthy());

      resolveMute(null);
      // 古い呼び出しの続き(app.muteUser の解決 → ProfileModal の後処理)が流れ切るまで待つ
      await new Promise((resolve) => setTimeout(resolve, 50));

      expect(invokeMock.mock.calls.filter(([cmd]) => cmd === "mute_user")).toHaveLength(1);
      expect(r.queryByText("ミュート中")).toBeNull();
    });
  });

  describe("ブロック・リノートミュート・リスト・通報・ブラウザで開く(Issue #546)", () => {
    function mockProfile(overrides: Record<string, unknown> = {}, onCommand?: (cmd: string) => unknown) {
      invokeMock.mockImplementation((cmd: string) => {
        const custom = onCommand?.(cmd);
        if (custom !== undefined) return custom;
        if (cmd === "get_user_profile") return Promise.resolve(profileResponse(overrides));
        if (cmd === "get_user_notes") return Promise.resolve([]);
        return Promise.resolve(null);
      });
    }
    const renderModal = () =>
      render(ProfileModal, { props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} } });
    const menuButton = (r: ReturnType<typeof renderModal>) =>
      r.findByRole("button", { name: "ユーザーメニュー" });
    const called = (cmd: string) => invokeMock.mock.calls.filter(([c]) => c === cmd);

    describe("ブロック", () => {
      it("「ブロック」を押すと確認が出て、キャンセルならblock_userを呼ばない", async () => {
        mockProfile();
        const r = renderModal();
        await fireEvent.click(await menuButton(r));

        await fireEvent.click(r.getByRole("button", { name: "ブロック" }));
        expect(r.getByRole("button", { name: "ブロックする" })).toBeTruthy(); // 確認ダイアログ(Modal も role=dialog なので、ボタンで判定する)
        await fireEvent.click(r.getByRole("button", { name: "キャンセル" }));

        expect(called("block_user")).toHaveLength(0);
        expect(r.queryByText("ブロック中")).toBeNull();
      });

      it("確認でOKするとblock_userを呼び、「ブロック中」バッジが出て、フォロー中ならフォロー解除表示になる", async () => {
        mockProfile({ isFollowing: true });
        const r = renderModal();
        await waitFor(() => expect(r.getByRole("button", { name: "フォロー解除" })).toBeTruthy());
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "ブロック" }));

        await fireEvent.click(r.getByRole("button", { name: "ブロックする" }));

        await waitFor(() =>
          expect(invokeMock).toHaveBeenCalledWith(
            "block_user",
            expect.objectContaining({ accountId: "acc1", userId: "u1" }),
          ),
        );
        await waitFor(() => expect(r.getByText("ブロック中")).toBeTruthy());
        // サーバーはブロック時にフォローを外す
        expect(r.getByRole("button", { name: "フォロー" })).toBeTruthy();
        expect(r.queryByRole("button", { name: "フォロー解除" })).toBeNull();
      });

      it("ブロック中は「ブロック中」バッジが出て、「ブロック解除」は確認なしでunblock_userを呼ぶ", async () => {
        mockProfile({ isBlocking: true });
        const r = renderModal();
        await waitFor(() => expect(r.getByText("ブロック中")).toBeTruthy());
        await fireEvent.click(await menuButton(r));

        await fireEvent.click(r.getByRole("button", { name: "ブロック解除" }));

        await waitFor(() =>
          expect(invokeMock).toHaveBeenCalledWith(
            "unblock_user",
            expect.objectContaining({ accountId: "acc1", userId: "u1" }),
          ),
        );
        await waitFor(() => expect(r.queryByText("ブロック中")).toBeNull());
      });

      it("ALREADY_BLOCKINGで失敗したら、表示を「ブロック中」に直し、補正メッセージを出す", async () => {
        mockProfile({}, (cmd) =>
          cmd === "block_user"
            ? Promise.reject(new Error("blocking/create: ALREADY_BLOCKING You are already blocking that user."))
            : undefined,
        );
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "ブロック" }));

        await fireEvent.click(r.getByRole("button", { name: "ブロックする" }));

        await waitFor(() => expect(r.getByText("ブロック中")).toBeTruthy());
        expect(r.getByText(/すでにブロック中です/)).toBeTruthy();
      });

      it("NOT_BLOCKINGで失敗したら、表示を「ブロック中でない」に直す", async () => {
        mockProfile({ isBlocking: true }, (cmd) =>
          cmd === "unblock_user" ? Promise.reject(new Error("blocking/delete: NOT_BLOCKING")) : undefined,
        );
        const r = renderModal();
        await waitFor(() => expect(r.getByText("ブロック中")).toBeTruthy());
        await fireEvent.click(await menuButton(r));

        await fireEvent.click(r.getByRole("button", { name: "ブロック解除" }));

        await waitFor(() => expect(r.queryByText("ブロック中")).toBeNull());
        expect(r.getByText(/すでにブロック中ではありません/)).toBeTruthy();
      });

      it("block_userが403(権限不足)で失敗したら、エラーを表示し、バッジは出さない", async () => {
        mockProfile({}, (cmd) =>
          cmd === "block_user"
            ? Promise.reject({ kind: "forbidden", message: "blocking/create: PERMISSION_DENIED" })
            : undefined,
        );
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "ブロック" }));

        await fireEvent.click(r.getByRole("button", { name: "ブロックする" }));

        await waitFor(() => expect(r.getByText(/PERMISSION_DENIED|forbidden|権限/)).toBeTruthy());
        expect(r.queryByText("ブロック中")).toBeNull();
      });
    });

    describe("リノートミュート", () => {
      it("「リノートをミュート」でrenote_mute_userを呼び、メニューを開き直すと「リノートのミュートを解除」になる", async () => {
        mockProfile();
        const r = renderModal();
        await fireEvent.click(await menuButton(r));

        await fireEvent.click(r.getByRole("button", { name: "リノートをミュート" }));

        await waitFor(() =>
          expect(invokeMock).toHaveBeenCalledWith(
            "renote_mute_user",
            expect.objectContaining({ accountId: "acc1", userId: "u1" }),
          ),
        );
        await fireEvent.click(await menuButton(r));
        expect(await r.findByRole("button", { name: "リノートのミュートを解除" })).toBeTruthy();
      });

      it("リノートミュート中なら、解除でunrenote_mute_userを呼ぶ", async () => {
        mockProfile({ isRenoteMuted: true });
        const r = renderModal();
        await fireEvent.click(await menuButton(r));

        await fireEvent.click(r.getByRole("button", { name: "リノートのミュートを解除" }));

        await waitFor(() =>
          expect(invokeMock).toHaveBeenCalledWith(
            "unrenote_mute_user",
            expect.objectContaining({ accountId: "acc1", userId: "u1" }),
          ),
        );
      });

      it("ALREADY_MUTINGで失敗したら、表示をリノートミュート中に直す", async () => {
        mockProfile({}, (cmd) =>
          cmd === "renote_mute_user" ? Promise.reject(new Error("renote-mute/create: ALREADY_MUTING")) : undefined,
        );
        const r = renderModal();
        await fireEvent.click(await menuButton(r));

        await fireEvent.click(r.getByRole("button", { name: "リノートをミュート" }));

        await waitFor(() => expect(r.getByText(/すでにリノートをミュート中です/)).toBeTruthy());
        await fireEvent.click(await menuButton(r));
        expect(await r.findByRole("button", { name: "リノートのミュートを解除" })).toBeTruthy();
      });

      it("isRenoteMutedがnullなら「リノートをミュート」を出さない(サーバーが返さない場合)", async () => {
        mockProfile({ isRenoteMuted: null });
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await r.findByRole("button", { name: "ブロック" }); // メニューが描画されたことを確かめてから、不在を見る
        expect(r.queryByRole("button", { name: "リノートをミュート" })).toBeNull();
      });
    });

    describe("リストに追加", () => {
      it("パネルを開くとlist_user_lists_with_membershipを呼び、押すとadd_user_to_listを呼ぶ", async () => {
        mockProfile({}, (cmd) =>
          cmd === "list_user_lists_with_membership"
            ? Promise.resolve([{ id: "L1", name: "友達", hasUser: false }])
            : undefined,
        );
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "リストに追加" }));

        const row = await r.findByRole("checkbox", { name: "友達" });
        await fireEvent.click(row);

        await waitFor(() =>
          expect(invokeMock).toHaveBeenCalledWith(
            "add_user_to_list",
            expect.objectContaining({ accountId: "acc1", listId: "L1", userId: "u1" }),
          ),
        );
        expect(invokeMock).toHaveBeenCalledWith(
          "list_user_lists_with_membership",
          expect.objectContaining({ accountId: "acc1", userId: "u1" }),
        );
        await waitFor(() => expect(row.getAttribute("aria-checked")).toBe("true"));
      });

      it("入っているリストを押すとremove_user_from_listを呼ぶ", async () => {
        mockProfile({}, (cmd) =>
          cmd === "list_user_lists_with_membership"
            ? Promise.resolve([{ id: "L1", name: "友達", hasUser: true }])
            : undefined,
        );
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "リストに追加" }));

        await fireEvent.click(await r.findByRole("checkbox", { name: "友達" }));

        await waitFor(() =>
          expect(invokeMock).toHaveBeenCalledWith(
            "remove_user_from_list",
            expect.objectContaining({ listId: "L1", userId: "u1" }),
          ),
        );
      });

      it("ALREADY_ADDEDで失敗したら、チェックを付けて補正メッセージを出し、メニューは開いたまま", async () => {
        mockProfile({}, (cmd) => {
          if (cmd === "list_user_lists_with_membership")
            return Promise.resolve([{ id: "L1", name: "友達", hasUser: false }]);
          if (cmd === "add_user_to_list") return Promise.reject(new Error("users/lists/push: ALREADY_ADDED"));
          return undefined;
        });
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "リストに追加" }));

        const row = await r.findByRole("checkbox", { name: "友達" });
        await fireEvent.click(row);

        await waitFor(() => expect(row.getAttribute("aria-checked")).toBe("true"));
        expect(r.getByText("すでにリストに入っています。表示を更新しました")).toBeTruthy();
        expect(r.getByTestId("user-menu")).toBeTruthy();
      });
    });

    describe("通報", () => {
      it("「通報」でダイアログが開き、コメントを送るとreport_userを呼び、閉じて「通報しました」を出す", async () => {
        mockProfile();
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "通報" }));

        await fireEvent.input(r.getByLabelText("通報の内容"), { target: { value: "スパムです" } });
        await fireEvent.click(r.getByRole("button", { name: "通報する" }));

        await waitFor(() =>
          expect(invokeMock).toHaveBeenCalledWith(
            "report_user",
            expect.objectContaining({ accountId: "acc1", userId: "u1", comment: "スパムです" }),
          ),
        );
        await waitFor(() => expect(r.queryByLabelText("通報の内容")).toBeNull());
        expect(r.getByText("通報しました")).toBeTruthy();
      });

      it("通報に失敗したら、ダイアログは開いたまま入力が残る", async () => {
        mockProfile({}, (cmd) =>
          cmd === "report_user" ? Promise.reject(new Error("users/report-abuse: CANNOT_REPORT_THE_ADMIN")) : undefined,
        );
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await fireEvent.click(r.getByRole("button", { name: "通報" }));
        await fireEvent.input(r.getByLabelText("通報の内容"), { target: { value: "スパムです" } });

        await fireEvent.click(r.getByRole("button", { name: "通報する" }));

        await waitFor(() => expect(r.getByText(/CANNOT_REPORT_THE_ADMIN/)).toBeTruthy());
        expect((r.getByLabelText("通報の内容") as HTMLTextAreaElement).value).toBe("スパムです");
        expect(r.queryByText("通報しました")).toBeNull();
      });
    });

    describe("ブラウザで開く", () => {
      it("profileUrlがあれば「ブラウザで開く」を出し、押すとopenUrlでその元ページを開く", async () => {
        mockProfile({ profileUrl: "https://remote.example/@alice" });
        const r = renderModal();
        await fireEvent.click(await menuButton(r));

        await fireEvent.click(r.getByRole("button", { name: "ブラウザで開く" }));

        expect(openUrlMock).toHaveBeenCalledWith("https://remote.example/@alice");
        expect(r.queryByTestId("user-menu")).toBeNull(); // メニューは閉じる
      });

      it("profileUrlがnullなら「ブラウザで開く」を出さない", async () => {
        mockProfile({ profileUrl: null });
        const r = renderModal();
        await fireEvent.click(await menuButton(r));
        await r.findByRole("button", { name: "ブロック" }); // メニューが描画されたことを確かめてから、不在を見る
        expect(r.queryByRole("button", { name: "ブラウザで開く" })).toBeNull();
      });
    });

    it("操作の応答が返る前にプロフィールが切り替わっても、新しいプロフィールの表示とbusyを汚さない", async () => {
      let rejectBlock!: (e: Error) => void;
      mockProfile({}, (cmd) =>
        cmd === "block_user" ? new Promise((_, rej) => (rejectBlock = rej)) : undefined,
      );
      const r = renderModal();
      await fireEvent.click(await menuButton(r));
      await fireEvent.click(r.getByRole("button", { name: "ブロック" }));
      await fireEvent.click(r.getByRole("button", { name: "ブロックする" }));
      await waitFor(() => expect(called("block_user")).toHaveLength(1));

      await r.rerender({ target: { userId: "u2" }, accountId: "acc1", onclose: () => {} });
      rejectBlock(new Error("blocking/create: ALREADY_BLOCKING"));
      await new Promise((resolve) => setTimeout(resolve, 0));

      expect(r.queryByText("ブロック中")).toBeNull();
      expect(r.queryByText(/すでにブロック中です/)).toBeNull();
    });
  });
});

describe("ProfileModal モバイル(Issue #450)", () => {
  function mockProfileWithNote() {
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_user_profile") return Promise.resolve(profileResponse());
      if (cmd === "get_user_notes" && args?.untilId == null) return Promise.resolve([makeNote({ id: "n1" })]);
      if (cmd === "get_user_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
  }

  it("PC版ではノート一覧が高さ上限付きの自前スクロール領域", async () => {
    mockProfileWithNote();
    const { getByText } = render(ProfileModal, {
      props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(getByText("hello")).toBeTruthy());
    const notes = document.querySelector('[data-testid="profile-notes-scroll"]') as HTMLElement;
    expect(notes.className).toMatch(/max-h-\[40vh\]/);
    expect(notes.className).toMatch(/overflow-y-auto/);
  });

  it("モバイルではプロフィール全体を1つのスクロール領域にし、ノート一覧の高さ上限を外す", async () => {
    app.ui = { ...app.ui, uiMode: "mobile" };
    mockProfileWithNote();
    const { getByText } = render(ProfileModal, {
      props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(getByText("hello")).toBeTruthy());
    const notes = document.querySelector('[data-testid="profile-notes-scroll"]') as HTMLElement;
    expect(notes.className).not.toMatch(/max-h-/);
    expect(notes.className).not.toMatch(/overflow-y-auto/);
    const scroller = document.querySelector('[data-testid="profile-scroll"]') as HTMLElement;
    expect(scroller.className).toMatch(/overflow-y-auto/);
    expect(scroller.contains(notes)).toBe(true);
  });

  it("モバイルでは全体スクロール領域の末尾付近で次のノートを取得する", async () => {
    app.ui = { ...app.ui, uiMode: "mobile" };
    mockProfileWithNote();
    const { getByText, queryByText } = render(ProfileModal, {
      props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
    });
    await waitFor(() => expect(getByText("hello")).toBeTruthy());
    // 初回のノート取得が完全に終わる(notesBusyが戻る)まで待つ。busy中のスクロールは無視される。
    await waitFor(() => expect(queryByText("読み込み中…")).toBeNull());
    invokeMock.mockClear();
    const scroller = document.querySelector('[data-testid="profile-scroll"]') as HTMLElement;
    Object.defineProperty(scroller, "scrollTop", { value: 500, configurable: true });
    Object.defineProperty(scroller, "clientHeight", { value: 400, configurable: true });
    Object.defineProperty(scroller, "scrollHeight", { value: 1000, configurable: true });
    await fireEvent.scroll(scroller);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("get_user_notes", expect.objectContaining({ untilId: "n1" })),
    );
  });
});
