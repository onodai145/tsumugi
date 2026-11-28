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
const { app } = await import("../lib/store.svelte");

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
  app.ui = { ...app.ui, lowBandwidth: false };
});

describe("MediaGrid", () => {
  it("HLS のプレイリストはプレイヤーを作らず、保存できるファイル行にする", () => {
    const { container, getByLabelText } = render(MediaGrid, {
      files: [file({ id: "h1", mimeType: "audio/x-mpegurl", name: "list.m3u8" })],
    });
    expect(container.querySelector("media-player")).toBeNull();
    expect(getByLabelText("list.m3u8 を保存")).toBeTruthy();
  });

  it("閲覧注意のプレイリストは、カバー付きのファイル行にする", () => {
    const { container } = render(MediaGrid, {
      files: [
        file({ id: "h2", mimeType: "video/mp4", url: "https://example.com/a.m3u8", isSensitive: true }),
      ],
    });
    expect(container.querySelector(".media-cell")).toBeNull();
    expect(container.querySelector("li .file-row")?.textContent).toContain("閲覧注意（クリックで表示）");
  });

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

describe("MediaGrid 低帯域幅モード", () => {
  it("画像は取得せず、ファイル名とサイズを添えたカバーを出し、タップでサムネイルを読み込む", async () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { container, getByText } = render(MediaGrid, {
      files: [
        file({
          id: "i1",
          mimeType: "image/png",
          name: "a.png",
          url: "https://example.com/full.png",
          thumbnailUrl: "https://example.com/thumb.png",
          size: 2048,
        }),
      ],
    });
    expect(container.querySelector("img")).toBeNull();
    getByText("a.png");
    getByText("画像 · 2.0 KB");
    await fireEvent.click(getByText("タップで読み込む"));
    expect(container.querySelector("img")?.getAttribute("src")).toBe("https://example.com/thumb.png");
  });

  it("動画・音声もタップするまで media-player を作らない", async () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { container, getAllByText } = render(MediaGrid, {
      files: [
        file({ id: "v1", mimeType: "video/mp4", name: "a.mp4" }),
        file({ id: "s1", mimeType: "audio/mpeg", name: "a.mp3" }),
      ],
    });
    expect(container.querySelector("media-player")).toBeNull();
    const covers = getAllByText("タップで読み込む");
    expect(covers).toHaveLength(2);
    await fireEvent.click(covers[0]);
    expect(container.querySelectorAll("media-player")).toHaveLength(1);
  });

  it("サイズが無ければファイル名と種類だけ、名前も無ければ MIME タイプを出す", () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { getByText, getAllByText } = render(MediaGrid, {
      files: [
        file({ id: "i1", mimeType: "image/png", name: "a.png", size: null }),
        file({ id: "i2", mimeType: "image/webp", name: undefined, size: undefined }),
      ],
    });
    getByText("a.png");
    getByText("image/webp");
    // サイズが無い 2 枚は、種類(「画像」)だけの行になる。
    expect(getAllByText("画像")).toHaveLength(2);
  });

  it("閲覧注意かつ低帯域幅でも、タップ 1 回で表示される(文言は閲覧注意を優先)", async () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { container, getByText } = render(MediaGrid, {
      files: [file({ id: "i1", mimeType: "image/png", name: "a.png", isSensitive: true })],
    });
    expect(container.querySelector("img")).toBeNull();
    await fireEvent.click(getByText("閲覧注意（クリックで表示）"));
    expect(container.querySelector("img")).not.toBeNull();
  });

  it("オフなら従来どおり、タップなしで画像を出す", () => {
    app.ui = { ...app.ui, lowBandwidth: false };
    const { container, queryByText } = render(MediaGrid, {
      files: [file({ id: "i1", mimeType: "image/png", name: "a.png" })],
    });
    expect(container.querySelector("img")).not.toBeNull();
    expect(queryByText("タップで読み込む")).toBeNull();
  });

  it("オンでも非メディア(保存ボタンの行)は変わらない", () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { getByText } = render(MediaGrid, { files: [file({})] });
    expect(getByText("report.pdf").closest(".file-row")).not.toBeNull();
  });
});
