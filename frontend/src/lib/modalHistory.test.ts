import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// history.back()のpopstateは非同期なので、イベントループを少し回して待つ
const settle = () => new Promise<void>((r) => setTimeout(r, 20));
// release直後に積まれるマイクロタスク(history.goの一括発行)を流す
const flushMicrotasks = () => Promise.resolve();

let mod: typeof import("./modalHistory");
const releases: Array<() => void> = [];

function open(onclose = vi.fn()) {
  const release = mod.pushModalHistory(onclose);
  releases.push(release);
  return { onclose, release };
}

beforeEach(async () => {
  // モジュール内のスタック/カウンタをテストごとに初期化する
  vi.resetModules();
  mod = await import("./modalHistory");
});

afterEach(async () => {
  vi.useRealTimers();
  releases.splice(0).forEach((r) => r());
  await settle();
  vi.restoreAllMocks();
});

describe("modalHistory", () => {
  it("開くと履歴を1つ積む", () => {
    const push = vi.spyOn(history, "pushState");
    open();
    expect(push).toHaveBeenCalledTimes(1);
    expect(push.mock.calls[0][0]).toEqual({ tsumugiModal: expect.any(Number) });
  });

  it("UIで閉じると履歴を1つ戻し、そのpopstateではoncloseを呼ばない", async () => {
    const go = vi.spyOn(history, "go");
    const { onclose, release } = open();
    release();
    await flushMicrotasks();
    expect(go).toHaveBeenCalledTimes(1);
    expect(go).toHaveBeenCalledWith(-1);
    await settle();
    expect(onclose).not.toHaveBeenCalled();
  });

  it("戻る操作で最上段のoncloseを呼び、その後のreleaseでは履歴を戻さない", async () => {
    const { onclose, release } = open();
    history.back();
    await settle();
    expect(onclose).toHaveBeenCalledTimes(1);
    const go = vi.spyOn(history, "go");
    release();
    await flushMicrotasks();
    expect(go).not.toHaveBeenCalled();
  });

  it("重なっていると戻る操作ごとに1枚ずつ閉じる", async () => {
    const a = open();
    const b = open();
    history.back();
    await settle();
    expect(b.onclose).toHaveBeenCalledTimes(1);
    expect(a.onclose).not.toHaveBeenCalled();
    b.release();
    history.back();
    await settle();
    expect(a.onclose).toHaveBeenCalledTimes(1);
  });

  it("同一tickに2枚閉じるとhistory.go(-2)を1回だけ発行し、二重にoncloseせず、その後のpushは即時に行われる", async () => {
    const go = vi.spyOn(history, "go");
    const a = open();
    const b = open();
    b.release();
    a.release();
    await flushMicrotasks();
    expect(go).toHaveBeenCalledTimes(1);
    expect(go).toHaveBeenCalledWith(-2);
    await settle();
    expect(a.onclose).not.toHaveBeenCalled();
    expect(b.onclose).not.toHaveBeenCalled();
    const push = vi.spyOn(history, "pushState");
    open();
    expect(push).toHaveBeenCalledTimes(1);
  });

  it("閉じた直後に開いたモーダルのpushは、履歴を戻す操作の完了まで遅延される", async () => {
    const a = open();
    a.release();
    const push = vi.spyOn(history, "pushState");
    open();
    expect(push).not.toHaveBeenCalled();
    await flushMicrotasks();
    expect(push).not.toHaveBeenCalled();
    await settle();
    expect(push).toHaveBeenCalledTimes(1);
    expect(push.mock.calls[0][0]).toEqual({ tsumugiModal: expect.any(Number) });
  });

  it("releaseの二重呼び出しは何もしない", async () => {
    const go = vi.spyOn(history, "go");
    const { release } = open();
    release();
    release();
    await flushMicrotasks();
    expect(go).toHaveBeenCalledTimes(1);
    expect(go).toHaveBeenCalledWith(-1);
  });

  it("自分の履歴に着地したpopstate(遅れて届いたもの等)ではoncloseしない", () => {
    const push = vi.spyOn(history, "pushState");
    const a = open();
    const id = push.mock.calls[0][0].tsumugiModal;
    window.dispatchEvent(new PopStateEvent("popstate", { state: { tsumugiModal: id } }));
    expect(a.onclose).not.toHaveBeenCalled();
  });

  it("より下の履歴に着地したpopstateは、積んである複数枚をまとめて閉じる", () => {
    const a = open();
    const b = open();
    window.dispatchEvent(new PopStateEvent("popstate", { state: null }));
    expect(b.onclose).toHaveBeenCalledTimes(1);
    expect(a.onclose).toHaveBeenCalledTimes(1);
  });

  it("history.goがpopstateを起こさなくても、一定時間後に遅延pushが再開する", async () => {
    vi.useFakeTimers();
    vi.spyOn(history, "go").mockImplementation(() => {});
    const a = open();
    a.release();
    await flushMicrotasks();
    const push = vi.spyOn(history, "pushState");
    open();
    expect(push).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1500);
    expect(push).toHaveBeenCalledTimes(1);
  });
});
