import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import type { Note } from "../bindings/tauri.gen";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: NoteMenu } = await import("./NoteMenu.svelte");
const { app } = await import("../lib/store.svelte");

const note = { id: "n1", text: "hi", user: { id: "u2" } } as unknown as Note;
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  invokeMock.mockReset();
  app.accounts = [];
  app.errorModal = null;
});
afterEach(() => cleanup());

describe("NoteMenu のクリップ操作の途中失敗(Issue #615)", () => {
  function mockClips(failAdd: boolean) {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_clips") return Promise.resolve([{ id: "c0", name: "既存" }]);
      if (cmd === "create_clip") return Promise.resolve({ id: "c1", name: "新規" });
      if (cmd === "add_note_to_clip") return failAdd ? Promise.reject("追加に失敗") : Promise.resolve(null);
      return Promise.resolve(null);
    });
  }

  it("クリップ作成後に追加が失敗しても、メニューを閉じてエラーを出し、同名のクリップを重ねて作らない", async () => {
    mockClips(true);
    const onclose = vi.fn();
    const { getByText, findByPlaceholderText } = render(NoteMenu, { accountId: "acc1", note, onclose });

    await fireEvent.click(getByText("クリップに追加"));
    await fireEvent.click(await waitFor(() => getByText("＋ 新規クリップを作成")));
    const input = await findByPlaceholderText("クリップ名");
    await fireEvent.input(input, { target: { value: "新規" } });
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(onclose).toHaveBeenCalled());

    expect(calls("create_clip")).toHaveLength(1);
    expect(app.errorModal).not.toBeNull();
  });

  it("既存クリップへの追加が失敗したら、利用者にエラーが見える", async () => {
    mockClips(true);
    const onclose = vi.fn();
    const { getByText, findByText } = render(NoteMenu, { accountId: "acc1", note, onclose });

    await fireEvent.click(getByText("クリップに追加"));
    await fireEvent.click(await findByText("既存"));
    await waitFor(() => expect(app.errorModal).not.toBeNull());
  });
});
