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
    replyUserId: null,
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
    await fireEvent.click(getByText("詳細条件を指定（ユーザー・インスタンス・日時）"));
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

describe("SearchModal サーバー検索", () => {
  const ADV_BASE = "詳細条件を指定（ユーザー・インスタンス）";
  const ADV_WITH_DATE = "詳細条件を指定（ユーザー・インスタンス・日時）";
  const NO_DATE_NOTE = "日時範囲の指定は Misskey 2025.7.0 以降のサーバーで利用できます";

  function mockServer(opts: { dateRange?: boolean; capsFail?: boolean; notes?: Note[] } = {}) {
    app.accounts = [makeAccount()];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_search_capabilities") {
        return opts.capsFail
          ? Promise.reject(new Error("boom"))
          : Promise.resolve({ dateRange: opts.dateRange ?? false });
      }
      if (cmd === "search_server_notes") return Promise.resolve(opts.notes ?? []);
      if (cmd === "search_cache_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
  }

  const calledCommands = () => invokeMock.mock.calls.map((c) => c[0]);

  it("サーバーを選ぶとTQLタブが隠れ、キーワードが空白だけの間は検索できない", async () => {
    mockServer();
    const { getByText, queryByText, getByTestId, getByPlaceholderText } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    const submit = () => getByTestId("search-submit") as HTMLButtonElement;
    const keyword = getByPlaceholderText("本文に含まれる語");

    expect(queryByText("エキスパート(TQL)")).toBeTruthy();
    expect(submit().disabled).toBe(false); // キャッシュ検索は条件なしでも検索できる

    await fireEvent.click(getByText("サーバー"));
    expect(queryByText("エキスパート(TQL)")).toBeNull();
    expect(submit().disabled).toBe(true);

    await fireEvent.input(keyword, { target: { value: "  " } });
    expect(submit().disabled).toBe(true);

    await fireEvent.input(keyword, { target: { value: "rust" } });
    expect(submit().disabled).toBe(false);
  });

  it("サーバー検索はsearch_server_notesを条件付きで呼び、キャッシュ検索は呼ばない", async () => {
    mockServer({ notes: [makeNote("n1", 100, "from server")] });
    const { getByText, getByTestId, getByPlaceholderText } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    await fireEvent.click(getByText("サーバー"));
    await fireEvent.input(getByPlaceholderText("本文に含まれる語"), { target: { value: " rust " } });
    await fireEvent.click(getByText(ADV_BASE));
    await fireEvent.input(getByPlaceholderText(/^@user@host/), { target: { value: "@bob@example.com" } });
    await fireEvent.input(getByPlaceholderText(/^misskey\.example/), { target: { value: "example.com" } });
    await fireEvent.click(getByTestId("search-submit"));

    await waitFor(() => expect(getByText("from server")).toBeTruthy());
    expect(invokeMock).toHaveBeenCalledWith("search_server_notes", {
      accountId: "acc1",
      query: "rust",
      acct: "@bob@example.com",
      host: "example.com",
      sinceDate: null,
      untilDate: null,
      untilId: null,
      limit: 20,
    });
    expect(calledCommands()).not.toContain("search_cache_notes");
  });

  it("日時対応のサーバーでは日時欄が出て、選んだ日時が秒でsearch_server_notesへ渡る", async () => {
    mockServer({ dateRange: true });
    const { getByText, findByText, getByTestId, getByPlaceholderText } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    await fireEvent.click(getByText("サーバー"));
    await fireEvent.click(await findByText(ADV_WITH_DATE));
    await findByText("日時（開始）");

    // flatpickrは素のinputにインスタンス(_flatpickr)を載せる。setDate(.., true)でonChangeが走る。
    // ModalはportalでbodyへDOMを移すため、render()のcontainerではなくdocumentから探す。
    const [fromInput, toInput] = Array.from(
      document.querySelectorAll<HTMLInputElement>("input[placeholder='未指定']"),
    );
    const from = new Date(2026, 0, 2, 3, 4, 0);
    const to = new Date(2026, 0, 3, 23, 59, 0);
    type Fp = { _flatpickr: { setDate(d: Date, t: boolean): void } };
    (fromInput as unknown as Fp)._flatpickr.setDate(from, true);
    (toInput as unknown as Fp)._flatpickr.setDate(to, true);

    await fireEvent.input(getByPlaceholderText("本文に含まれる語"), { target: { value: "rust" } });
    await fireEvent.click(getByTestId("search-submit"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "search_server_notes",
        expect.objectContaining({
          sinceDate: Math.floor(from.getTime() / 1000),
          untilDate: Math.floor(to.getTime() / 1000),
        }),
      ),
    );
  });

  it("日時非対応のサーバーでは日時欄を出さず、非対応の注記を出す", async () => {
    mockServer({ dateRange: false });
    const { getByText, findByText, queryByText } = render(SearchModal, { props: { onclose: () => {} } });
    await fireEvent.click(getByText("サーバー"));
    await fireEvent.click(getByText(ADV_BASE));

    await findByText(NO_DATE_NOTE);
    expect(queryByText("日時（開始）")).toBeNull();
  });

  it("能力の取得に失敗したら日時欄も注記も出さない", async () => {
    mockServer({ capsFail: true });
    const { getByText, queryByText } = render(SearchModal, { props: { onclose: () => {} } });
    await fireEvent.click(getByText("サーバー"));
    await fireEvent.click(getByText(ADV_BASE));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("get_search_capabilities", { accountId: "acc1" }),
    );
    await new Promise((r) => setTimeout(r, 0)); // 失敗したPromiseの処理を流す
    expect(queryByText("日時（開始）")).toBeNull();
    expect(queryByText(NO_DATE_NOTE)).toBeNull();
  });

  it("検索対象を切り替えると結果がクリアされる", async () => {
    mockServer({ notes: [makeNote("n1", 100, "from server")] });
    const { getByText, queryByText, findByText, getByTestId, getByPlaceholderText } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    await fireEvent.click(getByText("サーバー"));
    await fireEvent.input(getByPlaceholderText("本文に含まれる語"), { target: { value: "rust" } });
    await fireEvent.click(getByTestId("search-submit"));
    await findByText("from server");

    await fireEvent.click(getByText("キャッシュ"));
    expect(queryByText("from server")).toBeNull();
  });

  it("サーバー検索中にアカウントを変えると結果がクリアされる", async () => {
    mockServer({ notes: [makeNote("n1", 100, "from server")] });
    const second: Account = { ...makeAccount(), id: "acc2", username: "carol" };
    app.accounts = [makeAccount(), second];
    const { getByText, queryByText, findByText, getByTestId, getByPlaceholderText } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    await fireEvent.click(getByText("サーバー"));
    await fireEvent.input(getByPlaceholderText("本文に含まれる語"), { target: { value: "rust" } });
    await fireEvent.click(getByTestId("search-submit"));
    await findByText("from server");

    await fireEvent.click(getByTestId("account-select-trigger"));
    await fireEvent.click(getByTestId("account-select-option-acc2"));
    await waitFor(() => expect(queryByText("from server")).toBeNull());
  });

  it("検索後に入力欄を書き換えても、追加読み込みは検索時点の条件で続ける", async () => {
    app.accounts = [makeAccount()];
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_search_capabilities") return Promise.resolve({ dateRange: false });
      if (cmd === "search_server_notes" && args?.untilId == null) {
        return Promise.resolve([makeNote("n1", 200, "page1")]);
      }
      if (cmd === "search_server_notes" && args?.untilId === "n1") {
        return Promise.resolve([makeNote("n2", 100, "page2")]);
      }
      return Promise.resolve([]);
    });
    const { getByText, getByTestId, getByPlaceholderText, findByText } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    await fireEvent.click(getByText("サーバー"));
    const keyword = getByPlaceholderText("本文に含まれる語");
    await fireEvent.input(keyword, { target: { value: "rust" } });
    await fireEvent.click(getByTestId("search-submit"));
    await findByText("page1");

    // Enterは押さずに欄だけ書き換える（空にしてもエラーにならず、元の条件で続くこと）
    await fireEvent.input(keyword, { target: { value: "" } });
    const list = document.querySelector('[data-testid="search-results-scroll"]') as HTMLElement;
    Object.defineProperty(list, "scrollTop", { value: 500, configurable: true });
    Object.defineProperty(list, "clientHeight", { value: 400, configurable: true });
    Object.defineProperty(list, "scrollHeight", { value: 1200, configurable: true });
    await fireEvent.scroll(list);

    await findByText("page2");
    expect(invokeMock).toHaveBeenCalledWith(
      "search_server_notes",
      expect.objectContaining({ query: "rust", untilId: "n1" }),
    );
  });
});
