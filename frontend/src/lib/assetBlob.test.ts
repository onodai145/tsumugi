import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://localhost/${encodeURIComponent(path)}`,
}));

import { loadAssetBlobUrl } from "./assetBlob";

describe("loadAssetBlobUrl", () => {
  const fetchMock = vi.fn();
  const createObjectURL = vi.fn();

  beforeEach(() => {
    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("URL", Object.assign(URL, { createObjectURL }));
    createObjectURL.mockReturnValue("blob:mock");
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    fetchMock.mockReset();
    createObjectURL.mockReset();
  });

  it("fetches the asset URL and returns a blob: URL of the response body", async () => {
    const blob = new Blob(["x"], { type: "image/jpeg" });
    fetchMock.mockResolvedValue({ ok: true, blob: () => Promise.resolve(blob) });

    const url = await loadAssetBlobUrl("/data/backgrounds/a.jpg");

    expect(fetchMock).toHaveBeenCalledWith("asset://localhost/%2Fdata%2Fbackgrounds%2Fa.jpg");
    expect(createObjectURL).toHaveBeenCalledWith(blob);
    expect(url).toBe("blob:mock");
  });

  it("rejects without creating a blob: URL when the response is not ok", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 403, blob: () => Promise.resolve(new Blob()) });

    await expect(loadAssetBlobUrl("/data/backgrounds/a.jpg")).rejects.toThrow("status 403");
    expect(createObjectURL).not.toHaveBeenCalled();
  });
});
