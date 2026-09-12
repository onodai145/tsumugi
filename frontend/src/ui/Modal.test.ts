import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { createRawSnippet } from "svelte";
import Modal from "./Modal.svelte";

function textSnippet(text: string) {
  return createRawSnippet(() => ({
    render: () => `<span>${text}</span>`,
  }));
}

afterEach(() => {
  cleanup();
});

describe("Modal", () => {
  it("タイトルを表示し、×ボタンでoncloseを呼ぶ", async () => {
    const onclose = vi.fn();
    const { getByText, getByRole } = render(Modal, {
      props: { title: "設定", onclose, children: textSnippet("hi") },
    });
    expect(getByText("設定")).toBeTruthy();
    expect(getByText("hi")).toBeTruthy();
    await fireEvent.click(getByRole("button"));
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("Escapeキーでoncloseを呼ぶ", async () => {
    const onclose = vi.fn();
    const { getByRole } = render(Modal, {
      props: { title: "設定", onclose, children: textSnippet("hi") },
    });
    await fireEvent.keyDown(getByRole("presentation"), { key: "Escape" });
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("オーバーレイクリックでoncloseを呼ぶが、ダイアログ内クリックでは呼ばない", async () => {
    const onclose = vi.fn();
    const { getByRole, getByText } = render(Modal, {
      props: { title: "設定", onclose, children: textSnippet("hi") },
    });
    await fireEvent.click(getByText("hi"));
    expect(onclose).not.toHaveBeenCalled();
    await fireEvent.click(getByRole("presentation"));
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("maxHeight未指定時はダイアログにmax-heightもflex-colも付かない", () => {
    const { getByRole } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi") },
    });
    const dialog = getByRole("dialog");
    expect(dialog.style.maxHeight).toBe("");
    expect(dialog.className).not.toMatch(/flex-col/);
  });

  it("maxHeight指定時はダイアログがflex-colになり、max-heightが設定され、childrenがflex-1 min-h-0でラップされる", () => {
    const { getByRole, getByText } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi"), maxHeight: "78vh" },
    });
    const dialog = getByRole("dialog");
    expect(dialog.style.maxHeight).toBe("78vh");
    expect(dialog.className).toMatch(/flex-col/);
    const wrapper = getByText("hi").closest("div");
    expect(wrapper?.className).toMatch(/flex-1/);
    expect(wrapper?.className).toMatch(/min-h-0/);
  });
});
