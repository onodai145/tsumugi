import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { app } from "../../lib/store.svelte";
import type { UiPrefs } from "../../bindings/tauri.gen";

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

const { default: AppearanceSection } = await import("./AppearanceSection.svelte");

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  app.ui = { ...app.ui, lowBandwidth: false };
});

describe("AppearanceSection 低帯域幅モード", () => {
  it("既定ではチェックが外れている", () => {
    app.ui = { ...app.ui, lowBandwidth: undefined };
    const { getByLabelText } = render(AppearanceSection);
    expect((getByLabelText("低帯域幅モードを有効にする") as HTMLInputElement).checked).toBe(false);
  });

  it("保存済みの値が反映される", () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { getByLabelText } = render(AppearanceSection);
    expect((getByLabelText("低帯域幅モードを有効にする") as HTMLInputElement).checked).toBe(true);
  });

  it("チェックして保存すると lowBandwidth だけが変わり、他の項目は保持される", async () => {
    app.ui = { ...app.ui, lowBandwidth: false, fontFamily: "Meiryo, sans-serif" };
    const spy = vi.spyOn(app, "updateUiPrefs").mockResolvedValue(undefined);
    const { getByLabelText, getByText } = render(AppearanceSection);

    await fireEvent.click(getByLabelText("低帯域幅モードを有効にする"));
    getByText("保存").click();

    await vi.waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    const saved = (spy.mock.calls[0][0] as Partial<UiPrefs>);
    expect(saved.lowBandwidth).toBe(true);
    expect(saved.fontFamily).toBe("Meiryo, sans-serif");
  });
});
