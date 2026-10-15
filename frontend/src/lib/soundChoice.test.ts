import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../bindings/tauri.gen", () => ({ commands: { parseMml: vi.fn() } }));
vi.mock("./mmlRender", () => ({ renderScore: vi.fn() }));

import { commands } from "../bindings/tauri.gen";
import { renderScore } from "./mmlRender";
import { clearSoundChoiceCache, prewarmSoundChoice, resolveSoundChoice } from "./soundChoice";

const parseMml = vi.mocked(commands.parseMml);
const render = vi.mocked(renderScore);

const SCORE = { notes: [], totalSec: 1, trackCount: 1 };
const parsed = { status: "parsed" as const, score: SCORE };

beforeEach(() => {
  clearSoundChoiceCache();
  parseMml.mockReset();
  render.mockReset();
  parseMml.mockResolvedValue(parsed);
  render.mockResolvedValue("data:audio/wav;base64,AAAA");
});

describe("resolveSoundChoice", () => {
  it("mml: 以外(空・プリセット・data:)はそのまま返し、パースもレンダリングもしない", async () => {
    for (const c of ["", "beep", "chime", "data:audio/wav;base64,AAAA"]) {
      expect(await resolveSoundChoice(c)).toBe(c);
    }
    expect(parseMml).not.toHaveBeenCalled();
    expect(render).not.toHaveBeenCalled();
  });

  it("mml: は パース → レンダリング の順に処理して data: URL を返す", async () => {
    expect(await resolveSoundChoice("mml:c4d4")).toBe("data:audio/wav;base64,AAAA");
    expect(parseMml).toHaveBeenCalledWith("c4d4");
    expect(render).toHaveBeenCalledWith(SCORE);
  });

  it("同じ MML は 2 回目以降メモ化された結果を返す", async () => {
    await resolveSoundChoice("mml:c4");
    await resolveSoundChoice("mml:c4");
    expect(parseMml).toHaveBeenCalledTimes(1);
    expect(render).toHaveBeenCalledTimes(1);
  });

  it("レンダリング中に同じ MML が再度要求されても二重にレンダリングしない", async () => {
    let finish!: (v: string) => void;
    render.mockReturnValue(new Promise<string>((r) => (finish = r)));
    const a = resolveSoundChoice("mml:c4");
    const b = resolveSoundChoice("mml:c4");
    await vi.waitFor(() => expect(render).toHaveBeenCalledTimes(1));
    finish("data:audio/wav;base64,BBBB");
    expect(await a).toBe("data:audio/wav;base64,BBBB");
    expect(await b).toBe("data:audio/wav;base64,BBBB");
    expect(render).toHaveBeenCalledTimes(1);
  });

  it("構文エラーは位置と理由を含む Error で reject し、失敗はキャッシュしない", async () => {
    parseMml.mockResolvedValueOnce({ status: "invalid", error: { position: 3, message: "不明な文字 `x` です" } });
    await expect(resolveSoundChoice("mml:c4 x")).rejects.toThrow(/位置 3.*不明な文字/);
    expect(render).not.toHaveBeenCalled();
    parseMml.mockResolvedValueOnce(parsed);
    await expect(resolveSoundChoice("mml:c4 x")).resolves.toBe("data:audio/wav;base64,AAAA");
  });

  it("レンダリング失敗は reject し、次の呼び出しでやり直せる", async () => {
    render.mockRejectedValueOnce(new Error("offline failed"));
    await expect(resolveSoundChoice("mml:c4")).rejects.toThrow("offline failed");
    await expect(resolveSoundChoice("mml:c4")).resolves.toBe("data:audio/wav;base64,AAAA");
    expect(render).toHaveBeenCalledTimes(2);
  });

  it("キャッシュは 16 件まで。古いものから捨てる", async () => {
    for (let i = 0; i < 17; i++) await resolveSoundChoice(`mml:c${i}`);
    expect(parseMml).toHaveBeenCalledTimes(17);
    await resolveSoundChoice("mml:c16"); // 新しいものは残っている
    expect(parseMml).toHaveBeenCalledTimes(17);
    await resolveSoundChoice("mml:c0"); // 最古は捨てられている
    expect(parseMml).toHaveBeenCalledTimes(18);
  });
});

describe("prewarmSoundChoice", () => {
  it("mml: だけを先にレンダリングし、失敗しても例外を投げない", async () => {
    prewarmSoundChoice("beep");
    expect(parseMml).not.toHaveBeenCalled();
    render.mockRejectedValueOnce(new Error("boom"));
    expect(() => prewarmSoundChoice("mml:c4")).not.toThrow();
    await vi.waitFor(() => expect(render).toHaveBeenCalledTimes(1));
  });
});
