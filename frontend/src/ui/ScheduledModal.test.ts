import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import type { ScheduledNote } from "../bindings/tauri.gen";

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

beforeEach(() => {
  // 式本体にすると mock 自体が返り、vitest が beforeEach の返り値をクリーンアップ関数として呼んでしまう
  invokeMock.mockReset();
});
afterEach(() => cleanup());

describe("ScheduledModal", () => {
  it("予約を予約日時の昇順で表示し、過去のものは「投稿に失敗」と明示する", async () => {
    const now = Math.floor(Date.now() / 1000);
    invokeMock.mockResolvedValue([
      note("later", { scheduledAt: now + 7200 }),
      note("failed", { scheduledAt: now - 600 }),
      note("soon", { scheduledAt: now + 600 }),
    ]);
    const { findByTestId, container, queryByTestId } = render(ScheduledModal, {
      accountId: "acc1",
      onrestore: vi.fn(),
      onclose: vi.fn(),
    });
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
    invokeMock.mockResolvedValue([]);
    const { findByTestId } = render(ScheduledModal, { accountId: "acc1", onrestore: vi.fn(), onclose: vi.fn() });
    expect(await findByTestId("scheduled-empty")).toBeTruthy();
  });

  it("取り消しで cancel_scheduled_note を呼び、一覧から消す", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "list_scheduled_notes" ? Promise.resolve([note("a"), note("b")]) : Promise.resolve(null),
    );
    const { findByTestId, getByTestId, queryByTestId } = render(ScheduledModal, {
      accountId: "acc1",
      onrestore: vi.fn(),
      onclose: vi.fn(),
    });
    await fireEvent.click(await findByTestId("scheduled-cancel-a"));
    await waitFor(() => expect(queryByTestId("scheduled-item-a")).toBeNull());
    expect(invokeMock).toHaveBeenCalledWith("cancel_scheduled_note", { accountId: "acc1", draftId: "a" });
    expect(getByTestId("scheduled-item-b")).toBeTruthy();
  });

  it("取り消しに失敗したら一覧に残し、エラーを表示する", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_scheduled_notes") return Promise.resolve([note("a")]);
      return Promise.reject({ kind: "network", message: "offline" });
    });
    const { findByTestId, findByText, getByTestId } = render(ScheduledModal, {
      accountId: "acc1",
      onrestore: vi.fn(),
      onclose: vi.fn(),
    });
    await fireEvent.click(await findByTestId("scheduled-cancel-a"));
    expect(await findByText(/offline/)).toBeTruthy();
    expect(getByTestId("scheduled-item-a")).toBeTruthy();
  });

  it("「作成欄に戻す」で onrestore にその予約を渡す", async () => {
    const a = note("a");
    invokeMock.mockResolvedValue([a]);
    const onrestore = vi.fn();
    const { findByTestId } = render(ScheduledModal, { accountId: "acc1", onrestore, onclose: vi.fn() });
    await fireEvent.click(await findByTestId("scheduled-restore-a"));
    expect(onrestore).toHaveBeenCalledWith(a);
  });

  it("1ページ分(30件)返ったら「さらに読み込む」を出し、最後の ID をカーソルに続きを取る", async () => {
    const page1 = Array.from({ length: 30 }, (_, i) => note(`p1_${i}`));
    invokeMock.mockResolvedValueOnce(page1).mockResolvedValueOnce([note("p2_0")]);
    const { findByTestId, queryByTestId } = render(ScheduledModal, {
      accountId: "acc1",
      onrestore: vi.fn(),
      onclose: vi.fn(),
    });
    await fireEvent.click(await findByTestId("scheduled-more"));
    await findByTestId("scheduled-item-p2_0");
    expect(invokeMock).toHaveBeenLastCalledWith("list_scheduled_notes", {
      accountId: "acc1",
      untilId: "p1_29",
      limit: 30,
    });
    // 2ページ目は30件未満なので、もう「さらに読み込む」は出ない
    expect(queryByTestId("scheduled-more")).toBeNull();
  });
});
