import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/svelte";
import UrlPreviewCard from "./UrlPreviewCard.svelte";

const cachedUrlPreviewMock = vi.fn();
const fetchUrlPreviewMock = vi.fn();
vi.mock("../lib/urlPreview", () => ({
  cachedUrlPreview: (url: string) => cachedUrlPreviewMock(url),
  fetchUrlPreview: (url: string) => fetchUrlPreviewMock(url),
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
    render(UrlPreviewCard, { props: { url: "https://example.com/a" } });
    expect(screen.queryByText("記事タイトル")).toBeNull();
  });

  it("renders nothing when the fetch resolves to null", async () => {
    cachedUrlPreviewMock.mockReturnValue(undefined);
    fetchUrlPreviewMock.mockResolvedValue(null);
    render(UrlPreviewCard, { props: { url: "https://example.com/a" } });
    await waitFor(() => expect(fetchUrlPreviewMock).toHaveBeenCalled());
    expect(screen.queryByText("記事タイトル")).toBeNull();
  });

  it("renders the cached preview synchronously", () => {
    cachedUrlPreviewMock.mockReturnValue(PREVIEW);
    render(UrlPreviewCard, { props: { url: "https://example.com/a" } });
    expect(screen.getByText("記事タイトル")).toBeTruthy();
    expect(screen.getByText("説明文")).toBeTruthy();
    expect(screen.getByText("Example")).toBeTruthy();
  });

  it("blurs a sensitive preview until clicked", async () => {
    const sensitive = { ...PREVIEW, thumbnail: "https://example.com/t.png", sensitive: true };
    cachedUrlPreviewMock.mockReturnValue(sensitive);
    render(UrlPreviewCard, { props: { url: "https://example.com/a" } });
    const cover = screen.getByText("閲覧注意（クリックで表示）");
    expect(document.querySelector("img")).toBeNull();
    cover.click();
    await waitFor(() => expect(document.querySelector("img")).not.toBeNull());
  });

  it("does not embed the iframe until the play button is clicked", async () => {
    const withPlayer = {
      ...PREVIEW,
      thumbnail: "https://example.com/t.png",
      player: { url: "https://example.com/embed", width: 640, height: 360 },
    };
    cachedUrlPreviewMock.mockReturnValue(withPlayer);
    render(UrlPreviewCard, { props: { url: "https://example.com/a" } });
    expect(document.querySelector("iframe")).toBeNull();
    const playButton = screen.getByRole("button", { name: "再生" });
    playButton.click();
    await waitFor(() => expect(document.querySelector("iframe")).not.toBeNull());
  });
});
