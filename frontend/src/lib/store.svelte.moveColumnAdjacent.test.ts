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
const { topLevelLeafGroupIds } = await import("./swipeNav");

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

/// load_pane_layoutが返す木(=movePane適用後のRust側の状態を模したもの)。
/// 各テストが期待する並びをここに差し替える。
let nextPaneLayout = leafRoot(["g1", "g2", "g3"]);

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    // 生成bindingsのtypedError()がinvokeの生の戻り値をResultに包むため、ここでは生の値を返す。
    if (cmd === "load_pane_layout") return Promise.resolve(nextPaneLayout);
    return Promise.resolve(null);
  });
  app.groups = [makeGroup("g1"), makeGroup("g2"), makeGroup("g3")];
  app.paneRoot = leafRoot(["g1", "g2", "g3"]);
  nextPaneLayout = leafRoot(["g1", "g2", "g3"]);
});

describe("moveColumnAdjacent(Issue #354)", () => {
  it("nextで隣のカラムの直後(edge:right)へmovePaneし、paneRootを取り直す", async () => {
    // Rust側のmove_pane: remove_group(g1) -> [g2,g3] -> insert_sibling_at(g2, g1, Row, before=false)
    nextPaneLayout = leafRoot(["g2", "g1", "g3"]);
    await app.moveColumnAdjacent("g1", "next");
    expect(invokeMock).toHaveBeenCalledWith("move_pane", { draggedGroupId: "g1", targetGroupId: "g2", edge: "right" });
    expect(invokeMock).toHaveBeenCalledWith("load_pane_layout");
    expect(app.paneRoot).toEqual(nextPaneLayout);
    expect(topLevelLeafGroupIds(app.paneRoot)).toEqual(["g2", "g1", "g3"]);
  });

  it("prevで隣のカラムの手前(edge:left)へmovePaneする", async () => {
    nextPaneLayout = leafRoot(["g1", "g3", "g2"]);
    await app.moveColumnAdjacent("g3", "prev");
    expect(invokeMock).toHaveBeenCalledWith("move_pane", { draggedGroupId: "g3", targetGroupId: "g2", edge: "left" });
    expect(topLevelLeafGroupIds(app.paneRoot)).toEqual(["g1", "g3", "g2"]);
  });

  it("先頭カラムでprevを指定しても何もしない", async () => {
    const before = app.paneRoot;
    await app.moveColumnAdjacent("g1", "prev");
    expect(invokeMock).not.toHaveBeenCalledWith("move_pane", expect.anything());
    expect(app.paneRoot).toBe(before);
  });

  it("末尾カラムでnextを指定しても何もしない", async () => {
    const before = app.paneRoot;
    await app.moveColumnAdjacent("g3", "next");
    expect(invokeMock).not.toHaveBeenCalledWith("move_pane", expect.anything());
    expect(app.paneRoot).toBe(before);
  });

  it("2カラムでも隣と入れ替わる(Rust側でrootがLeafに畳まれてから再Splitされる経路)", async () => {
    app.paneRoot = leafRoot(["g1", "g2"]);
    nextPaneLayout = leafRoot(["g2", "g1"]);
    await app.moveColumnAdjacent("g1", "next");
    expect(invokeMock).toHaveBeenCalledWith("move_pane", { draggedGroupId: "g1", targetGroupId: "g2", edge: "right" });
    expect(topLevelLeafGroupIds(app.paneRoot)).toEqual(["g2", "g1"]);
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
