import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
const invokeMock = vi.fn().mockResolvedValue({ status: "ok", data: null });
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { app } = await import("./store.svelte");

function makeGroup(id: string) {
  return { id, width: 320, auto: false, tabs: [{ id: `${id}-t1` }] as never, activeTabId: `${id}-t1` };
}

beforeEach(() => {
  invokeMock.mockClear();
  app.groups = [];
  app.focusedGroupId = null;
});

describe("focusColumn(Issue #296)", () => {
  it("存在するグループへフォーカスを移す", () => {
    app.groups = [makeGroup("g1"), makeGroup("g2")];
    app.focusColumn("g2");
    expect(app.focusedGroupId).toBe("g2");
  });

  it("存在しないグループIDでは何も変更しない", () => {
    app.groups = [makeGroup("g1")];
    app.focusedGroupId = "g1";
    app.focusColumn("missing");
    expect(app.focusedGroupId).toBe("g1");
  });
});
