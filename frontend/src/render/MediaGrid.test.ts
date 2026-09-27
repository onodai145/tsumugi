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
// vidstack はカスタム要素を登録するだけで、振り分けの検証には不要
vi.mock("vidstack/player", () => ({}));
vi.mock("vidstack/player/ui", () => ({}));

const { default: MediaGrid } = await import("./MediaGrid.svelte");

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

afterEach(cleanup);

describe("MediaGrid", () => {
  it("画像はグリッドのセル、非メディアはリスト行に振り分ける", () => {
    const { container, getByText } = render(MediaGrid, {
      files: [file({ id: "i1", mimeType: "image/png", name: "a.png" }), file({ id: "p1" })],
    });
    expect(container.querySelectorAll(".media-cell")).toHaveLength(1);
    expect(container.querySelector(".media-cell img")).not.toBeNull();
    const row = getByText("report.pdf").closest(".file-row");
    expect(row).not.toBeNull();
    expect(row?.closest(".media-cell")).toBeNull();
  });

  it("非メディアだけならグリッドを描画しない", () => {
    const { container } = render(MediaGrid, { files: [file({})] });
    expect(container.querySelector(".media-cell")).toBeNull();
    expect(container.querySelectorAll(".file-row")).toHaveLength(1);
  });

  it("空配列では何も描画しない", () => {
    const { container } = render(MediaGrid, { files: [] });
    expect(container.querySelector(".media-cell")).toBeNull();
    expect(container.querySelector(".file-row")).toBeNull();
  });

  it("閲覧注意の非メディアはカバーをクリックすると保存ボタンの行に切り替わる", async () => {
    const { getByText, queryByText } = render(MediaGrid, {
      files: [file({ isSensitive: true })],
    });
    expect(queryByText("report.pdf")).toBeNull();
    await fireEvent.click(getByText("閲覧注意（クリックで表示）"));
    getByText("report.pdf");
    getByText("1.5 KB");
  });
});
