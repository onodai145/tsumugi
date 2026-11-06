import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { app } from "../lib/store.svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: ReauthBanner } = await import("./ReauthBanner.svelte");

afterEach(() => {
  cleanup();
  app.needsLogin = new Set();
});

describe("ReauthBanner(Issue #553)", () => {
  it("再認証が必要なアカウントが無いときは、何も描画しない", () => {
    app.needsLogin = new Set();
    render(ReauthBanner, { props: { onOpenAccounts: () => {} } });
    expect(screen.queryByTestId("reauth-banner")).toBeNull();
  });

  it("件数を表示する", () => {
    app.needsLogin = new Set(["a1", "a2", "a3"]);
    render(ReauthBanner, { props: { onOpenAccounts: () => {} } });
    expect(screen.getByTestId("reauth-banner").textContent).toContain("3件のアカウントは再認証が必要です");
  });

  it("ボタンで、設定のアカウントを開く導線を呼ぶ", async () => {
    app.needsLogin = new Set(["a1"]);
    const open = vi.fn();
    render(ReauthBanner, { props: { onOpenAccounts: open } });

    await fireEvent.click(screen.getByTestId("reauth-banner-open"));

    expect(open).toHaveBeenCalledTimes(1);
  });

  it("再認証が済んで0件になったら消える", async () => {
    app.needsLogin = new Set(["a1"]);
    render(ReauthBanner, { props: { onOpenAccounts: () => {} } });
    expect(screen.getByTestId("reauth-banner")).toBeTruthy();

    app.needsLogin = new Set();

    await vi.waitFor(() => expect(screen.queryByTestId("reauth-banner")).toBeNull());
  });
});
