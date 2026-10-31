import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DelayParams, Glide, MmlScore, NoteEvent, ReverbParams, Timbre, TimbreParams } from "../bindings/tauri.gen";
import { MAX_EFFECT_TAIL_SEC } from "./mmlEffects";

type Call = { hz: number; d: number; t: number; v: number; midi?: number; glide?: { fromHz: number; sec: number } };
const created: { timbre: string; volumeDb: number; params: unknown; out: unknown; calls: Call[] }[] = [];

/** Tone のエフェクト用ノードの代役。作ったもの・送りの設定を記録する */
const toneNodes: { kind: string; args: unknown[]; connected: unknown[]; destinations: number; sets: [number, number][]; ramps: [number, number][] }[] = [];
let reverbReady: Promise<unknown> = Promise.resolve();
class FakeToneNode {
  connected: unknown[] = [];
  destinations = 0;
  sets: [number, number][] = [];
  ramps: [number, number][] = [];
  gain = {
    setValueAtTime: (v: number, t: number) => void this.sets.push([v, t]),
    linearRampToValueAtTime: (v: number, t: number) => void this.ramps.push([v, t]),
  };
  ready: Promise<unknown>;
  constructor(
    public kind: string,
    public args: unknown[],
  ) {
    this.ready = kind === "Reverb" ? reverbReady : Promise.resolve();
    toneNodes.push(this);
  }
  connect(to: unknown) {
    this.connected.push(to);
    return this;
  }
  toDestination() {
    this.destinations++;
    return this;
  }
}
const toneClass = (kind: string) =>
  class extends FakeToneNode {
    constructor(...args: unknown[]) {
      super(kind, args);
    }
  };

vi.mock("./mmlTimbres", () => {
  const mk = (name: string, gainDb: number, perNote = false, percussive = false) => ({
    number: 0,
    name,
    gainDb,
    releaseSec: 0.1,
    perNote,
    percussive,
    create: (_Tone: unknown, volumeDb: number, params?: unknown, out?: unknown) => {
      const rec = { timbre: name, volumeDb, params, out, calls: [] as Call[] };
      created.push(rec);
      return {
        trigger: (hz: number, d: number, t: number, v: number, midi?: number, glide?: { fromHz: number; sec: number }) =>
          rec.calls.push({ hz, d, t, v, midi, glide }),
      };
    },
  });
  return {
    TIMBRES: { fm: mk("fm", 0), square: mk("square", -10), noise: mk("noise", -4.5, true, true), drum: mk("drum", -6, false, true) },
  };
});

let activeRenders = 0;
let maxActiveRenders = 0;
const offline = vi.fn(async (cb: () => void | Promise<void>, _dur: number, _ch: number, sr: number) => {
  activeRenders++;
  maxActiveRenders = Math.max(maxActiveRenders, activeRenders);
  try {
    await cb();
  } finally {
    activeRenders--;
  }
  return { get: () => ({ sampleRate: sr, getChannelData: () => new Float32Array([0, 0.5, -0.5]) }) };
});
vi.mock("tone", () => ({
  Offline: offline,
  Frequency: (midi: number) => ({ toFrequency: () => midi * 10 }),
  gainToDb: (g: number) => 20 * Math.log10(g),
  Gain: toneClass("Gain"),
  Reverb: toneClass("Reverb"),
  FeedbackDelay: toneClass("FeedbackDelay"),
}));

const { renderScore, TAIL_SEC } = await import("./mmlRender");

const note = (
  track: number,
  timbre: Timbre,
  startSec = 0,
  midi = 60,
  params: TimbreParams | null = null,
  glide: Glide | null = null,
): NoteEvent => ({
  track,
  startSec,
  durationSec: 0.25,
  midi,
  velocity: 0.5,
  timbre,
  params,
  glide,
  reverb: null,
  delay: null,
});
const score = (notes: NoteEvent[], trackCount = 1, totalSec = 1): MmlScore => ({ notes, totalSec, trackCount });

