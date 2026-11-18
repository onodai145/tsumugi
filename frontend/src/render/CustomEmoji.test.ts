import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import CustomEmoji from "./CustomEmoji.svelte";

// CustomEmoji は低帯域幅モードの判定で store.svelte.ts を読むため、Tauri ランタイム外でも import できるようスタブする。
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

const { app } = await import("../lib/store.svelte");

afterEach(() => {
  cleanup();
  app.ui = { ...app.ui, lowBandwidth: false };
});

describe("CustomEmoji", () => {
  it("renders an img with alt/title when a url is provided", () => {
    const { getByRole } = render(CustomEmoji, {
      props: { name: "blob_cat", url: "https://example.com/e.png" },
    });
    const img = getByRole("img");
    expect(img.getAttribute("src")).toBe("https://example.com/e.png");
    expect(img.getAttribute("alt")).toBe(":blob_cat:");
    expect(img.getAttribute("title")).toBe(":blob_cat:");
  });

  it("omits the title attribute when showTitle is false", () => {
    const { getByRole } = render(CustomEmoji, {
      props: { name: "blob_cat", url: "https://example.com/e.png", showTitle: false },
    });
    expect(getByRole("img").hasAttribute("title")).toBe(false);
  });

  it("renders fallback text when no url is provided", () => {
    const { getByText, queryByRole } = render(CustomEmoji, { props: { name: "blob_cat" } });
    expect(getByText(":blob_cat:")).toBeTruthy();
    expect(queryByRole("img")).toBeNull();
  });

  it("低帯域幅モードでは、url があっても画像を取得せず :name: をテキスト表示する(Issue #591)", () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { getByText, queryByRole } = render(CustomEmoji, {
      props: { name: "blob_cat", url: "https://example.com/e.png" },
    });
    expect(queryByRole("img")).toBeNull();
    expect(getByText(":blob_cat:")).toBeTruthy();
  });
});
