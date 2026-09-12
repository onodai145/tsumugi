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

const { default: DrivePicker } = await import("./DrivePicker.svelte");

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
});

describe("DrivePicker", () => {
  it("タイトル「ドライブから選択」を表示し、フォルダ一覧を読み込む", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drive_folders") return Promise.resolve([{ id: "f1", name: "写真" }]);
      if (cmd === "list_drive_files") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const { getByText } = render(DrivePicker, {
      props: { accountId: "acc1", onSelect: vi.fn(), onclose: () => {} },
    });
    expect(getByText("ドライブから選択")).toBeTruthy();
    await waitFor(() => expect(getByText(/写真/)).toBeTruthy());
  });

  it("×ボタンでoncloseを呼ぶ", async () => {
    invokeMock.mockResolvedValue([]);
    const onclose = vi.fn();
    const { getAllByRole } = render(DrivePicker, {
      props: { accountId: "acc1", onSelect: vi.fn(), onclose },
    });
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    await fireEvent.click(getAllByRole("button")[0]);
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("ファイルを選択して添付ボタンでonSelectを呼ぶ", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drive_folders") return Promise.resolve([]);
      if (cmd === "list_drive_files") {
        return Promise.resolve([
          { id: "file1", name: "a.png", mimeType: "image/png", url: "https://example.com/a.png", thumbnailUrl: null, isSensitive: false },
        ]);
      }
      return Promise.resolve(null);
    });
    const onSelect = vi.fn();
    const { getByText } = render(DrivePicker, {
      props: { accountId: "acc1", onSelect, onclose: () => {} },
    });
    await waitFor(() => expect(document.querySelector('img[alt="a.png"]')).toBeTruthy());
    const fileButtons = document.querySelectorAll('img[alt="a.png"]');
    await fireEvent.click(fileButtons[0].closest("button")!);
    await fireEvent.click(getByText("添付"));
    expect(onSelect).toHaveBeenCalledWith([expect.objectContaining({ id: "file1" })]);
  });
});
