import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: LayoutSection } = await import("./LayoutSection.svelte");
const { app } = await import("../../lib/store.svelte");

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  app.ui = { ...app.ui, composePosition: "top" };
});

describe("LayoutSection 投稿欄の位置(Issue #548)", () => {
  it("既定では「上」が選択されている", () => {
    const { getByRole } = render(LayoutSection);
    expect(getByRole("button", { name: "上" }).getAttribute("aria-pressed")).toBe("true");
    expect(getByRole("button", { name: "下" }).getAttribute("aria-pressed")).toBe("false");
  });

  it("保存済みの設定が「下」なら「下」が選択されている", () => {
    app.ui = { ...app.ui, composePosition: "bottom" };
    const { getByRole } = render(LayoutSection);
    expect(getByRole("button", { name: "下" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("「下」を選んで保存すると composePosition: bottom が他の設定を保ったまま保存される", async () => {
    const setUiPrefs = vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const { getByRole } = render(LayoutSection);

    await fireEvent.click(getByRole("button", { name: "下" }));
    await fireEvent.click(getByRole("button", { name: /保存/ }));

    expect(setUiPrefs).toHaveBeenCalledTimes(1);
    expect(setUiPrefs.mock.calls[0][0]).toMatchObject({
      composePosition: "bottom",
      defaultColumnWidth: app.ui.defaultColumnWidth,
    });
  });
});
