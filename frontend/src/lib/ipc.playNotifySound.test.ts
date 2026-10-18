import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../bindings/tauri.gen", () => ({ commands: { playNotifySound: vi.fn() } }));
vi.mock("./soundChoice", () => ({ resolveSoundChoice: vi.fn() }));

import { commands } from "../bindings/tauri.gen";
import { playNotifySound } from "./ipc";
import { resolveSoundChoice } from "./soundChoice";

const play = vi.mocked(commands.playNotifySound);
const resolve = vi.mocked(resolveSoundChoice);

beforeEach(() => {
  play.mockReset();
  resolve.mockReset();
  play.mockResolvedValue({ status: "ok", data: null });
});

describe("playNotifySound", () => {
  it("解決した値を Rust の play_notify_sound に渡す", async () => {
    resolve.mockResolvedValue("data:audio/wav;base64,AAAA");
    playNotifySound("mml:c4");
    await vi.waitFor(() => expect(play).toHaveBeenCalledWith("data:audio/wav;base64,AAAA"));
  });

  it("プリセットなどはそのまま渡す", async () => {
    resolve.mockImplementation(async (c) => c);
    playNotifySound("chime");
    await vi.waitFor(() => expect(play).toHaveBeenCalledWith("chime"));
  });

  it("解決に失敗したら beep で代替し、理由を onError に渡す(無音にしない)", async () => {
    const err = new Error("MML 構文エラー");
    resolve.mockRejectedValue(err);
    const onError = vi.fn();
    playNotifySound("mml:bad", onError);
    await vi.waitFor(() => expect(play).toHaveBeenCalledWith("beep"));
    expect(onError).toHaveBeenCalledWith(err);
  });

  it("onError が無くても解決失敗で beep を鳴らす", async () => {
    resolve.mockRejectedValue(new Error("x"));
    playNotifySound("mml:bad");
    await vi.waitFor(() => expect(play).toHaveBeenCalledWith("beep"));
  });

  it("IPC 自体が失敗しても未処理の reject を出さない", async () => {
    // Vitest は未処理の reject があるとテスト実行全体を失敗させるので、reject が漏れれば検知できる
    resolve.mockResolvedValue("beep");
    play.mockRejectedValue(new Error("ipc down"));
    playNotifySound("beep");
    await vi.waitFor(() => expect(play).toHaveBeenCalledTimes(1));
    await new Promise((r) => setTimeout(r, 20));
  });
});

describe("playNotifySound の生成待ち(Issue #500)", () => {
  it("生成が終わった時点で isStale() が true なら、鳴らさずに捨てる", async () => {
    let finish!: (v: string) => void;
    resolve.mockReturnValue(new Promise<string>((r) => (finish = r)));
    let stale = false;
    const onSettled = vi.fn();
    playNotifySound("mml:c4", undefined, { isStale: () => stale, onSettled });
    stale = true; // 生成中に、待たない・編集された
    finish("data:audio/wav;base64,AAAA");
    await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));
    expect(play).not.toHaveBeenCalled();
  });

  it("古くなければ鳴らし、鳴らし終えてから onSettled を 1 回呼ぶ", async () => {
    resolve.mockResolvedValue("data:audio/wav;base64,AAAA");
    const order: string[] = [];
    play.mockImplementation(async () => {
      order.push("play");
      return { status: "ok", data: null };
    });
    const onSettled = vi.fn(() => order.push("settled"));
    playNotifySound("mml:c4", undefined, { isStale: () => false, onSettled });
    await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));
    expect(order).toEqual(["play", "settled"]);
  });

  it("古い生成が失敗しても、beep も onError も出さない(捨てた生成の失敗を見せない)", async () => {
    let fail!: (e: Error) => void;
    resolve.mockReturnValue(new Promise<string>((_, rej) => (fail = rej)));
    let stale = false;
    const onError = vi.fn();
    const onSettled = vi.fn();
    playNotifySound("mml:bad", onError, { isStale: () => stale, onSettled });
    stale = true;
    fail(new Error("MML 構文エラー"));
    await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));
    expect(play).not.toHaveBeenCalled();
    expect(onError).not.toHaveBeenCalled();
  });

  it("古くない生成が失敗したら、従来どおり beep と onError で、onSettled も呼ぶ", async () => {
    const err = new Error("x");
    resolve.mockRejectedValue(err);
    const onError = vi.fn();
    const onSettled = vi.fn();
    playNotifySound("mml:bad", onError, { isStale: () => false, onSettled });
    await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));
    expect(play).toHaveBeenCalledWith("beep");
    expect(onError).toHaveBeenCalledWith(err);
  });

  it("IPC が失敗しても onSettled は呼ぶ(ボタンが生成中のまま固まらない)", async () => {
    resolve.mockResolvedValue("beep");
    play.mockRejectedValue(new Error("ipc down"));
    const onSettled = vi.fn();
    playNotifySound("beep", undefined, { onSettled });
    await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));
  });
});
