import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";

// isMobilePlatform はモジュール読み込み時に決まるので、android 固定のモックで別ファイルにしている。
vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "android" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("9.9.9") }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: Settings } = await import("./Settings.svelte");

afterEach(() => cleanup());

describe("Settings タブ構成(モバイル)", () => {
  it("モバイル(android)ではMCPサーバーのタブを出さない", () => {
    const { queryByTestId } = render(Settings, {
      props: { onclose: () => {}, onAddAccount: () => {}, onReauth: () => {} },
    });
    expect(queryByTestId("settings-tab-mcp")).toBeNull();
    expect(queryByTestId("settings-tab-mobile")).not.toBeNull(); // 比較: モバイル専用タブは出る
  });
});
