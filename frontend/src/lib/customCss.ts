// カスタムCSS機能（Issue #93）。UiPrefs.customCss を <head> 末尾の <style> へ反映する。
// textContent 経由なので、CSS内の "</style>" 等が <style> の外へ漏れることは無い。
const STYLE_ID = "tsumugi-custom-css";

export function applyCustomCss(css: string, safeMode: boolean): void {
  let el = document.getElementById(STYLE_ID);
  if (!el) {
    el = document.createElement("style");
    el.id = STYLE_ID;
  }
  // 常に <head> の末尾へ移す。app.css や後から挿入された <style> よりも後ろに置き、
  // 同じ詳細度ならユーザーCSSが勝つようにする。
  document.head.appendChild(el);
  el.textContent = safeMode ? "" : css;
}
