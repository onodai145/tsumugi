import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("9.9.9") }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

const { default: App } = await import("./App.svelte");
const { app } = await import("./lib/store.svelte");

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_drafts") return Promise.resolve([]);
    return Promise.resolve(null);
  });
  // 起動処理(boot)は走らせず、起動済み・アカウントありの状態だけを作る。
  vi.spyOn(app, "boot").mockResolvedValue(undefined);
  app.accounts = [
    {
      id: "acc1",
      host: "misskey.io",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
      instance: null,
    },
  ];
  app.booting = false;
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  app.accounts = [];
  app.booting = true;
  app.ui = { ...app.ui, uiMode: "auto", composePosition: "top", tabBarPosition: "top" };
});

function layout(container: HTMLElement) {
  const header = container.querySelector("header");
  const main = container.querySelector("main");
  return { header, main };
}

describe("App 投稿欄の位置(Issue #548)", () => {
  it("既定(top)では投稿欄のheaderがmainより前にある", () => {
    const { container } = render(App);
    const { header, main } = layout(container);
    expect(header).not.toBeNull();
    expect(main).not.toBeNull();
    expect(header!.compareDocumentPosition(main!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(header!.classList.contains("border-b")).toBe(true);
  });

  it("bottomでは投稿欄のheaderがmainより後ろにあり、区切り線が上側に付く", () => {
    app.ui = { ...app.ui, composePosition: "bottom" };
    const { container } = render(App);
    const { header, main } = layout(container);
    expect(header).not.toBeNull();
    expect(main!.compareDocumentPosition(header!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(header!.classList.contains("border-t")).toBe(true);
    expect(header!.classList.contains("border-b")).toBe(false);
  });

  it("bottomでは上端のセーフエリアをmainが確保し、topではheaderが確保する", () => {
    app.ui = { ...app.ui, composePosition: "bottom" };
    const bottom = layout(render(App).container);
    expect(bottom.main!.className).toContain("pt-[var(--safe-top)]");
    cleanup();

    app.ui = { ...app.ui, composePosition: "top" };
    const top = layout(render(App).container);
    expect(top.main!.className).not.toContain("pt-[var(--safe-top)]");
  });

  it("モバイル版UIでは位置の設定に関わらず投稿欄のheaderを出さない", () => {
    app.ui = { ...app.ui, uiMode: "mobile", composePosition: "bottom" };
    const { container } = render(App);
    expect(layout(container).header).toBeNull();
  });
});

describe("App モバイル版の投稿ボタンとタブバーの位置(Issue #577)", () => {
  it("タブバーが上なら投稿ボタンは既定の高さに置く", () => {
    app.ui = { ...app.ui, uiMode: "mobile", tabBarPosition: "top" };
    const { getByTitle } = render(App);
    expect(getByTitle("投稿").className).toContain("bottom-[calc(20px+var(--safe-bottom))]");
  });

  it("タブバーが下なら、投稿ボタンはタブバーの右端(メニューボタン)に重ならないよう持ち上げる", () => {
    app.ui = { ...app.ui, uiMode: "mobile", tabBarPosition: "bottom" };
    const { getByTitle } = render(App);
    expect(getByTitle("投稿").className).toContain("bottom-[calc(60px+var(--safe-bottom))]");
  });
});

describe("App グローバルショートカットとモーダル(Issue #570)", () => {
  function pressT() {
    document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "t", bubbles: true }));
  }

  it("モーダルが無ければキーマップが実行される", () => {
    const run = vi.spyOn(app, "runKeyAction").mockImplementation(() => {});
    render(App);
    pressT();
    expect(run).toHaveBeenCalledWith("note.renote");
  });

  it("aria-modal のダイアログが表示されている間はキーマップを実行しない", () => {
    const run = vi.spyOn(app, "runKeyAction").mockImplementation(() => {});
    render(App);
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    document.body.appendChild(dialog);
    try {
      pressT();
      expect(run).not.toHaveBeenCalled();
    } finally {
      dialog.remove();
    }
    pressT();
    expect(run).toHaveBeenCalledWith("note.renote");
  });
});
