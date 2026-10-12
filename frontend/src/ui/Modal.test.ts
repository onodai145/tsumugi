import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { createRawSnippet, tick } from "svelte";

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

const { default: Modal } = await import("./Modal.svelte");
const { default: ModalFocusHost } = await import("./ModalFocusHost.test.host.svelte");
const { app } = await import("../lib/store.svelte");

const settle = () => new Promise<void>((r) => setTimeout(r, 20));

function textSnippet(text: string) {
  return createRawSnippet(() => ({
    render: () => `<span>${text}</span>`,
  }));
}

afterEach(async () => {
  cleanup();
  app.ui = { ...app.ui, uiMode: "auto" };
  // 全画面モーダルのhistory.go()によるpopstateを次のテストに持ち越さない
  await settle();
  vi.restoreAllMocks();
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

describe("Modal モバイル全画面(Issue #450)", () => {
  function mobile() {
    app.ui = { ...app.ui, uiMode: "mobile" };
  }

  it("全画面レイアウトになり、黒オーバーレイ・角丸の箱ではない", async () => {
    mobile();
    const { getByRole } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi") },
    });
    await tick();
    const overlay = getByRole("presentation");
    expect(overlay.className).toMatch(/bg-background/);
    expect(overlay.className).not.toMatch(/bg-black/);
    const dialog = getByRole("dialog");
    expect(dialog.className).toMatch(/h-full/);
    expect(dialog.className).not.toMatch(/rounded-xl/);
  });

  it("width/maxHeightは無視される", async () => {
    mobile();
    const { getByRole } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi"), width: "640px", maxHeight: "78vh" },
    });
    await tick();
    const dialog = getByRole("dialog");
    expect(dialog.style.maxHeight).toBe("");
    expect(dialog.style.getPropertyValue("--modal-w")).toBe("");
  });

  it("背景クリックでは閉じず、戻るボタンとEscapeでは閉じる", async () => {
    mobile();
    const onclose = vi.fn();
    const { getByRole, getByTestId } = render(Modal, {
      props: { title: "設定", onclose, children: textSnippet("hi") },
    });
    await tick();
    await fireEvent.click(getByRole("presentation"));
    expect(onclose).not.toHaveBeenCalled();
    await fireEvent.keyDown(getByRole("presentation"), { key: "Escape" });
    expect(onclose).toHaveBeenCalledTimes(1);
    await fireEvent.click(getByTestId("modal-close"));
    expect(onclose).toHaveBeenCalledTimes(2);
  });

  it("開くと履歴を1つ積み、戻る操作でoncloseが呼ばれる", async () => {
    mobile();
    const push = vi.spyOn(history, "pushState");
    const onclose = vi.fn();
    render(Modal, { props: { title: "設定", onclose, children: textSnippet("hi") } });
    await tick();
    expect(push).toHaveBeenCalledTimes(1);
    history.back();
    await settle();
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  it("アンマウントで積んだ履歴を戻す", async () => {
    mobile();
    const go = vi.spyOn(history, "go");
    const onclose = vi.fn();
    const { unmount } = render(Modal, { props: { title: "設定", onclose, children: textSnippet("hi") } });
    await tick();
    unmount();
    await Promise.resolve();
    expect(go).toHaveBeenCalledTimes(1);
    expect(go).toHaveBeenCalledWith(-1);
    await settle();
    expect(onclose).not.toHaveBeenCalled();
  });

  it("開いたままUIモードをPC版に切り替えると履歴を解放し箱型に戻る", async () => {
    mobile();
    const go = vi.spyOn(history, "go");
    const { getByRole } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi") },
    });
    await tick();
    app.ui = { ...app.ui, uiMode: "desktop" };
    await tick();
    await Promise.resolve();
    expect(go).toHaveBeenCalledTimes(1);
    expect(getByRole("dialog").className).toMatch(/rounded-xl/);
  });

  it("PC⇄モバイルを切り替えてもchildrenのDOMは作り直されない(設定の保存表示などが消えない)", async () => {
    const { getByText } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi") },
    });
    await tick();
    const before = getByText("hi");
    mobile();
    await tick();
    expect(getByText("hi")).toBe(before);
  });

  it("maxHeight指定でも切り替えでchildrenのDOMは作り直されない", async () => {
    const { getByText } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi"), maxHeight: "78vh" },
    });
    await tick();
    const before = getByText("hi");
    mobile();
    await tick();
    expect(getByText("hi")).toBe(before);
  });

  it("compactはモバイルでも箱型で、履歴に触れない", async () => {
    mobile();
    const push = vi.spyOn(history, "pushState");
    const go = vi.spyOn(history, "go");
    const onclose = vi.fn();
    const { getByRole, unmount } = render(Modal, {
      props: { title: "エラー", onclose, children: textSnippet("hi"), compact: true },
    });
    await tick();
    expect(getByRole("dialog").className).toMatch(/rounded-xl/);
    await fireEvent.click(getByRole("presentation"));
    expect(onclose).toHaveBeenCalledTimes(1);
    unmount();
    await Promise.resolve();
    expect(push).not.toHaveBeenCalled();
    expect(go).not.toHaveBeenCalled();
  });

  it("全画面モーダルの上でcompactを開閉しても下の履歴は壊れない", async () => {
    mobile();
    const go = vi.spyOn(history, "go");
    const base = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("base") },
    });
    await tick();
    const err = render(Modal, {
      props: { title: "エラー", onclose: () => {}, children: textSnippet("err"), compact: true },
    });
    await tick();
    err.unmount();
    await Promise.resolve();
    expect(go).not.toHaveBeenCalled();
    base.unmount();
    await Promise.resolve();
    expect(go).toHaveBeenCalledTimes(1);
  });

  it("全画面本文の下余白にセーフエリアとキーボード高さ(--keyboard-inset)を含める", async () => {
    mobile();
    const { getByText } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi") },
    });
    await tick();
    const body = getByText("hi").closest("div") as HTMLElement;
    expect(body.className).toContain("var(--safe-bottom)");
    expect(body.className).toContain("var(--keyboard-inset)");
  });

  it("maxHeight指定の全画面本文もキーボード高さを含み、内容が収まらないときはスクロールする", async () => {
    mobile();
    const { getByText } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi"), maxHeight: "100%" },
    });
    await tick();
    const body = getByText("hi").closest("div") as HTMLElement;
    expect(body.className).toContain("var(--keyboard-inset)");
    expect(body.className).toMatch(/overflow-y-auto/);
    expect(body.className).not.toMatch(/overflow-hidden/);
  });

  it("PC版(箱型)の本文はキーボード高さに影響されない", async () => {
    const { getByText } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi") },
    });
    await tick();
    const body = getByText("hi").closest("div") as HTMLElement;
    expect(body.className).not.toContain("--keyboard-inset");
  });

  it("全画面モーダルの上でcompactを開いても全画面側の余白式は変わらない", async () => {
    mobile();
    const base = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("base") },
    });
    await tick();
    const before = (base.getByText("base").closest("div") as HTMLElement).className;
    const err = render(Modal, {
      props: { title: "エラー", onclose: () => {}, children: textSnippet("err"), compact: true },
    });
    await tick();
    expect((base.getByText("base").closest("div") as HTMLElement).className).toBe(before);
    expect(before).toContain("var(--keyboard-inset)");
    err.unmount();
  });

  it("子がマウント時にフォーカスした入力欄を、Modalのフォーカス処理が奪わない(返信・共有の事前入力)", async () => {
    mobile();
    const { getByTestId } = render(ModalFocusHost);
    await tick();
    expect(document.activeElement).toBe(getByTestId("child-input"));
  });

  it("PC版・compactでも子のフォーカスを奪わない", async () => {
    const { getByTestId } = render(ModalFocusHost, { props: { compact: true } });
    await tick();
    expect(document.activeElement).toBe(getByTestId("child-input"));
  });
});
