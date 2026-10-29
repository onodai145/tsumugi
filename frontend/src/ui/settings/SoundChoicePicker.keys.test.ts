import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, within } from "@testing-library/svelte";
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

const { commands } = await import("../../lib/ipc");
const { app } = await import("../../lib/store.svelte");
const { default: SoundChoicePicker } = await import("./SoundChoicePicker.svelte");
const { MML_KEY_GROUPS } = await import("../../lib/mmlKeys");

const okScore = { notes: [], totalSec: 2.5, trackCount: 1 };
const allKeys = MML_KEY_GROUPS.flatMap((g) => g.keys);

async function viewOf(findByTestId: (id: string) => Promise<HTMLElement>): Promise<EditorView> {
  const view = EditorView.findFromDOM(await findByTestId("mml-input"));
  if (!view) throw new Error("EditorView が見つからない");
  return view;
}

function setDoc(view: EditorView, text: string, cursor = text.length) {
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text }, selection: { anchor: cursor } });
}

beforeEach(() => {
  vi.spyOn(commands, "parseMml").mockResolvedValue({ status: "parsed", score: okScore });
  app.ui.uiMode = "mobile";
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  app.ui.uiMode = "auto";
});

describe("SoundChoicePicker の入力補助キー", () => {
  it("モバイル UI の MML モードでは、入力欄の下にキーが出る", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    await viewOf(findByTestId);
    const keys = getByTestId("mml-keys");
    expect(keys.getAttribute("role")).toBe("group");
    expect(keys.getAttribute("aria-label")).toBe("入力補助");
    const editor = getByTestId("mml-input");
    expect(editor.compareDocumentPosition(keys) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("モバイル UI では、入力補助キーの下に音色のボタンが続く(Issue #512)", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    await viewOf(findByTestId);
    expect(getByTestId("mml-keys").compareDocumentPosition(getByTestId("mml-timbres")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("デスクトップ UI では出ない", async () => {
    app.ui.uiMode = "desktop";
    const { findByTestId, queryByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    await viewOf(findByTestId);
    expect(queryByTestId("mml-keys")).toBeNull();
  });

  it("uiMode が auto で OS がデスクトップなら出ない", async () => {
    app.ui.uiMode = "auto";
    const { findByTestId, queryByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    await viewOf(findByTestId);
    expect(queryByTestId("mml-keys")).toBeNull();
  });

  it("MML 以外のモードでは出ない", () => {
    const { queryByTestId } = render(SoundChoicePicker, { value: "beep" });
    expect(queryByTestId("mml-keys")).toBeNull();
  });

  it("グループごとに、読み上げ名の付いた group で並ぶ", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    await viewOf(findByTestId);
    const keys = getByTestId("mml-keys");
    const groups = within(keys).getAllByRole("group");
    expect(groups.map((g) => g.getAttribute("aria-label"))).toEqual(MML_KEY_GROUPS.map((g) => g.label));
  });

  it("どのキーも、押すとカーソル位置にその文字が入る", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:" });
    const view = await viewOf(findByTestId);
    for (const key of allKeys) {
      setDoc(view, "");
      await fireEvent.click(getByTestId(`mml-key-${key.id}`));
      expect(view.state.doc.toString(), key.id).toBe(key.insert);
    }
  });

  it("キーの表示と読み上げ名は、定義どおり", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:" });
    await viewOf(findByTestId);
    for (const key of allKeys) {
      const button = getByTestId(`mml-key-${key.id}`);
      expect(button.textContent?.trim(), key.id).toBe(key.label);
      expect(button.getAttribute("aria-label"), key.id).toBe(key.aria);
    }
  });

  it("続けて押すと空白なしでつながる(c 8 . → c8.)", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:" });
    const view = await viewOf(findByTestId);
    await fireEvent.click(getByTestId("mml-key-c"));
    await fireEvent.click(getByTestId("mml-key-len-8"));
    await fireEvent.click(getByTestId("mml-key-dot"));
    expect(view.state.doc.toString()).toBe("c8.");
  });

  it("文書の途中のカーソル位置に入れ、前後に空白を足さない", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:" });
    const view = await viewOf(findByTestId);
    setDoc(view, "cd", 1);
    await fireEvent.click(getByTestId("mml-key-sharp"));
    expect(view.state.doc.toString()).toBe("c#d");
    expect(view.state.selection.main.head).toBe(2);
  });

  it("押してもエディタからフォーカスを奪わない(mousedown を止める)", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    await viewOf(findByTestId);
    const notPrevented = await fireEvent.mouseDown(getByTestId("mml-key-c"));
    expect(notPrevented).toBe(false);
  });

  it("タップしやすいよう、キーの高さは 40px 以上", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    await viewOf(findByTestId);
    for (const key of allKeys) {
      expect(getByTestId(`mml-key-${key.id}`).className, key.id).toContain("min-h-10");
    }
  });

  it("フォームの送信を起こさないよう、type=button", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    await viewOf(findByTestId);
    for (const key of allKeys) {
      expect(getByTestId(`mml-key-${key.id}`).getAttribute("type"), key.id).toBe("button");
    }
  });
});
