// ダイアログの要素(role="dialog" aria-modal="true")に付けるSvelteアクション(Issue #698 / #700)。
// フォーカス中の要素が消える・無効になると、フォーカスは body に移り、ダイアログ側の onkeydown には
// Escape が届かなくなる。その場合だけ window で受け、一番手前の画面が自分なら閉じる。
// data-escape-layer は、aria-modal を持たずに自分で Escape を処理する画面(MediaViewer)の目印。
export function escapeOnLostFocus(node: HTMLElement, close: () => void) {
  function onKeydown(e: KeyboardEvent) {
    if (e.key !== "Escape" || e.defaultPrevented || e.target !== document.body) return;
    const layers = document.querySelectorAll('[aria-modal="true"], [data-escape-layer]');
    if (layers[layers.length - 1] !== node) return;
    // 閉じた直後に、下の画面が同じ Escape で自分を一番手前と判定しないようにする。
    e.preventDefault();
    close();
  }
  window.addEventListener("keydown", onKeydown);
  return {
    update(next: () => void) {
      close = next;
    },
    destroy() {
      window.removeEventListener("keydown", onKeydown);
    },
  };
}
