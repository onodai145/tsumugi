import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import type { LocalScheduledNote, LocalScheduleStatus, ScheduledNote } from "../bindings/tauri.gen";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

const { default: ScheduledModal } = await import("./ScheduledModal.svelte");

function note(id: string, over: Partial<ScheduledNote> = {}): ScheduledNote {
  return {
    id,
    scheduledAt: Math.floor(Date.now() / 1000) + 3600,
    text: `本文${id}`,
    cw: null,
    visibility: "public",
    localOnly: false,
    reactionAcceptance: "all",
    channelId: null,
    poll: null,
    fileIds: [],
    replyNote: null,
    quoteNote: null,
    ...over,
  };
}

function local(id: string, status: LocalScheduleStatus = "pending", over: Partial<ScheduledNote> = {}, error: string | null = null): LocalScheduledNote {
  return { note: note(id, over), status, error };
}

/// コマンドごとに返す値を決める。指定の無いコマンドは null(書き込み系は成功)。
function mockCommands(handlers: Record<string, (args?: unknown) => unknown>) {
  invokeMock.mockImplementation((cmd: string, args?: unknown) => {
    const h = handlers[cmd];
    if (!h) return Promise.resolve(cmd.startsWith("list_") ? [] : null);
    try {
      return Promise.resolve(h(args));
    } catch (e) {
      return Promise.reject(e);
    }
  });
}

const props = (over: Record<string, unknown> = {}) => ({
  accountId: "acc1",
  serverSide: true,
  reloadToken: 0,
  onrestore: vi.fn(),
  onclose: vi.fn(),
  ...over,
});

beforeEach(() => {
  // 式本体にすると mock 自体が返り、vitest が beforeEach の返り値をクリーンアップ関数として呼んでしまう
  invokeMock.mockReset();
});
afterEach(() => cleanup());

describe("ScheduledModal(サーバー予約)", () => {
  it("予約を予約日時の昇順で表示し、過去のものは「投稿に失敗」と明示する", async () => {
    const now = Math.floor(Date.now() / 1000);
    mockCommands({
      list_scheduled_notes: () => [
        note("later", { scheduledAt: now + 7200 }),
        note("failed", { scheduledAt: now - 600 }),
        note("soon", { scheduledAt: now + 600 }),
      ],
    });
    const { findByTestId, container, queryByTestId } = render(ScheduledModal, props());
    await findByTestId("scheduled-item-soon");
    const order = [...container.ownerDocument.querySelectorAll("[data-testid^='scheduled-item-']")].map((e) =>
      e.getAttribute("data-testid"),
    );
    expect(order).toEqual(["scheduled-item-failed", "scheduled-item-soon", "scheduled-item-later"]);
    expect(queryByTestId("scheduled-failed-failed")).not.toBeNull();
    expect(queryByTestId("scheduled-failed-soon")).toBeNull();
    expect(invokeMock).toHaveBeenCalledWith("list_scheduled_notes", { accountId: "acc1", untilId: null, limit: 30 });
  });

  it("予約が無いときは空表示", async () => {
    mockCommands({});
    const { findByTestId } = render(ScheduledModal, props());
    expect(await findByTestId("scheduled-empty")).toBeTruthy();
  });

  it("取り消しで cancel_scheduled_note を呼び、一覧から消す", async () => {
    mockCommands({ list_scheduled_notes: () => [note("a"), note("b")] });
    const { findByTestId, getByTestId, queryByTestId } = render(ScheduledModal, props());
    await fireEvent.click(await findByTestId("scheduled-cancel-a"));
    await waitFor(() => expect(queryByTestId("scheduled-item-a")).toBeNull());
    expect(invokeMock).toHaveBeenCalledWith("cancel_scheduled_note", { accountId: "acc1", draftId: "a" });
    expect(getByTestId("scheduled-item-b")).toBeTruthy();
  });

  it("取り消しに失敗したら一覧に残し、エラーを表示する", async () => {
    mockCommands({
      list_scheduled_notes: () => [note("a")],
      cancel_scheduled_note: () => {
        throw { kind: "network", message: "offline" };
      },
    });
    const { findByTestId, findByText, getByTestId } = render(ScheduledModal, props());
    await fireEvent.click(await findByTestId("scheduled-cancel-a"));
    expect(await findByText(/offline/)).toBeTruthy();
    expect(getByTestId("scheduled-item-a")).toBeTruthy();
  });

  it("「作成欄に戻す」で onrestore にサーバー予約として渡す", async () => {
    const a = note("a");
    mockCommands({ list_scheduled_notes: () => [a] });
    const p = props();
    const { findByTestId } = render(ScheduledModal, p);
    await fireEvent.click(await findByTestId("scheduled-restore-a"));
    expect(p.onrestore).toHaveBeenCalledWith({ origin: "server", note: a });
  });

  it("1ページ分(30件)返ったら「さらに読み込む」を出し、最後の ID をカーソルに続きを取る", async () => {
    const page1 = Array.from({ length: 30 }, (_, i) => note(`p1_${i}`));
    let call = 0;
    mockCommands({ list_scheduled_notes: () => (call++ === 0 ? page1 : [note("p2_0")]) });
    const { findByTestId, queryByTestId } = render(ScheduledModal, props());
    await fireEvent.click(await findByTestId("scheduled-more"));
    await findByTestId("scheduled-item-p2_0");
    expect(invokeMock).toHaveBeenCalledWith("list_scheduled_notes", { accountId: "acc1", untilId: "p1_29", limit: 30 });
    expect(queryByTestId("scheduled-more")).toBeNull();
  });

  it("1ページ目の取得に失敗したら、空表示は出さずエラーと再読み込みを出す", async () => {
    mockCommands({
      list_scheduled_notes: () => {
        throw { kind: "network", message: "offline" };
      },
    });
    const { findByText, findByTestId, queryByTestId } = render(ScheduledModal, props());
    expect(await findByText(/offline/)).toBeTruthy();
    expect(await findByTestId("scheduled-retry")).toBeTruthy();
    // 「予約なし」と誤解して二重に予約しないよう、空表示は出さない
    expect(queryByTestId("scheduled-empty")).toBeNull();
  });

  it("再読み込みに成功すると一覧が出てエラーが消える", async () => {
    let call = 0;
    mockCommands({
      list_scheduled_notes: () => {
        if (call++ === 0) throw { kind: "network", message: "offline" };
        return [note("a")];
      },
    });
    const { findByTestId, queryByText, queryByTestId } = render(ScheduledModal, props());
    await fireEvent.click(await findByTestId("scheduled-retry"));
    await findByTestId("scheduled-item-a");
    expect(queryByText(/offline/)).toBeNull();
    expect(queryByTestId("scheduled-retry")).toBeNull();
  });

  it("serverSide が false のときはサーバー予約を取得しない", async () => {
    mockCommands({ list_local_scheduled_notes: () => [local("l1")] });
    const { findByTestId } = render(ScheduledModal, props({ serverSide: false }));
    await findByTestId("scheduled-item-l1");
    expect(invokeMock).not.toHaveBeenCalledWith("list_scheduled_notes", expect.anything());
  });
});

