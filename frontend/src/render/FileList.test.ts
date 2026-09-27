import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import type { DriveFile } from "../bindings/tauri.gen";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("../lib/mediaDownload", () => ({ saveMediaToDisk: vi.fn() }));

const { default: FileList } = await import("./FileList.svelte");
const { saveMediaToDisk } = await import("../lib/mediaDownload");
const { openUrl } = await import("@tauri-apps/plugin-opener");

function file(overrides: Partial<DriveFile>): DriveFile {
  return {
    id: "f1",
    mimeType: "application/pdf",
    isSensitive: false,
    url: "https://example.com/f1",
    thumbnailUrl: null,
    name: "report.pdf",
    size: 1536,
    ...overrides,
  };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("FileList", () => {
  it("ファイル名とサイズを表示する", () => {
    const { getByText } = render(FileList, { files: [file({})], revealed: {}, onreveal: vi.fn() });
    getByText("report.pdf");
    getByText("1.5 KB");
  });

  it("sizeが無ければサイズを表示しない", () => {
    const { queryByText } = render(FileList, {
      files: [file({ size: undefined })],
      revealed: {},
      onreveal: vi.fn(),
    });
    expect(queryByText(/KB|MB|GB/)).toBeNull();
  });

  it("クリックでsaveMediaToDiskを呼び、openUrlは呼ばない", async () => {
    const { getByRole } = render(FileList, { files: [file({})], revealed: {}, onreveal: vi.fn() });
    await fireEvent.click(getByRole("button", { name: /report\.pdf/ }));
    expect(saveMediaToDisk).toHaveBeenCalledWith("https://example.com/f1", "report.pdf", expect.any(Function));
    expect(openUrl).not.toHaveBeenCalled();
  });

  it("閲覧注意ファイルは開示するまでファイル名を出さず、クリックでonrevealを呼ぶ", async () => {
    const onreveal = vi.fn();
    const f = file({ isSensitive: true });
    const { getByText, queryByText } = render(FileList, { files: [f], revealed: {}, onreveal });
    expect(queryByText("report.pdf")).toBeNull();
    await fireEvent.click(getByText("閲覧注意（クリックで表示）"));
    expect(onreveal).toHaveBeenCalledWith(f);
    expect(saveMediaToDisk).not.toHaveBeenCalled();
  });
});
