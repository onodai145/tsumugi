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

function leafRoot(groupIds: string[]) {
  return {
    type: "split" as const,
    id: "root",
    direction: "row" as const,
    children: groupIds.map((groupId) => ({
      node: { type: "leaf" as const, id: `leaf-${groupId}`, groupId },
      size: null,
      auto: true,
    })),
  };
}

beforeEach(() => {
  invokeMock.mockClear();
  app.groups = [makeGroup("g1"), makeGroup("g2"), makeGroup("g3")];
  app.paneRoot = leafRoot(["g1", "g2", "g3"]);
});

describe("moveColumnAdjacent(Issue #354)", () => {
  it("nextで隣のカラムと入れ替え、reorderGroupsを呼ぶ", async () => {
    await app.moveColumnAdjacent("g1", "next");
    expect(app.groups.map((g) => g.id)).toEqual(["g2", "g1", "g3"]);
    expect(invokeMock).toHaveBeenCalledWith("reorder_groups", { orderedIds: ["g2", "g1", "g3"] });
  });

  it("先頭カラムでprevを指定しても何もしない", async () => {
    await app.moveColumnAdjacent("g1", "prev");
    expect(app.groups.map((g) => g.id)).toEqual(["g1", "g2", "g3"]);
    expect(invokeMock).not.toHaveBeenCalledWith("reorder_groups", expect.anything());
  });
});

describe("canMoveColumnAdjacent(Issue #354)", () => {
  it("先頭カラムはprevへ移動できない", () => {
    expect(app.canMoveColumnAdjacent("g1", "prev")).toBe(false);
  });
  it("末尾カラムはnextへ移動できない", () => {
    expect(app.canMoveColumnAdjacent("g3", "next")).toBe(false);
  });
  it("中間カラムはどちらへも移動できる", () => {
    expect(app.canMoveColumnAdjacent("g2", "prev")).toBe(true);
    expect(app.canMoveColumnAdjacent("g2", "next")).toBe(true);
  });
});
