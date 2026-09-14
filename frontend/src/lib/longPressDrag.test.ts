import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CANCEL_THRESHOLD_PX, LONG_PRESS_MS, createLongPressDrag } from "./longPressDrag";

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

describe("createLongPressDrag", () => {
  it("400ms経過するとarmedになりonArmedが呼ばれる", () => {
    const onArmed = vi.fn();
    const ctl = createLongPressDrag({ onArmed });
    ctl.onPointerDown(0, 0);
    expect(ctl.armed).toBe(false);
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(ctl.armed).toBe(true);
    expect(onArmed).toHaveBeenCalledTimes(1);
  });

  it("成立前に閾値を超えて動くと中断し、armedにならない", () => {
    const onArmed = vi.fn();
    const ctl = createLongPressDrag({ onArmed });
    ctl.onPointerDown(0, 0);
    ctl.onPointerMove(CANCEL_THRESHOLD_PX + 1, 0);
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(ctl.armed).toBe(false);
    expect(onArmed).not.toHaveBeenCalled();
  });

  it("閾値以内の動きでは中断しない", () => {
    const ctl = createLongPressDrag({ onArmed: () => {} });
    ctl.onPointerDown(0, 0);
    ctl.onPointerMove(CANCEL_THRESHOLD_PX, 0);
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(ctl.armed).toBe(true);
  });

  it("成立前にpointerupが来ると中断する", () => {
    const onArmed = vi.fn();
    const ctl = createLongPressDrag({ onArmed });
    ctl.onPointerDown(0, 0);
    ctl.onPointerUp();
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(ctl.armed).toBe(false);
    expect(onArmed).not.toHaveBeenCalled();
  });

  it("pointercancelでも中断する", () => {
    const onArmed = vi.fn();
    const ctl = createLongPressDrag({ onArmed });
    ctl.onPointerDown(0, 0);
    ctl.onPointerCancel();
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(ctl.armed).toBe(false);
    expect(onArmed).not.toHaveBeenCalled();
  });

  it("成立後にpointerupするとarmedがfalseに戻る", () => {
    const ctl = createLongPressDrag({ onArmed: () => {} });
    ctl.onPointerDown(0, 0);
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(ctl.armed).toBe(true);
    ctl.onPointerUp();
    expect(ctl.armed).toBe(false);
  });
});
