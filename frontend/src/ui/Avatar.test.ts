import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import AvatarTestHost from "./Avatar.test.host.svelte";

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

const { app } = await import("../lib/store.svelte");

afterEach(() => cleanup());

describe("Avatar", () => {
  it("isCat=trueのとき耳要素を描画する", () => {
    const { container } = render(AvatarTestHost, { props: { isCat: true } });
    expect(container.querySelector(".ears")).not.toBeNull();
  });

  it("isCat=falseのとき耳要素を描画しない", () => {
    const { container } = render(AvatarTestHost, { props: { isCat: false } });
    expect(container.querySelector(".ears")).toBeNull();
  });

  it("childrenをそのまま描画する", () => {
    const { getByTestId } = render(AvatarTestHost, { props: { isCat: false } });
    expect(getByTestId("inner-img")).not.toBeNull();
  });
});

describe("Avatar catMode", () => {
  afterEach(() => {
    app.ui = { ...app.ui, catMode: "respect" };
  });

  it("cat: isCat=falseでも耳を描画する", () => {
    app.ui = { ...app.ui, catMode: "cat" };
    const { container } = render(AvatarTestHost, { props: { isCat: false } });
    expect(container.querySelector(".ears")).not.toBeNull();
  });

  it("human: isCat=trueでも耳を描画しない", () => {
    app.ui = { ...app.ui, catMode: "human" };
    const { container } = render(AvatarTestHost, { props: { isCat: true } });
    expect(container.querySelector(".ears")).toBeNull();
  });
});
