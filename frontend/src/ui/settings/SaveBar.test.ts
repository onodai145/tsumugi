import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
const invokeMock = vi.fn().mockResolvedValue(null);
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: SaveBar } = await import("./SaveBar.svelte");

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
});

describe("SaveBar", () => {
  it("「保存」ボタンを押すと onsave が呼ばれる", async () => {
    const onsave = vi.fn();
    render(SaveBar, { props: { busy: false, saved: false, err: "", onsave } });
    await fireEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(onsave).toHaveBeenCalledTimes(1);
  });

  it("busy の間はボタンが「保存中…」になり押せない", () => {
    render(SaveBar, { props: { busy: true, saved: false, err: "", onsave: vi.fn() } });
    const btn = screen.getByRole("button", { name: "保存中…" }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  it("disabled を渡すとボタンが押せない", () => {
    render(SaveBar, { props: { busy: false, disabled: true, saved: false, err: "", onsave: vi.fn() } });
    expect((screen.getByRole("button", { name: "保存" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("saved のとき「保存しました」を表示し、そうでなければ出さない", () => {
    const { rerender } = render(SaveBar, { props: { busy: false, saved: false, err: "", onsave: vi.fn() } });
    expect(screen.queryByText("保存しました")).toBeNull();
    rerender({ busy: false, saved: true, err: "", onsave: vi.fn() });
    expect(screen.getByText("保存しました")).toBeTruthy();
  });

  it("エラー文もバーの中に出る(スクロールで隠れないよう同じ sticky 領域に置く)", () => {
    render(SaveBar, { props: { busy: false, saved: false, err: "保存に失敗しました", onsave: vi.fn() } });
    const bar = screen.getByTestId("settings-save-bar");
    expect(bar.contains(screen.getByText("保存に失敗しました"))).toBe(true);
  });

  it("スクロール領域の下端に張り付く(sticky)", () => {
    render(SaveBar, { props: { busy: false, saved: false, err: "", onsave: vi.fn() } });
    expect(screen.getByTestId("settings-save-bar").className).toContain("sticky");
  });

  it("testid を渡すとボタンに付く", () => {
    render(SaveBar, { props: { busy: false, saved: false, err: "", onsave: vi.fn(), testid: "mute-save" } });
    expect(screen.getByTestId("mute-save").textContent?.trim()).toBe("保存");
  });
});
