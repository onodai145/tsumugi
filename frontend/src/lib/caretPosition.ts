// textarea内の指定文字位置のキャレット座標(textarea左上を原点とするpx)を返す。
// ミラーdiv方式: textareaと同じスタイルを与えた非表示divへキャレット位置までの
// テキストを流し込み、末尾に置いたマーカー要素の offsetLeft/offsetTop を読む定番の手法。
const MIRRORED_PROPERTIES = [
  "boxSizing", "width", "height", "overflowX", "overflowY",
  "borderTopWidth", "borderRightWidth", "borderBottomWidth", "borderLeftWidth", "borderStyle",
  "paddingTop", "paddingRight", "paddingBottom", "paddingLeft",
  "fontStyle", "fontVariant", "fontWeight", "fontSize", "lineHeight", "fontFamily",
  "textAlign", "textTransform", "textIndent", "textDecoration",
  "letterSpacing", "wordSpacing", "tabSize", "whiteSpace", "wordWrap", "wordBreak",
] as const satisfies readonly (keyof CSSStyleDeclaration)[];

export interface CaretCoordinates {
  left: number;
  top: number;
  height: number;
}

export function getCaretCoordinates(el: HTMLTextAreaElement, position: number): CaretCoordinates {
  const div = document.createElement("div");
  div.id = "mfm-completion-caret-mirror";
  document.body.appendChild(div);

  const style = div.style;
  const computed = window.getComputedStyle(el);

  style.position = "absolute";
  style.visibility = "hidden";
  style.top = "0";
  style.left = "-9999px";
  style.whiteSpace = "pre-wrap";
  style.wordWrap = "break-word";

  for (const prop of MIRRORED_PROPERTIES) {
    const value = computed[prop as keyof CSSStyleDeclaration];
    if (typeof value === "string") {
      (style as unknown as Record<string, string>)[prop] = value;
    }
  }
  style.width = computed.width;

  div.textContent = el.value.slice(0, position);
  const marker = document.createElement("span");
  marker.textContent = el.value.slice(position) || ".";
  div.appendChild(marker);

  const coords: CaretCoordinates = {
    left: marker.offsetLeft - el.scrollLeft,
    top: marker.offsetTop - el.scrollTop,
    height: marker.offsetHeight,
  };

  document.body.removeChild(div);
  return coords;
}
