import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { createRawSnippet } from "svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
const openUrlMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: openUrlMock }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { installExternalLinkHandler } = await import("./externalLinks");
const { default: Modal } = await import("../ui/Modal.svelte");

let uninstall: () => void;

beforeEach(() => {
  openUrlMock.mockReset();
  openUrlMock.mockResolvedValue(undefined);
  uninstall = installExternalLinkHandler();
});

afterEach(() => {
  uninstall();
  cleanup();
  document.body.replaceChildren();
});

/** target="_blank" の `<a>` を body に足して返す。 */
function addLink(href: string, attrs: Record<string, string> = { target: "_blank" }, parent: HTMLElement = document.body) {
  const a = document.createElement("a");
  a.setAttribute("href", href);
  for (const [k, v] of Object.entries(attrs)) a.setAttribute(k, v);
  a.textContent = "link";
  parent.appendChild(a);
  return a;
}

describe("installExternalLinkHandler", () => {
  describe("外部リンクのクリック", () => {
    it.each([
      "https://example.com/a?b=1",
      "http://example.com/a",
      "mailto:someone@example.com",
      "tel:+810000000000",
    ])("%s は openUrl で開き、WebView 自身の遷移は止める", async (href) => {
      const a = addLink(href);
      const notPrevented = await fireEvent.click(a);
      expect(openUrlMock).toHaveBeenCalledExactlyOnceWith(href);
      expect(notPrevented).toBe(false);
    });

    it("target を持たない外部リンクも、WebView 自身では遷移させない", async () => {
      const a = addLink("https://example.com/no-target", {});
      const notPrevented = await fireEvent.click(a);
      expect(openUrlMock).toHaveBeenCalledExactlyOnceWith("https://example.com/no-target");
      expect(notPrevented).toBe(false);
    });

    it("リンク内の子要素(アイコンなど)をクリックしても開く", async () => {
      const a = addLink("https://example.com/child");
      const icon = document.createElement("span");
      a.appendChild(icon);
      await fireEvent.click(icon);
      expect(openUrlMock).toHaveBeenCalledExactlyOnceWith("https://example.com/child");
    });
  });

  describe("祖先が click の伝播を止めていても開く(Issue #657)", () => {
    it("バブリングを stopPropagation する祖先の下のリンク", async () => {
      const wrapper = document.createElement("div");
      wrapper.addEventListener("click", (e) => e.stopPropagation());
      document.body.appendChild(wrapper);
      const a = addLink("https://example.com/in-wrapper", { target: "_blank" }, wrapper);
      const notPrevented = await fireEvent.click(a);
      expect(openUrlMock).toHaveBeenCalledExactlyOnceWith("https://example.com/in-wrapper");
      expect(notPrevented).toBe(false);
    });

    it("実際の Modal(ダイアログ本体が click を止める)の中のリンク", async () => {
      const children = createRawSnippet(() => ({
        render: () => `<a href="https://example.com/in-modal" target="_blank" data-testid="modal-link">bio link</a>`,
      }));
      const { getByTestId } = render(Modal, { props: { title: "プロフィール", onclose: vi.fn(), children } });
      const notPrevented = await fireEvent.click(getByTestId("modal-link"));
      expect(openUrlMock).toHaveBeenCalledExactlyOnceWith("https://example.com/in-modal");
      expect(notPrevented).toBe(false);
    });
  });

  describe("横取りしないもの", () => {
    it("アプリ自身のオリジンへのリンク", async () => {
      const a = addLink(`${window.location.origin}/inner`);
      await fireEvent.click(a);
      expect(openUrlMock).not.toHaveBeenCalled();
    });

    it("javascript: など許可していないスキーム", async () => {
      const a = addLink("javascript:void(0)");
      await fireEvent.click(a);
      expect(openUrlMock).not.toHaveBeenCalled();
    });

    it("href の無い <a>", async () => {
      const a = document.createElement("a");
      a.textContent = "no href";
      document.body.appendChild(a);
      await fireEvent.click(a);
      expect(openUrlMock).not.toHaveBeenCalled();
    });

    it("リンクの外のクリック", async () => {
      const div = document.createElement("div");
      document.body.appendChild(div);
      await fireEvent.click(div);
      expect(openUrlMock).not.toHaveBeenCalled();
    });

    it("左ボタン以外のクリック", async () => {
      const a = addLink("https://example.com/right");
      await fireEvent.click(a, { button: 2 });
      expect(openUrlMock).not.toHaveBeenCalled();
    });

    it("先に別のハンドラが既定動作を止めたクリック", async () => {
      const early = (e: Event) => e.preventDefault();
      // 捕捉フェーズで、インストール済みのハンドラより前に走らせる(登録順: 先に登録したものが先)
      uninstall();
      window.addEventListener("click", early, true);
      uninstall = installExternalLinkHandler();
      const a = addLink("https://example.com/prevented");
      await fireEvent.click(a);
      window.removeEventListener("click", early, true);
      expect(openUrlMock).not.toHaveBeenCalled();
    });
  });

  it("解除後は何も横取りしない", async () => {
    uninstall();
    const a = addLink("https://example.com/after-uninstall");
    await fireEvent.click(a);
    expect(openUrlMock).not.toHaveBeenCalled();
  });
});
