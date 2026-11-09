import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import UserListsPanel from "./UserListsPanel.svelte";

afterEach(() => cleanup());

const LISTS = [
  { id: "L1", name: "友達", hasUser: true },
  { id: "L2", name: "仕事", hasUser: false },
];

function mount(overrides: Partial<{ load: () => Promise<typeof LISTS>; toggle: (id: string, add: boolean) => Promise<void> }> = {}) {
  const load = overrides.load ?? vi.fn().mockResolvedValue(LISTS);
  const toggle = overrides.toggle ?? vi.fn().mockResolvedValue(undefined);
  const onback = vi.fn();
  const r = render(UserListsPanel, { props: { load, toggle, onback } });
  return { ...r, load, toggle, onback };
}

describe("UserListsPanel", () => {
  it("開くとloadを呼び、取得中は「読み込み中…」を出す", async () => {
    let resolve!: (v: typeof LISTS) => void;
    const load = vi.fn(() => new Promise<typeof LISTS>((r) => (resolve = r)));
    const { getByText, findByRole } = mount({ load });
    expect(getByText("読み込み中…")).toBeTruthy();
    resolve(LISTS);
    expect(await findByRole("checkbox", { name: "友達" })).toBeTruthy();
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("入っているリストにはチェックが付き、入っていないリストには付かない", async () => {
    const { findByRole } = mount();
    expect((await findByRole("checkbox", { name: "友達" })).getAttribute("aria-checked")).toBe("true");
    expect((await findByRole("checkbox", { name: "仕事" })).getAttribute("aria-checked")).toBe("false");
  });

  it("入っていないリストを押すとtoggle(id, true)を呼び、完了後にチェックが付く", async () => {
    const { findByRole, toggle } = mount();
    const row = await findByRole("checkbox", { name: "仕事" });

    await fireEvent.click(row);

    expect(toggle).toHaveBeenCalledWith("L2", true);
    await waitFor(() => expect(row.getAttribute("aria-checked")).toBe("true"));
  });

  it("入っているリストを押すとtoggle(id, false)を呼び、完了後にチェックが外れる", async () => {
    const { findByRole, toggle } = mount();
    const row = await findByRole("checkbox", { name: "友達" });

    await fireEvent.click(row);

    expect(toggle).toHaveBeenCalledWith("L1", false);
    await waitFor(() => expect(row.getAttribute("aria-checked")).toBe("false"));
  });

  it("toggleが失敗したら、チェックは変えず、エラーを出し、パネルは開いたまま", async () => {
    const toggle = vi.fn().mockRejectedValue(new Error("boom"));
    const { findByRole, getByText } = mount({ toggle });
    const row = await findByRole("checkbox", { name: "仕事" });

    await fireEvent.click(row);

    await waitFor(() => expect(getByText(/boom/)).toBeTruthy());
    expect(row.getAttribute("aria-checked")).toBe("false");
  });

  it("他のクライアントで追加済みでALREADY_ADDEDになったら、チェックを付けて補正メッセージを出す", async () => {
    const toggle = vi.fn().mockRejectedValue(new Error("users/lists/push: ALREADY_ADDED"));
    const { findByRole, getByText, queryByText } = mount({ toggle });
    const row = await findByRole("checkbox", { name: "仕事" });

    await fireEvent.click(row);

    await waitFor(() => expect(row.getAttribute("aria-checked")).toBe("true"));
    expect(getByText("すでにリストに入っています。表示を更新しました")).toBeTruthy();
    expect(queryByText(/ALREADY_ADDED/)).toBeNull();
  });

  it("トグルの完了を待つ間は、同じ行を二重に押してもtoggleは1回だけ", async () => {
    let resolve!: () => void;
    const toggle = vi.fn(() => new Promise<void>((r) => (resolve = r)));
    const { findByRole } = mount({ toggle });
    const row = await findByRole("checkbox", { name: "仕事" });

    await fireEvent.click(row);
    await fireEvent.click(row);

    expect(toggle).toHaveBeenCalledTimes(1);
    resolve();
  });

  it("loadが失敗したらエラーと再試行を出し、再試行でもう一度loadを呼ぶ", async () => {
    const load = vi.fn().mockRejectedValueOnce(new Error("boom")).mockResolvedValueOnce(LISTS);
    const { findByText, getByRole, findByRole } = mount({ load });
    expect(await findByText(/boom/)).toBeTruthy();

    await fireEvent.click(getByRole("button", { name: "再試行" }));

    expect(await findByRole("checkbox", { name: "友達" })).toBeTruthy();
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("リストが0件なら「リストがありません」を出す", async () => {
    const { findByText } = mount({ load: vi.fn().mockResolvedValue([]) });
    expect(await findByText("リストがありません")).toBeTruthy();
  });

  it("「戻る」でonbackを呼ぶ", async () => {
    const { getByRole, onback } = mount();
    await fireEvent.click(getByRole("button", { name: "戻る" }));
    expect(onback).toHaveBeenCalledTimes(1);
  });
});
