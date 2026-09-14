// タッチでの長押しドラッグ検知(Issue #354)。native drag-and-dropはタッチでは
// dragstartが発火しないため、pointerdown起点で「長押し(400ms)が成立したらドラッグ開始」
// という状態遷移をDOM非依存の純粋なロジックとして提供する。
// 指が閾値(8px)以上動いた状態でタイマーが成立前なら中断し、タップ/横スクロールに委ねる。

export const LONG_PRESS_MS = 400;
export const CANCEL_THRESHOLD_PX = 8;

export interface LongPressDragController {
  /** 長押しが成立してドラッグ中かどうか。 */
  readonly armed: boolean;
  onPointerDown(x: number, y: number): void;
  onPointerMove(x: number, y: number): void;
  onPointerUp(): void;
  onPointerCancel(): void;
}

/// callbacks.onArmedは、長押しタイマーが成立した瞬間に一度だけ呼ばれる
/// (呼び出し側はここでvibrate・視覚効果の付与・並び替え開始処理を行う)。
export function createLongPressDrag(callbacks: { onArmed: () => void }): LongPressDragController {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let startX = 0;
  let startY = 0;
  let armed = false;

  function clearTimer() {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  }

  return {
    get armed() {
      return armed;
    },
    onPointerDown(x, y) {
      clearTimer();
      armed = false;
      startX = x;
      startY = y;
      timer = setTimeout(() => {
        timer = null;
        armed = true;
        callbacks.onArmed();
      }, LONG_PRESS_MS);
    },
    onPointerMove(x, y) {
      if (armed || timer === null) return;
      const dx = x - startX;
      const dy = y - startY;
      if (Math.hypot(dx, dy) > CANCEL_THRESHOLD_PX) clearTimer();
    },
    onPointerUp() {
      clearTimer();
      armed = false;
    },
    onPointerCancel() {
      clearTimer();
      armed = false;
    },
  };
}
