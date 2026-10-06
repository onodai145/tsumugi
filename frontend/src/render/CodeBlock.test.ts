import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: CodeBlock } = await import("./CodeBlock.svelte");

afterEach(() => {
  cleanup();
  delete (HTMLElement.prototype as { offsetHeight?: number }).offsetHeight;
});

describe("CodeBlock", () => {
  // WebKitGTK では、overflow:auto な <pre> を含むカードを遅延レイアウトに任せると
  // スクロールバー分の高さが親に反映されず、フッターがスクロールバーに重なったまま固まる。
  // DOM挿入の直後に同期でレイアウトを強制すると数フレーム後に解消する（Issue #166）。
  it("マウント直後とハイライト反映直後に、その時点の <pre> を含むレイアウトを同期で強制する", async () => {
    const seenAtRead: string[] = [];
    Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
      configurable: true,
      get() {
        if (this.classList?.contains("mfm-codeblock")) {
          seenAtRead.push(this.querySelector("pre")?.className ?? "");
        }
        return 0;
      },
    });

    const { container } = render(CodeBlock, { code: "const x = 1;", lang: "javascript" });
    await waitFor(() => expect(container.querySelector("pre")!.className).toContain("shiki "));

    expect(seenAtRead).toContain("shiki-plain");
    expect(seenAtRead.some((c) => c.startsWith("shiki "))).toBe(true);
  });
});
