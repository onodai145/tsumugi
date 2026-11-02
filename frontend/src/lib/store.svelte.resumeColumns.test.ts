import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Account, Column, OpenedColumn } from "../bindings/tauri.gen";
import type { TabView } from "./store.svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue({ status: "ok", data: null }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { app } = await import("./store.svelte");
const { commands } = await import("./ipc");

const account: Account = {
  id: "acc1",
  host: "misskey.test",
  username: "me",
  userId: "u1",
  displayName: "Me",
  avatarUrl: null,
  instance: null,
  isCat: false,
  avatarBlurhash: null,
};

function column(id: string, accountId: string): Column {
  return {
    id,
    accountId,
    kind: { type: "home" },
    order: 0,
    filter: { kind: "keywords", value: [] },
    notifySound: false,
    notifyDesktop: false,
    notifySoundChoice: "",
    groupId: "g1",
    title: null,
  };
}

function opened(c: Column): OpenedColumn {
  return { column: c, group: { id: "g1", order: 0, width: 300, auto: false }, notes: [], notifications: [] };
}

function shownTab(id: string): TabView {
  return {
    id,
    accountId: "acc1",
    kind: { type: "home" },
    title: "ホーム",
    customTitle: null,
    filter: { kind: "keywords", value: [] },
    notifyDesktop: false,
    notifySound: false,
    notifySoundChoice: "",
    notes: [],
    notifications: [],
    state: "connected",
    loadingMore: false,
    gapMarker: null,
    fillingGap: false,
    selectedNoteId: null,
    selectionMoveSeq: 0,
    epoch: 0,
  };
}

beforeEach(() => {
  app.accounts = [account];
  app.groups = [{ id: "g1", width: 300, auto: false, tabs: [shownTab("c1")], activeTabId: "c1" }];
  vi.spyOn(commands, "completeMiauth").mockResolvedValue({ status: "ok", data: account } as never);
  vi.spyOn(commands, "syncServerMutes").mockResolvedValue({
    status: "ok",
    data: { blockedUsers: 0, wordRules: 0 },
  } as never);
});

afterEach(() => {
  vi.restoreAllMocks();
  app.groups = [];
  app.accounts = [];
});

describe("再認証後のカラム復元(Issue #544)", () => {
  it("再認証したアカウントのカラムのうち、まだ表示されていないものだけを開く", async () => {
    vi.spyOn(commands, "listColumns").mockResolvedValue({
      status: "ok",
      data: [column("c1", "acc1"), column("c2", "acc1"), column("c3", "other")],
    } as never);
    const resume = vi
      .spyOn(commands, "resumeColumn")
      .mockResolvedValue({ status: "ok", data: opened(column("c2", "acc1")) } as never);

    await app.completeAccount("session1");

    expect(resume).toHaveBeenCalledTimes(1);
    expect(resume).toHaveBeenCalledWith("c2");
    expect(app.groups[0].tabs.map((t) => t.id)).toEqual(["c1", "c2"]);
  });

  it("開き直しに失敗しても、再認証自体は成功のままにする", async () => {
    vi.spyOn(commands, "listColumns").mockResolvedValue({
      status: "ok",
      data: [column("c2", "acc1")],
    } as never);
    vi.spyOn(commands, "resumeColumn").mockResolvedValue({
      status: "error",
      error: { kind: "unauthorized", message: "no token" },
    } as never);

    await expect(app.completeAccount("session1")).resolves.toBeUndefined();

    expect(app.groups[0].tabs.map((t) => t.id)).toEqual(["c1"]);
  });
});
