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

// このテストファイルはモバイル向けのハプティクス発火を検証するため、実機OS判定に依らず
// isMobilePlatform を true に固定する(@tauri-apps/plugin-os のモックは "linux" のまま)。
// vi.mock はファイルスコープで効くため、この固定化の影響を他のComposeBarテストから
// 隔離する目的で本ファイルを独立させている(最終レビュー指摘)。
vi.mock("../lib/platform", () => ({ isMobilePlatform: true }));

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

describe("ハプティクス(Issue #26)", () => {
  it("投稿成功後にvibrateコマンドをmediumパターンで呼ぶ", async () => {
    setupAccount();
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "post_note") return Promise.resolve({ id: "n1" });
      return Promise.resolve(null);
    });
    const { getByTestId } = render(ComposeBar);
    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "hello" } });
    await fireEvent.click(getByTestId("compose-submit"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("vibrate", { pattern: "medium" });
    });
  });
});
