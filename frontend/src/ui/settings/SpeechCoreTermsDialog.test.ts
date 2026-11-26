import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { createRawSnippet, tick } from "svelte";

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

const { default: SpeechCoreTermsDialog } = await import("./SpeechCoreTermsDialog.svelte");
const { default: Modal } = await import("../Modal.svelte");

const terms = { version: "v1", voiceModels: "# VOICEVOX 音声モデル 利用規約\n\nクレジット表記が必要です", onnxruntime: "# VOICEVOX ONNX Runtime 利用規約\n\n再配布できます" };

afterEach(cleanup);

describe("SpeechCoreTermsDialog", () => {
  it("2 つの規約の全文・クレジットの注意・取得するサイズを出す", () => {
    const { getByTestId } = render(SpeechCoreTermsDialog, { terms, sizeBytes: 59_308_488, onAccept: vi.fn(), onCancel: vi.fn() });
    const text = getByTestId("speech-terms-dialog").textContent ?? "";
    expect(text).toContain("VOICEVOX 音声モデル 利用規約");
    expect(text).toContain("VOICEVOX ONNX Runtime 利用規約");
    expect(text).toContain("クレジット");
    expect(text).toContain("59.3 MB");
  });

  it("同意と取りやめは、それぞれのコールバックだけを呼ぶ。初期フォーカスは取りやめ側(Enter 一発で同意にならない)", async () => {
    const onAccept = vi.fn();
    const onCancel = vi.fn();
    const { getByTestId } = render(SpeechCoreTermsDialog, { terms, sizeBytes: 1_000_000, onAccept, onCancel });
    expect(document.activeElement).toBe(getByTestId("speech-terms-cancel"));
    await fireEvent.click(getByTestId("speech-terms-cancel"));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onAccept).not.toHaveBeenCalled();
    await fireEvent.click(getByTestId("speech-terms-accept"));
    expect(onAccept).toHaveBeenCalledTimes(1);
  });

  it("取得を始めている間は、同意を押せない", () => {
    const { getByTestId } = render(SpeechCoreTermsDialog, { terms, sizeBytes: 1, busy: true, onAccept: vi.fn(), onCancel: vi.fn() });
    expect((getByTestId("speech-terms-accept") as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("SpeechCoreTermsDialog のフォーカスが外れた後の Escape(Issue #700)", () => {
  const body = createRawSnippet(() => ({ render: () => `<span>settings</span>` }));

  it("bodyに移った後のEscapeは、手前のダイアログだけをキャンセルする", async () => {
    const onclose = vi.fn();
    render(Modal, { props: { title: "親", onclose, children: body } });
    await tick();
    const onCancel = vi.fn();
    render(SpeechCoreTermsDialog, { terms, sizeBytes: 1, onAccept: vi.fn(), onCancel });
    await tick();
    (document.activeElement as HTMLElement).blur();
    expect(document.activeElement).toBe(document.body);
    await fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onclose).not.toHaveBeenCalled();
  });
});
