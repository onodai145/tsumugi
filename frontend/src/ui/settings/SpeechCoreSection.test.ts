import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { commands, events } from "../../lib/ipc";
import type { PackInfo, SpeechCoreInfo } from "../../lib/ipc";

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

const { default: SpeechCoreSection } = await import("./SpeechCoreSection.svelte");

const ok = <T,>(data: T) => ({ status: "ok" as const, data });
const err = (message: string) => ({ status: "error" as const, error: { kind: "invalid" as const, message } });
const pack = (status: PackInfo["status"] = { type: "notInstalled" }, sizeBytes = 1_000_000): PackInfo => ({ status, sizeBytes, partialBytes: 0 });
const TERMS = { version: "v1", voiceModels: "音声モデルの規約", onnxruntime: "ONNX Runtime の規約" };

const info = (over: Partial<SpeechCoreInfo> = {}): SpeechCoreInfo => ({
  supported: true,
  termsVersion: "v1",
  termsAccepted: false,
  engine: pack({ type: "notInstalled" }, 31_870_280),
  voices: [
    {
      id: "0",
      pack: pack({ type: "notInstalled" }, 59_308_488),
      characters: [
        { name: "四国めたん", credit: "VOICEVOX:四国めたん", styles: [{ id: "2", name: "ノーマル" }] },
        { name: "ずんだもん", credit: "VOICEVOX:ずんだもん", styles: [{ id: "3", name: "ノーマル" }] },
      ],
    },
    { id: "1", pack: pack({ type: "notInstalled" }, 60_571_189), characters: [{ name: "冥鳴ひまり", credit: "VOICEVOX:冥鳴ひまり", styles: [{ id: "14", name: "ノーマル" }] }] },
  ],
  ...over,
});

let emit: (i: SpeechCoreInfo) => void = () => {};
const unlisten = vi.fn();

