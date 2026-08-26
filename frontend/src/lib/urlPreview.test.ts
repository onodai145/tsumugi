import { beforeEach, describe, expect, it, vi } from "vitest";

const fetchUrlPreviewMock = vi.fn();
vi.mock("./ipc", () => ({ commands: { fetchUrlPreview: fetchUrlPreviewMock } }));
vi.mock("./store.svelte", () => ({ app: { defaultAccountId: () => "acc1" } }));

const { cachedUrlPreview, fetchUrlPreview } = await import("./urlPreview");

const PREVIEW = {
  url: "https://example.com/a",
  title: "タイトル",
  description: null,
  thumbnail: null,
  icon: null,
  sitename: null,
  sensitive: false,
  player: null,
};

beforeEach(() => {
  fetchUrlPreviewMock.mockReset();
});

describe("urlPreview cache", () => {
  it("is undefined before the first fetch", () => {
    expect(cachedUrlPreview("https://example.com/never-fetched")).toBeUndefined();
  });

  it("caches a successful response with content", async () => {
    fetchUrlPreviewMock.mockResolvedValue({ status: "ok", data: PREVIEW });
    const result = await fetchUrlPreview("https://example.com/a");
    expect(result).toEqual(PREVIEW);
    expect(cachedUrlPreview("https://example.com/a")).toEqual(PREVIEW);
  });

  it("caches null permanently when the response has no OG fields", async () => {
    const empty = { ...PREVIEW, url: "https://example.com/empty", title: null };
    fetchUrlPreviewMock.mockResolvedValue({ status: "ok", data: empty });
    const result = await fetchUrlPreview("https://example.com/empty");
    expect(result).toBeNull();
    expect(cachedUrlPreview("https://example.com/empty")).toBeNull();
  });

  it("does not cache a typed error (transient failure)", async () => {
    fetchUrlPreviewMock.mockResolvedValue({
      status: "error",
      error: { kind: "network", message: "boom" },
    });
    const result = await fetchUrlPreview("https://example.com/net-error");
    expect(result).toBeNull();
    expect(cachedUrlPreview("https://example.com/net-error")).toBeUndefined();
  });

  it("does not cache when the IPC call itself throws", async () => {
    fetchUrlPreviewMock.mockRejectedValue(new Error("command not registered"));
    const result = await fetchUrlPreview("https://example.com/ipc-fail");
    expect(result).toBeNull();
    expect(cachedUrlPreview("https://example.com/ipc-fail")).toBeUndefined();
  });

  it("dedupes concurrent fetches for the same URL", async () => {
    let resolveFn: (v: unknown) => void = () => {};
    fetchUrlPreviewMock.mockReturnValue(
      new Promise((resolve) => {
        resolveFn = resolve;
      }),
    );
    const p1 = fetchUrlPreview("https://example.com/concurrent");
    const p2 = fetchUrlPreview("https://example.com/concurrent");
    resolveFn({ status: "ok", data: PREVIEW });
    await Promise.all([p1, p2]);
    expect(fetchUrlPreviewMock).toHaveBeenCalledTimes(1);
  });
});
