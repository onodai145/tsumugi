import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MmlScore, NoteEvent, Timbre } from "../bindings/tauri.gen";

type Call = { hz: number; d: number; t: number; v: number; midi?: number };
const created: { timbre: string; volumeDb: number; calls: Call[] }[] = [];

vi.mock("./mmlTimbres", () => {
  const mk = (name: string, gainDb: number, perNote = false) => ({
    number: 0,
    name,
    gainDb,
    releaseSec: 0.1,
    perNote,
    create: (_Tone: unknown, volumeDb: number) => {
      const rec = { timbre: name, volumeDb, calls: [] as Call[] };
      created.push(rec);
      return {
        trigger: (hz: number, d: number, t: number, v: number, midi?: number) => rec.calls.push({ hz, d, t, v, midi }),
      };
    },
  });
  return { TIMBRES: { fm: mk("fm", 0), square: mk("square", -10), noise: mk("noise", -4.5, true) } };
});

const offline = vi.fn(async (cb: () => void, _dur: number, _ch: number, sr: number) => {
  cb();
  return { get: () => ({ sampleRate: sr, getChannelData: () => new Float32Array([0, 0.5, -0.5]) }) };
});
vi.mock("tone", () => ({
  Offline: offline,
  Frequency: (midi: number) => ({ toFrequency: () => midi * 10 }),
  gainToDb: (g: number) => 20 * Math.log10(g),
}));

const { renderScore, TAIL_SEC } = await import("./mmlRender");

const note = (track: number, timbre: Timbre, startSec = 0, midi = 60): NoteEvent => ({
  track,
  startSec,
  durationSec: 0.25,
  midi,
  velocity: 0.5,
  timbre,
});
const score = (notes: NoteEvent[], trackCount = 1, totalSec = 1): MmlScore => ({ notes, totalSec, trackCount });

beforeEach(() => {
  created.length = 0;
  offline.mockClear();
});

describe("renderScore", () => {
  it("WAV の data: URL を返し、長さは totalSec + TAIL_SEC(モノラル・22.05kHz)", async () => {
    const url = await renderScore(score([note(0, "fm")], 1, 2));
    expect(url.startsWith("data:audio/wav;base64,")).toBe(true);
    expect(offline).toHaveBeenCalledWith(expect.any(Function), 2 + TAIL_SEC, 1, 22050);
  });

  it("同じ (トラック, 音色) の音は 1 つの音源で鳴らす", async () => {
    await renderScore(score([note(0, "fm", 0), note(0, "fm", 0.5), note(0, "fm", 1)]));
    expect(created).toHaveLength(1);
    expect(created[0].calls.map((c) => c.t)).toEqual([0, 0.5, 1]);
    expect(created[0].calls[0]).toEqual({ hz: 600, d: 0.25, t: 0, v: 0.5, midi: 60 });
  });

  it("別の音色・別のトラックは別の音源になる", async () => {
    await renderScore(score([note(0, "fm"), note(0, "square", 0.5), note(1, "fm"), note(1, "fm", 0.5)], 2));
    expect(created.map((c) => c.timbre).sort()).toEqual(["fm", "fm", "square"]);
  });

  it("perNote の音色は 1 音ごとに音源を作る", async () => {
    await renderScore(score([note(0, "noise", 0), note(0, "noise", 0.5), note(0, "noise", 1)]));
    expect(created).toHaveLength(3);
    expect(created.every((c) => c.calls.length === 1)).toBe(true);
  });

  it("音源の trigger に、ノート番号 midi も渡す", async () => {
    await renderScore(score([note(0, "fm", 0, 36), note(0, "fm", 0.5, 42)]));
    expect(created[0].calls.map((c) => c.midi)).toEqual([36, 42]);
  });

  it("音量は gainDb に、複数トラックのときだけ 1/トラック数 の減衰を足す", async () => {
    await renderScore(score([note(0, "square")], 1));
    expect(created[0].volumeDb).toBe(-10);

    created.length = 0;
    await renderScore(score([note(0, "square"), note(1, "fm")], 2));
    const byName = Object.fromEntries(created.map((c) => [c.timbre, c.volumeDb]));
    expect(byName.square).toBeCloseTo(-10 + 20 * Math.log10(1 / 2), 6);
    expect(byName.fm).toBeCloseTo(20 * Math.log10(1 / 2), 6);
  });

  it("start/duration/velocity が null(非有限値)なら 0 として鳴らす", async () => {
    const n = { ...note(0, "fm"), startSec: null, durationSec: null, velocity: null } as unknown as NoteEvent;
    await renderScore(score([n]));
    expect(created[0].calls[0]).toMatchObject({ d: 0, t: 0, v: 0 });
  });

  it("レンダリング結果が空なら例外にする", async () => {
    offline.mockResolvedValueOnce({ get: () => undefined } as never);
    await expect(renderScore(score([note(0, "fm")]))).rejects.toThrow("空");
  });
});
