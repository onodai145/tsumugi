import { afterEach, describe, expect, it, vi } from "vitest";

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

const { app } = await import("./store.svelte");
const { effectiveIsCat } = await import("./catMode");

afterEach(() => {
  app.ui = { ...app.ui, catMode: "respect" };
});

describe("effectiveIsCat", () => {
  it("respect: isCat をそのまま返す", () => {
    app.ui = { ...app.ui, catMode: "respect" };
    expect(effectiveIsCat(true)).toBe(true);
    expect(effectiveIsCat(false)).toBe(false);
  });

  it("cat: 常に true", () => {
    app.ui = { ...app.ui, catMode: "cat" };
    expect(effectiveIsCat(false)).toBe(true);
  });

  it("human: 常に false", () => {
    app.ui = { ...app.ui, catMode: "human" };
    expect(effectiveIsCat(true)).toBe(false);
  });

  it("未設定(undefined)は respect 扱い", () => {
    app.ui = { ...app.ui, catMode: undefined as unknown as string };
    expect(effectiveIsCat(true)).toBe(true);
    expect(effectiveIsCat(false)).toBe(false);
  });
});
