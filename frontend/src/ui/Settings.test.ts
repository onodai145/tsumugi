import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { tick } from "svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("9.9.9") }));
const invokeMock = vi.fn().mockResolvedValue(null);
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: Settings } = await import("./Settings.svelte");
const { app } = await import("../lib/store.svelte");

function renderSettings() {
  return render(Settings, {
    props: { onclose: () => {}, onAddAccount: () => {}, onReauth: () => {} },
  });
}

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
  app.ui = { ...app.ui, developerOptionsEnabled: false, uiMode: "auto" };
});

describe("Settings タブ構成(Issue #326)", () => {
  it("外部連携タブを表示し、キャッシュバックエンドタブは単独では表示しない", () => {
    const { queryByTestId } = renderSettings();
    expect(queryByTestId("settings-tab-externalIntegration")).not.toBeNull();
    expect(queryByTestId("settings-tab-data")).not.toBeNull();
    expect(queryByTestId("settings-tab-cacheBackend")).toBeNull();
  });

  it("開発者オプション未解除時は「開発者オプション」タブを表示しない", () => {
    const { queryByTestId } = renderSettings();
    expect(queryByTestId("settings-tab-developer")).toBeNull();
  });

  it("開発者オプション解除後は「開発者オプション」タブを表示する", () => {
    app.ui = { ...app.ui, developerOptionsEnabled: true };
    const { queryByTestId } = renderSettings();
    expect(queryByTestId("settings-tab-developer")).not.toBeNull();
  });

  it("モーダルを開いたまま開発者オプションが解除されると開発者オプションタブが動的に現れる", async () => {
    const { queryByTestId, findByTestId } = renderSettings();
    expect(queryByTestId("settings-tab-developer")).toBeNull();

    app.ui = { ...app.ui, developerOptionsEnabled: true };

    expect(await findByTestId("settings-tab-developer")).not.toBeNull();
  });
});

describe("Settings モバイルのタブ帯(Issue #450)", () => {
  it("PC版ではタブ帯(settings-tab-strip)を出さない", () => {
    const { queryByTestId } = renderSettings();
    expect(queryByTestId("settings-tab-strip")).toBeNull();
  });

  it("モバイル版では横スクロールのタブ帯を出し、タブ切り替えができる", async () => {
    app.ui = { ...app.ui, uiMode: "mobile" };
    const { getByTestId } = renderSettings();
    const strip = getByTestId("settings-tab-strip");
    expect(strip.className).toMatch(/overflow-x-auto/);
    // stickyはスクロール領域のパディング(Modal本文のp-4)の内側を基準に止まるため、
    // -top-4でパディング分を打ち消さないと、ヘッダーとの間に16pxの隙間ができて本文が透ける。
    expect(strip.className).toMatch(/(^|\s)sticky(\s|$)/);
    expect(strip.className).toMatch(/(^|\s)-top-4(\s|$)/);
    expect(strip.className).not.toMatch(/(^|\s)top-0(\s|$)/);
    const layout = getByTestId("settings-tab-layout");
    expect(strip.contains(layout)).toBe(true);
    expect(layout.getAttribute("aria-selected")).toBe("false");
    await fireEvent.click(layout);
    expect(layout.getAttribute("aria-selected")).toBe("true");
  });

  it("モバイル版でも開発者オプション解除でタブ帯にタブが動的に現れる", async () => {
    app.ui = { ...app.ui, uiMode: "mobile" };
    const { getByTestId, queryByTestId, findByTestId } = renderSettings();
    expect(queryByTestId("settings-tab-developer")).toBeNull();
    app.ui = { ...app.ui, developerOptionsEnabled: true };
    const dev = await findByTestId("settings-tab-developer");
    expect(getByTestId("settings-tab-strip").contains(dev)).toBe(true);
  });

  it("UIモードを切り替えても本文セクションのDOMは作り直されない(保存直後の表示が消えない)", async () => {
    const { getByTestId } = renderSettings();
    const section = getByTestId("settings-section");
    const child = section.firstElementChild;
    expect(child).not.toBeNull();
    app.ui = { ...app.ui, uiMode: "mobile" };
    await tick();
    expect(getByTestId("settings-section")).toBe(section);
    expect(getByTestId("settings-section").firstElementChild).toBe(child);
  });
});

describe("Settings の高さ上限(モーダル外にスクロールが出ない)", () => {
  // jsdom ではレイアウトを測れないため、構造で守る。Modal の枠(border+p-4+ヘッダー+余白)は54pxあり、
  // 内側に max-h-[calc(84vh-3rem)] を手計算で置くと 3rem(48px)との差 6px ぶんダイアログが上限を超え、
  // 上下の画面余白(16vh)と合わせて画面全体にスクロールバーが出ていた。上限は Modal の maxHeight(枠込み)に任せる。
  it("PC版では上限を Modal の maxHeight(枠込み)で与え、内側に手計算の max-h を置かない", () => {
    const { getByRole, getByTestId } = renderSettings();
    expect((getByRole("dialog") as HTMLElement).style.maxHeight).toBe("83vh");
    expect(getByTestId("settings-section").closest("[class*='max-h-']")).toBeNull();
  });

  it("モバイル版(全画面)では maxHeight を渡さない", async () => {
    app.ui = { ...app.ui, uiMode: "mobile" };
    const { getByRole } = renderSettings();
    await tick();
    expect((getByRole("dialog") as HTMLElement).style.maxHeight).toBe("");
  });
});
