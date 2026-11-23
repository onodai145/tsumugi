import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { EditorView } from "@codemirror/view";
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
    speech: { enabled: false, speakAuthor: true, queueLimit: 5, voiceId: "" },
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

// Issue #75: 通知音ピッカーが非表示の間は、入力途中の(未検証の) MML 下書きを保存しない。
describe("AddColumnModal の通知音の種類(Issue #75)", () => {
  function setupEdit(notifySoundChoice: string) {
    const tab = makeTab({ type: "home" }, { kind: "keywords", value: [] });
    tab.notifySound = true;
    tab.notifySoundChoice = notifySoundChoice;
    app.groups = [{ id: "group1", width: 400, auto: false, tabs: [tab], activeTabId: tab.id }];
    return tab;
  }
  const notifyCall = () => invokeMock.mock.calls.find((c) => c[0] === "set_column_notify");

  it("通知音を外してピッカーが消えたら、MML の下書きではなく元の値で保存する", async () => {
    const tab = setupEdit("chime");
    const onclose = vi.fn();
    const { getByText, getByTestId, findByTestId, getByLabelText } = render(AddColumnModal, { onclose, groupId: null, editTab: tab });

    await fireEvent.click(getByText("チャイム"));
    await fireEvent.click(getByText("MML（打ち込み）"));
    // MML の入力欄は動的 import なので、現れるまで待ってから CodeMirror に入力する
    const view = EditorView.findFromDOM(await findByTestId("mml-input"))!;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: "c4 x" } });
    await fireEvent.click(getByLabelText("通知音"));
    await fireEvent.click(getByTestId("add-column-submit"));
    await waitFor(() => expect(onclose).toHaveBeenCalled());

    expect(notifyCall()?.[1]).toMatchObject({ notifySound: false, notifySoundChoice: "chime" });
  });

  it("ピッカーが見えているときは選んだプリセットをそのまま保存する", async () => {
    const tab = setupEdit("");
    const onclose = vi.fn();
    const { getByText, getByTestId } = render(AddColumnModal, { onclose, groupId: null, editTab: tab });

    await fireEvent.click(getByText("継承（グローバル設定）"));
    await fireEvent.click(getByText("ピン"));
    await fireEvent.click(getByTestId("add-column-submit"));
    await waitFor(() => expect(onclose).toHaveBeenCalled());

    expect(notifyCall()?.[1]).toMatchObject({ notifySound: true, notifySoundChoice: "ping" });
  });
});

