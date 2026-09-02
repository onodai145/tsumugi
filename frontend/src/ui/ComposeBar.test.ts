import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, fireEvent, waitFor } from "@testing-library/svelte";
import { app } from "../lib/store.svelte";

// store.svelte.ts が起動時に @tauri-apps/plugin-os の platform() を呼ぶため、
// Tauri ランタイム外(jsdom)で import が失敗しないようスタブする(NoteCard.test.tsと同じパターン)。
vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

// __TAURI_INVOKE(生成bindings内でのinvoke呼び出し)は素の値を返す(typedError()側で
// { status: "ok", data } に包まれる)。ここで { status, data } を返してしまうと二重に
// 包まれてしまい unwrapAcc() 側の判定が壊れるため、コマンドごとの「生の戻り値」を返す。
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

const { default: ComposeBar } = await import("./ComposeBar.svelte");

function setupAccount() {
  app.accounts = [
    {
      id: "acc1",
      host: "misskey.io",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
      instance: null,
    },
  ];
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_drafts") return Promise.resolve([]);
    if (cmd === "get_auto_draft") return Promise.resolve(null);
    return Promise.resolve(null);
  });
  setupAccount();
  // ComposeBarの自動復元effectはapp.booting===falseになるまで待つ(App.svelteの
  // app.boot()完了を模す)。ComposeBar単体テストではboot()自体は呼ばず、フラグだけ倒す。
  app.booting = false;
});

afterEach(() => {
  cleanup();
  app.accounts = [];
  app.booting = true;
});

describe("ComposeBar 下書き", () => {
  it("マウント時にget_auto_draftを呼ぶ", async () => {
    render(ComposeBar);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("get_auto_draft", { accountId: "acc1" });
    });
  });

  it("入力後2秒でsave_auto_draftを呼ぶ", async () => {
    vi.useFakeTimers();
    try {
      const { getByTestId } = render(ComposeBar);
      await vi.advanceTimersByTimeAsync(0); // マウント時のget_auto_draft(非同期)を先に消化する
      await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "書きかけ" } });
      // 2000msのデバウンスであること自体を検証する: 直前(1900ms)ではまだ呼ばれておらず、
      // 2000msに達した時点で初めて呼ばれることを確認する(単に「いつかは呼ばれる」だけでは
      // デバウンスの有無を区別できないため)。
      await vi.advanceTimersByTimeAsync(1900);
      expect(invokeMock).not.toHaveBeenCalledWith("save_auto_draft", expect.anything());
      await vi.advanceTimersByTimeAsync(100);
      expect(invokeMock).toHaveBeenCalledWith(
        "save_auto_draft",
        expect.objectContaining({ accountId: "acc1" }),
      );
    } finally {
      vi.useRealTimers();
    }
  });

  it("空に戻すとclear_auto_draftを呼ぶ", async () => {
    const { getByTestId } = render(ComposeBar);
    // マウント直後(自動復元完了直後)の時点では、まだ何も入力していないので
    // clear_auto_draftが呼ばれてはならない(復元しようとしている自動下書きを誤って
    // 消してしまう回帰: 自動保存effectと自動復元effectの宣言順序に依存するハザードへの
    // リグレッションガード)。
    expect(invokeMock).not.toHaveBeenCalledWith("clear_auto_draft", expect.anything());
    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "a" } });
    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "" } });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("clear_auto_draft", { accountId: "acc1" });
    });
  });

  it("デバウンスが既に発火済みの場合、アンマウント時にsave_auto_draftを再送しない", async () => {
    vi.useFakeTimers();
    try {
      const { getByTestId, unmount } = render(ComposeBar);
      await vi.advanceTimersByTimeAsync(0); // マウント時のget_auto_draft(非同期)を先に消化する
      await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "デバウンス完了済み" } });
      await vi.advanceTimersByTimeAsync(2000); // デバウンスを確定させる(save_auto_draftが1回発火)
      const saveCallsBeforeUnmount = invokeMock.mock.calls.filter((c) => c[0] === "save_auto_draft").length;
      expect(saveCallsBeforeUnmount).toBe(1);
      unmount();
      const saveCallsAfterUnmount = invokeMock.mock.calls.filter((c) => c[0] === "save_auto_draft").length;
      expect(saveCallsAfterUnmount).toBe(1); // 発火済みタイマーをonDestroyが誤って再送しない
    } finally {
      vi.useRealTimers();
    }
  });

  it("デバウンス確定前にアンマウントされてもsave_auto_draftがflushされる", async () => {
    vi.useFakeTimers();
    try {
      const { getByTestId, unmount } = render(ComposeBar);
      await vi.advanceTimersByTimeAsync(0); // マウント時のget_auto_draft(非同期)を先に消化する
      await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "閉じる直前の入力" } });
      // 2000msのデバウンスが確定する前(モバイル投稿モーダルを閉じる操作を模す)にアンマウントする
      await vi.advanceTimersByTimeAsync(500);
      expect(invokeMock).not.toHaveBeenCalledWith("save_auto_draft", expect.anything());
      unmount();
      expect(invokeMock).toHaveBeenCalledWith(
        "save_auto_draft",
        expect.objectContaining({ accountId: "acc1" }),
      );
    } finally {
      vi.useRealTimers();
    }
  });

  it("手動下書きを呼び出すとtextが復元され、投稿成功後にdelete_draftが呼ばれる", async () => {
    const draft = {
      id: "d1",
      accountId: "acc1",
      kind: "manual",
      text: "保存済み本文",
      cw: null,
      visibility: "public",
      localOnly: false,
      reactionAcceptance: "all",
      channelId: null,
      poll: null,
      fileIds: [],
      replyNote: null,
      quoteNote: null,
      createdAt: 0,
      updatedAt: 0,
    };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drafts") return Promise.resolve([draft]);
      if (cmd === "get_auto_draft") return Promise.resolve(null);
      if (cmd === "post_note") return Promise.resolve({ id: "n1" });
      if (cmd === "delete_draft") return Promise.resolve(null);
      if (cmd === "clear_auto_draft") return Promise.resolve(null);
      return Promise.resolve(null);
    });
    const { getByTitle, getByText, getByTestId } = render(ComposeBar);
    await fireEvent.click(getByTitle("下書き"));
    await waitFor(() => expect(getByText("保存済み本文")).toBeTruthy());
    await fireEvent.click(getByText("保存済み本文"));
    expect((getByTestId("compose-textarea") as HTMLTextAreaElement).value).toBe("保存済み本文");

    await fireEvent.click(getByTestId("compose-submit"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("delete_draft", { accountId: "acc1", draftId: "d1" });
    });
  });
});
