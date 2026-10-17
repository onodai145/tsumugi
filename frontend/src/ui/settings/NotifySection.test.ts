import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { EditorView } from "@codemirror/view";
import { app } from "../../lib/store.svelte";

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
const { default: NotifySection } = await import("./NotifySection.svelte");

beforeEach(() => {
  vi.spyOn(commands, "parseMml").mockResolvedValue({
    status: "invalid",
    error: { position: 3, message: "不明な文字 `x` です" },
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

// Issue #75: 通知音ピッカーが非表示の間は、入力途中の(未検証の) MML 下書きを保存しない。
describe("NotifySection の通知音の種類(Issue #75)", () => {
  it("通知音を外してピッカーが消えたら、MML の下書きではなく保存済みの値で保存する", async () => {
    app.notify = { desktop: false, sound: true, soundChoice: "chime" };
    const spy = vi.spyOn(app, "setNotify").mockResolvedValue(undefined);
    const { getByText, findByTestId, getByLabelText } = render(NotifySection);

    await fireEvent.click(getByText("チャイム"));
    await fireEvent.click(getByText("MML（打ち込み）"));
    // MML の入力欄は動的 import なので、現れるまで待ってから CodeMirror に入力する
    const view = EditorView.findFromDOM(await findByTestId("mml-input"))!;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: "c4 x" } });
    await fireEvent.click(getByLabelText(/通知音を鳴らす/));
    await fireEvent.click(getByText("保存"));

    await vi.waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    expect(spy.mock.calls[0][0]).toEqual({ desktop: false, sound: false, soundChoice: "chime" });
  });

  it("ピッカーが見えているときは選んだプリセットをそのまま保存する", async () => {
    app.notify = { desktop: false, sound: true, soundChoice: "chime" };
    const spy = vi.spyOn(app, "setNotify").mockResolvedValue(undefined);
    const { getByText } = render(NotifySection);

    await fireEvent.click(getByText("チャイム"));
    await fireEvent.click(getByText("ピン"));
    await fireEvent.click(getByText("保存"));

    await vi.waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    expect(spy.mock.calls[0][0]).toEqual({ desktop: false, sound: true, soundChoice: "ping" });
  });
});