describe("AddColumnModal の読み上げ(Issue #625)", () => {
  function setupEdit(speech = { enabled: false, speakAuthor: true, queueLimit: 5, voiceId: "" }) {
    const tab = makeTab({ type: "home" }, { kind: "keywords", value: [] });
    tab.speech = speech;
    app.groups = [{ id: "group1", width: 400, auto: false, tabs: [tab], activeTabId: tab.id }];
    return tab;
  }
  const speechCall = () => invokeMock.mock.calls.find((c) => c[0] === "set_column_speech");

  it("全体の読み上げがオフのときは、トグルが無効で案内が出る", () => {
    app.speech = { ...app.speech, enabled: false };
    const tab = setupEdit();
    const { getByTestId, getByText } = render(AddColumnModal, { onclose: vi.fn(), groupId: null, editTab: tab });
    expect((getByTestId("speech-column-enabled") as HTMLInputElement).disabled).toBe(true);
    expect(getByText(/設定で読み上げをオンにしてください/)).toBeTruthy();
  });

  it("全体がオンで、トグルをオンにして保存すると set_column_speech を呼ぶ", async () => {
    app.speech = { ...app.speech, enabled: true };
    const tab = setupEdit();
    const onclose = vi.fn();
    const { getByTestId } = render(AddColumnModal, { onclose, groupId: null, editTab: tab });
    await fireEvent.click(getByTestId("speech-column-enabled"));
    await fireEvent.click(getByTestId("add-column-submit"));
    await waitFor(() => expect(onclose).toHaveBeenCalled());
    expect(speechCall()?.[1]).toMatchObject({ columnId: "tab1", speech: { enabled: true, speakAuthor: true, queueLimit: 5, voiceId: "" } });
  });

  it("変更がなければ set_column_speech を呼ばない", async () => {
    app.speech = { ...app.speech, enabled: true };
    const tab = setupEdit();
    const onclose = vi.fn();
    const { getByTestId } = render(AddColumnModal, { onclose, groupId: null, editTab: tab });
    await fireEvent.click(getByTestId("add-column-submit"));
    await waitFor(() => expect(onclose).toHaveBeenCalled());
    expect(speechCall()).toBeUndefined();
  });

  it("内蔵エンジンのときは、取得済みの声を list_speech_core_voices から取得して、選択肢に出す", async () => {
    app.speech = { ...app.speech, enabled: true, engine: "voicevoxCore", externalUrl: "" };
    invokeMock.mockImplementation(async (cmd: string) => (cmd === "list_speech_core_voices" ? [{ id: "3", label: "ずんだもん（ノーマル）" }] : null));
    // 声の選択肢はエキスパートモード(TQL のタブ)でだけ出る。
    const tab = makeTab({ type: "tql" }, { kind: "tql", value: 'from home where text -> "foo"' });
    tab.speech = { enabled: true, speakAuthor: true, queueLimit: 5, voiceId: "" };
    app.groups = [{ id: "group1", width: 400, auto: false, tabs: [tab], activeTabId: tab.id }];
    const { getByTestId } = render(AddColumnModal, { onclose: vi.fn(), groupId: null, editTab: tab });
    const trigger = getByTestId("speech-column-voice");
    expect(trigger.tagName, "テーマの効く Dropdown(ネイティブの <select> ではない)").toBe("BUTTON");
    expect(trigger.textContent).toContain("既定の声を使う");
    await waitFor(async () => {
      await fireEvent.click(trigger);
      expect(getByTestId("speech-column-voice-option-3").textContent).toContain("ずんだもん（ノーマル）");
    });
    expect(invokeMock.mock.calls.some((c) => c[0] === "list_speech_voices")).toBe(false);
  });

  it("外部エンジンで URL が空のときは、声の一覧を取得しない", async () => {
    app.speech = { ...app.speech, enabled: true, engine: "external", externalUrl: "" };
    invokeMock.mockResolvedValue(null);
    const tab = setupEdit({ enabled: true, speakAuthor: true, queueLimit: 5, voiceId: "" });
    render(AddColumnModal, { onclose: vi.fn(), groupId: null, editTab: tab });
    await new Promise((r) => setTimeout(r, 20));
    expect(invokeMock.mock.calls.some((c) => c[0] === "list_speech_voices" || c[0] === "list_speech_core_voices")).toBe(false);
  });

  it("通知カラムには読み上げの設定を出さない", () => {
    app.speech = { ...app.speech, enabled: true };
    const tab = makeTab({ type: "notifications" }, { kind: "keywords", value: [] });
    app.groups = [{ id: "group1", width: 400, auto: false, tabs: [tab], activeTabId: tab.id }];
    const { queryByTestId } = render(AddColumnModal, { onclose: vi.fn(), groupId: null, editTab: tab });
    expect(queryByTestId("speech-column-enabled")).toBeNull();
  });
});


describe("AddColumnModal の2段階処理の途中失敗(Issue #615)", () => {
  const opened = {
    column: { id: "tabNew", accountId: ACCOUNT_ID, kind: { type: "home" }, order: 0, filter: { kind: "keywords", value: [] }, notifySound: false, notifyDesktop: false, notifySoundChoice: "", groupId: "g1", title: null },
    group: { id: "g1", width: 400, auto: false },
    notes: [],
    notifications: [],
  };
  const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

  function mockBackend(fail: string) {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "add_column") return Promise.resolve(opened);
      if (cmd === fail) return Promise.reject("失敗");
      return Promise.resolve(null);
    });
  }

  it("カラム追加後に通知設定の保存が失敗しても、モーダルは閉じて再試行の余地を残さない(重複しない)", async () => {
    mockBackend("set_column_notify");
    const onclose = vi.fn();
    const { getByTestId, getByLabelText } = render(AddColumnModal, { onclose, groupId: null });
    await fireEvent.click(getByLabelText("通知音"));
    await fireEvent.click(getByTestId("add-column-submit"));
    await waitFor(() => expect(onclose).toHaveBeenCalled());

    expect(calls("add_column")).toHaveLength(1);
    expect(app.errorModal).not.toBeNull();
  });

  it("カラム追加後にペイン配置の取得が失敗しても、作成済みのカラムを残してモーダルを閉じ、エラーを出す", async () => {
    mockBackend("load_pane_layout");
    const onclose = vi.fn();
    const { getByTestId } = render(AddColumnModal, { onclose, groupId: null });
    await fireEvent.click(getByTestId("add-column-submit"));
    await waitFor(() => expect(onclose).toHaveBeenCalled());

    expect(calls("add_column")).toHaveLength(1);
    expect(app.errorModal).not.toBeNull();
    expect(app.groups.flatMap((g) => g.tabs).map((t) => t.id)).toContain("tabNew");
  });
});
