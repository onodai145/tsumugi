import { afterEach, describe, expect, it, vi } from "vitest";
import { escapeOnLostFocus } from "./escapeOnLostFocus";

const cleanups: (() => void)[] = [];

function layer(close = vi.fn()) {
  const node = document.createElement("div");
  node.setAttribute("aria-modal", "true");
  document.body.appendChild(node);
  const action = escapeOnLostFocus(node, close);
  const remove = () => {
    action.destroy();
    node.remove();
  };
  cleanups.push(remove);
  return { node, close, action, remove };
}

function press(key = "Escape", target: EventTarget = document.body) {
  const e = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
  target.dispatchEvent(e);
  return e;
}

afterEach(() => {
  for (const remove of cleanups.splice(0)) remove();
  document.body.innerHTML = "";
});

describe("escapeOnLostFocus(Issue #700)", () => {
  it("bodyに届いたEscapeで閉じる処理を呼び、preventDefaultする", () => {
    const { close } = layer();
    const e = press();
    expect(close).toHaveBeenCalledOnce();
    expect(e.defaultPrevented).toBe(true);
  });

  it("Escape以外のキー・body以外が対象のイベントでは呼ばない", () => {
    const { close, node } = layer();
    press("Enter");
    press("Escape", node);
    expect(close).not.toHaveBeenCalled();
  });

  it("処理済み(defaultPrevented)のEscapeでは呼ばない", () => {
    const { close } = layer();
    const consume = (e: Event) => e.preventDefault();
    document.addEventListener("keydown", consume);
    press();
    document.removeEventListener("keydown", consume);
    expect(close).not.toHaveBeenCalled();
  });

  it("手前にdata-escape-layerの要素があれば呼ばない", () => {
    const { close } = layer();
    const viewer = document.createElement("div");
    viewer.setAttribute("data-escape-layer", "");
    document.body.appendChild(viewer);
    press();
    expect(close).not.toHaveBeenCalled();
  });

  it.each([
    ["先に登録した方が手前", true],
    ["後に登録した方が手前", false],
  ])("%sでも、手前だけが閉じ、消えた後は下に届く", (_name, firstOnTop) => {
    const first = layer();
    const second = layer();
    if (firstOnTop) document.body.appendChild(first.node);
    const [top, bottom] = firstOnTop ? [first, second] : [second, first];
    press();
    expect(top.close).toHaveBeenCalledOnce();
    expect(bottom.close).not.toHaveBeenCalled();
    top.remove();
    press();
    expect(top.close).toHaveBeenCalledOnce();
    expect(bottom.close).toHaveBeenCalledOnce();
  });

  it("destroy後は、要素がDOMに残っていても呼ばない", () => {
    const { close, action } = layer();
    action.destroy();
    press();
    expect(close).not.toHaveBeenCalled();
  });

  it("updateで差し替えた処理を呼ぶ", () => {
    const { close, action } = layer();
    const next = vi.fn();
    action.update(next);
    press();
    expect(next).toHaveBeenCalledOnce();
    expect(close).not.toHaveBeenCalled();
  });
});