beforeEach(() => {
  created.length = 0;
  offline.mockClear();
  toneNodes.length = 0;
  reverbReady = Promise.resolve();
  activeRenders = 0;
  maxActiveRenders = 0;
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

  const P1: TimbreParams = { envelope: { attackMs: 10, decayMs: 200, sustain: 60, releaseMs: 300 }, vibrato: null, detune: 0 };
  const P2: TimbreParams = { envelope: null, vibrato: { rateHz: 5, depth: 30 }, detune: 12 };

  it("音源を作るとき、パラメーターを渡す。パラメーターなしは null", async () => {
    await renderScore(score([note(0, "fm", 0, 60, P1), note(0, "square", 0.5)]));
    expect(created.map((c) => [c.timbre, c.params])).toEqual([
      ["fm", P1],
      ["square", null],
    ]);
  });

  it("同じ (トラック, 音色, パラメーター) の音は 1 つの音源、パラメーターが違えば別の音源", async () => {
    await renderScore(
      score([note(0, "fm", 0, 60, P1), note(0, "fm", 0.5, 62, { ...P1 }), note(0, "fm", 1, 64, P2), note(0, "fm", 1.5, 65)]),
    );
    expect(created).toHaveLength(3);
    expect(created.map((c) => c.calls.length).sort()).toEqual([1, 1, 2]);
  });

  it("デチューンは音源の組に数えず、鳴らす周波数に 2^(セント/1200) を掛ける", async () => {
    const detuned = (cents: number): TimbreParams => ({ envelope: null, vibrato: null, detune: cents });
    await renderScore(
      score([note(0, "fm", 0, 60, detuned(1200)), note(0, "fm", 0.5, 60, detuned(-1200)), note(0, "fm", 1, 60, detuned(0)), note(0, "fm", 1.5, 60)]),
    );
    expect(created).toHaveLength(1); // 同じ音源(パラメーターは null)
    expect(created[0].params).toBeNull();
    expect(created[0].calls.map((c) => c.hz)).toEqual([1200, 300, 600, 600]);
  });

  it("デチューンがあっても、エンベロープ・ビブラートが同じなら 1 つの音源で、音源に渡すパラメーターの detune は 0", async () => {
    await renderScore(score([note(0, "fm", 0, 60, { ...P1, detune: 100 }), note(0, "fm", 0.5, 60, { ...P1, detune: -100 })]));
    expect(created).toHaveLength(1);
    expect(created[0].params).toEqual({ ...P1, detune: 0 });
    expect(created[0].calls.map((c) => c.hz)).toEqual([600 * 2 ** (100 / 1200), 600 * 2 ** (-100 / 1200)]);
  });

  it("打楽器は、デチューンも無視する", async () => {
    await renderScore(score([note(0, "drum", 0, 60, { envelope: null, vibrato: null, detune: 1200 })]));
    expect(created[0].calls[0].hz).toBe(600);
  });

  it("打楽器の音色は、パラメーターが違っても 1 つの音源を使い回し、パラメーターは null で作る", async () => {
    await renderScore(score([note(0, "drum", 0, 60, P1), note(0, "drum", 0.5, 60, P2), note(0, "drum", 1)]));
    expect(created).toHaveLength(1);
    expect(created[0].params).toBeNull();
    expect(created[0].calls).toHaveLength(3);
  });

  it("単音用(perNote)の打楽器も、パラメーターは null で作る", async () => {
    await renderScore(score([note(0, "noise", 0, 60, P1)]));
    expect(created[0].params).toBeNull();
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

describe("renderScore: ポルタメント(Issue #499)", () => {
  const g = (fromMidi: number, ms: number): Glide => ({ fromMidi, ms });

  it("glide を、滑り始めの周波数 fromHz と秒 sec にして、音源の trigger に渡す", async () => {
    await renderScore(score([note(0, "fm", 0, 60), note(0, "fm", 0.5, 64, null, g(60, 80))]));
    expect(created[0].calls[0].glide).toBeUndefined();
    expect(created[0].calls[1].hz).toBe(640);
    expect(created[0].calls[1].glide).toEqual({ fromHz: 600, sec: 0.08 });
  });

  it("滑りが音の長さ(0.25 秒)より長いときは、音の長さに縮める", async () => {
    await renderScore(score([note(0, "fm", 0.5, 64, null, g(60, 1000))]));
    expect(created[0].calls[0].glide).toEqual({ fromHz: 600, sec: 0.25 });
  });

  it("音の長さが 0 のときは、glide を渡さない(0 秒の滑りはない)", async () => {
    const n = { ...note(0, "fm", 0.5, 64, null, g(60, 80)), durationSec: 0 };
    await renderScore(score([n]));
    expect(created[0].calls[0].glide).toBeUndefined();
  });

  it("デチューンは、滑り始めと目的の高さの両方に同じ倍率で掛かる", async () => {
    const octave: TimbreParams = { envelope: null, vibrato: null, detune: 1200 };
    await renderScore(score([note(0, "fm", 0.5, 64, octave, g(60, 80))]));
    expect(created[0].calls[0].hz).toBe(1280);
    expect(created[0].calls[0].glide).toEqual({ fromHz: 1200, sec: 0.08 });
  });

  it("glide があっても、音源の組は増えない", async () => {
    await renderScore(score([note(0, "fm", 0, 60), note(0, "fm", 0.5, 64, null, g(60, 80)), note(0, "fm", 1, 67, null, g(64, 80))]));
    expect(created).toHaveLength(1);
    expect(created[0].calls).toHaveLength(3);
  });

  it("打楽器は glide を無視する", async () => {
    await renderScore(score([note(0, "drum", 0.5, 64, null, g(60, 80)), note(0, "noise", 1, 64, null, g(60, 80))]));
    expect(created.every((c) => c.calls.every((x) => x.glide === undefined))).toBe(true);
  });
});

describe("renderScore: エフェクト(Issue #499)", () => {
  const withFx = (n: NoteEvent, fx: { reverb?: ReverbParams; delay?: DelayParams }): NoteEvent => ({
    ...n,
    reverb: fx.reverb ?? null,
    delay: fx.delay ?? null,
  });
  const reverb = (level: number, decayMs: number): ReverbParams => ({ level, decayMs });
  const delay = (level: number, timeMs: number, feedback: number): DelayParams => ({ level, timeMs, feedback });

  it("エフェクトを使わない MML は、音源のつなぎ先を渡さず、エフェクト用のノードも作らない", async () => {
    await renderScore(score([note(0, "fm"), note(1, "square")], 2));
    expect(created.every((c) => c.out === undefined)).toBe(true);
    expect(toneNodes).toHaveLength(0);
  });

  it("エフェクトを使うトラックだけ、音源をそのトラックのバスにつなぐ", async () => {
    await renderScore(score([withFx(note(0, "fm"), { reverb: reverb(30, 2000) }), note(1, "fm")], 2));
    const bus = toneNodes.find((n) => n.kind === "Gain")!;
    expect(created[0].out).toBe(bus);
    expect(created[1].out).toBeUndefined();
  });

  it("エフェクトの量・種類が違っても、音源の組は増えない", async () => {
    await renderScore(
      score([
        withFx(note(0, "fm", 0), { reverb: reverb(30, 2000) }),
        withFx(note(0, "fm", 0.5), { reverb: reverb(60, 500) }),
        withFx(note(0, "fm", 1), { delay: delay(25, 300, 40) }),
        note(0, "fm", 1.5),
      ]),
    );
    expect(created).toHaveLength(1);
    expect(created[0].calls).toHaveLength(4);
  });

  it("描画の長さは、曲の長さ + TAIL_SEC + エフェクトの尾(最大 5 秒)", async () => {
    await renderScore(score([withFx(note(0, "fm"), { reverb: reverb(30, 2000) })], 1, 2));
    expect(offline.mock.calls[0][1]).toBeCloseTo(2 + TAIL_SEC + 2.01, 6);
    offline.mockClear();
    await renderScore(score([withFx(note(0, "fm"), { delay: delay(100, 1000, 90) })], 1, 2));
    expect(offline.mock.calls[0][1]).toBeCloseTo(2 + TAIL_SEC + MAX_EFFECT_TAIL_SEC, 6);
  });

  it("送りは、各音の始まりの時刻に、その音の組の量にする。ほかの組は 0 にする", async () => {
    await renderScore(
      score([
        withFx(note(0, "fm", 0), { reverb: reverb(30, 2000) }),
        withFx(note(0, "fm", 0.5), { reverb: reverb(60, 2000) }),
        withFx(note(0, "fm", 1), { reverb: reverb(40, 500) }),
        note(0, "fm", 1.5),
      ]),
    );
    const sends = toneNodes.filter((n) => n.kind === "Gain" && n.args[0] === 0); // 2000ms の組、500ms の組
    expect(sends[0].sets).toEqual([[0.3, 0], [0.6, 0.5], [0, 1]]);
    expect(sends[1].sets).toEqual([[0.4, 1], [0, 1.5]]);
  });

  it("リバーブの準備(インパルス応答の生成)が終わるまで、音を鳴らさない", async () => {
    let release!: () => void;
    reverbReady = new Promise<void>((resolve) => (release = resolve));
    const done = renderScore(score([withFx(note(0, "fm"), { reverb: reverb(30, 2000) })]));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(created).toHaveLength(0);
    release();
    await done;
    expect(created[0].calls).toHaveLength(1);
  });

  it("リバーブ入りの描画が同時に走っても、Tone のコンテキストが混ざらないよう、1 つずつ順に走らせる", async () => {
    let release!: () => void;
    reverbReady = new Promise<void>((resolve) => (release = resolve));
    const s = score([withFx(note(0, "fm"), { reverb: reverb(30, 2000) })]);
    const first = renderScore(s);
    await new Promise((resolve) => setTimeout(resolve, 0)); // 1 つ目が、リバーブの準備待ちに入ってから
    const second = renderScore(s);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(maxActiveRenders).toBe(1);
    expect(offline).toHaveBeenCalledTimes(1); // 2 つ目は、1 つ目が終わるまで始まらない
    release();
    await Promise.all([first, second]);
    expect(offline).toHaveBeenCalledTimes(2);
    expect(maxActiveRenders).toBe(1);
  });

  it("描画が失敗しても、次の描画は走る", async () => {
    offline.mockRejectedValueOnce(new Error("boom"));
    await expect(renderScore(score([note(0, "fm")]))).rejects.toThrow("boom");
    await expect(renderScore(score([note(0, "fm")]))).resolves.toMatch(/^data:audio\/wav/);
  });

  it("エフェクトの出力は、描画の終わりの手前で 0 までフェードする(尾が頭打ちで急に切れないように)", async () => {
    await renderScore(score([withFx(note(0, "fm"), { delay: delay(100, 1000, 90) })], 1, 2));
    const end = 2 + TAIL_SEC + MAX_EFFECT_TAIL_SEC;
    const trim = toneNodes.find((n) => n.kind === "Gain" && n.args[0] === 1 && n.destinations === 1 && n.connected.length === 0)!;
    expect(trim.ramps).toEqual([[0, end]]);
    expect(trim.sets.at(-1)![1]).toBeCloseTo(end - 0.3, 6);
  });
});
