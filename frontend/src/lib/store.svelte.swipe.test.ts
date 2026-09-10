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

function makeGroup(id: string, tabIds: string[], activeTabId: string) {
  return {
    id,
    width: 320,
    auto: false,
    tabs: tabIds.map((tid) => ({ id: tid }) as never),
    activeTabId,
  };
}

beforeEach(() => {
  invokeMock.mockClear();
  app.groups = [];
  app.paneRoot = { type: "split", id: "root", direction: "row", children: [] };
  app.focusedGroupId = null;
});

describe("applySwipe(Issue #296)", () => {
  it("次のタブがあればactiveTabIdをタブ送りする", () => {
    app.groups = [makeGroup("g1", ["t1", "t2"], "t1")];
    app.paneRoot = { type: "leaf", id: "l1", groupId: "g1" };

    const target = app.applySwipe("g1", "next");

    expect(target).toEqual({ kind: "tab", groupId: "g1", tabId: "t2" });
    expect(app.groups[0].activeTabId).toBe("t2");
  });

  it("末尾タブでさらに次へスワイプすると次のカラムへfocusedGroupIdを移す", () => {
    app.groups = [makeGroup("g1", ["t1"], "t1"), makeGroup("g2", ["t2"], "t2")];
    app.paneRoot = {
      type: "split",
      id: "root",
      direction: "row",
      children: [
        { node: { type: "leaf", id: "l1", groupId: "g1" }, size: null, auto: true },
        { node: { type: "leaf", id: "l2", groupId: "g2" }, size: null, auto: true },
      ],
    };

    const target = app.applySwipe("g1", "next");

    expect(target).toEqual({ kind: "group", groupId: "g2" });
    expect(app.focusedGroupId).toBe("g2");
    // カラム移動ではタブは変えない
    expect(app.groups[1].activeTabId).toBe("t2");
  });

  it("移動先が無ければ何も変更せずnullを返す", () => {
    app.groups = [makeGroup("g1", ["t1"], "t1")];
    app.paneRoot = { type: "leaf", id: "l1", groupId: "g1" };
    app.focusedGroupId = null;

    const target = app.applySwipe("g1", "next");

    expect(target).toBeNull();
    expect(app.groups[0].activeTabId).toBe("t1");
    expect(app.focusedGroupId).toBeNull();
  });
});
