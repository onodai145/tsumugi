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
  it("既定は ja が選ばれている", () => {
    app.ui = { ...app.ui, translateTargetLang: "ja" };
    const { getByLabelText } = render(ExternalIntegrationSection);
    expect((getByLabelText("翻訳先言語") as HTMLSelectElement).value).toBe("ja");
  });

  it("変更して保存すると translateTargetLang だけが変わり、他の項目は保持される", async () => {
    app.ui = { ...app.ui, translateTargetLang: "ja", searchEngineUrl: "https://example.com/?q={query}" };
    const spy = vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const { getByLabelText, getByText } = render(ExternalIntegrationSection);

    const select = getByLabelText("翻訳先言語") as HTMLSelectElement;
    select.value = "en";
    select.dispatchEvent(new Event("change", { bubbles: true }));
    getByText("保存").click();

    await vi.waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    const saved = spy.mock.calls[0][0];
    expect(saved.translateTargetLang).toBe("en");
    expect(saved.searchEngineUrl).toBe("https://example.com/?q={query}");
  });

  it("プリセットに無い保存値(手編集した設定ファイル等)も選択肢に出して保持する", () => {
    app.ui = { ...app.ui, translateTargetLang: "pt-BR" };
    const { getByLabelText } = render(ExternalIntegrationSection);
    const select = getByLabelText("翻訳先言語") as HTMLSelectElement;
    expect(select.value).toBe("pt-BR");
  });
});