beforeEach(() => {
  unlisten.mockReset();
  vi.spyOn(commands, "getSpeechCoreTerms").mockResolvedValue(ok(TERMS) as never);
  vi.spyOn(events.speechCoreStatusChanged, "listen").mockImplementation((async (cb: (e: { payload: { info: SpeechCoreInfo } }) => void) => {
    emit = (i) => cb({ payload: { info: i } });
    return unlisten;
  }) as never);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SpeechCoreSection", () => {
  it("対応しない環境では、理由だけを出して、取得の操作を出さない", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info({ supported: false })));
    const { findByTestId, queryByTestId } = render(SpeechCoreSection);
    expect((await findByTestId("speech-core-unsupported")).textContent).toContain("この環境");
    expect(queryByTestId("speech-core-engine-get")).toBeNull();
    expect(queryByTestId("speech-core-voice-0-get")).toBeNull();
  });

  it("未取得: エンジン一式のサイズが出て、声の取得はエンジン一式の取得後まで押せない", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info()));
    const { findByTestId, getByTestId } = render(SpeechCoreSection);
    expect((await findByTestId("speech-core-engine-status")).textContent).toContain("未取得");
    expect(getByTestId("speech-core-section").textContent).toContain("31.9 MB");
    expect((getByTestId("speech-core-engine-get") as HTMLButtonElement).disabled).toBe(false);
    expect((getByTestId("speech-core-voice-0-get") as HTMLButtonElement).disabled).toBe(true);
    expect(getByTestId("speech-core-voice-0-label").textContent).toContain("四国めたん、ずんだもん");
    expect(getByTestId("speech-core-voice-0-label").textContent).toContain("59.3 MB");
  });

  it("未同意で取得を押すと規約が出て、同意するまで開始しない。取りやめても開始しない", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info()));
    const start = vi.spyOn(commands, "startSpeechCoreDownload").mockResolvedValue(ok(null));
    const { findByTestId, getByTestId, queryByTestId } = render(SpeechCoreSection);
    await fireEvent.click(await findByTestId("speech-core-engine-get"));
    expect((await findByTestId("speech-terms-dialog")).textContent).toContain("音声モデルの規約");
    expect(start).not.toHaveBeenCalled();
    await fireEvent.click(getByTestId("speech-terms-cancel"));
    expect(queryByTestId("speech-terms-dialog")).toBeNull();
    expect(start).not.toHaveBeenCalled();
  });

  it("同意すると、対象と現行の規約の版を渡して開始する", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info()));
    const start = vi.spyOn(commands, "startSpeechCoreDownload").mockResolvedValue(ok(null));
    const { findByTestId, getByTestId, queryByTestId } = render(SpeechCoreSection);
    await fireEvent.click(await findByTestId("speech-core-engine-get"));
    await fireEvent.click(await findByTestId("speech-terms-accept"));
    await vi.waitFor(() => expect(start).toHaveBeenCalledWith("engine", "v1"));
    await vi.waitFor(() => expect(queryByTestId("speech-terms-dialog")).toBeNull());
    expect(getByTestId("speech-core-section")).toBeTruthy();
  });

  it("同意済みの版なら、規約を出さずに開始する(声の取得は、その VVM の ID を渡す)", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info({ termsAccepted: true, engine: pack({ type: "ready" }) })));
    const start = vi.spyOn(commands, "startSpeechCoreDownload").mockResolvedValue(ok(null));
    const { findByTestId, queryByTestId } = render(SpeechCoreSection);
    const get = (await findByTestId("speech-core-voice-1-get")) as HTMLButtonElement;
    expect(get.disabled).toBe(false);
    await fireEvent.click(get);
    await vi.waitFor(() => expect(start).toHaveBeenCalledWith("1", "v1"));
    expect(queryByTestId("speech-terms-dialog")).toBeNull();
  });

  it("開始が拒否されたら、理由を表示する", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info({ termsAccepted: true })));
    vi.spyOn(commands, "startSpeechCoreDownload").mockResolvedValue(err("すでにダウンロード中です"));
    const { findByTestId } = render(SpeechCoreSection);
    await fireEvent.click(await findByTestId("speech-core-engine-get"));
    expect((await findByTestId("speech-core-error")).textContent).toContain("すでにダウンロード中です");
  });

  it("Event の状態で表示が変わる: 取得中は進捗とキャンセル、ほかの取得は押せない", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info({ termsAccepted: true, engine: pack({ type: "ready" }) })));
    const cancel = vi.spyOn(commands, "cancelSpeechCoreDownload").mockResolvedValue(ok(null));
    const { findByTestId, getByTestId } = render(SpeechCoreSection);
    await findByTestId("speech-core-engine-status");
    const next = info({ termsAccepted: true, engine: pack({ type: "ready" }) });
    next.voices[0].pack = pack({ type: "downloading", downloaded: 29_654_244, total: 59_308_488 });
    emit(next);
    await vi.waitFor(() => expect(getByTestId("speech-core-voice-0-status").textContent).toContain("50%"));
    expect((getByTestId("speech-core-voice-1-get") as HTMLButtonElement).disabled, "取得は同時に 1 つ").toBe(true);
    await fireEvent.click(getByTestId("speech-core-voice-0-cancel"));
    await vi.waitFor(() => expect(cancel).toHaveBeenCalled());
  });

  it("取得済みの声は削除でき、押すと対象の ID で削除を呼ぶ。失敗した声は取得し直せる", async () => {
    const i = info({ termsAccepted: true, engine: pack({ type: "ready" }) });
    i.voices[0].pack = pack({ type: "ready" });
    i.voices[1].pack = pack({ type: "failed", reason: "通信が途切れました" });
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(i));
    const del = vi.spyOn(commands, "deleteSpeechCorePack").mockResolvedValue(ok(null));
    const { findByTestId, getByTestId } = render(SpeechCoreSection);
    expect((await findByTestId("speech-core-voice-1-status")).textContent).toContain("通信が途切れました");
    expect((getByTestId("speech-core-voice-1-get") as HTMLButtonElement).disabled).toBe(false);
    await fireEvent.click(getByTestId("speech-core-voice-0-delete"));
    await vi.waitFor(() => expect(del).toHaveBeenCalledWith("0"));
  });

  it("購読を解除して閉じる。取得の応答より新しい Event を、古い応答で上書きしない", async () => {
    let resolveInfo: (v: unknown) => void = () => {};
    const getInfo = vi.spyOn(commands, "getSpeechCoreInfo").mockReturnValue(new Promise((r) => (resolveInfo = r)) as never);
    const { getByTestId, unmount } = render(SpeechCoreSection);
    // 購読と、状態の取得の要求の両方が済んでから、Event を流す(取得の要求より前に流すと、「古い応答」にならない)。
    await vi.waitFor(() => expect(events.speechCoreStatusChanged.listen).toHaveBeenCalled());
    await vi.waitFor(() => expect(getInfo).toHaveBeenCalled());
    const newer = info({ engine: pack({ type: "ready" }) });
    emit(newer);
    resolveInfo(ok(info())); // 応答は、Event より古いスナップショット。
    await vi.waitFor(() => expect(getByTestId("speech-core-engine-status").textContent).toContain("取得済み"));
    unmount();
    expect(unlisten).toHaveBeenCalled();
  });
});
