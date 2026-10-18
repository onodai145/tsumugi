import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { EditorView } from "@codemirror/view";

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
// 重い Tone.js のレンダリングは使わず、生成の終わりをテストが決める
vi.mock("../../lib/soundChoice", async (orig) => ({
  ...(await orig<typeof import("../../lib/soundChoice")>()),
  resolveSoundChoice: vi.fn(),
}));

const { commands } = await import("../../lib/ipc");
const { resolveSoundChoice } = await import("../../lib/soundChoice");
const { default: SoundChoicePicker } = await import("./SoundChoicePicker.svelte");

const resolve = vi.mocked(resolveSoundChoice);
const okScore = { notes: [], totalSec: 2.5, trackCount: 1 };

let finish: (url: string) => void;
let fail: (e: Error) => void;
let play: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  vi.spyOn(commands, "parseMml").mockResolvedValue({ status: "parsed", score: okScore });
  play = vi.spyOn(commands, "playNotifySound").mockResolvedValue({ status: "ok", data: null });
  resolve.mockReset();
  resolve.mockImplementation(
    () =>
      new Promise<string>((res, rej) => {
        finish = res;
        fail = rej;
      }),
  );
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

/** エディタが現れ、構文が通って「試聴」が押せるようになるまで待つ */
async function ready(value = "mml:c4") {
  const r = render(SoundChoicePicker, { value });
  const view = EditorView.findFromDOM(await r.findByTestId("mml-input"))!;
  await vi.waitFor(() => expect(r.getByTestId("mml-preview").hasAttribute("disabled")).toBe(false));
  return { ...r, view };
}

describe("SoundChoicePicker の MML 試聴の生成待ち(Issue #500)", () => {
  it("試聴を押すと「生成中」になり、できたら鳴らして「試聴」に戻る", async () => {
    const { getByTestId, queryByTestId } = await ready();
    await fireEvent.click(getByTestId("mml-preview"));
    expect(getByTestId("mml-preview").textContent).toContain("生成中");
    expect(getByTestId("mml-preview").hasAttribute("disabled")).toBe(true);
    expect(getByTestId("mml-preview-cancel")).toBeTruthy();
    finish("data:audio/wav;base64,AAAA");
    await vi.waitFor(() => expect(play).toHaveBeenCalledWith("data:audio/wav;base64,AAAA"));
    await vi.waitFor(() => expect(getByTestId("mml-preview").textContent?.trim()).toBe("試聴"));
    expect(queryByTestId("mml-preview-cancel")).toBeNull();
  });

  it("生成中は経過秒数が増える", async () => {
    const { getByTestId } = await ready();
    await fireEvent.click(getByTestId("mml-preview"));
    expect(getByTestId("mml-preview").textContent).toContain("0 秒");
    await vi.waitFor(() => expect(getByTestId("mml-preview").textContent).toContain("1 秒"), { timeout: 3000 });
  });

  it("「待たない」を押すと、すぐ「試聴」に戻り、あとで生成が終わっても鳴らさない", async () => {
    const { getByTestId, queryByTestId } = await ready();
    await fireEvent.click(getByTestId("mml-preview"));
    await fireEvent.click(getByTestId("mml-preview-cancel"));
    expect(getByTestId("mml-preview").textContent?.trim()).toBe("試聴");
    expect(queryByTestId("mml-preview-cancel")).toBeNull();
    finish("data:audio/wav;base64,AAAA");
    await new Promise((r) => setTimeout(r, 20));
    expect(play).not.toHaveBeenCalled();
  });

  it("生成中に入力を編集したら、前の生成は捨てて「試聴」に戻る", async () => {
    const { getByTestId, view } = await ready();
    await fireEvent.click(getByTestId("mml-preview"));
    view.dispatch({ changes: { from: view.state.doc.length, insert: " d4" } });
    await vi.waitFor(() => expect(getByTestId("mml-preview").textContent).not.toContain("生成中"));
    finish("data:audio/wav;base64,AAAA");
    await new Promise((r) => setTimeout(r, 20));
    expect(play).not.toHaveBeenCalled();
  });

  it("「待たない」のあとに押し直すと、新しい生成を待ち、古い方の結果は鳴らさない", async () => {
    const { getByTestId } = await ready();
    await fireEvent.click(getByTestId("mml-preview"));
    const first = finish;
    await fireEvent.click(getByTestId("mml-preview-cancel"));
    await fireEvent.click(getByTestId("mml-preview"));
    const second = finish;
    first("data:audio/wav;base64,OLD");
    await new Promise((r) => setTimeout(r, 20));
    expect(play).not.toHaveBeenCalled();
    expect(getByTestId("mml-preview").textContent).toContain("生成中");
    second("data:audio/wav;base64,NEW");
    await vi.waitFor(() => expect(play).toHaveBeenCalledWith("data:audio/wav;base64,NEW"));
    expect(play).toHaveBeenCalledTimes(1);
  });

  it("設定画面を閉じた(アンマウントした)あとに生成が終わっても、鳴らさない", async () => {
    const { getByTestId, unmount } = await ready();
    await fireEvent.click(getByTestId("mml-preview"));
    unmount();
    finish("data:audio/wav;base64,AAAA");
    await new Promise((r) => setTimeout(r, 20));
    expect(play).not.toHaveBeenCalled();
  });

  it("生成に失敗したら、理由を出して「試聴」に戻る", async () => {
    const { getByTestId, findByText } = await ready();
    await fireEvent.click(getByTestId("mml-preview"));
    fail(new Error("レンダリング結果が空でした"));
    await findByText(/試聴に失敗しました.*レンダリング結果が空でした/);
    await vi.waitFor(() => expect(getByTestId("mml-preview").textContent?.trim()).toBe("試聴"));
  });
});
