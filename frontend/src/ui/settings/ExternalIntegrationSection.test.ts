import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import { app } from "../../lib/store.svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: ExternalIntegrationSection } = await import("./ExternalIntegrationSection.svelte");

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ExternalIntegrationSection 翻訳先言語", () => {
  it("OSネイティブの<select>ではなく、テーマ適用済みのDropdownを使う", () => {
    // ネイティブ<select>の一覧はWebKitGTK/OSが描画し、アプリのテーマが効かない(Issue #440の修正)。
    const { container } = render(ExternalIntegrationSection);
    expect(container.querySelector("select")).toBeNull();
  });

  it("既定は ja(日本語)が選ばれている", () => {
    app.ui = { ...app.ui, translateTargetLang: "ja" };
    const { getByTestId } = render(ExternalIntegrationSection);
    expect(getByTestId("translate-target-lang").textContent).toContain("日本語");
  });

  it("変更して保存すると translateTargetLang だけが変わり、他の項目は保持される", async () => {
    app.ui = { ...app.ui, translateTargetLang: "ja", searchEngineUrl: "https://example.com/?q={query}" };
    const spy = vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const { getByTestId, getByText } = render(ExternalIntegrationSection);

    getByTestId("translate-target-lang").click();
    await vi.waitFor(() => expect(getByTestId("translate-target-lang-option-en")).toBeTruthy());
    getByTestId("translate-target-lang-option-en").click();
    getByText("保存").click();

    await vi.waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    const saved = spy.mock.calls[0][0];
    expect(saved.translateTargetLang).toBe("en");
    expect(saved.searchEngineUrl).toBe("https://example.com/?q={query}");
  });

  it("プリセットに無い保存値(手編集した設定ファイル等)も選択肢に出して保持する", async () => {
    app.ui = { ...app.ui, translateTargetLang: "pt-BR" };
    const spy = vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const { getByTestId, getByText } = render(ExternalIntegrationSection);
    expect(getByTestId("translate-target-lang").textContent).toContain("pt-BR");

    getByText("保存").click();
    await vi.waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    expect(spy.mock.calls[0][0].translateTargetLang).toBe("pt-BR");
  });
});