describe("ScheduledModal(ローカル予約)", () => {
  it("サーバー予約とローカル予約を、予約日時の昇順で 1 つの一覧にする", async () => {
    const now = Math.floor(Date.now() / 1000);
    mockCommands({
      list_scheduled_notes: () => [note("s1", { scheduledAt: now + 300 })],
      list_local_scheduled_notes: () => [local("l1", "pending", { scheduledAt: now + 100 }), local("l2", "pending", { scheduledAt: now + 900 })],
    });
    const { findByTestId, container } = render(ScheduledModal, props());
    await findByTestId("scheduled-item-l2");
    const order = [...container.ownerDocument.querySelectorAll("[data-testid^='scheduled-item-']")].map((e) =>
      e.getAttribute("data-testid"),
    );
    expect(order).toEqual(["scheduled-item-l1", "scheduled-item-s1", "scheduled-item-l2"]);
  });

  it("待機中のローカル予約には注意書きを出し、状態ラベルは出さない", async () => {
    mockCommands({ list_local_scheduled_notes: () => [local("l1", "pending")] });
    const { findByTestId, queryByTestId } = render(ScheduledModal, props({ serverSide: false }));
    const notice = await findByTestId("scheduled-local-notice-l1");
    expect(notice.textContent).toContain("アプリを起動している間だけ");
    expect(queryByTestId("scheduled-local-status-l1")).toBeNull();
  });

  it("期限切れ・失敗は状態と理由を出し、「今すぐ投稿」「作成欄に戻す」「取り消し」を出す", async () => {
    mockCommands({
      list_local_scheduled_notes: () => [
        local("e1", "expired"),
        local("f1", "failed", {}, "通信がタイムアウトしました。投稿されたか確認してください"),
      ],
    });
    const { findByTestId, getByTestId } = render(ScheduledModal, props({ serverSide: false }));
    await findByTestId("scheduled-item-e1");
    expect(getByTestId("scheduled-local-status-e1").textContent).toContain("期限切れ");
    expect(getByTestId("scheduled-local-status-f1").textContent).toContain("投稿に失敗");
    expect(getByTestId("scheduled-item-f1").textContent).toContain("タイムアウト");
    for (const id of ["e1", "f1"]) {
      expect(getByTestId(`scheduled-run-now-${id}`)).toBeTruthy();
      expect(getByTestId(`scheduled-restore-${id}`)).toBeTruthy();
      expect(getByTestId(`scheduled-cancel-${id}`)).toBeTruthy();
    }
  });

  // Review Focus 2: 送信中の予約は、取り消しも今すぐ投稿も作成欄に戻すもできない
  it("投稿中のローカル予約は操作できない", async () => {
    mockCommands({ list_local_scheduled_notes: () => [local("p1", "posting")] });
    const { findByTestId, getByTestId, queryByTestId } = render(ScheduledModal, props({ serverSide: false }));
    await findByTestId("scheduled-item-p1");
    expect(getByTestId("scheduled-local-status-p1").textContent).toContain("投稿中");
    for (const kind of ["run-now", "restore", "cancel"]) {
      expect(queryByTestId(`scheduled-${kind}-p1`)).toBeNull();
    }
  });

  it("待機中のローカル予約には「今すぐ投稿」を出さない", async () => {
    mockCommands({ list_local_scheduled_notes: () => [local("l1", "pending")] });
    const { findByTestId, queryByTestId } = render(ScheduledModal, props({ serverSide: false }));
    await findByTestId("scheduled-item-l1");
    expect(queryByTestId("scheduled-run-now-l1")).toBeNull();
  });

  it("ローカル予約の取り消しは cancel_local_scheduled_note を呼び、一覧から消す", async () => {
    mockCommands({ list_local_scheduled_notes: () => [local("l1"), local("l2")] });
    const { findByTestId, queryByTestId, getByTestId } = render(ScheduledModal, props({ serverSide: false }));
    await fireEvent.click(await findByTestId("scheduled-cancel-l1"));
    await waitFor(() => expect(queryByTestId("scheduled-item-l1")).toBeNull());
    expect(invokeMock).toHaveBeenCalledWith("cancel_local_scheduled_note", { accountId: "acc1", id: "l1" });
    expect(invokeMock).not.toHaveBeenCalledWith("cancel_scheduled_note", expect.anything());
    expect(getByTestId("scheduled-item-l2")).toBeTruthy();
  });

  it("「今すぐ投稿」で post_local_scheduled_now を呼び、ローカル一覧を読み直す", async () => {
    let listed = 0;
    mockCommands({
      list_local_scheduled_notes: () => (listed++ === 0 ? [local("e1", "expired")] : []),
    });
    const { findByTestId, queryByTestId } = render(ScheduledModal, props({ serverSide: false }));
    await fireEvent.click(await findByTestId("scheduled-run-now-e1"));
    expect(invokeMock).toHaveBeenCalledWith("post_local_scheduled_now", { accountId: "acc1", id: "e1" });
    await waitFor(() => expect(queryByTestId("scheduled-item-e1")).toBeNull());
  });

  it("「今すぐ投稿」に失敗したらエラーを出し、一覧には残す", async () => {
    mockCommands({
      list_local_scheduled_notes: () => [local("e1", "expired")],
      post_local_scheduled_now: () => {
        throw { kind: "invalid", message: "投稿処理中です" };
      },
    });
    const { findByTestId, findByText, getByTestId } = render(ScheduledModal, props({ serverSide: false }));
    await fireEvent.click(await findByTestId("scheduled-run-now-e1"));
    expect(await findByText(/投稿処理中です/)).toBeTruthy();
    expect(getByTestId("scheduled-item-e1")).toBeTruthy();
  });

  it("「作成欄に戻す」で onrestore にローカル予約として渡す", async () => {
    const l = local("l1", "expired", {}, null);
    mockCommands({ list_local_scheduled_notes: () => [l] });
    const p = props({ serverSide: false });
    const { findByTestId } = render(ScheduledModal, p);
    await fireEvent.click(await findByTestId("scheduled-restore-l1"));
    expect(p.onrestore).toHaveBeenCalledWith({ origin: "local", note: l.note, status: "expired", error: null });
  });

  it("reloadToken が変わるとローカル予約を読み直す(サーバー予約は読み直さない)", async () => {
    let listed = 0;
    mockCommands({
      list_scheduled_notes: () => [note("s1")],
      list_local_scheduled_notes: () => (listed++ === 0 ? [local("l1", "pending")] : [local("l1", "failed", {}, "理由")]),
    });
    const { findByTestId, rerender, getByTestId } = render(ScheduledModal, props());
    await findByTestId("scheduled-item-l1");
    await rerender(props({ reloadToken: 1 }));
    await waitFor(() => expect(getByTestId("scheduled-local-status-l1").textContent).toContain("投稿に失敗"));
    const serverCalls = invokeMock.mock.calls.filter((c) => c[0] === "list_scheduled_notes").length;
    expect(serverCalls).toBe(1);
  });

  it("ローカルだけ取得に失敗したときも、エラーを出して空表示にはしない", async () => {
    mockCommands({
      list_local_scheduled_notes: () => {
        throw { kind: "db", message: "broken" };
      },
    });
    const { findByText, findByTestId, queryByTestId } = render(ScheduledModal, props({ serverSide: false }));
    expect(await findByText(/broken/)).toBeTruthy();
    expect(await findByTestId("scheduled-retry")).toBeTruthy();
    expect(queryByTestId("scheduled-empty")).toBeNull();
  });
});
