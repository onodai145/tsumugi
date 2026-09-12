import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import type { PaneNode } from "../bindings/tauri.gen";

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

const { default: ColumnSettings } = await import("./ColumnSettings.svelte");
const { app } = await import("../lib/store.svelte");

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
  app.groups = [];
  app.paneRoot = { type: "split", id: "boot", direction: "row", children: [] };
});

function setupSingleLeafGroup() {
  app.groups = [{ id: "g1", width: 300, auto: false, tabs: [], activeTabId: "" }];
  app.paneRoot = { type: "leaf", id: "leaf1", groupId: "g1" } satisfies PaneNode;
}

describe("ColumnSettings", () => {
  it("タイトル「カラム設定」と幅設定フォームを表示する", () => {
    setupSingleLeafGroup();
    const { getByText } = render(ColumnSettings, { props: { groupId: "g1", onclose: () => {} } });
    expect(getByText("カラム設定")).toBeTruthy();
    expect(getByText("固定（ドラッグで調整）")).toBeTruthy();
  });

  it("×ボタンでoncloseを呼ぶ", async () => {
    setupSingleLeafGroup();
    const onclose = vi.fn();
    const { getByRole } = render(ColumnSettings, { props: { groupId: "g1", onclose } });
    await fireEvent.click(getByRole("button", { name: "" }));
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("「自動調整」を選ぶとsetGroupAuto(true)を呼ぶ", async () => {
    setupSingleLeafGroup();
    const { getByText } = render(ColumnSettings, { props: { groupId: "g1", onclose: () => {} } });
    await fireEvent.click(getByText("自動調整（ウィンドウ幅に合わせて均等割付）"));
    expect(invokeMock).toHaveBeenCalledWith("set_group_auto", expect.objectContaining({ groupId: "g1", auto: true }));
  });
});
