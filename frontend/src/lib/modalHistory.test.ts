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
});
