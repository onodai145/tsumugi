import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Account } from "../bindings/tauri.gen";
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
const other: Account = { ...account, id: "acc2", username: "you", userId: "u2" };

function tab(id: string, accountId: string, state: TabView["state"]): TabView {
  return {
    id,
    accountId,
    kind: { type: "home" },
    title: "ホーム",
    customTitle: null,
    filter: { kind: "keywords", value: [] },
    notifyDesktop: false,
    notifySound: false,
    notifySoundChoice: "",
    speech: { enabled: false, speakAuthor: true, queueLimit: 5, voiceId: "" },
    notes: [],
    notifications: [],
    state,
    loadingMore: false,
    gapMarker: null,
    fillingGap: false,
    selectedNoteId: null,
    selectionMoveSeq: 0,
    epoch: 0,
  };
}

function setTabs(...tabs: TabView[]) {
  app.groups = [{ id: "g1", width: 300, auto: false, tabs, activeTabId: tabs[0].id }];
}

function authLogs() {
  return app.logs.filter((l) => l.reauthAccountId !== undefined);
}

beforeEach(() => {
  app.accounts = [account, other];
  app.logs = [];
  vi.spyOn(commands, "completeMiauth").mockResolvedValue({ status: "ok", data: account } as never);
  vi.spyOn(commands, "syncServerMutes").mockResolvedValue({
    status: "ok",
    data: { blockedUsers: 0, wordRules: 0 },
  } as never);
  vi.spyOn(commands, "listColumns").mockResolvedValue({ status: "ok", data: [] } as never);
});

afterEach(() => {
  vi.restoreAllMocks();
  app.groups = [];
  app.accounts = [];
  app.logs = [];
});

describe("認証失敗でカラムがErrorになったとき(Issue #622)", () => {
  it("再認証ボタン付きのログを、アカウントごとに1回だけ出す", () => {
    setTabs(tab("c1", "acc1", "connected"), tab("c2", "acc1", "connected"), tab("c3", "acc2", "connected"));

    app.applyConnectionState({ columnId: "c1", state: "error" });
    app.applyConnectionState({ columnId: "c2", state: "error" }); // 同じアカウントの2つ目
    app.applyConnectionState({ columnId: "c3", state: "error" }); // 別アカウント

    expect(authLogs().map((l) => l.reauthAccountId).sort()).toEqual(["acc1", "acc2"]);
    expect(authLogs().find((l) => l.reauthAccountId === "acc1")?.text).toBe(
      "認証に失敗しました: @me@misskey.test のトークンが失効している可能性があります",
    );
    expect(app.groups[0].tabs.map((t) => t.state)).toEqual(["error", "error", "error"]);
  });

  it("Error以外の状態ではログを出さない", () => {
    setTabs(tab("c1", "acc1", "connected"));

    app.applyConnectionState({ columnId: "c1", state: "reconnecting" });

    expect(authLogs()).toEqual([]);
  });
});

describe("再認証したあとのストリームの再接続(Issue #622)", () => {
  it("そのアカウントのErrorのタブにだけ reconnectColumn を呼ぶ", async () => {
    setTabs(tab("c1", "acc1", "error"), tab("c2", "acc1", "connected"), tab("c3", "acc2", "error"));
    const reconnect = vi.spyOn(commands, "reconnectColumn").mockResolvedValue({ status: "ok", data: null } as never);

    await app.completeAccount("session1");

    expect(reconnect).toHaveBeenCalledTimes(1);
    expect(reconnect).toHaveBeenCalledWith("c1");
  });

  it("再認証したのにまた失敗したら、もう一度ログを出す(重複抑止は再認証でリセットされる)", async () => {
    setTabs(tab("c1", "acc1", "connected"));
    vi.spyOn(commands, "reconnectColumn").mockResolvedValue({ status: "ok", data: null } as never);
    app.applyConnectionState({ columnId: "c1", state: "error" });
    expect(authLogs()).toHaveLength(1);

    await app.completeAccount("session1");
    app.applyConnectionState({ columnId: "c1", state: "error" });

    expect(authLogs()).toHaveLength(2);
  });

  it("再接続したカラムの表示中ノートを、新しいタスクでキャプチャし直す", async () => {
    // 止まったタスクの購読(キャプチャ)は一緒に消えているので、そのままだと表示中のノートの
    // リアクション・投票・削除が、アプリを再起動するまで反映されなくなる。
    const errored = tab("c1", "acc1", "error");
    errored.notes = [{ id: "n2" }, { id: "n1" }] as never;
    setTabs(errored);
    vi.spyOn(commands, "reconnectColumn").mockResolvedValue({ status: "ok", data: null } as never);
    const capture = vi.spyOn(commands, "captureNotes").mockResolvedValue({ status: "ok", data: null } as never);

    await app.completeAccount("session1");

    expect(capture).toHaveBeenCalledWith("c1", ["n2", "n1"]);
  });

  it("再接続に失敗したカラムは、キャプチャし直さない", async () => {
    const errored = tab("c1", "acc1", "error");
    errored.notes = [{ id: "n1" }] as never;
    setTabs(errored);
    vi.spyOn(commands, "reconnectColumn").mockResolvedValue({
      status: "error",
      error: { kind: "unauthorized", message: "no token" },
    } as never);
    const capture = vi.spyOn(commands, "captureNotes").mockResolvedValue({ status: "ok", data: null } as never);

    await app.completeAccount("session1");

    expect(capture).not.toHaveBeenCalled();
  });

  it("reconnectColumn が失敗しても、再認証自体は成功のままにする", async () => {
    setTabs(tab("c1", "acc1", "error"));
    vi.spyOn(commands, "reconnectColumn").mockResolvedValue({
      status: "error",
      error: { kind: "unauthorized", message: "no token" },
    } as never);

    await expect(app.completeAccount("session1")).resolves.toBeUndefined();
  });
});
