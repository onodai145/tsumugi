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
