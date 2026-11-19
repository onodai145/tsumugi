import { openUrl } from "@tauri-apps/plugin-opener";

/** OS 側(既定のブラウザやメーラー等)へ渡すスキーム。opener プラグインの既定と同じ。 */
const EXTERNAL_PROTOCOLS = new Set(["http:", "https:", "mailto:", "tel:"]);

/**
 * 外部リンクのクリックを、WebView 自身の遷移ではなく OS 側で開く(Issue #657)。
 *
 * opener プラグインにも同じ働きをする `window` のバブリング click があるが、モーダルのダイアログ本体
 * など `stopPropagation()` する祖先の下では `window` まで届かない。届かないと `<a target="_blank">` が
 * WebView 自身でそのページへ遷移し(メイン WebView に外部ページが載り、戻ると状態も失われる)、
 * スマホのプロフィールで起きていた。ここは**捕捉フェーズ**で受けるので、祖先が伝播を止めても先に届く。
 * 先に `preventDefault()` するため、プラグイン側(`defaultPrevented` なら何もしない)と二重に開くことはない。
 *
 * 戻り値は解除関数。
 */
export function installExternalLinkHandler(): () => void {
  const onClick = (e: MouseEvent) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.altKey) return;
    if (!(e.target instanceof Element)) return;
    const anchor = e.target.closest("a");
    if (!anchor) return;
    let url: URL;
    try {
      url = new URL(anchor.href);
    } catch {
      return;
    }
    if (!EXTERNAL_PROTOCOLS.has(url.protocol)) return;
    if ((url.protocol === "http:" || url.protocol === "https:") && url.origin === window.location.origin) return;
    e.preventDefault();
    openUrl(url.href).catch((err) => console.error("failed to open external link", err));
  };
  window.addEventListener("click", onClick, true);
  return () => window.removeEventListener("click", onClick, true);
}
