import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import type { Account, ColumnKind, FilterQuery } from "../bindings/tauri.gen";
import type { TabView } from "../lib/store.svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: AddColumnModal } = await import("./AddColumnModal.svelte");
const { app } = await import("../lib/store.svelte");

const ACCOUNT_ID = "acc1";

function makeAccount(): Account {
  return {
    id: ACCOUNT_ID,
    host: "misskey.io",
    username: "alice",
    userId: "u1",
    displayName: "Alice",
    avatarUrl: null,
  };
}

function makeTab(kind: ColumnKind, filter: FilterQuery): TabView {
  return {
    id: "tab1",
    accountId: ACCOUNT_ID,
    kind,
    title: "自動名",
    customTitle: null,
    filter,
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
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) =>
    cmd === "list_user_lists" ? Promise.resolve([{ id: "list1", name: "リスト" }]) : Promise.resolve(null),
  );
  app.accounts = [makeAccount()];
});

afterEach(() => {
  cleanup();
  app.groups = [];
  app.accounts = [];
});

// Issue #59: 編集モーダルが組み立てる kind/filter が、元のタブの値と等価にならないと
// store.updateColumn の「名前だけの変更」判定が黙って外れ、従来どおり全リセット(update_column)に戻る。
// 各ソース種別について、モーダルの初期化→保存の往復で kind/filter が保たれることを確認する。
describe("AddColumnModal 編集モードで名前だけ変更した場合(Issue #59)", () => {
  const noKeywords: FilterQuery = { kind: "keywords", value: [] };
  const cases: [string, ColumnKind, FilterQuery][] = [
    ["home", { type: "home" }, noKeywords],
    ["local + TQLフィルタ", { type: "local" }, { kind: "tql", value: 'text -> "foo"' }],
    ["tql(エキスパート)", { type: "tql" }, { kind: "tql", value: 'from home where text -> "foo"' }],
    ["tag", { type: "tag", tag: "misskey" }, noKeywords],
    ["search", { type: "search", query: "foo bar" }, noKeywords],
    ["user(acct未入力で元のuserIdを維持)", { type: "user", userId: "u1" }, noKeywords],
    ["list", { type: "list", listId: "list1" }, noKeywords],
    ["notifications", { type: "notifications" }, noKeywords],
  ];

  it.each(cases)("%s: update_columnを呼ばずrename_columnだけで保存する", async (_label, kind, filter) => {
    const tab = makeTab(kind, filter);
    app.groups = [{ id: "group1", width: 400, auto: false, tabs: [tab], activeTabId: tab.id }];
    const onclose = vi.fn();

    const { getByTestId } = render(AddColumnModal, { onclose, groupId: null, editTab: tab });
    await fireEvent.input(getByTestId("add-column-name-input"), { target: { value: "新しい名前" } });
    await fireEvent.click(getByTestId("add-column-submit"));
    await waitFor(() => expect(onclose).toHaveBeenCalled());

    const commands = invokeMock.mock.calls.map((c) => c[0]);
    expect(commands).not.toContain("update_column");
    expect(commands).toContain("rename_column");
    expect(app.groups[0].tabs[0].customTitle).toBe("新しい名前");
  });
});
