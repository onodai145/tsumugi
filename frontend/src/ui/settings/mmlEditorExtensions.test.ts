import { afterEach, describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { errorField, mmlHighlight, setErrorRange } from "./mmlEditorExtensions";

const views: EditorView[] = [];

function mount(doc: string): { view: EditorView; texts: (cls: string) => string[] } {
  const parent = document.createElement("div");
  document.body.appendChild(parent);
  const view = new EditorView({
    parent,
    state: EditorState.create({ doc, extensions: [mmlHighlight, errorField] }),
  });
  views.push(view);
  return {
    view,
    texts: (cls) => [...parent.querySelectorAll(`.${cls}`)].map((e) => e.textContent ?? ""),
  };
}

afterEach(() => {
  while (views.length) views.pop()!.destroy();
  document.body.replaceChildren();
});

describe("mmlHighlight", () => {
  it("コマンドと数字に色のクラスを付ける", () => {
    const { texts } = mount("t120 o5 c4");
    expect(texts("mml-command")).toEqual(["t", "o"]);
    expect(texts("mml-number")).toEqual(["120", "5", "4"]);
  });

  it("音色・繰り返し・区切り・休符", () => {
    const { texts } = mount("@bell c [d]2 r ; e");
    expect(texts("mml-timbre")).toEqual(["@bell"]);
    expect(texts("mml-repeat")).toEqual(["[", "]2"]);
    expect(texts("mml-rest")).toEqual(["r"]);
    expect(texts("mml-separator")).toEqual([";"]);
  });

  it("構文が誤っていても例外を出さず、描画できる", () => {
    expect(() => mount("c4 x @zzz ]]")).not.toThrow();
  });

  it("編集すると色分けも付け直される", () => {
    const { view, texts } = mount("c4");
    view.dispatch({ changes: { from: 2, insert: " t120" } });
    expect(texts("mml-command")).toEqual(["t"]);
    expect(texts("mml-number")).toEqual(["4", "120"]);
  });
});

describe("errorField", () => {
  it("setErrorRange で範囲に下線のクラスを付け、null で消す", () => {
    const { view, texts } = mount("c4 x");
    view.dispatch({ effects: setErrorRange.of({ from: 3, to: 4 }) });
    expect(texts("mml-error")).toEqual(["x"]);
    view.dispatch({ effects: setErrorRange.of(null) });
    expect(texts("mml-error")).toEqual([]);
  });

  it("手前を編集すると、下線は同じ文字に付いたままずれる", () => {
    const { view, texts } = mount("c4 x");
    view.dispatch({ effects: setErrorRange.of({ from: 3, to: 4 }) });
    view.dispatch({ changes: { from: 0, insert: "d" } });
    expect(texts("mml-error")).toEqual(["x"]);
  });

  it("範囲が文書の長さを超えていたら丸める。全く外なら付けない(例外を出さない)", () => {
    const { view, texts } = mount("c4 x");
    view.dispatch({ effects: setErrorRange.of({ from: 3, to: 99 }) });
    expect(texts("mml-error")).toEqual(["x"]);
    view.dispatch({ effects: setErrorRange.of({ from: 10, to: 12 }) });
    expect(texts("mml-error")).toEqual([]);
  });

  it("空文書に範囲を設定しても例外を出さず、何も付けない", () => {
    const { view, texts } = mount("");
    expect(() => view.dispatch({ effects: setErrorRange.of({ from: 0, to: 1 }) })).not.toThrow();
    expect(texts("mml-error")).toEqual([]);
  });
});
