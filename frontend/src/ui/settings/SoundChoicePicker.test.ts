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

const { commands } = await import("../../lib/ipc");
const { default: SoundChoicePicker } = await import("./SoundChoicePicker.svelte");
const { TIMBRE_LIST } = await import("../../lib/mmlTimbres");

const okScore = { notes: [], totalSec: 2.5, trackCount: 1 };

beforeEach(() => {
  vi.spyOn(commands, "parseMml").mockResolvedValue({ status: "parsed", score: okScore });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SoundChoicePicker の MML モード", () => {
  it("保存済みの mml: を開くと MML モードで、テキストエリアに本文が入る", async () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:t120 o5 c4" });
    expect((getByTestId("mml-input") as HTMLTextAreaElement).value).toBe("t120 o5 c4");
    await vi.waitFor(() => expect(getByTestId("mml-status").textContent).toContain("2.5"));
  });

  it("構文が通れば再生時間を出し、invalid=false を通知する", async () => {
    const onvalidity = vi.fn();
    render(SoundChoicePicker, { value: "mml:c4", onvalidity });
    await vi.waitFor(() => expect(onvalidity).toHaveBeenLastCalledWith(false));
  });

  it("構文エラーは位置(1 始まり)と理由を出し、invalid=true を通知する", async () => {
    vi.spyOn(commands, "parseMml").mockResolvedValue({
      status: "invalid",
      error: { position: 3, message: "不明な文字 `x` です" },
    });
    const onvalidity = vi.fn();
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4 x", onvalidity });
    await vi.waitFor(() => expect(getByTestId("mml-status").textContent).toContain("4"));
    expect(getByTestId("mml-status").textContent).toContain("不明な文字");
    expect(onvalidity).toHaveBeenLastCalledWith(true);
  });

  it("parse_mml の IPC が失敗したら、位置なしのメッセージを出し invalid=true のままにする(未処理の reject はテスト失敗になる)", async () => {
    vi.spyOn(commands, "parseMml").mockRejectedValue(new Error("ipc down"));
    const onvalidity = vi.fn();
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4", onvalidity });
    await vi.waitFor(() => expect(getByTestId("mml-status").textContent).toContain("ipc down"));
    const text = getByTestId("mml-status").textContent ?? "";
    expect(text).toContain("構文を確認できませんでした");
    expect(text).not.toContain("文字目");
    expect(text).not.toContain("確認中");
    expect(onvalidity).toHaveBeenLastCalledWith(true);
  });

  it("本文が空なら parse_mml を呼ばずに invalid=true にする", async () => {
    const onvalidity = vi.fn();
    render(SoundChoicePicker, { value: "mml:", onvalidity });
    await vi.waitFor(() => expect(onvalidity).toHaveBeenLastCalledWith(true));
    expect(commands.parseMml).not.toHaveBeenCalled();
  });

  it("入力すると 300ms 待って、最後のテキストだけをパースする", async () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c" });
    await vi.waitFor(() => expect(commands.parseMml).toHaveBeenCalledTimes(1));
    const input = getByTestId("mml-input") as HTMLTextAreaElement;
    await fireEvent.input(input, { target: { value: "cd" } });
    await fireEvent.input(input, { target: { value: "cde" } });
    await vi.waitFor(() => expect(commands.parseMml).toHaveBeenLastCalledWith("cde"));
    expect(commands.parseMml).toHaveBeenCalledTimes(2); // "c" と "cde"(途中の "cd" は呼ばれない)
  });

  it("プリセットのときは MML の入力欄を出さない", () => {
    const { queryByTestId } = render(SoundChoicePicker, { value: "chime" });
    expect(queryByTestId("mml-input")).toBeNull();
  });

  it("MML モードを抜けると invalid=false に戻す(親の保存ボタンを止めたままにしない)", async () => {
    const onvalidity = vi.fn();
    const { unmount } = render(SoundChoicePicker, { value: "mml:", onvalidity });
    await vi.waitFor(() => expect(onvalidity).toHaveBeenLastCalledWith(true));
    unmount();
    expect(onvalidity).toHaveBeenLastCalledWith(false);
  });

  it("記法ヘルプに音色の一覧(番号と名前)が出る", async () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const text = getByTestId("mml-timbres").textContent ?? "";
    expect(TIMBRE_LIST).toHaveLength(14);
    for (const p of TIMBRE_LIST) expect(text).toContain(`@${p.number} ${p.name}`);
  });
});
