import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, within } from "@testing-library/svelte";
import { commands, events } from "../../lib/ipc";
import type { LocalModelInfo, LocalModelStatus } from "../../lib/ipc";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
const openUrl = vi.fn();
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: (u: string) => openUrl(u) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: LocalModelCard } = await import("./LocalModelCard.svelte");

const ok = <T,>(data: T) => ({ status: "ok" as const, data });
const LICENSE_URL = "https://example.com/license-ja";
const info = (over: Partial<LocalModelInfo> = {}): LocalModelInfo => ({
  status: { type: "notInstalled" },
  sizeBytes: 573_126_848,
  licenseVersion: "2026-11-09",
  licenseUrl: LICENSE_URL,
  licenseAccepted: false,
  supported: true,
  ...over,
});

let emit: (s: LocalModelStatus) => void = () => {};
const unlisten = vi.fn();

beforeEach(() => {
  openUrl.mockReset();
  unlisten.mockReset();
  vi.spyOn(events.localModelStatusChanged, "listen").mockImplementation((async (cb: (e: { payload: { status: LocalModelStatus } }) => void) => {
    emit = (status) => cb({ payload: { status } });
    return unlisten;
  }) as never);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("LocalModelCard", () => {
  it("未取得: サイズと Built with PLaMo を出し、ダウンロードだけが押せる", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info()));
    const { findByTestId, getByTestId, queryByTestId } = render(LocalModelCard);
    expect((await findByTestId("local-model-status")).textContent).toContain("未取得");
    expect(getByTestId("local-model-card").textContent).toContain("573.1 MB");
    expect(getByTestId("local-model-card").textContent).toContain("Built with PLaMo");
    expect(getByTestId("local-model-download")).toBeTruthy();
    expect(queryByTestId("local-model-cancel")).toBeNull();
    expect(queryByTestId("local-model-delete")).toBeNull();
  });

  it("未同意でダウンロードを押すと同意ダイアログが出て、同意するまで開始しない。キャンセルでも開始しない", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info()));
    const start = vi.spyOn(commands, "startLocalModelDownload").mockResolvedValue(ok(null));
    const { findByTestId, getByTestId, queryByTestId } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-download"));
    expect(getByTestId("license-dialog").textContent).toContain("商用");
    expect(getByTestId("license-dialog").textContent).toContain("573.1 MB");
    expect(start).not.toHaveBeenCalled();
    await fireEvent.click(getByTestId("license-cancel"));
    expect(queryByTestId("license-dialog")).toBeNull();
    expect(start).not.toHaveBeenCalled();
  });

  it("同意ダイアログのリンクとカードのリンクは、Rust が返したライセンス全文の URL を開く", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info()));
    const { findByTestId, getByText } = render(LocalModelCard);
    const download = await findByTestId("local-model-download");
    await fireEvent.click(getByText("PLaMo Community License", { selector: "button" }));
    expect(openUrl).toHaveBeenCalledWith(LICENSE_URL);
    openUrl.mockReset();
    await fireEvent.click(download);
    await fireEvent.click(getByText("ライセンス全文（日本語）を開く"));
    expect(openUrl).toHaveBeenCalledWith(LICENSE_URL);
  });

  it("同意してダウンロード: 現行のライセンス版を渡して開始し、同意済みを再取得する", async () => {
    const get = vi
      .spyOn(commands, "getLocalModelStatus")
      .mockResolvedValueOnce(ok(info()))
      .mockResolvedValue(ok(info({ status: { type: "downloading", downloaded: 0, total: 573_126_848 }, licenseAccepted: true })));
    const start = vi.spyOn(commands, "startLocalModelDownload").mockResolvedValue(ok(null));
    const { findByTestId, getByTestId, queryByTestId } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-download"));
    await fireEvent.click(getByTestId("license-accept"));
    await vi.waitFor(() => expect(start).toHaveBeenCalledWith("2026-11-09"));
    await vi.waitFor(() => expect(get).toHaveBeenCalledTimes(2));
    await vi.waitFor(() => expect(queryByTestId("license-dialog")).toBeNull());
  });

  it("同意済みなら、ダイアログを出さずに開始する", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info({ licenseAccepted: true })));
    const start = vi.spyOn(commands, "startLocalModelDownload").mockResolvedValue(ok(null));
    const { findByTestId, queryByTestId } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-download"));
    await vi.waitFor(() => expect(start).toHaveBeenCalledWith("2026-11-09"));
    expect(queryByTestId("license-dialog")).toBeNull();
  });

  it("開始に失敗したら、理由を出す（ダイアログは閉じる）", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info()));
    vi.spyOn(commands, "startLocalModelDownload").mockResolvedValue({
      status: "error",
      error: { kind: "invalid", message: "LOCAL_UNSUPPORTED" },
    } as never);
    const { findByTestId, getByTestId, queryByTestId, findByRole } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-download"));
    await fireEvent.click(getByTestId("license-accept"));
    expect((await findByRole("alert")).textContent).toContain("LOCAL_UNSUPPORTED");
    expect(queryByTestId("license-dialog")).toBeNull();
  });

  it("進行中のダウンロード: 開き直しても進捗が見え、イベントで更新され、キャンセルできる", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(
      ok(info({ status: { type: "downloading", downloaded: 100_000_000, total: 573_126_848 }, licenseAccepted: true })),
    );
    const cancel = vi.spyOn(commands, "cancelLocalModelDownload").mockResolvedValue(ok(null));
    const { findByTestId, getByTestId, queryByTestId } = render(LocalModelCard);
    const bar = await findByTestId("local-model-progress");
    expect(bar.getAttribute("aria-valuenow")).toBe("17");
    emit({ type: "downloading", downloaded: 286_563_424, total: 573_126_848 });
    await vi.waitFor(() => expect(getByTestId("local-model-progress").getAttribute("aria-valuenow")).toBe("50"));
    expect(queryByTestId("local-model-download")).toBeNull();
    await fireEvent.click(getByTestId("local-model-cancel"));
    expect(cancel).toHaveBeenCalled();
  });

  it("検証中はボタンを押せない。完了イベントで取得済みになり、削除が出る", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(
      ok(info({ status: { type: "verifying" }, licenseAccepted: true })),
    );
    const { findByTestId, getByTestId, queryByTestId } = render(LocalModelCard);
    expect((await findByTestId("local-model-status")).textContent).toContain("検証中");
    expect(queryByTestId("local-model-download")).toBeNull();
    expect(queryByTestId("local-model-cancel")).toBeNull();
    expect(queryByTestId("local-model-delete")).toBeNull();
    emit({ type: "ready" });
    await vi.waitFor(() => expect(getByTestId("local-model-status").textContent).toContain("取得済み"));
    expect(getByTestId("local-model-delete")).toBeTruthy();
  });

  it("失敗: 理由を出し、もう一度ダウンロードできる", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(
      ok(info({ status: { type: "failed", reason: "通信が途切れました" }, licenseAccepted: true })),
    );
    const { findByTestId, getByTestId } = render(LocalModelCard);
    expect((await findByTestId("local-model-status")).textContent).toContain("通信が途切れました");
    expect(getByTestId("local-model-download")).toBeTruthy();
  });

  it("削除は確認を挟む。確認してから deleteLocalModel を呼び、状態を取り直す", async () => {
    const get = vi
      .spyOn(commands, "getLocalModelStatus")
      .mockResolvedValue(ok(info({ status: { type: "ready" }, licenseAccepted: true })));
    const del = vi.spyOn(commands, "deleteLocalModel").mockResolvedValue(ok(null));
    const { findByTestId, getByRole } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-delete"));
    expect(del).not.toHaveBeenCalled();
    await fireEvent.click(within(getByRole("dialog")).getByText("削除"));
    await vi.waitFor(() => expect(del).toHaveBeenCalled());
    await vi.waitFor(() => expect(get).toHaveBeenCalledTimes(2));
  });

  it("削除の確認をキャンセルしたら、削除しない", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info({ status: { type: "ready" }, licenseAccepted: true })));
    const del = vi.spyOn(commands, "deleteLocalModel").mockResolvedValue(ok(null));
    const { findByTestId, getByRole, queryByRole } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-delete"));
    await fireEvent.click(within(getByRole("dialog")).getByText("キャンセル"));
    expect(queryByRole("dialog")).toBeNull();
    expect(del).not.toHaveBeenCalled();
  });

  it("非対応の環境では、カードの代わりに非対応の文を出す", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info({ supported: false })));
    const { findByText, queryByTestId } = render(LocalModelCard);
    expect(await findByText("この環境では PLaMo による翻訳は使えません。")).toBeTruthy();
    expect(queryByTestId("local-model-download")).toBeNull();
  });

  it("アンマウントでイベントの購読を解除する", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info()));
    const { findByTestId, unmount } = render(LocalModelCard);
    await findByTestId("local-model-status");
    unmount();
    expect(unlisten).toHaveBeenCalled();
  });
});
