import { beforeEach, describe, expect, it, vi } from "vitest";

const mountMock = vi.fn();
const isFramedMock = vi.fn();
vi.mock("svelte", () => ({ mount: mountMock }));
vi.mock("./app.css", () => ({}));
vi.mock("./App.svelte", () => ({ default: {} }));
vi.mock("./lib/ipc", () => ({ commands: {} }));
vi.mock("./lib/externalLinks", () => ({ installExternalLinkHandler: vi.fn() }));
vi.mock("./lib/appOrigin", () => ({ isFramed: isFramedMock }));

beforeEach(() => {
  vi.resetModules();
  mountMock.mockReset();
  document.body.innerHTML = '<div id="app"></div>';
});

describe("main", () => {
  it("mounts the app in a top-level window", async () => {
    isFramedMock.mockReturnValue(false);
    await import("./main");
    expect(mountMock).toHaveBeenCalledTimes(1);
  });

  // Issue #613: iframe の中で本体がもう一度起動すると、boot() が二重に走る。
  it("does not mount the app inside a frame", async () => {
    isFramedMock.mockReturnValue(true);
    await import("./main");
    expect(mountMock).not.toHaveBeenCalled();
  });
});
