import { describe, expect, it } from "vitest";
import type { DelayParams, NoteEvent, ReverbParams } from "../bindings/tauri.gen";
import {
  DELAY_TRIM_DB,
  MAX_EFFECT_TAIL_SEC,
  REVERB_PREDELAY_SEC,
  REVERB_TRIM_DB,
  createTrackEffects,
  delayTailSec,
  effectTailSec,
  reverbTailSec,
} from "./mmlEffects";
import type { ToneModule, VoiceOutput } from "./mmlTimbres";

/** 描画の終わりの時刻(秒)。テストでは十分先に置く */
const END = 10;
const r = (level: number, decayMs: number): ReverbParams => ({ level, decayMs });
const d = (level: number, timeMs: number, feedback: number): DelayParams => ({ level, timeMs, feedback });
const note = (track: number, startSec: number, fx: { reverb?: ReverbParams; delay?: DelayParams } = {}): NoteEvent => ({
  track,
  startSec,
  durationSec: 0.25,
  midi: 60,
  velocity: 0.5,
  timbre: "fm",
  params: null,
  glide: null,
  reverb: fx.reverb ?? null,
  delay: fx.delay ?? null,
});

/** Tone のノードの代役。作ったもの・つないだ先・送りの設定を記録する */
function fakeTone(options: { reverbReady?: Promise<unknown> } = {}) {
  const nodes: FakeNode[] = [];
  class FakeNode {
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
      this.ready = kind === "Reverb" && options.reverbReady ? options.reverbReady : Promise.resolve();
      nodes.push(this);
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
  const cls = (kind: string) =>
    class extends FakeNode {
      constructor(...args: unknown[]) {
        super(kind, args);
      }
    };
  const Tone = { Gain: cls("Gain"), Reverb: cls("Reverb"), FeedbackDelay: cls("FeedbackDelay") } as unknown as ToneModule;
  return { Tone, nodes, of: (kind: string) => nodes.filter((n) => n.kind === kind) };
}

describe("エフェクトの尾", () => {
  it("リバーブの尾は、長さ + プリディレイ。5 秒で頭打ち", () => {
    expect(reverbTailSec(2000)).toBeCloseTo(2 + REVERB_PREDELAY_SEC, 6);
    expect(reverbTailSec(5000)).toBe(MAX_EFFECT_TAIL_SEC);
  });

  it("ディレイの尾は、間隔 ×(1/1000 に下がるまでの繰り返し回数 + 1)。5 秒で頭打ち", () => {
    expect(delayTailSec(300, 0)).toBeCloseTo(0.3, 6); // 繰り返しなし: 1 回だけ
    expect(delayTailSec(300, 40)).toBeCloseTo(0.3 * 9, 6); // ceil(ln 0.001 / ln 0.4) = 8 回 + 元の 1
    expect(delayTailSec(1000, 90)).toBe(MAX_EFFECT_TAIL_SEC);
  });

  it("使ったエフェクトの尾の最大を返す。使わなければ 0", () => {
    expect(effectTailSec([note(0, 0), note(1, 1)])).toBe(0);
    expect(effectTailSec([note(0, 0, { reverb: r(30, 2000) })])).toBeCloseTo(2.01, 6);
    // ディレイ(2.7 秒)のほうが長い
    expect(effectTailSec([note(0, 0, { reverb: r(30, 2000) }), note(1, 0, { delay: d(25, 300, 40) })])).toBeCloseTo(2.7, 6);
  });
});

describe("createTrackEffects", () => {
  it("エフェクトを使わなければ、ノードを何も作らず、つなぎ先も無い。apply は何もしない", () => {
    const { Tone, nodes } = fakeTone();
    const fx = createTrackEffects(Tone, [note(0, 0), note(1, 0.5)], END);
    expect(nodes).toHaveLength(0);
    expect(fx.outFor(0)).toBeUndefined();
    expect(() => fx.apply(note(0, 0))).not.toThrow();
  });

  it("使うトラックにだけバス(Gain)を作って出力へつなぎ、そのトラックの音源のつなぎ先にする", () => {
    const { Tone, of } = fakeTone();
    const fx = createTrackEffects(Tone, [note(0, 0, { reverb: r(30, 2000) }), note(1, 0)], END);
    const bus = of("Gain")[0];
    expect(bus.args).toEqual([1]);
    expect(bus.destinations).toBe(1);
    expect(fx.outFor(0)).toBe(bus);
    expect(fx.outFor(1)).toBeUndefined();
  });

  it("リバーブは、長さと PreDelay を渡して作り、送り(Gain 0)→ リバーブ → 補正(Gain)→ 出力につなぐ", () => {
    const { Tone, of } = fakeTone();
    createTrackEffects(Tone, [note(0, 0, { reverb: r(30, 2000) })], END);
    const [bus, send, trim] = of("Gain");
    const [reverb] = of("Reverb");
    expect(reverb.args).toEqual([{ decay: 2, preDelay: REVERB_PREDELAY_SEC }]);
    expect(send.args).toEqual([0]);
    expect(bus.connected).toEqual([send]);
    expect(send.connected).toEqual([reverb]);
    expect(reverb.connected).toEqual([trim]);
    expect(trim.args[0]).toBeCloseTo(10 ** (REVERB_TRIM_DB / 20), 9);
    expect(trim.destinations).toBe(1);
  });

  it("ディレイは、間隔(秒)・繰り返し(0〜1)・最大遅延 1 秒で作り、補正して出力へつなぐ", () => {
    const { Tone, of } = fakeTone();
    createTrackEffects(Tone, [note(0, 0, { delay: d(25, 300, 40) })], END);
    const [, send, trim] = of("Gain");
    const [delay] = of("FeedbackDelay");
    expect(delay.args).toEqual([{ delayTime: 0.3, feedback: 0.4, maxDelay: 1 }]);
    expect(send.connected).toEqual([delay]);
    expect(delay.connected).toEqual([trim]);
    expect(trim.args[0]).toBeCloseTo(10 ** (DELAY_TRIM_DB / 20), 9);
  });

  it("組が同じなら 1 つを使い回し、長さや(間隔, 繰り返し)が違えば別に作る。量が違うだけなら同じ組", () => {
    const { Tone, of } = fakeTone();
    createTrackEffects(Tone, [
      note(0, 0, { reverb: r(30, 2000) }),
      note(0, 1, { reverb: r(60, 2000) }),
      note(0, 2, { reverb: r(30, 500) }),
      note(0, 3, { delay: d(25, 300, 40) }),
      note(0, 4, { delay: d(50, 300, 40) }),
      note(0, 5, { delay: d(25, 300, 60) }),
    ], END);
    expect(of("Reverb")).toHaveLength(2);
    expect(of("FeedbackDelay")).toHaveLength(2);
    expect(of("Gain").filter((g) => g.args[0] === 1 && g.destinations === 1 && g.connected.length > 0)).toHaveLength(1); // バスは 1 つ
  });

  it("別のトラックは別のバスと別のエフェクトを持つ", () => {
    const { Tone, of } = fakeTone();
    const fx = createTrackEffects(Tone, [note(0, 0, { reverb: r(30, 2000) }), note(1, 0, { reverb: r(30, 2000) })], END);
    expect(of("Reverb")).toHaveLength(2);
    expect(fx.outFor(0)).not.toBe(fx.outFor(1));
  });

  it("apply は、音の始まりの時刻に、その音の組の送りだけを量(0〜1)にし、ほかの組は 0 にする", () => {
    const { Tone, of } = fakeTone();
    const notes = [
      note(0, 0, { reverb: r(30, 2000) }),
      note(0, 0.5, { reverb: r(60, 2000) }),
      note(0, 1, { reverb: r(40, 500) }),
      note(0, 1.5),
    ];
    const fx = createTrackEffects(Tone, notes, END);
    for (const n of notes) fx.apply(n);
    const sends = of("Gain").filter((g) => g.args[0] === 0); // 作った順: 2000ms の組、500ms の組
    expect(sends[0].sets).toEqual([[0.3, 0], [0.6, 0.5], [0, 1]]);
    expect(sends[1].sets).toEqual([[0.4, 1], [0, 1.5]]);
  });

  it("同じ状態が続くときは、送りを重ねて設定しない(0 のままの組にも設定しない)", () => {
    const { Tone, of } = fakeTone();
    const notes = [note(0, 0, { reverb: r(30, 2000) }), note(0, 0.5, { reverb: r(30, 2000) }), note(0, 1, { reverb: r(30, 2000) })];
    const fx = createTrackEffects(Tone, notes, END);
    for (const n of notes) fx.apply(n);
    expect(of("Gain").find((g) => g.args[0] === 0)!.sets).toEqual([[0.3, 0]]);
  });

  it("リバーブとディレイを同時に使う音は、両方の送りを量にする", () => {
    const { Tone, of } = fakeTone();
    const n = note(0, 0.25, { reverb: r(30, 2000), delay: d(50, 300, 40) });
    const fx = createTrackEffects(Tone, [n], END);
    fx.apply(n);
    const sends = of("Gain").filter((g) => g.args[0] === 0);
    expect(sends.map((s) => s.sets)).toEqual([[[0.3, 0.25]], [[0.5, 0.25]]]);
  });

  it("トラックをまたいで送りが混ざらない", () => {
    const { Tone, of } = fakeTone();
    const a = note(0, 0, { reverb: r(30, 2000) });
    const b = note(1, 0, { reverb: r(60, 2000) });
    const fx = createTrackEffects(Tone, [a, b], END);
    fx.apply(a);
    fx.apply(b);
    const sends = of("Gain").filter((g) => g.args[0] === 0);
    expect(sends.map((s) => s.sets)).toEqual([[[0.3, 0]], [[0.6, 0]]]);
  });

  it("補正の Gain は、描画の終わりの 0.3 秒前から終わりまでに 0 へフェードする(5 秒で頭打ちの尾が急に切れないように)", () => {
    const { Tone, of } = fakeTone();
    createTrackEffects(Tone, [note(0, 0, { delay: d(100, 1000, 90) })], END);
    const trim = of("Gain")[2];
    expect(trim.sets).toEqual([[1, END - 0.3]]);
    expect(trim.ramps).toEqual([[0, END]]);
    // 描画が 0.3 秒より短くても、開始時刻は 0 以上
    const short = fakeTone();
    createTrackEffects(short.Tone, [note(0, 0, { delay: d(100, 1000, 90) })], 0.2);
    expect(short.of("Gain")[2].sets[0][1]).toBe(0);
  });

  it("ready は、リバーブのインパルス応答の生成が終わるまで待つ", async () => {
    let release!: () => void;
    const { Tone } = fakeTone({ reverbReady: new Promise<void>((resolve) => (release = resolve)) });
    const fx = createTrackEffects(Tone, [note(0, 0, { reverb: r(30, 2000) })], END);
    let done = false;
    void fx.ready().then(() => (done = true));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(done).toBe(false);
    release();
    await fx.ready();
    expect(done).toBe(true);
  });
});

// 型の確認用(VoiceOutput が Tone のノードであること)
const _typecheck: VoiceOutput | undefined = undefined;
void _typecheck;
