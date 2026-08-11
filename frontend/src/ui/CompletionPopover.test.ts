import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import CompletionPopover from "./CompletionPopover.svelte";
import type { CompletionItem } from "../lib/mfmCompletion";

// jsdom は scrollIntoView を実装していないため、選択項目を追従スクロールさせる
// $effect がエラーにならないよう no-op スタブを補う。
beforeEach(() => {
  Element.prototype.scrollIntoView ??= () => {};
});

afterEach(() => cleanup());

const emojiItem: CompletionItem = {
  key: "custom:neko",
  label: "neko",
  insertText: ":neko:",
  thumbnail: { type: "custom", url: "https://example.com/neko.png" },
};
const unicodeItem: CompletionItem = {
  key: "unicode:grin",
  label: "grin",
  insertText: ":grin:",
  thumbnail: { type: "unicode", char: "😁" },
};
const textItem: CompletionItem = { key: "tada", label: "tada", insertText: "tada" };
const avatarItem: CompletionItem = {
  key: "user:1",
  label: "@alice",
  insertText: "@alice",
  thumbnail: { type: "avatar", url: "https://example.com/avatar.png" },
};

describe("CompletionPopover", () => {
  it("renders one row per item with its label", () => {
    const { getByText } = render(CompletionPopover, {
      props: { items: [emojiItem, textItem], selectedIndex: 0, left: 10, top: 20, onpick: () => {} },
    });
    expect(getByText("neko")).toBeTruthy();
    expect(getByText("tada")).toBeTruthy();
  });

  it("renders a thumbnail image for a custom emoji item", () => {
    const { getByRole } = render(CompletionPopover, {
      props: { items: [emojiItem], selectedIndex: 0, left: 0, top: 0, onpick: () => {} },
    });
    expect(getByRole("img").getAttribute("src")).toBe("https://example.com/neko.png");
  });

  it("renders a thumbnail image for an avatar item", () => {
    const { getByRole } = render(CompletionPopover, {
      props: { items: [avatarItem], selectedIndex: 0, left: 0, top: 0, onpick: () => {} },
    });
    expect(getByRole("img").getAttribute("src")).toBe("https://example.com/avatar.png");
  });

  it("renders the raw character for a unicode emoji item (no image)", () => {
    const { getByText, queryByRole } = render(CompletionPopover, {
      props: { items: [unicodeItem], selectedIndex: 0, left: 0, top: 0, onpick: () => {} },
    });
    expect(getByText("😁")).toBeTruthy();
    expect(queryByRole("img")).toBeNull();
  });

  it("marks the item at selectedIndex as selected", () => {
    const { getAllByRole } = render(CompletionPopover, {
      props: { items: [emojiItem, textItem], selectedIndex: 1, left: 0, top: 0, onpick: () => {} },
    });
    const options = getAllByRole("option");
    expect(options[0].getAttribute("aria-selected")).toBe("false");
    expect(options[1].getAttribute("aria-selected")).toBe("true");
  });

  it("calls onpick with the clicked item's index", async () => {
    const onpick = vi.fn();
    const { getAllByRole } = render(CompletionPopover, {
      props: { items: [emojiItem, textItem], selectedIndex: 0, left: 0, top: 0, onpick },
    });
    await fireEvent.mouseDown(getAllByRole("option")[1]);
    expect(onpick).toHaveBeenCalledWith(1);
  });

  it("prevents the default mousedown action so the textarea never loses focus", async () => {
    const { getAllByRole } = render(CompletionPopover, {
      props: { items: [emojiItem], selectedIndex: 0, left: 0, top: 0, onpick: () => {} },
    });
    const event = await fireEvent.mouseDown(getAllByRole("option")[0]);
    expect(event).toBe(false); // fireEventはpreventDefaultされたイベントでfalseを返す
  });

  it("positions itself using the left/top props", () => {
    // portal(→lib/portal.ts)がルート要素を document.body 直下へ移動するため、
    // render() の container ではなく baseElement(既定で document.body)側から探す。
    const { baseElement } = render(CompletionPopover, {
      props: { items: [textItem], selectedIndex: 0, left: 42, top: 99, onpick: () => {} },
    });
    const el = baseElement.querySelector('[data-testid="completion-popover"]') as HTMLElement;
    expect(el.style.left).toBe("42px");
    expect(el.style.top).toBe("99px");
  });
});
