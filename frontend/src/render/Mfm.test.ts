import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import Mfm from "./Mfm.svelte";
import { openProfile } from "../lib/profileModal.svelte";

vi.mock("../lib/profileModal.svelte", () => ({ openProfile: vi.fn() }));

// MfmNode.svelte は UnicodeEmoji.svelte / CodeBlock.svelte を静的importしており、
// それらが ../lib/store.svelte 経由で ../lib/platform.ts の platform() を
// モジュール評価時に同期呼び出しする。Tauriランタイム外(jsdom)では未モックだと
// 即座に例外になるため最小限モックする。
vi.mock("@tauri-apps/plugin-os", () => ({
  platform: () => "linux",
}));

function mockMatchMedia(matches: boolean) {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches,
    media: query,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  }));
}

// MfmNode.svelte はどの fn ノードでも mfmFn() を呼び、mfmFn() は常に
// prefers-reduced-motion を window.matchMedia で判定する。jsdomは未実装なので
// $[...] を含むケース全般でスタブが要る。
beforeEach(() => {
  mockMatchMedia(false);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("Mfm", () => {
  it("renders bold/italic/strike", () => {
    expect(render(Mfm, { props: { text: "**bold**" } }).container.querySelector("b")?.textContent).toBe(
      "bold",
    );
    expect(render(Mfm, { props: { text: "*italic*" } }).container.querySelector("i")?.textContent).toBe(
      "italic",
    );
    expect(render(Mfm, { props: { text: "~~strike~~" } }).container.querySelector("s")?.textContent).toBe(
      "strike",
    );
  });

  it("renders small/center", () => {
    expect(
      render(Mfm, { props: { text: "<small>small</small>" } }).container.querySelector("small")
        ?.textContent,
    ).toBe("small");
    const center = render(Mfm, { props: { text: "<center>hi</center>" } }).container.querySelector("div");
    expect(center?.textContent).toBe("hi");
    expect(center?.style.textAlign).toBe("center");
  });

  it("renders a quote as a blockquote", () => {
    const { container } = render(Mfm, { props: { text: "> quoted" } });
    expect(container.querySelector("blockquote.mfm-quote")?.textContent).toBe("quoted");
  });

  it("renders a link with its label and an external-link icon", () => {
    const { container } = render(Mfm, { props: { text: "[label](https://example.com)" } });
    const a = container.querySelector("a.mfm-link");
    expect(a?.getAttribute("href")).toBe("https://example.com");
    expect(a?.textContent).toBe("label");
    expect(a?.querySelector("svg")).toBeTruthy();
  });

  it("renders a bare url as a link showing the url itself", () => {
    const { container } = render(Mfm, { props: { text: "https://example.com/path" } });
    const a = container.querySelector("a.mfm-link");
    expect(a?.getAttribute("href")).toBe("https://example.com/path");
    expect(a?.textContent).toBe("https://example.com/path");
  });

  it("renders a mention", () => {
    const { container } = render(Mfm, { props: { text: "@alice@example.com" } });
    expect(container.querySelector("span.mfm-mention")?.textContent).toBe("@alice@example.com");
  });

  it("mentionクリックでopenProfileが呼ばれる", () => {
    const { container } = render(Mfm, { props: { text: "@alice@example.com hi" } });
    const mention = container.querySelector("span.mfm-mention") as HTMLElement;
    mention.click();
    expect(openProfile).toHaveBeenCalledWith({ username: "alice", host: "example.com" });
  });

  it("ローカルユーザーへのmentionはhost:nullで呼ばれる", () => {
    const { container } = render(Mfm, { props: { text: "@bob hi" } });
    const mention = container.querySelector("span.mfm-mention") as HTMLElement;
    mention.click();
    expect(openProfile).toHaveBeenCalledWith({ username: "bob", host: null });
  });

  it("renders a hashtag", () => {
    const { container } = render(Mfm, { props: { text: "#tag" } });
    expect(container.querySelector("span.mfm-hashtag")?.textContent).toBe("#tag");
  });

  it("renders a custom emoji image when a url is supplied", () => {
    const { getByRole } = render(Mfm, {
      props: { text: ":blob_cat:", emojis: { blob_cat: "https://example.com/e.png" } },
    });
    expect(getByRole("img").getAttribute("src")).toBe("https://example.com/e.png");
  });

  it("falls back to :name: text when the emoji is unknown", () => {
    const { container } = render(Mfm, { props: { text: ":blob_cat:" } });
    expect(container.querySelector(".custom-emoji-fallback")?.textContent).toBe(":blob_cat:");
  });

  it("renders inline code", () => {
    const { container } = render(Mfm, { props: { text: "`code`" } });
    expect(container.querySelector("code.mfm-code")?.textContent).toBe("code");
  });

  it("applies a known fn's styling (x2 doubles the font size)", () => {
    const { container } = render(Mfm, { props: { text: "$[x2 hi]" } });
    const span = container.querySelector("span");
    expect(span?.textContent).toBe("hi");
    expect(span?.style.fontSize).toBe("2em");
  });

  it("shows an unknown fn literally instead of dropping it", () => {
    const { container } = render(Mfm, { props: { text: "$[nonexistent hi]" } });
    expect(container.textContent).toBe("$[nonexistent hi]");
  });

  it("renders $[ruby base rt] as a <ruby> element with a reading", () => {
    const { container } = render(Mfm, { props: { text: "$[ruby base rt]" } });
    const ruby = container.querySelector("ruby");
    expect(ruby?.querySelector("rt")?.textContent).toBe("rt");
    expect(ruby?.textContent).toBe("basert");
  });

  it("renders $[unixtime ...] as a localized date/time with a matching title", () => {
    const { container } = render(Mfm, { props: { text: "$[unixtime 1700000000]" } });
    const el = container.querySelector(".mfm-unixtime");
    const expected = new Date(1700000000 * 1000).toLocaleString();
    expect(el?.getAttribute("title")).toBe(expected);
    expect(el?.textContent).toBe(`🕛 ${expected}`);
  });

  it("wraps $[sparkle ...] content in the Sparkle component", () => {
    const { container } = render(Mfm, { props: { text: "$[sparkle hi]" } });
    const layer = container.querySelector('[data-testid="sparkle-layer"]');
    const sparkle = layer?.parentElement;
    expect(sparkle?.textContent?.trim()).toBe("hi");
  });

  it("renders $[clickable ...] content without a wrapper element", () => {
    const { container } = render(Mfm, { props: { text: "$[clickable hi]" } });
    expect(container.textContent).toBe("hi");
    expect(container.querySelector("span, a, b")).toBeNull();
  });

  it("renders <plain>...</plain> content as literal text, not parsed MFM", () => {
    const { container } = render(Mfm, { props: { text: "<plain>**not bold**</plain>" } });
    expect(container.textContent).toBe("**not bold**");
    expect(container.querySelector("b")).toBeNull();
  });

  it("nyaizes text nodes when nyaize is true", () => {
    const { container } = render(Mfm, { props: { text: "こんな", nyaize: true } });
    expect(container.textContent).toBe("こんにゃ");
  });

  it("does not nyaize when nyaize is false", () => {
    const { container } = render(Mfm, { props: { text: "こんな", nyaize: false } });
    expect(container.textContent).toBe("こんな");
  });

  it("does not nyaize a link label even when nyaize is true (disableNyaize)", () => {
    const { container } = render(Mfm, {
      props: { text: "[こんな](https://example.com)", nyaize: true },
    });
    expect(container.querySelector("a")?.textContent).toBe("こんな");
  });

  it("does not nyaize inside a quote even when nyaize is true (disableNyaize)", () => {
    const { container } = render(Mfm, { props: { text: "> こんな", nyaize: true } });
    expect(container.querySelector("blockquote")?.textContent).toBe("こんな");
  });

  it("renders nothing for empty text", () => {
    const { container } = render(Mfm, { props: { text: "" } });
    expect(container.textContent).toBe("");
  });
});
