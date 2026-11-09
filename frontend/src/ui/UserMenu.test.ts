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

describe("UserMenu のその他の項目(Issue #546)", () => {
  const base = { muted: false, onmute: vi.fn(), onunmute: vi.fn() };

  it("新しいpropsを渡さなければ、ミュートだけを出す(従来どおり)", () => {
    const { queryByRole } = render(UserMenu, { props: base });
    for (const name of ["リノートをミュート", "ブロック", "リストに追加", "ブラウザで開く", "通報"]) {
      expect(queryByRole("button", { name }), name).toBeNull();
    }
  });

  it("renoteMutedがnullなら「リノートをミュート」を出さない", () => {
    const { queryByRole } = render(UserMenu, { props: { ...base, renoteMuted: null, onrenotemute: vi.fn() } });
    expect(queryByRole("button", { name: "リノートをミュート" })).toBeNull();
  });

  it("リノートをミュートしていなければ「リノートをミュート」を出し、押すとonrenotemuteを呼ぶ", async () => {
    const onrenotemute = vi.fn();
    const { getByRole, queryByRole } = render(UserMenu, {
      props: { ...base, renoteMuted: false, onrenotemute, onunrenotemute: vi.fn() },
    });
    expect(queryByRole("button", { name: "リノートのミュートを解除" })).toBeNull();

    await fireEvent.click(getByRole("button", { name: "リノートをミュート" }));

    expect(onrenotemute).toHaveBeenCalledTimes(1);
  });

  it("リノートをミュート中なら「リノートのミュートを解除」を出し、押すとonunrenotemuteを呼ぶ", async () => {
    const onunrenotemute = vi.fn();
    const { getByRole, queryByRole } = render(UserMenu, {
      props: { ...base, renoteMuted: true, onrenotemute: vi.fn(), onunrenotemute },
    });
    expect(queryByRole("button", { name: "リノートをミュート" })).toBeNull();

    await fireEvent.click(getByRole("button", { name: "リノートのミュートを解除" }));

    expect(onunrenotemute).toHaveBeenCalledTimes(1);
  });

  it("ブロックしていなければ「ブロック」を押すとonblockを呼ぶ(確認は呼び出し側)", async () => {
    const onblock = vi.fn();
    const { getByRole } = render(UserMenu, { props: { ...base, blocking: false, onblock, onunblock: vi.fn() } });

    await fireEvent.click(getByRole("button", { name: "ブロック" }));

    expect(onblock).toHaveBeenCalledTimes(1);
  });

  it("ブロック中なら「ブロック解除」を出し、押すとonunblockを呼ぶ", async () => {
    const onunblock = vi.fn();
    const { getByRole, queryByRole } = render(UserMenu, {
      props: { ...base, blocking: true, onblock: vi.fn(), onunblock },
    });
    expect(queryByRole("button", { name: "ブロック" })).toBeNull();

    await fireEvent.click(getByRole("button", { name: "ブロック解除" }));

    expect(onunblock).toHaveBeenCalledTimes(1);
  });

  it("blockingがnullなら「ブロック」を出さない", () => {
    const { queryByRole } = render(UserMenu, { props: { ...base, blocking: null, onblock: vi.fn() } });
    expect(queryByRole("button", { name: "ブロック" })).toBeNull();
  });

  it("「リストに追加」を押すとパネルに切り替わり、loadを呼ぶ。「戻る」で最初の表示に戻る", async () => {
    const loadLists = vi.fn().mockResolvedValue([{ id: "L1", name: "友達", hasUser: false }]);
    const { getByRole, findByRole, queryByRole } = render(UserMenu, {
      props: { ...base, loadLists, togglelist: vi.fn() },
    });
    expect(loadLists).not.toHaveBeenCalled(); // 開くまでは取得しない

    await fireEvent.click(getByRole("button", { name: "リストに追加" }));

    expect(await findByRole("checkbox", { name: "友達" })).toBeTruthy();
    expect(loadLists).toHaveBeenCalledTimes(1);

    await fireEvent.click(getByRole("button", { name: "戻る" }));
    expect(getByRole("button", { name: "リストに追加" })).toBeTruthy();
    expect(queryByRole("checkbox", { name: "友達" })).toBeNull();
  });

  it("loadListsとtogglelistのどちらかが無ければ「リストに追加」を出さない", () => {
    const { queryByRole } = render(UserMenu, { props: { ...base, loadLists: vi.fn() } });
    expect(queryByRole("button", { name: "リストに追加" })).toBeNull();
  });

  it("profileUrlがあれば「ブラウザで開く」を出し、押すとonopenを呼ぶ。無ければ出さない", async () => {
    const onopen = vi.fn();
    const a = render(UserMenu, { props: { ...base, profileUrl: "https://remote.example/@alice", onopen } });
    await fireEvent.click(a.getByRole("button", { name: "ブラウザで開く" }));
    expect(onopen).toHaveBeenCalledTimes(1);
    cleanup();

    const b = render(UserMenu, { props: { ...base, profileUrl: null, onopen } });
    expect(b.queryByRole("button", { name: "ブラウザで開く" })).toBeNull();
  });

  it("「通報」を押すとonreportを呼ぶ", async () => {
    const onreport = vi.fn();
    const { getByRole } = render(UserMenu, { props: { ...base, onreport } });
    await fireEvent.click(getByRole("button", { name: "通報" }));
    expect(onreport).toHaveBeenCalledTimes(1);
  });

  it("mutedがnull(サーバーが返さない)なら、ミュート関連の項目は出さず、他の項目は出す", () => {
    const { queryByRole, getByRole } = render(UserMenu, {
      props: { muted: null, onmute: vi.fn(), onunmute: vi.fn(), blocking: false, onreport: vi.fn() },
    });
    expect(queryByRole("button", { name: "ミュート" })).toBeNull();
    expect(queryByRole("button", { name: "ミュート解除" })).toBeNull();
    expect(getByRole("button", { name: "ブロック" })).toBeTruthy();
    expect(getByRole("button", { name: "通報" })).toBeTruthy();
  });
});
