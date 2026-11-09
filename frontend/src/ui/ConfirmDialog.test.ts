import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { createRawSnippet, tick } from "svelte";

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

const { default: ConfirmDialog } = await import("./ConfirmDialog.svelte");
const { default: Modal } = await import("./Modal.svelte");
const { app } = await import("../lib/store.svelte");

const settle = () => new Promise<void>((r) => setTimeout(r, 20));

afterEach(async () => {
  cleanup();
  app.ui = { ...app.ui, uiMode: "auto" };
  // 履歴を戻すhistory.go()によるpopstateを次のテストに持ち越さない
  await settle();
  vi.restoreAllMocks();
});

describe("ConfirmDialog のフォーカス(Issue #570)", () => {
  it("開いた時にダイアログ内(キャンセルボタン)へフォーカスする", () => {
    const { getByText } = render(ConfirmDialog, {
      props: { message: "削除しますか?", onConfirm: vi.fn(), onCancel: vi.fn() },
    });
    expect(document.activeElement).toBe(getByText("キャンセル").closest("button"));
  });

  it("開いた直後にクリックせずとも Escape でキャンセルできる", async () => {
    const onCancel = vi.fn();
    render(ConfirmDialog, { props: { message: "削除しますか?", onConfirm: vi.fn(), onCancel } });
    await fireEvent.keyDown(document.activeElement as HTMLElement, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});

describe("ConfirmDialog のモバイル履歴(Issue #573)", () => {
  const body = createRawSnippet(() => ({ render: () => `<span>profile</span>` }));

  function mobile() {
    app.ui = { ...app.ui, uiMode: "mobile" };
  }

  it("モバイルでは開くと履歴を1つ積み、戻るで親モーダルを閉じずダイアログだけ閉じる", async () => {
    mobile();
    const onclose = vi.fn();
    render(Modal, { props: { title: "プロフィール", onclose, children: body } });
    await tick();
    const onCancel = vi.fn();
    const push = vi.spyOn(history, "pushState");
    render(ConfirmDialog, { props: { message: "ブロックしますか?", onConfirm: vi.fn(), onCancel } });
    await tick();
    expect(push).toHaveBeenCalledTimes(1);

    history.back();
    await settle();
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onclose).not.toHaveBeenCalled();
  });

  it("UI側(キャンセルボタン)で閉じて消えると、積んだ履歴を戻す", async () => {
    mobile();
    const go = vi.spyOn(history, "go");
    const { getByText, unmount } = render(ConfirmDialog, {
      props: { message: "削除しますか?", onConfirm: vi.fn(), onCancel: vi.fn() },
    });
    await tick();
    await fireEvent.click(getByText("キャンセル"));
    unmount();
    await Promise.resolve();
    expect(go).toHaveBeenCalledTimes(1);
    expect(go).toHaveBeenCalledWith(-1);
  });

  it("PC(箱型)では履歴を積まない", async () => {
    app.ui = { ...app.ui, uiMode: "desktop" };
    const push = vi.spyOn(history, "pushState");
    render(ConfirmDialog, { props: { message: "削除しますか?", onConfirm: vi.fn(), onCancel: vi.fn() } });
    await tick();
    expect(push).not.toHaveBeenCalled();
  });
});
