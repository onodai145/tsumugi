import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Account, Column, OpenedColumn } from "../bindings/tauri.gen";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("./uiScale", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./uiScale")>()),
  applyUiScale: vi.fn().mockResolvedValue(undefined),
}));

const { app } = await import("./store.svelte");

const SIX_HOURS = 6 * 60 * 60 * 1000;

function account(id: string): Account {
  return {
    id,
    host: "misskey.test",
    username: id,
    userId: `u-${id}`,
    displayName: id,
    avatarUrl: null,
    instance: null,
    isCat: false,
    avatarBlurhash: null,
  };
}

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

/// 呼ばれたコマンドの引数(呼ばれた順)。
const callsOf = (cmd: string) =>
  invokeMock.mock.calls.filter(([c]) => c === cmd).map(([, args]) => args as Record<string, string>);

let tokenless: string[] | Error;

beforeEach(() => {
  tokenless = ["acc2"];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, string>) => {
    switch (cmd) {
      case "list_accounts":
        return [account("acc1"), account("acc2")];
      case "accounts_without_token":
        if (tokenless instanceof Error) throw tokenless;
        return tokenless;
      case "get_ui_prefs":
        return { ...app.ui };
      case "sync_server_mutes":
        return { blockedUsers: 0, wordRules: 0 };
      case "refresh_instance_meta":
        return account(args!.accountId);
      case "list_groups":
        return [{ id: "g1", order: 0, width: 300, auto: false }];
      case "load_pane_layout":
        return null;
      case "list_columns":
        return [column("c1", "acc1"), column("c2", "acc2")];
      case "resume_column":
        return opened(column(args!.columnId, args!.columnId === "c1" ? "acc1" : "acc2"));
      default:
        return { status: "ok", data: null };
    }
  });
  app.logs = [];
  app.ui = { ...app.ui, theme: "auto" };
  app.needsLogin = new Set();
});

afterEach(() => {
  app.teardown();
  vi.useRealTimers();
  app.accounts = [];
  app.groups = [];
  app.needsLogin = new Set();
});

async function boot() {
  vi.useFakeTimers();
  const bootPromise = app.boot();
  await vi.advanceTimersByTimeAsync(0);
  await bootPromise.catch(() => {});
}

describe("トークンなしアカウントの扱い(Issue #553)", () => {
  it("起動時、トークンなしのアカウントには、ミュート同期・インスタンス情報取得・タブ復元をしない", async () => {
    await boot();

    expect([...app.needsLogin]).toEqual(["acc2"]);
    expect(callsOf("sync_server_mutes").map((a) => a.accountId)).toEqual(["acc1"]);
    expect(callsOf("refresh_instance_meta").map((a) => a.accountId)).toEqual(["acc1"]);
    expect(callsOf("resume_column").map((a) => a.columnId)).toEqual(["c1"]);
  });

  it("起動時に、エラーも警告もログに出さない", async () => {
    await boot();

    expect(app.logs.filter((l) => l.level === "error" || l.level === "warn")).toEqual([]);
  });

  it("accounts_without_token が失敗したら、従来どおり全アカウントを対象にする(起動は止めない)", async () => {
    tokenless = new Error("keyring unavailable");

    await boot();

    expect(app.needsLogin.size).toBe(0);
    expect(callsOf("sync_server_mutes").map((a) => a.accountId).sort()).toEqual(["acc1", "acc2"]);
    expect(callsOf("resume_column").map((a) => a.columnId).sort()).toEqual(["c1", "c2"]);
  });

  it("6時間ごとの定期同期でも、トークンなしのアカウントを同期しない", async () => {
    await boot();
    invokeMock.mockClear();

    await vi.advanceTimersByTimeAsync(SIX_HOURS + 1_000);

    expect(callsOf("sync_server_mutes").map((a) => a.accountId)).toEqual(["acc1"]);
  });

  it("再認証が完了したら needsLogin から外し、そのアカウントのミュート同期とカラム復元を行う", async () => {
    app.accounts = [account("acc2")];
    app.groups = [{ id: "g1", width: 300, auto: false, tabs: [], activeTabId: "" }];
    app.needsLogin = new Set(["acc2"]);
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, string>) => {
      switch (cmd) {
        case "complete_miauth":
          return account("acc2");
        case "sync_server_mutes":
          return { blockedUsers: 0, wordRules: 0 };
        case "list_columns":
          return [column("c2", "acc2")];
        case "resume_column":
          return opened(column(args!.columnId, "acc2"));
        default:
          return { status: "ok", data: null };
      }
    });

    await app.completeAccount("session1");

    expect(app.needsLogin.size).toBe(0);
    expect(callsOf("sync_server_mutes").map((a) => a.accountId)).toEqual(["acc2"]);
    expect(callsOf("resume_column").map((a) => a.columnId)).toEqual(["c2"]);
  });

  it("アカウントを削除したら needsLogin から外す", async () => {
    app.accounts = [account("acc1"), account("acc2")];
    app.needsLogin = new Set(["acc2"]);

    await app.removeAccount("acc2");

    expect(app.needsLogin.size).toBe(0);
  });
});
