import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import VisibilitySelect from "./VisibilitySelect.svelte";
import ReactionAcceptanceSelect from "./ReactionAcceptanceSelect.svelte";
import Dropdown from "./Dropdown.svelte";

// モバイルの投稿画面ではボタン群が画面下端(キーボード直上)に並ぶ。メニューが下向きに
// 出て画面外/キーボードの裏に隠れないよう、下に空きが無いときはアンカーの上に出す。
function rectAt(top: number, height = 30): DOMRect {
  return {
    top,
    bottom: top + height,
    left: 20,
    right: 120,
    width: 100,
    height,
    x: 20,
    y: top,
    toJSON: () => ({}),
  } as DOMRect;
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  document.documentElement.style.removeProperty("--keyboard-inset");
});

const menuStyle = (role: string) => (document.querySelector(`[role="${role}"]`) as HTMLElement).getAttribute("style") ?? "";

describe("セレクトのメニューは下に空きが無ければ上に出る", () => {
  it("VisibilitySelect: 画面下端のトリガーでは上に出る(bottom指定)", async () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rectAt(730));
    const { getByTitle } = render(VisibilitySelect, { props: { value: "public" } });
    await fireEvent.click(getByTitle(/公開範囲/));
    expect(menuStyle("listbox")).toMatch(/bottom:/);
    expect(menuStyle("listbox")).not.toMatch(/top:/);
  });

  it("VisibilitySelect: 上部のトリガーでは従来どおり下に出る(top指定)", async () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rectAt(40));
    const { getByTitle } = render(VisibilitySelect, { props: { value: "public" } });
    await fireEvent.click(getByTitle(/公開範囲/));
    expect(menuStyle("listbox")).toMatch(/top:/);
    expect(menuStyle("listbox")).not.toMatch(/bottom:/);
  });

  it("VisibilitySelect: キーボードが下を覆っているぶんも考慮して上に出る", async () => {
    document.documentElement.style.setProperty("--keyboard-inset", "300px");
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rectAt(420)); // 見える下端 768-300=468
    const { getByTitle } = render(VisibilitySelect, { props: { value: "public" } });
    await fireEvent.click(getByTitle(/公開範囲/));
    expect(menuStyle("listbox")).toMatch(/bottom:/);
  });

  it("ReactionAcceptanceSelect: 画面下端のトリガーでは上に出る", async () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rectAt(730));
    const { getByRole } = render(ReactionAcceptanceSelect, { props: { value: "all" } });
    await fireEvent.click(getByRole("button"));
    expect(menuStyle("listbox")).toMatch(/bottom:/);
    expect(menuStyle("listbox")).not.toMatch(/top:/);
  });

  it("Dropdown: 画面下端のトリガーでは上に出る", async () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rectAt(730));
    const { getByTestId } = render(Dropdown, {
      props: { value: "a", options: [{ value: "a", label: "A" }, { value: "b", label: "B" }], testid: "dd" },
    });
    await fireEvent.click(getByTestId("dd"));
    expect(menuStyle("listbox")).toMatch(/bottom:/);
    expect(menuStyle("listbox")).not.toMatch(/top:/);
  });
});
