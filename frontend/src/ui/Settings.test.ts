import { afterEach, describe, expect, it, vi } from "vitest";
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
  app.ui = { ...app.ui, developerOptionsEnabled: false };
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
});
