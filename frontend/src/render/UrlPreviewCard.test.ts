import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/svelte";
import UrlPreviewCard from "./UrlPreviewCard.svelte";

const cachedUrlPreviewMock = vi.fn();
const fetchUrlPreviewMock = vi.fn();
// importOriginal で isSafeUrl だけ実体を残す形にできないか試したが、urlPreview.ts 経由で
// store.svelte.ts (Tauri plugin-os 等) が読み込まれテスト環境で例外になるため断念。
// isSafeUrl はここではスタブとして再定義する(urlPreview.ts の実装と手動で同期させる必要がある点は既知のトレードオフ)。
vi.mock("../lib/urlPreview", () => ({
  cachedUrlPreview: (url: string) => cachedUrlPreviewMock(url),
  fetchUrlPreview: (url: string) => fetchUrlPreviewMock(url),
  isSafeUrl: (url: string) => /^https?:\/\//i.test(url),
}));

afterEach(() => {
  cleanup();
  cachedUrlPreviewMock.mockReset();
  fetchUrlPreviewMock.mockReset();
});

const PREVIEW = {
  url: "https://example.com/a",
  title: "記事タイトル",
  description: "説明文",
  thumbnail: null,
  icon: null,
  sitename: "Example",
  sensitive: false,
  player: null,
};

