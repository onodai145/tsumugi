import { afterEach, describe, expect, it, vi } from "vitest";

const getPendingShare = vi.fn();
const openCompose = vi.fn();
const defaultAccountId = vi.fn((..._args: unknown[]) => "acc1");

vi.mock("./ipc", () => ({
  commands: {
    getPendingShare: (...args: unknown[]) => getPendingShare(...args),
  },
}));

vi.mock("./store.svelte", () => ({
  app: {
    openCompose: (...args: unknown[]) => openCompose(...args),
    defaultAccountId: (...args: unknown[]) => defaultAccountId(...args),
  },
}));

const { pollPendingShare, setupPendingShareListener } = await import("./pendingShare");

afterEach(() => {
  vi.clearAllMocks();
});

describe("pollPendingShare", () => {
  it("保留中の共有があればopenComposeに渡す", async () => {
    getPendingShare.mockResolvedValue({ text: "共有テキスト", filePaths: ["/tmp/a.png"] });
    await pollPendingShare();
    expect(openCompose).toHaveBeenCalledWith("acc1", {
      text: "共有テキスト",
      filePaths: ["/tmp/a.png"],
    });
  });

  it("nullならopenComposeを呼ばない", async () => {
    getPendingShare.mockResolvedValue(null);
    await pollPendingShare();
    expect(openCompose).not.toHaveBeenCalled();
  });

  it("textもfilePathsも空ならopenComposeを呼ばない", async () => {
    getPendingShare.mockResolvedValue({ text: null, filePaths: [] });
    await pollPendingShare();
    expect(openCompose).not.toHaveBeenCalled();
  });
});

describe("pollPendingShareの失敗", () => {
  it("getPendingShareが拒否してもsetupPendingShareListener経由では未処理rejectionにならない", async () => {
    // getPendingShareが拒否したとき、setupPendingShareListener内部でcatchされていなければ
    // このテスト自体がunhandled rejectionとして失敗する(vitestはデフォルトでそれを検出する)。
    getPendingShare.mockRejectedValue(new Error("boom"));
    const cleanup = setupPendingShareListener();
    // マイクロタスクキューが捌かれるまで待つ
    await new Promise((resolve) => setTimeout(resolve, 0));
    cleanup();
  });
});

describe("setupPendingShareListener", () => {
  it("登録直後に1回ポーリングする", () => {
    getPendingShare.mockResolvedValue(null);
    const cleanup = setupPendingShareListener();
    expect(getPendingShare).toHaveBeenCalledTimes(1);
    cleanup();
  });

  it("visibilitychangeでvisibleになるたびポーリングする", () => {
    getPendingShare.mockResolvedValue(null);
    const cleanup = setupPendingShareListener();
    getPendingShare.mockClear();
    Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
    document.dispatchEvent(new Event("visibilitychange"));
    expect(getPendingShare).toHaveBeenCalledTimes(1);
    cleanup();
  });

  it("windowのfocusでもポーリングする", () => {
    getPendingShare.mockResolvedValue(null);
    const cleanup = setupPendingShareListener();
    getPendingShare.mockClear();
    window.dispatchEvent(new Event("focus"));
    expect(getPendingShare).toHaveBeenCalledTimes(1);
    cleanup();
  });

  it("戻り値のクリーンアップでリスナーを解除できる", () => {
    getPendingShare.mockResolvedValue(null);
    const cleanup = setupPendingShareListener();
    cleanup();
    getPendingShare.mockClear();
    Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
    document.dispatchEvent(new Event("visibilitychange"));
    window.dispatchEvent(new Event("focus"));
    expect(getPendingShare).not.toHaveBeenCalled();
  });
});
