import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import type { Account, Note, User } from "../bindings/tauri.gen";

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

const { default: SearchModal } = await import("./SearchModal.svelte");
const { app } = await import("../lib/store.svelte");

function makeAccount(): Account {
  return {
    id: "acc1",
    host: "misskey.io",
    username: "alice",
    userId: "u1",
    displayName: "Alice",
    avatarUrl: null,
  };
}

function makeUser(): User {
  return {
    id: "u1",
    username: "alice",
    host: null,
    name: "Alice",
    avatarUrl: null,
    isBot: false,
    isCat: false,
    followersCount: 0,
    followingCount: 0,
    notesCount: 0,
  };
}

function makeNote(id: string, createdAt: number, text = "hello"): Note {
  return {
    id,
    createdAt,
    text,
    cw: null,
    visibility: "public",
    localOnly: false,
    user: makeUser(),
    replyId: null,
    renoteId: null,
    renote: null,
    files: [],
    poll: null,
    tags: [],
    mentions: [],
    emojis: {},
    channelId: null,
    via: null,
    lang: null,
    reactions: {},
    reactionCount: 0,
    renoteCount: 0,
    replyCount: 0,
    myReaction: null,
    isRenotedByMe: false,
    isFavoritedByMe: false,
    isPinned: false,
  };
}

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
  app.accounts = [];
});

describe("SearchModal", () => {
  it("キーワード/ユーザーの入力から組み立てたTQLでsearch_cache_notesを呼ぶ", async () => {
    app.accounts = [makeAccount()];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "search_cache_notes") return Promise.resolve([makeNote("n1", 100)]);
      return Promise.resolve(null);
    });
    const { getByPlaceholderText, getByTestId, getByText } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    await fireEvent.input(getByPlaceholderText("本文に含まれる語"), { target: { value: "rust" } });
    await fireEvent.input(getByPlaceholderText(/^@user@host/), { target: { value: "@bob@example.com" } });
    await fireEvent.click(getByTestId("search-submit"));

    await waitFor(() => expect(getByText("hello")).toBeTruthy());
    expect(invokeMock).toHaveBeenCalledWith(
      "search_cache_notes",
      expect.objectContaining({
        accountId: "acc1",
        filter: { kind: "tql", value: 'text -> "rust" && user.acct == "@bob@example.com"' },
        untilId: null,
        limit: 20,
      }),
    );
  });

  it("条件を何も入れずに検索すると空のTQL(全件)で呼び、0件なら該当なしを表示する", async () => {
    app.accounts = [makeAccount()];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "search_cache_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const { getByTestId, getByText } = render(SearchModal, { props: { onclose: () => {} } });
    await fireEvent.click(getByTestId("search-submit"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "search_cache_notes",
        expect.objectContaining({ filter: { kind: "tql", value: "" } }),
      ),
    );
    await waitFor(() => expect(getByText("該当するノートが見つかりませんでした")).toBeTruthy());
  });

  it("末尾までスクロールすると最後のノートIDをuntilIdにして追加取得する", async () => {
    app.accounts = [makeAccount()];
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "search_cache_notes" && args?.untilId == null) {
        return Promise.resolve([makeNote("n1", 200)]);
      }
      if (cmd === "search_cache_notes" && args?.untilId === "n1") {
        return Promise.resolve([makeNote("n2", 100)]);
      }
      return Promise.resolve([]);
    });
    const { getByTestId } = render(SearchModal, { props: { onclose: () => {} } });
    await fireEvent.click(getByTestId("search-submit"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());

    const list = document.querySelector('[data-testid="search-results-scroll"]') as HTMLElement;
    Object.defineProperty(list, "scrollTop", { value: 500, configurable: true });
    Object.defineProperty(list, "clientHeight", { value: 400, configurable: true });
    Object.defineProperty(list, "scrollHeight", { value: 1200, configurable: true });
    await fireEvent.scroll(list);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "search_cache_notes",
        expect.objectContaining({ untilId: "n1" }),
      ),
    );
  });

  it("次のページが前のページと同じノートIDを含んでいても重複表示しない", async () => {
    app.accounts = [makeAccount()];
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "search_cache_notes" && args?.untilId == null) {
        return Promise.resolve([makeNote("n1", 200, "first")]);
      }
      if (cmd === "search_cache_notes" && args?.untilId === "n1") {
        // バックエンドの id < ? 絞り込みと created_at DESC 順のズレにより、
        // 前ページと同じ id が再度返ってくることがある(Issue #248 レビュー指摘)。
        return Promise.resolve([makeNote("n1", 200, "first"), makeNote("n2", 100, "second")]);
      }
      return Promise.resolve([]);
    });
    const { getByTestId, getAllByText } = render(SearchModal, { props: { onclose: () => {} } });
    await fireEvent.click(getByTestId("search-submit"));
    await waitFor(() => expect(getAllByText("first").length).toBe(1));

    const list = document.querySelector('[data-testid="search-results-scroll"]') as HTMLElement;
    Object.defineProperty(list, "scrollTop", { value: 500, configurable: true });
    Object.defineProperty(list, "clientHeight", { value: 400, configurable: true });
    Object.defineProperty(list, "scrollHeight", { value: 1200, configurable: true });
    await fireEvent.scroll(list);

    await waitFor(() => expect(getAllByText("second").length).toBe(1));
    expect(getAllByText("first").length).toBe(1);
  });

  it("エキスパートモードでは組み立てたTQLではなく入力したTQLをそのまま送る", async () => {
    app.accounts = [makeAccount()];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "search_cache_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const { getByText, getByPlaceholderText, getByTestId } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    await fireEvent.click(getByText("エキスパート(TQL)"));
    const tqlField = getByPlaceholderText(/has_files/) as HTMLInputElement;
    await fireEvent.input(tqlField, { target: { value: "has_files" } });
    await waitFor(() => expect(tqlField.value).toBe("has_files"));
    await fireEvent.click(getByTestId("search-submit"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "search_cache_notes",
        expect.objectContaining({ filter: { kind: "tql", value: "has_files" } }),
      ),
    );
  });
});