describe("UrlPreviewCard", () => {
  it("renders nothing while the preview has not been fetched yet", () => {
    cachedUrlPreviewMock.mockReturnValue(undefined);
    fetchUrlPreviewMock.mockReturnValue(new Promise(() => {}));
    render(UrlPreviewCard, { props: { url: "https://example.com/a", instanceHost: undefined } });
    expect(screen.queryByText("記事タイトル")).toBeNull();
  });

  it("renders nothing when the fetch resolves to null", async () => {
    cachedUrlPreviewMock.mockReturnValue(undefined);
    fetchUrlPreviewMock.mockResolvedValue(null);
    render(UrlPreviewCard, { props: { url: "https://example.com/a", instanceHost: undefined } });
    await waitFor(() => expect(fetchUrlPreviewMock).toHaveBeenCalled());
    expect(screen.queryByText("記事タイトル")).toBeNull();
  });

  it("renders the cached preview synchronously", () => {
    cachedUrlPreviewMock.mockReturnValue(PREVIEW);
    render(UrlPreviewCard, { props: { url: "https://example.com/a", instanceHost: undefined } });
    expect(screen.getByText("記事タイトル")).toBeTruthy();
    expect(screen.getByText("説明文")).toBeTruthy();
    expect(screen.getByText("Example")).toBeTruthy();
  });

  it("blurs a sensitive preview until clicked", async () => {
    const sensitive = { ...PREVIEW, thumbnail: "https://example.com/t.png", sensitive: true };
    cachedUrlPreviewMock.mockReturnValue(sensitive);
    render(UrlPreviewCard, { props: { url: "https://example.com/a", instanceHost: undefined } });
    const cover = screen.getByText("閲覧注意");
    expect(document.querySelector("img")).toBeNull();
    cover.click();
    await waitFor(() => expect(document.querySelector("img")).not.toBeNull());
  });

  it("does not embed the iframe until the play button is clicked, and expands from the compact thumbnail into the full-width player layout", async () => {
    const withPlayer = {
      ...PREVIEW,
      thumbnail: "https://example.com/t.png",
      player: { url: "https://example.com/embed", width: 640, height: 360 },
    };
    cachedUrlPreviewMock.mockReturnValue(withPlayer);
    render(UrlPreviewCard, { props: { url: "https://example.com/a", instanceHost: undefined } });
    expect(document.querySelector("iframe")).toBeNull();
    // 通常時は横長レイアウト: 小さい固定サイズのサムネイル欄に再生ボタンが乗る
    expect(document.querySelector(".preview-thumb")).not.toBeNull();
    expect(document.querySelector(".preview-media")).toBeNull();
    const playButton = screen.getByRole("button", { name: "再生" });
    playButton.click();
    // 再生後は縦長レイアウトに展開し、小サムネイル欄は無くなる
    await waitFor(() => expect(document.querySelector("iframe")).not.toBeNull());
    const media = document.querySelector(".preview-media") as HTMLElement | null;
    expect(media).not.toBeNull();
    expect(document.querySelector(".preview-thumb")).toBeNull();
    // player.width/height(640x360)を実際のアスペクト比として反映する（固定比率で引き伸ばさない）
    expect(media?.style.aspectRatio).toBe("640 / 360");
  });

  it("falls back to a 16:9 aspect ratio for the expanded player when width/height are missing", async () => {
    const withPlayer = {
      ...PREVIEW,
      thumbnail: "https://example.com/t.png",
      player: { url: "https://example.com/embed", width: null, height: null },
    };
    cachedUrlPreviewMock.mockReturnValue(withPlayer);
    render(UrlPreviewCard, { props: { url: "https://example.com/a", instanceHost: undefined } });
    screen.getByRole("button", { name: "再生" }).click();
    await waitFor(() => expect(document.querySelector("iframe")).not.toBeNull());
    const media = document.querySelector(".preview-media") as HTMLElement | null;
    expect(media?.style.aspectRatio).toBe("16 / 9");
  });

  it("uses the fixed height directly (not an aspect ratio) when the player only reports height (e.g. Spotify's width:null oEmbed)", async () => {
    const withPlayer = {
      ...PREVIEW,
      thumbnail: "https://example.com/t.png",
      player: { url: "https://example.com/embed", width: null, height: 152 },
    };
    cachedUrlPreviewMock.mockReturnValue(withPlayer);
    render(UrlPreviewCard, { props: { url: "https://example.com/a", instanceHost: undefined } });
    screen.getByRole("button", { name: "再生" }).click();
    await waitFor(() => expect(document.querySelector("iframe")).not.toBeNull());
    const media = document.querySelector(".preview-media") as HTMLElement | null;
    expect(media?.style.height).toBe("152px");
    expect(media?.style.aspectRatio).toBe("");
  });

  it("does not linkify the card when preview.url has an unsafe scheme", () => {
    const unsafe = { ...PREVIEW, url: "javascript:alert(1)" };
    cachedUrlPreviewMock.mockReturnValue(unsafe);
    render(UrlPreviewCard, { props: { url: "https://example.com/a", instanceHost: undefined } });
    expect(screen.getByText("記事タイトル")).toBeTruthy();
    expect(document.querySelector("a")).toBeNull();
  });

  it("proxies the thumbnail through the media proxy when instanceHost is known", () => {
    const withThumb = { ...PREVIEW, thumbnail: "https://remote.example/t.png" };
    cachedUrlPreviewMock.mockReturnValue(withThumb);
    render(UrlPreviewCard, { props: { url: "https://example.com/a", instanceHost: "misskey.example" } });
    const img = document.querySelector("img");
    expect(img?.getAttribute("src")).toBe(
      `https://misskey.example/proxy/image.webp?${new URLSearchParams({ url: "https://remote.example/t.png", fallback: "1" })}`,
    );
  });

  it("leaves the thumbnail as the raw URL when instanceHost is undefined", () => {
    const withThumb = { ...PREVIEW, thumbnail: "https://remote.example/t.png" };
    cachedUrlPreviewMock.mockReturnValue(withThumb);
    render(UrlPreviewCard, { props: { url: "https://example.com/a", instanceHost: undefined } });
    const img = document.querySelector("img");
    expect(img?.getAttribute("src")).toBe("https://remote.example/t.png");
  });

  it("does not offer the play button or embed the iframe when preview.player.url has an unsafe scheme", async () => {
    const unsafePlayer = {
      ...PREVIEW,
      thumbnail: "https://example.com/t.png",
      player: { url: "javascript:alert(1)", width: 640, height: 360 },
    };
    cachedUrlPreviewMock.mockReturnValue(unsafePlayer);
    render(UrlPreviewCard, { props: { url: "https://example.com/a", instanceHost: undefined } });
    expect(screen.queryByRole("button", { name: "再生" })).toBeNull();
    expect(document.querySelector("iframe")).toBeNull();
  });
});
