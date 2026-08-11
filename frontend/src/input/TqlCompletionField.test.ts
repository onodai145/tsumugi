import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";

// store.svelte.ts が起動時に @tauri-apps/plugin-os の platform() を呼ぶため、
// Tauri ランタイム外(jsdom)で import が失敗しないようスタブする。
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

const { default: TqlCompletionField } = await import("./TqlCompletionField.svelte");
const { app } = await import("../lib/store.svelte");

// list("...") 確定後は閉じ引用符ができてIDトリガーが外れ、通常の構文補完(Rust IPC)へ
// フォールバックする。このテストの主眼はそちらではないため、空配列を返すようにしておく。
vi.spyOn(app, "tqlComplete").mockResolvedValue([]);

// jsdom は scrollIntoView を実装していないため、選択項目を追従スクロールさせる
// CompletionPopover 内の $effect がエラーにならないよう no-op スタブを補う。
beforeEach(() => {
  Element.prototype.scrollIntoView ??= () => {};
});

afterEach(() => cleanup());

describe("TqlCompletionField 補完確定後の再検証", () => {
  it("補完を確定するとoninputが呼ばれ、確定後の文字列で検証がやり直される(回帰テスト)", async () => {
    // list("...") の途中はID補完(lists prop から同期的に候補を出す。IPC不要)がトリガーされる。
    const oninput = vi.fn();
    const { container, getByRole } = render(TqlCompletionField, {
      props: {
        mode: "query",
        value: "",
        oninput,
        lists: [{ id: "abc123", name: "Friends" }],
      },
    });
    const textarea = container.querySelector("textarea") as HTMLTextAreaElement;

    await fireEvent.focus(textarea);
    textarea.value = 'from list("Fri';
    textarea.selectionStart = textarea.value.length;
    textarea.selectionEnd = textarea.value.length;
    await fireEvent.input(textarea);

    // 入力イベントで一旦呼ばれた分をリセットし、以降は「確定」由来の呼び出しだけを見る。
    oninput.mockClear();

    const option = getByRole("option");
    await fireEvent.mouseDown(option);

    // 補完確定で value が書き換わったこと自体は既存動作。ここでの主張は
    // 「確定後、親コンポーネントの再検証(oninput)が呼ばれる」こと。
    expect(textarea.value).toBe('from list("abc123")');
    expect(oninput).toHaveBeenCalledTimes(1);
  });
});
