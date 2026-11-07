import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import UserMenu from "./UserMenu.svelte";

afterEach(() => cleanup());

describe("UserMenu", () => {
  it("未ミュートなら「ミュート」だけを出し、押すと期間の一覧に切り替わる", async () => {
    const { getByRole, queryByRole } = render(UserMenu, {
      props: { muted: false, onmute: vi.fn(), onunmute: vi.fn() },
    });
    expect(queryByRole("button", { name: "ミュート解除" })).toBeNull();
    expect(queryByRole("button", { name: "1時間" })).toBeNull();

    await fireEvent.click(getByRole("button", { name: "ミュート" }));

    for (const label of ["無期限", "10分", "1時間", "1日", "1週間"]) {
      expect(getByRole("button", { name: label })).toBeTruthy();
    }
  });

  it("期間を押すと、その期間でonmuteを呼ぶ", async () => {
    const onmute = vi.fn();
    const { getByRole } = render(UserMenu, { props: { muted: false, onmute, onunmute: vi.fn() } });
    await fireEvent.click(getByRole("button", { name: "ミュート" }));

    await fireEvent.click(getByRole("button", { name: "1時間" }));

    expect(onmute).toHaveBeenCalledWith("oneHour");
  });

  it.each([
    ["無期限", "indefinitely"],
    ["10分", "tenMinutes"],
    ["1日", "oneDay"],
    ["1週間", "oneWeek"],
  ])("「%s」は %s を渡す", async (label, value) => {
    const onmute = vi.fn();
    const { getByRole } = render(UserMenu, { props: { muted: false, onmute, onunmute: vi.fn() } });
    await fireEvent.click(getByRole("button", { name: "ミュート" }));

    await fireEvent.click(getByRole("button", { name: label }));

    expect(onmute).toHaveBeenCalledWith(value);
  });

  it("「戻る」で最初の表示に戻る", async () => {
    const { getByRole, queryByRole } = render(UserMenu, {
      props: { muted: false, onmute: vi.fn(), onunmute: vi.fn() },
    });
    await fireEvent.click(getByRole("button", { name: "ミュート" }));

    await fireEvent.click(getByRole("button", { name: "戻る" }));

    expect(getByRole("button", { name: "ミュート" })).toBeTruthy();
    expect(queryByRole("button", { name: "1時間" })).toBeNull();
  });

  it("ミュート中は「ミュート解除」だけを出し、押すとonunmuteを呼ぶ", async () => {
    const onunmute = vi.fn();
    const { getByRole, queryByRole } = render(UserMenu, {
      props: { muted: true, onmute: vi.fn(), onunmute },
    });
    expect(queryByRole("button", { name: "ミュート" })).toBeNull();

    await fireEvent.click(getByRole("button", { name: "ミュート解除" }));

    expect(onunmute).toHaveBeenCalledTimes(1);
  });
});
