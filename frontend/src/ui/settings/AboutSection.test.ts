import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";

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
// jsdomはmatchMediaを未実装のため、setUiPrefs内の#applyTheme("auto")が参照できるスタブを用意する
// (frontend/src/lib/store.svelte.test.tsのmockPrefersColorSchemeDarkと同じ理由)。
vi.stubGlobal("matchMedia", (query: string) => ({
  matches: false,
  media: query,
  addEventListener: () => {},
  removeEventListener: () => {},
  dispatchEvent: () => false,
}));

const { default: AboutSection } = await import("./AboutSection.svelte");
const { app } = await import("../../lib/store.svelte");

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
  app.ui = { ...app.ui, developerOptionsEnabled: false };
});

async function tap(target: HTMLElement, times: number) {
  for (let i = 0; i < times; i++) {
    await fireEvent.click(target);
  }
}

describe("AboutSection 開発者オプション解除(Issue #326)", () => {
  it("バージョン表示を7回タップするとdeveloperOptionsEnabled:trueで保存する", async () => {
    const { getByTestId } = render(AboutSection);
    await tap(getByTestId("settings-version-tap-target"), 7);

    expect(invokeMock).toHaveBeenCalledWith(
      "set_ui_prefs",
      expect.objectContaining({ prefs: expect.objectContaining({ developerOptionsEnabled: true }) }),
    );
  });

  it("6回のタップでは保存が発生しない", async () => {
    const { getByTestId } = render(AboutSection);
    await tap(getByTestId("settings-version-tap-target"), 6);

    expect(invokeMock).not.toHaveBeenCalledWith("set_ui_prefs", expect.anything());
  });

  it("7回タップすると有効化メッセージを表示する", async () => {
    const { getByTestId, findByText } = render(AboutSection);
    await tap(getByTestId("settings-version-tap-target"), 7);

    expect(await findByText("開発者オプションを有効にしました")).toBeTruthy();
  });

  it("既に有効な場合はタップしても再度保存しない", async () => {
    app.ui = { ...app.ui, developerOptionsEnabled: true };
    const { getByTestId } = render(AboutSection);
    await tap(getByTestId("settings-version-tap-target"), 7);

    expect(invokeMock).not.toHaveBeenCalledWith("set_ui_prefs", expect.anything());
  });
});
