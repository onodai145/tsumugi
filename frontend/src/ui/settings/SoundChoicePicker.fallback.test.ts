import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";

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
// MmlEditor の chunk の読み込みに失敗した状況(動的 import が reject する)
vi.mock("./MmlEditor.svelte", () => {
  throw new Error("chunk load failed");
});

const { commands } = await import("../../lib/ipc");
const { default: SoundChoicePicker } = await import("./SoundChoicePicker.svelte");

beforeEach(() => {
  vi.spyOn(commands, "parseMml").mockResolvedValue({
    status: "parsed",
    score: { notes: [], totalSec: 1, trackCount: 1 },
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SoundChoicePicker: エディタの読み込み失敗(Issue #500)", () => {
  it("MmlEditor を読み込めなくても、MML を入力できるフォールバックを出す", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const fallback = (await findByTestId("mml-input-fallback")) as HTMLTextAreaElement;
    expect(fallback.value).toBe("c4");
    expect(getByTestId("mml-editor-load-error").textContent).toContain("エディタを読み込めなかった");
    await fireEvent.input(fallback, { target: { value: "cd" } });
    await vi.waitFor(() => expect(commands.parseMml).toHaveBeenLastCalledWith("cd"));
  });
});
