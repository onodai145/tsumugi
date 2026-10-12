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
