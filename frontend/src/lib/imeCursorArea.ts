// IME の変換候補ウィンドウの位置補正(Issue #502)。
// Linux/Wayland では WebKitGTK が IM に渡すキャレット位置が不正確なため、document のイベントを
// 監視して実キャレット矩形を求め、Rust の set_ime_cursor_area で IM コンテキストに上書きする。
// 起動するかの判定(Linux か)は呼び出し側(main.ts)の責務。Wayland かの判定は Rust 側で行う。
import { caretRect, isImeTarget, type CaretAnchor, type CaretRect } from "./caretRect";
import { setImeCursorArea } from "./ipc";

/// WebKit は編集状態が変わるたびに(変換中は 1 文字ごとに)自前の不正確な座標を送り直すので、
/// それより後に上書きする。実機(Hyprland + fcitx5)で、待ち時間 0 + 再送 40ms が最もちらつきが
/// 少なかった。1 回目は素早く送って候補のずれて見える時間を縮め、WebKit の送信が遅れて着いた
/// 場合に備えて 2 回目でもう一度上書きする。
const SEND_DELAY_MS = 0;
const RESEND_DELAY_MS = 40;

const DOCUMENT_EVENTS = [
  "focusin",
  "input",
  "keyup",
  "mouseup",
  "selectionchange",
  "compositionupdate",
] as const;

export interface ImeCursorAreaOptions {
  doc?: Document;
  send?: (rect: CaretRect) => void;
  delayMs?: number;
  /// 1 回目の送信の何 ms 後にもう一度送るか。0 なら再送しない。
  resendMs?: number;
}

/// 変換開始時点の位置。text 系は文字位置、contenteditable は選択範囲の先頭に畳んだ Range。
function recordAnchor(doc: Document): CaretAnchor {
  const el = doc.activeElement;
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    try {
      return el.selectionStart;
    } catch {
      return null;
    }
  }
  const selection = doc.getSelection();
  if (selection && selection.rangeCount > 0) {
    const range = selection.getRangeAt(0).cloneRange();
    range.collapse(true);
    return range;
  }
  return null;
}

/// 監視を始める。戻り値は解除関数(何度呼んでも安全)。
export function installImeCursorArea(options: ImeCursorAreaOptions = {}): () => void {
  const doc = options.doc ?? document;
  const send = options.send ?? setImeCursorArea;
  const delayMs = options.delayMs ?? SEND_DELAY_MS;
  const resendMs = options.resendMs ?? RESEND_DELAY_MS;

  let composing = false;
  let anchor: CaretAnchor = null;
  // 変換を始めた入力欄。compositionend が来ないまま(欄が消える・フォーカスが移る)別の欄へ
  // 移ったときに、古い変換状態を引きずらないために使う。
  let anchorEl: Element | null = null;
  let pending = false;
  let rafId: number | undefined;
  let timerId: ReturnType<typeof setTimeout> | undefined;
  let resendTimerId: ReturnType<typeof setTimeout> | undefined;

  const sendNow = () => {
    try {
      const el = doc.activeElement;
      if (!isImeTarget(el)) return;
      if (composing && el !== anchorEl) {
        composing = false;
        anchor = null;
        anchorEl = null;
      }
      const rect = caretRect(el, composing ? anchor : null);
      if (rect) send(rect);
    } catch {
      // 位置補正の失敗でタイピングを妨げない。
    }
  };

  const flush = () => {
    pending = false;
    sendNow();
    if (resendMs > 0) {
      if (resendTimerId !== undefined) clearTimeout(resendTimerId);
      resendTimerId = setTimeout(sendNow, resendMs);
    }
  };

  // 待機中に来たイベントは 1 回の送信にまとめる。待機は延長しない(キー連打で先送りされ続けない)。
  const schedule = () => {
    if (pending) return;
    pending = true;
    rafId = requestAnimationFrame(() => {
      timerId = setTimeout(flush, delayMs);
    });
  };

  const onCompositionStart = () => {
    composing = true;
    anchor = recordAnchor(doc);
    anchorEl = doc.activeElement;
    schedule();
  };
  const onCompositionEnd = () => {
    composing = false;
    anchor = null;
    anchorEl = null;
    schedule();
  };

  const passive = { passive: true } as const;
  const capturePassive = { capture: true, passive: true } as const;
  for (const type of DOCUMENT_EVENTS) doc.addEventListener(type, schedule, passive);
  doc.addEventListener("scroll", schedule, capturePassive);
  doc.addEventListener("compositionstart", onCompositionStart, passive);
  doc.addEventListener("compositionend", onCompositionEnd, passive);
  window.addEventListener("resize", schedule, passive);

  return () => {
    // 解除は capture の一致だけで足りる(removeEventListener のオプションに passive は無い)。
    for (const type of DOCUMENT_EVENTS) doc.removeEventListener(type, schedule);
    doc.removeEventListener("scroll", schedule, true);
    doc.removeEventListener("compositionstart", onCompositionStart);
    doc.removeEventListener("compositionend", onCompositionEnd);
    window.removeEventListener("resize", schedule);
    if (rafId !== undefined) cancelAnimationFrame(rafId);
    if (timerId !== undefined) clearTimeout(timerId);
    if (resendTimerId !== undefined) clearTimeout(resendTimerId);
    pending = false;
  };
}
