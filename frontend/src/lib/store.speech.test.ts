import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
// 生成された commands.* は、invoke が返した生の値を { status: "ok", data } に包み、reject を { status: "error" } に変える。
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { app } = await import("./store.svelte");

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(null);
});
afterEach(() => vi.restoreAllMocks());

describe("読み上げの設定(store)", () => {
  it("既定は、全体オフ・外部エンジン・URL なし・状態は unknown", () => {
    expect(app.speech).toMatchObject({ enabled: false, engine: "external", externalUrl: "", speedScale: 1, volume: 1 });
    expect(app.speechStatus).toEqual({ type: "unknown" });
  });

  it("setSpeechSettings は保存した結果(Rust が整えた値)を app.speech に反映する", async () => {
    const saved = { enabled: true, engine: "external" as const, externalUrl: "http://h:1", defaultVoiceId: "3", speedScale: 1.2, volume: 1 };
    invokeMock.mockResolvedValueOnce(saved);
    await app.setSpeechSettings({ ...saved, externalUrl: "http://h:1/" });
    expect(invokeMock).toHaveBeenCalledWith("set_speech_settings", { settings: { ...saved, externalUrl: "http://h:1/" } });
    expect(app.speech).toEqual(saved);
  });

  it("返ってきた設定に欠けた項目があっても、既定値で補う", async () => {
    invokeMock.mockResolvedValueOnce({ enabled: true, speedScale: null });
    await app.setSpeechSettings({ ...app.speech, enabled: true });
    expect(app.speech).toEqual({ enabled: true, engine: "external", externalUrl: "", defaultVoiceId: "", speedScale: 1, volume: 1 });
  });

  it("setSpeechSettings が拒否されたら、app.speech は変えず、エラーを投げる", async () => {
    const before = { ...app.speech };
    invokeMock.mockRejectedValueOnce({ kind: "invalid", message: "URL の形式が正しくありません" });
    await expect(app.setSpeechSettings({ ...before, externalUrl: "ftp://x" })).rejects.toBeTruthy();
    expect(app.speech).toEqual(before);
  });

  it("setColumnSpeech は該当タブの speech を更新して、コマンドを呼ぶ", async () => {
    const tab = { id: "t1", speech: { enabled: false, speakAuthor: true, queueLimit: 5, voiceId: "" } } as never;
    app.groups = [{ id: "g", width: 300, auto: false, tabs: [tab], activeTabId: "t1" }] as never;
    const next = { enabled: true, speakAuthor: false, queueLimit: 3, voiceId: "9" };
    invokeMock.mockResolvedValueOnce(next);
    await app.setColumnSpeech("t1", next);
    expect((app.groups[0].tabs[0] as { speech: unknown }).speech).toEqual(next);
    expect(invokeMock).toHaveBeenCalledWith("set_column_speech", { columnId: "t1", speech: next });
  });

  it("setColumnSpeech は、Rust が丸めて返した値を表示に反映する(50 を入れても 20)", async () => {
    const tab = { id: "t1", speech: { enabled: false, speakAuthor: true, queueLimit: 5, voiceId: "" } } as never;
    app.groups = [{ id: "g", width: 300, auto: false, tabs: [tab], activeTabId: "t1" }] as never;
    invokeMock.mockResolvedValueOnce({ enabled: true, speakAuthor: true, queueLimit: 20, voiceId: "" });
    await app.setColumnSpeech("t1", { enabled: true, speakAuthor: true, queueLimit: 50, voiceId: "" });
    expect((app.groups[0].tabs[0] as { speech: { queueLimit: number } }).speech.queueLimit).toBe(20);
  });

  it("setColumnSpeech が失敗したら、表示を元の値に戻す", async () => {
    const before = { enabled: false, speakAuthor: true, queueLimit: 5, voiceId: "" };
    const tab = { id: "t1", speech: { ...before } } as never;
    app.groups = [{ id: "g", width: 300, auto: false, tabs: [tab], activeTabId: "t1" }] as never;
    invokeMock.mockRejectedValueOnce({ kind: "notFound", message: "column t1" });
    await app.setColumnSpeech("t1", { enabled: true, speakAuthor: false, queueLimit: 9, voiceId: "7" });
    expect((app.groups[0].tabs[0] as { speech: unknown }).speech).toEqual(before);
  });
});
