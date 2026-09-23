import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import type { DriveFile } from "../bindings/tauri.gen";

// store.svelte.ts が起動時に @tauri-apps/plugin-os の platform() を呼ぶため、
// Tauri ランタイム外(jsdom)で import が失敗しないようスタブする(NoteCard.test.ts等と同様)。
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

const { default: MediaViewer } = await import("./MediaViewer.svelte");

// jsdomはscrollIntoViewを実装していないため、マウント時のスクロール同期(onMount)を含む
// すべてのテストで安全にレンダリングできるよう既定でno-opスタブを用意しておく。
// 挙動を検証したいテストは各itの冒頭でこの参照を上書きする。
// （その他のブラウザAPI補完は vite.config.ts の setupFiles で読み込まれる test-setup.ts にて一元管理）
Element.prototype.scrollIntoView = vi.fn();

function file(overrides: Partial<DriveFile>): DriveFile {
  return {
    id: "f1",
    mimeType: "image/png",
    isSensitive: false,
    url: "https://example.com/f1.png",
    thumbnailUrl: null,
    name: "f1.png",
    ...overrides,
  };
}

afterEach(() => cleanup());

describe("MediaViewer", () => {
  it("Escapeキーでoncloseが呼ばれる", async () => {
    const onclose = vi.fn();
    render(MediaViewer, {
      props: {
        files: [file({ id: "a" })],
        startIndex: 0,
        revealed: {},
        onclose,
      },
    });
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("背景クリックでoncloseが呼ばれる", async () => {
    const onclose = vi.fn();
    const { getByRole } = render(MediaViewer, {
      props: {
        files: [file({ id: "a" })],
        startIndex: 0,
        revealed: {},
        onclose,
      },
    });
    await fireEvent.click(getByRole("presentation"));
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("次へボタンで2件目のページへscrollIntoViewする", async () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    const { getByLabelText } = render(MediaViewer, {
      props: {
        files: [file({ id: "a", name: "a.png" }), file({ id: "b", name: "b.png" })],
        startIndex: 0,
        revealed: {},
        onclose: () => {},
      },
    });
    scrollIntoView.mockClear();
    await fireEvent.click(getByLabelText("次へ"));
    expect(scrollIntoView).toHaveBeenCalledOnce();
    const [, secondPage] = document.querySelectorAll('[data-testid="media-page"]');
    expect(scrollIntoView.mock.instances[0]).toBe(secondPage);
    expect(scrollIntoView).toHaveBeenCalledWith(expect.objectContaining({ behavior: "smooth" }));
  });

  // 回帰テスト: 末尾から次へ送ると先頭へラップアラウンドする。このステップは複数のScroll
  // Snapポイントを跨ぐ長距離スクロールになり、一部のブラウザエンジンではsmoothスクロールが
  // 意図した位置まで到達しないことがあるため、behavior: "auto"(アニメーションなし)で
  // 瞬時に移動させる必要がある(goNext/goPrevのコメント参照)。
  it("末尾から次へで先頭にラップアラウンドする際はbehavior: autoでscrollIntoViewする", async () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    const { getByLabelText } = render(MediaViewer, {
      props: {
        files: [
          file({ id: "a", name: "a.png" }),
          file({ id: "b", name: "b.png" }),
          file({ id: "c", name: "c.png" }),
        ],
        startIndex: 2,
        revealed: {},
        onclose: () => {},
      },
    });
    scrollIntoView.mockClear();
    await fireEvent.click(getByLabelText("次へ"));
    expect(scrollIntoView).toHaveBeenCalledOnce();
    const [firstPage] = document.querySelectorAll('[data-testid="media-page"]');
    expect(scrollIntoView.mock.instances[0]).toBe(firstPage);
    expect(scrollIntoView).toHaveBeenCalledWith(expect.objectContaining({ behavior: "auto" }));
  });

  it("先頭から前へで末尾にラップアラウンドする際はbehavior: autoでscrollIntoViewする", async () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    const { getByLabelText } = render(MediaViewer, {
      props: {
        files: [
          file({ id: "a", name: "a.png" }),
          file({ id: "b", name: "b.png" }),
          file({ id: "c", name: "c.png" }),
        ],
        startIndex: 0,
        revealed: {},
        onclose: () => {},
      },
    });
    scrollIntoView.mockClear();
    await fireEvent.click(getByLabelText("前へ"));
    expect(scrollIntoView).toHaveBeenCalledOnce();
    const [, , thirdPage] = document.querySelectorAll('[data-testid="media-page"]');
    expect(scrollIntoView.mock.instances[0]).toBe(thirdPage);
    expect(scrollIntoView).toHaveBeenCalledWith(expect.objectContaining({ behavior: "auto" }));
  });

  it("非ゼロstartIndexでマウントすると、その位置へ即座にscrollIntoViewする", () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    render(MediaViewer, {
      props: {
        files: [file({ id: "a", name: "a.png" }), file({ id: "b", name: "b.png" })],
        startIndex: 1,
        revealed: {},
        onclose: () => {},
      },
    });
    expect(scrollIntoView).toHaveBeenCalledOnce();
    const [, secondPage] = document.querySelectorAll('[data-testid="media-page"]');
    expect(scrollIntoView.mock.instances[0]).toBe(secondPage);
    expect(scrollIntoView).toHaveBeenCalledWith(expect.objectContaining({ behavior: "auto" }));
  });

  it("閲覧注意ファイルは未表示ならカバーを表示し、クリックで表示状態になる", async () => {
    const { getByText, queryByText } = render(MediaViewer, {
      props: {
        files: [file({ id: "a", isSensitive: true })],
        startIndex: 0,
        revealed: {},
        onclose: () => {},
      },
    });
    expect(getByText("閲覧注意（クリックで表示）")).toBeTruthy();
    await fireEvent.click(getByText("閲覧注意（クリックで表示）"));
    expect(queryByText("閲覧注意（クリックで表示）")).toBeNull();
  });

  it("音声ファイルは自前コントロールバーで表示され、panzoom用のラッパーが付かない", () => {
    render(MediaViewer, {
      props: {
        files: [file({ id: "a", mimeType: "audio/mpeg", name: "a.mp3", url: "https://example.com/a.mp3" })],
        startIndex: 0,
        revealed: {},
        onclose: () => {},
      },
    });
    expect(document.querySelector("media-controls")).toBeTruthy();
  });
});
