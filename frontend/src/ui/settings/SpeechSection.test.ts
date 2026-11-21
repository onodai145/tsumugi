import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { app } from "../../lib/store.svelte";
import { commands, events } from "../../lib/ipc";

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
    // OS ネイティブの <select> ではなく、テーマの効く Dropdown(スタイルガイド §選択式の入力)。
    const trigger = getByTestId("speech-voice");
    expect(trigger.tagName).toBe("BUTTON");
    expect(trigger.textContent, "声が未選択なら先頭を選ぶ").toContain("ずんだもん（ノーマル）");
    await fireEvent.click(trigger);
    expect(getByTestId("speech-voice-option-2").textContent).toContain("四国めたん（ノーマル）");
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

  describe("内蔵エンジン(Issue #625 段階2)", () => {
    const pack = (type: "ready" | "notInstalled") => ({ status: { type } as never, sizeBytes: 1000, partialBytes: 0 });
    const coreInfo = (ready = true) => ({
      supported: true,
      termsVersion: "v1",
      termsAccepted: true,
      engine: pack("ready"),
      voices: [
        {
          id: "0",
          pack: pack(ready ? "ready" : "notInstalled"),
          characters: [{ name: "ずんだもん", credit: "VOICEVOX:ずんだもん", styles: [{ id: "3", name: "ノーマル" }] }],
        },
      ],
    });

    beforeEach(() => {
      vi.spyOn(events.speechCoreStatusChanged, "listen").mockResolvedValue((() => {}) as never);
    });

    it("保存前にラジオを内蔵へ切り替えても、外部エンジンの古い状態を内蔵の状態として出さない", async () => {
      app.speechStatus = { type: "ok" };
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(coreInfo()) as never);
      const { getByTestId, queryByTestId } = render(SpeechSection);
      expect(getByTestId("speech-status").textContent).toContain("接続できています");
      await fireEvent.click(getByTestId("speech-engine-core"));
      expect(queryByTestId("speech-status")).toBeNull();
    });

    it("外部エンジンの声の ID が内蔵に無いときは、内蔵に切り替えた時点で、取得済みの声を選び直す", async () => {
      app.speech = { ...app.speech, defaultVoiceId: "999" };
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(coreInfo()) as never);
      const { getByTestId } = render(SpeechSection);
      await fireEvent.click(getByTestId("speech-engine-core"));
      await vi.waitFor(() => expect(getByTestId("speech-voice").textContent).toContain("ずんだもん（ノーマル）"));
    });

    it("対応しない環境では、内蔵のラジオを選べない(外部エンジンのまま)", async () => {
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok({ ...coreInfo(), supported: false }) as never);
      const { getByTestId } = render(SpeechSection);
      await vi.waitFor(() => expect((getByTestId("speech-engine-core") as HTMLInputElement).disabled).toBe(true));
      expect((getByTestId("speech-engine-external") as HTMLInputElement).disabled).toBe(false);
      expect((getByTestId("speech-engine-external") as HTMLInputElement).checked).toBe(true);
    });

    it("既定は外部エンジン。内蔵に切り替えると、URL の入力が消えて、取得の部品が出る", async () => {
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(coreInfo()) as never);
      const { getByTestId, queryByTestId, findByTestId } = render(SpeechSection);
      expect((getByTestId("speech-engine-external") as HTMLInputElement).checked).toBe(true);
      expect(queryByTestId("speech-core-section")).toBeNull();
      await fireEvent.click(getByTestId("speech-engine-core"));
      expect(await findByTestId("speech-core-section")).toBeTruthy();
      expect(queryByTestId("speech-url")).toBeNull();
    });

    it("内蔵で保存すると、engine: voicevoxCore を渡す(URL は空でもよい)", async () => {
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(coreInfo()) as never);
      const spy = vi.spyOn(app, "setSpeechSettings").mockResolvedValue(undefined);
      const { getByTestId, getByText } = render(SpeechSection);
      await fireEvent.click(getByTestId("speech-engine-core"));
      await fireEvent.click(getByText("保存"));
      await vi.waitFor(() => expect(spy).toHaveBeenCalledWith(expect.objectContaining({ engine: "voicevoxCore", externalUrl: "" })));
    });

    it("声の選択肢は取得済みの VVM の声だけで、選んだ声のクレジットが出る", async () => {
      app.speech = { ...app.speech, engine: "voicevoxCore", defaultVoiceId: "3" };
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(coreInfo()) as never);
      const { findByTestId, getByTestId } = render(SpeechSection);
      await vi.waitFor(() => expect(getByTestId("speech-voice").textContent).toContain("ずんだもん（ノーマル）"));
      await fireEvent.click(getByTestId("speech-voice"));
      expect(getByTestId("speech-voice-option-3").textContent).toContain("ずんだもん（ノーマル）");
      expect(document.querySelector('[data-testid="speech-voice-option-14"]'), "未取得の声は選択肢に出ない").toBeNull();
      expect((await findByTestId("speech-core-credit")).textContent).toContain("VOICEVOX:ずんだもん");
    });

    it("取得済みの声がなければ、取得を促す(接続テストは出さない)", async () => {
      app.speech = { ...app.speech, engine: "voicevoxCore" };
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(coreInfo(false)) as never);
      const { findByText, queryByTestId } = render(SpeechSection);
      expect(await findByText(/声を取得してください/)).toBeTruthy();
      expect(queryByTestId("speech-connect-test")).toBeNull();
    });

    it("内蔵のエラー状態は、識別コードではなく日本語で出す", () => {
      app.speech = { ...app.speech, engine: "voicevoxCore" };
      app.speechStatus = { type: "error", message: "invalid: SPEECH_CORE_NOT_READY" };
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(coreInfo()) as never);
      const { getByTestId } = render(SpeechSection);
      expect(getByTestId("speech-status").textContent).toContain("未取得");
      expect(getByTestId("speech-status").textContent).not.toContain("SPEECH_CORE_NOT_READY");
    });
  });
});
