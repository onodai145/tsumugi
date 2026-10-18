import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import { EditorView } from "@codemirror/view";
import { undo } from "@codemirror/commands";
import MmlEditor from "./MmlEditor.svelte";

afterEach(() => cleanup());

function viewOf(el: HTMLElement): EditorView {
  const view = EditorView.findFromDOM(el);
  if (!view) throw new Error("EditorView が見つからない");
  return view;
}

describe("MmlEditor", () => {
  it("value が文書に入る", () => {
    const { getByTestId } = render(MmlEditor, { value: "t120 o5 c4" });
    expect(viewOf(getByTestId("mml-input")).state.doc.toString()).toBe("t120 o5 c4");
  });

  it("外から value を変えると、文書も変わる", async () => {
    const { getByTestId, rerender } = render(MmlEditor, { value: "c4" });
    await rerender({ value: "d8 e8" });
    expect(viewOf(getByTestId("mml-input")).state.doc.toString()).toBe("d8 e8");
  });

  it("色分けのクラスが付く", () => {
    const { container } = render(MmlEditor, { value: "t120 c4" });
    expect(container.querySelector(".mml-command")?.textContent).toBe("t");
  });

  it("errorPosition の位置に下線が付き、null にすると消える", async () => {
    const { container, rerender } = render(MmlEditor, { value: "c4 x", errorPosition: 3 });
    expect(container.querySelector(".mml-error")?.textContent).toBe("x");
    await rerender({ value: "c4 x", errorPosition: null });
    expect(container.querySelector(".mml-error")).toBeNull();
  });

  it("手前にサロゲートペアがあっても、Rust の文字位置で正しい文字に下線が付く", () => {
    const { container } = render(MmlEditor, { value: "😀cx", errorPosition: 2 });
    expect(container.querySelector(".mml-error")?.textContent).toBe("x");
  });

  it("errorPosition が文書の長さ以上でも例外を出さず、最後の 1 文字に付く", () => {
    const { container } = render(MmlEditor, { value: "c4", errorPosition: 5000 });
    expect(container.querySelector(".mml-error")?.textContent).toBe("4");
  });

  it("空の文書では、下線を付けない", () => {
    const { container } = render(MmlEditor, { value: "", errorPosition: 0 });
    expect(container.querySelector(".mml-error")).toBeNull();
  });

  it("focusAt は Rust の文字位置にカーソルを置く", () => {
    const { component, getByTestId } = render(MmlEditor, { value: "😀cx" });
    component.focusAt(2);
    expect(viewOf(getByTestId("mml-input")).state.selection.main.head).toBe(3);
  });

  it("focusAt が文書の長さ以上なら、末尾に置く", () => {
    const { component, getByTestId } = render(MmlEditor, { value: "c4" });
    component.focusAt(5000);
    expect(viewOf(getByTestId("mml-input")).state.selection.main.head).toBe(2);
  });

  it("a11y: 名前・複数行・ソフトキーボードの自動修正の抑止が付く", () => {
    const { container } = render(MmlEditor, { value: "c4", describedBy: "st us" });
    const content = container.querySelector(".cm-content")!;
    expect(content.getAttribute("role")).toBe("textbox");
    expect(content.getAttribute("aria-multiline")).toBe("true");
    expect(content.getAttribute("aria-label")).toBe("MML");
    expect(content.getAttribute("aria-describedby")).toBe("st us");
    expect(content.getAttribute("aria-invalid")).toBeNull();
    expect(content.getAttribute("autocapitalize")).toBe("off");
    expect(content.getAttribute("autocorrect")).toBe("off");
    expect(content.getAttribute("spellcheck")).toBe("false");
  });

  it("a11y: invalid のとき aria-invalid が付き、戻すと外れる", async () => {
    const { container, rerender } = render(MmlEditor, { value: "c4 x", invalid: true });
    const content = container.querySelector(".cm-content")!;
    expect(content.getAttribute("aria-invalid")).toBe("true");
    await rerender({ value: "c4 x", invalid: false });
    expect(content.getAttribute("aria-invalid")).toBeNull();
  });

  describe("insertText", () => {
    function setup(value: string, anchor?: number, head?: number) {
      const r = render(MmlEditor, { value });
      const view = viewOf(r.getByTestId("mml-input"));
      if (anchor !== undefined) view.dispatch({ selection: { anchor, head: head ?? anchor } });
      return { ...r, view };
    }

    it("空の文書には、そのまま入れて、後ろに空白を足す", () => {
      const { component, view } = setup("");
      component.insertText("@bell");
      expect(view.state.doc.toString()).toBe("@bell ");
      expect(view.state.selection.main.head).toBe(6);
    });

    it("文末の音符に直接つながらないよう、前に空白を足す", () => {
      const { component, view } = setup("c4", 2);
      component.insertText("@bell");
      expect(view.state.doc.toString()).toBe("c4 @bell ");
      expect(view.state.selection.main.head).toBe(9);
    });

    it("前後がすでに空白なら、空白を足さない", () => {
      const { component, view } = setup("c4  d4", 3);
      component.insertText("@bell");
      expect(view.state.doc.toString()).toBe("c4 @bell d4");
    });

    it("後ろが空白でない文字なら、空白を足してつながらないようにする", () => {
      const { component, view } = setup("c4 d4", 3);
      component.insertText("@bell");
      expect(view.state.doc.toString()).toBe("c4 @bell d4");
      expect(view.state.selection.main.head).toBe(9);
    });

    it("選択範囲は置き換える", () => {
      const { component, view } = setup("c4 d4 e4", 3, 5);
      component.insertText("@pulse");
      expect(view.state.doc.toString()).toBe("c4 @pulse e4");
    });

    it("Ctrl+Z 相当の undo で元に戻る", () => {
      const { component, view } = setup("c4", 2);
      component.insertText("@bell");
      undo(view);
      expect(view.state.doc.toString()).toBe("c4");
    });
  });

  it("空のとき、プレースホルダーを出す", () => {
    const { container } = render(MmlEditor, { value: "" });
    expect(container.querySelector(".cm-placeholder")?.textContent).toBe("t120 o5 l8 cdefgab>c");
  });

  it("編集で下線が自分で消えているときは、errorPosition が null になっても余計な dispatch をしない(IME 変換中の更新を増やさない)", async () => {
    const { container, getByTestId, rerender } = render(MmlEditor, { value: "c4 x", errorPosition: 3 });
    expect(container.querySelector(".mml-error")).not.toBeNull();
    const view = viewOf(getByTestId("mml-input"));
    view.dispatch({ changes: { from: 4, insert: "y" } }); // 編集 → 下線は消える
    expect(container.querySelector(".mml-error")).toBeNull();
    const dispatch = vi.spyOn(EditorView.prototype, "dispatch");
    await rerender({ value: "c4 xy", errorPosition: null });
    expect(dispatch).not.toHaveBeenCalled();
    dispatch.mockRestore();
  });
});
