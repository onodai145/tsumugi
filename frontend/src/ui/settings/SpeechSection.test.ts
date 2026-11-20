import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { app } from "../../lib/store.svelte";
import { commands } from "../../lib/ipc";

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

const { default: SpeechSection } = await import("./SpeechSection.svelte");

const ok = <T,>(data: T) => ({ status: "ok" as const, data });
const err = (message: string) => ({ status: "error" as const, error: { kind: "invalid" as const, message } });

beforeEach(() => {
  app.speech = { enabled: false, engine: "external", externalUrl: "", defaultVoiceId: "", speedScale: 1, volume: 1 };
  app.speechStatus = { type: "unknown" };
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SpeechSection", () => {
  it("既定はオフ・URL なし。規約への注意書きがある", () => {
    const { getByTestId } = render(SpeechSection);
    expect((getByTestId("speech-enabled") as HTMLInputElement).checked).toBe(false);
    expect((getByTestId("speech-url") as HTMLInputElement).value).toBe("");
    expect(getByTestId("speech-credit").textContent).toContain("クレジット");
  });

  it("接続テストは入力欄の URL(未保存)で声の一覧を取得し、選択肢に反映する", async () => {
    const spy = vi.spyOn(commands, "listSpeechVoices").mockResolvedValue(
      ok([
        { id: "3", label: "ずんだもん（ノーマル）" },
        { id: "2", label: "四国めたん（ノーマル）" },
      ]),
    );
    const { getByTestId, findByTestId } = render(SpeechSection);
    await fireEvent.input(getByTestId("speech-url"), { target: { value: "http://192.168.1.10:50021" } });
    await fireEvent.click(getByTestId("speech-connect-test"));

    await vi.waitFor(() => expect(spy).toHaveBeenCalledWith("http://192.168.1.10:50021"));
    expect((await findByTestId("speech-connect-result")).textContent).toContain("2");
    const select = getByTestId("speech-voice") as HTMLSelectElement;
    expect(Array.from(select.options).map((o) => o.textContent?.trim())).toContain("ずんだもん（ノーマル）");
    expect(select.value, "声が未選択なら先頭を選ぶ").toBe("3");
  });

  it("URL が空のあいだは、接続テストを押せない", () => {
    const { getByTestId } = render(SpeechSection);
    expect((getByTestId("speech-connect-test") as HTMLButtonElement).disabled).toBe(true);
  });

  it("接続できなかったら、理由を表示する", async () => {
    vi.spyOn(commands, "listSpeechVoices").mockResolvedValue(err("SPEECH_ENGINE_UNREACHABLE: connect"));
    const { getByTestId, findByTestId } = render(SpeechSection);
    await fireEvent.input(getByTestId("speech-url"), { target: { value: "http://h:1" } });
    await fireEvent.click(getByTestId("speech-connect-test"));
    const result = (await findByTestId("speech-connect-result")).textContent ?? "";
    expect(result).toContain("接続できませんでした");
    expect(result).toContain("SPEECH_ENGINE_UNREACHABLE");
  });

  it("保存すると、入力した値を setSpeechSettings に渡す(URL は前後の空白を除く)", async () => {
    const spy = vi.spyOn(app, "setSpeechSettings").mockResolvedValue(undefined);
    const { getByTestId, getByText } = render(SpeechSection);
    await fireEvent.click(getByTestId("speech-enabled"));
    await fireEvent.input(getByTestId("speech-url"), { target: { value: "  http://h:1  " } });
    await fireEvent.input(getByTestId("speech-speed"), { target: { value: "1.5" } });
    await fireEvent.click(getByText("保存"));
    await vi.waitFor(() =>
      expect(spy).toHaveBeenCalledWith({
        enabled: true,
        engine: "external",
        externalUrl: "http://h:1",
        defaultVoiceId: "",
        speedScale: 1.5,
        volume: 1,
      }),
    );
  });

  it("保存が拒否されたら、理由を表示する", async () => {
    vi.spyOn(app, "setSpeechSettings").mockRejectedValue(new Error("URL の形式が正しくありません"));
    const { getByTestId, getByText, findByText } = render(SpeechSection);
    await fireEvent.input(getByTestId("speech-url"), { target: { value: "ftp://x" } });
    await fireEvent.click(getByText("保存"));
    expect(await findByText(/URL の形式が正しくありません/)).toBeTruthy();
  });

  it("試し聞きと停止は、それぞれのコマンドを呼ぶ", async () => {
    const tryIt = vi.spyOn(commands, "testSpeech").mockResolvedValue(ok(null));
    const stop = vi.spyOn(commands, "stopSpeech").mockResolvedValue(ok(null));
    const { getByTestId } = render(SpeechSection);
    await fireEvent.click(getByTestId("speech-try"));
    await vi.waitFor(() => expect(tryIt).toHaveBeenCalled());
    await fireEvent.click(getByTestId("speech-stop"));
    await vi.waitFor(() => expect(stop).toHaveBeenCalled());
  });

  it("試し聞きが失敗したら、理由を表示する", async () => {
    vi.spyOn(commands, "testSpeech").mockResolvedValue(err("SPEECH_NO_VOICE"));
    const { getByTestId, findByText } = render(SpeechSection);
    await fireEvent.click(getByTestId("speech-try"));
    expect(await findByText(/SPEECH_NO_VOICE/)).toBeTruthy();
  });

  it("エンジンに繋がらない状態を表示する", () => {
    app.speechStatus = { type: "error", message: "SPEECH_ENGINE_UNREACHABLE: connect" };
    const { getByTestId } = render(SpeechSection);
    expect(getByTestId("speech-status").textContent).toContain("接続できません");
  });
});
