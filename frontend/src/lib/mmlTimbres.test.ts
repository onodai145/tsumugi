import { describe, expect, it } from "vitest";
import type { Timbre } from "../bindings/tauri.gen";
import { TAIL_SEC } from "./mmlRender";
import { TIMBRES, TIMBRE_LIST, glideVoice, type ToneModule, type VoiceOutput } from "./mmlTimbres";

// Rust 側(domain/mml.rs の Timbre::ALL と mml.rs の TIMBRE_TABLE)と同じ並び。変えるときは両方を直す。
const EXPECTED: [number, Timbre][] = [
  [0, "fm"],
  [1, "square"],
  [2, "pulse"],
  [3, "triangle"],
  [4, "saw"],
  [5, "sine"],
  [6, "organ"],
  [7, "epiano"],
  [8, "bell"],
  [9, "am"],
  [10, "lead"],
  [11, "bass"],
  [12, "drum"],
  [13, "noise"],
  [14, "drumkit"],
];

describe("TIMBRES", () => {
  it("番号と名前が Rust 側の表と一致する", () => {
    expect(TIMBRE_LIST.map((p) => [p.number, p.name])).toEqual(EXPECTED);
  });

  it("キーと name が一致し、全 Timbre にプリセットがある", () => {
    for (const [, name] of EXPECTED) expect(TIMBRES[name].name).toBe(name);
    expect(Object.keys(TIMBRES).sort()).toEqual(EXPECTED.map(([, n]) => n).sort());
  });

  it("全プリセットのリリースは TAIL_SEC 以下で、gainDb は有限の妥当な範囲", () => {
    for (const p of TIMBRE_LIST) {
      expect(p.releaseSec, p.name).toBeGreaterThan(0);
      expect(p.releaseSec, p.name).toBeLessThanOrEqual(TAIL_SEC);
      expect(Number.isFinite(p.gainDb), p.name).toBe(true);
      expect(p.gainDb, p.name).toBeGreaterThanOrEqual(-30);
      expect(p.gainDb, p.name).toBeLessThanOrEqual(6);
    }
  });

  it("fm は現行の音(補正 0 dB)のまま", () => {
    expect(TIMBRES.fm.gainDb).toBe(0);
    expect(TIMBRES.fm.releaseSec).toBe(0.1);
  });

  it("単音用の音源だけが perNote である", () => {
    expect(TIMBRE_LIST.filter((p) => p.perNote).map((p) => p.name)).toEqual(["noise"]);
  });
});

/** drumkit の音源の作り方を確かめるための、Tone.js の代役。作ったものと、鳴らした呼び出しを記録する */
function fakeTone() {
  const made: { cls: string; args: unknown[]; node: FakeNode }[] = [];
  const hits: { cls: string; args: unknown[]; node: FakeNode }[] = [];
  class FakeNode {
    volume = { value: 0 };
    connected: unknown[] = [];
    constructor(
      public cls: string,
      public args: unknown[],
    ) {
      made.push({ cls, args, node: this });
    }
    connect(to: unknown) {
      this.connected.push(to);
      return this;
    }
    destinations = 0;
    sets: unknown[] = [];
    toDestination() {
      this.destinations++;
      return this;
    }
    set(options: unknown) {
      this.sets.push(options);
      return this;
    }
    triggerAttackRelease(...args: unknown[]) {
      // 本物の Tone の単音用音源(NoiseSynth は第 2 引数、MetalSynth は第 3 引数が時刻)は、前の開始時刻より厳密に後でないと例外を投げる
      const time = this.cls === "NoiseSynth" ? args[1] : this.cls === "MetalSynth" ? args[2] : undefined;
      if (typeof time === "number") {
        if (this.lastStart !== undefined && !(time - this.lastStart > 1e-6)) {
          throw new Error("Start time must be strictly greater than previous start time");
        }
        this.lastStart = time;
      }
      hits.push({ cls: this.cls, args, node: this });
      return this;
    }
    lastStart: number | undefined;
  }
  const cls = (name: string) =>
    class extends FakeNode {
      constructor(...args: unknown[]) {
        super(name, args);
      }
    };
  const Tone = {
    Volume: cls("Volume"),
    PolySynth: cls("PolySynth"),
    MembraneSynth: cls("MembraneSynth"),
    NoiseSynth: cls("NoiseSynth"),
    MetalSynth: cls("MetalSynth"),
    Filter: cls("Filter"),
    Vibrato: cls("Vibrato"),
    Synth: cls("Synth"),
    FMSynth: cls("FMSynth"),
    AMSynth: cls("AMSynth"),
    DuoSynth: cls("DuoSynth"),
    MonoSynth: cls("MonoSynth"),
  } as unknown as ToneModule;
  return { Tone, made, hits };
}

/** `glideVoice` の土台にする、`Monophonic` の代役。`frequency` の呼び出しと、元の `setNote` の呼び出しを記録する */
class FakeMonophonic {
  calls: unknown[][] = [];
  frequency = {
    setValueAtTime: (...a: unknown[]) => this.calls.push(["set", ...a]),
    exponentialRampToValueAtTime: (...a: unknown[]) => this.calls.push(["ramp", ...a]),
  };
  setNote(note: unknown, time?: number) {
    this.calls.push(["base", note, time]);
    return this;
  }
}

describe("glideVoice(ポルタメント、Issue #499)", () => {
  it("指定が無い時刻は、元の setNote のまま", () => {
    const Voice = glideVoice(FakeMonophonic, new Map());
    const v = new Voice();
    v.setNote(440, 1);
    expect(v.calls).toEqual([["base", 440, 1]]);
  });

  it("指定のある時刻は、fromHz に置いて、目的の高さへ指数的に動かす(元の setNote は呼ばない)", () => {
    const glides = new Map([[1.25, { fromHz: 300, sec: 0.08 }]]);
    const v = new (glideVoice(FakeMonophonic, glides))();
    v.setNote(440, 1.25);
    expect(v.calls).toHaveLength(2);
    expect(v.calls[0]).toEqual(["set", 300, 1.25]);
    expect(v.calls[1][0]).toBe("ramp");
    expect(v.calls[1][1]).toBe(440);
    expect(v.calls[1][2]).toBeCloseTo(1.33, 9);
  });

  it("指定は 1 回使うと消え、ほかの時刻には影響しない", () => {
    const glides = new Map([[1, { fromHz: 300, sec: 0.1 }]]);
    const v = new (glideVoice(FakeMonophonic, glides))();
    v.setNote(440, 2); // 別の時刻
    expect(glides.size).toBe(1);
    v.setNote(440, 1);
    expect(glides.size).toBe(0);
    v.setNote(440, 1); // 2 回目は元の setNote
    expect(v.calls.map((c) => c[0])).toEqual(["base", "set", "ramp", "base"]);
  });

  it("音名など数値でない note は、指定があっても元の setNote", () => {
    const v = new (glideVoice(FakeMonophonic, new Map([[1, { fromHz: 300, sec: 0.1 }]])))();
    v.setNote("C4", 1);
    expect(v.calls).toEqual([["base", "C4", 1]]);
  });
});

describe("旋律の音源とポルタメント(Issue #499)", () => {
  it("PolySynth に渡す声は glideVoice の継承クラスで、trigger の glide が、その時刻の setNote に届く", () => {
    const { Tone, made } = fakeTone();
    (Tone as unknown as Record<string, unknown>).FMSynth = FakeMonophonic;
    const voice = TIMBRES.fm.create(Tone, -3);
    voice.trigger(440, 0.5, 1.25, 0.8, 64, { fromHz: 300, sec: 0.08 });
    voice.trigger(440, 0.5, 2, 0.8, 64); // 滑らない音
    const Cls = made.find((m) => m.cls === "PolySynth")!.args[0] as new () => FakeMonophonic;
    expect(Object.getPrototypeOf(Cls.prototype)).toBe(FakeMonophonic.prototype); // 元の声の継承クラス
    const slid = new Cls();
    slid.setNote(440, 1.25);
    expect(slid.calls.map((c) => c[0])).toEqual(["set", "ramp"]);
    const plain = new Cls();
    plain.setNote(440, 2);
    expect(plain.calls.map((c) => c[0])).toEqual(["base"]);
  });

  it("旋律の 12 種すべてが、元の声(Synth・FMSynth・AMSynth・DuoSynth・MonoSynth のどれか)の継承クラスを PolySynth に渡す", () => {
    for (const p of TIMBRE_LIST.filter((p) => !p.percussive)) {
      const { Tone, made } = fakeTone();
      p.create(Tone, -3);
      const Cls = made.find((m) => m.cls === "PolySynth")!.args[0] as object;
      const bases = ["Synth", "FMSynth", "AMSynth", "DuoSynth", "MonoSynth"].map((n) => (Tone as unknown as Record<string, unknown>)[n]);
      expect(bases, p.name).toContain(Object.getPrototypeOf(Cls)); // 元の声そのものではなく、その継承クラス
      expect(bases, p.name).not.toContain(Cls);
    }
  });

  it("打楽器の音源は、glide が付いても例外を出さず、そのまま鳴らす", () => {
    const { Tone, hits } = fakeTone();
    const voice = TIMBRES.drum.create(Tone, -3);
    expect(() => voice.trigger(440, 0.5, 1, 0.8, 60, { fromHz: 300, sec: 0.08 })).not.toThrow();
    expect(hits).toHaveLength(1);
  });
});

describe("drumkit の音源", () => {
  it("drumkit は単音用ではない(1 トラックに 1 つ作って使い回す)", () => {
    expect(TIMBRES.drumkit.perNote).toBeUndefined();
    expect(TIMBRES.drumkit.number).toBe(14);
  });

  it("割り当てのない高さは、音源を作らず、鳴らさない", () => {
    const { Tone, hits } = fakeTone();
    const voice = TIMBRES.drumkit.create(Tone, -3);
    const before = hits.length;
    for (const midi of [0, 34, 52, 60, 81, 127]) voice.trigger(440, 0.5, 0, 0.8, midi);
    voice.trigger(440, 0.5, 0, 0.8); // midi が無いときも鳴らさない
    expect(hits).toHaveLength(before);
  });

  it("GM の番号の楽器で鳴り、書いた長さは無視して、楽器ごとの固定の長さで鳴らす", () => {
    const { Tone, hits } = fakeTone();
    const voice = TIMBRES.drumkit.create(Tone, -3);
    voice.trigger(440, 0.01, 1.0, 0.7, 36); // キック
    voice.trigger(440, 5, 2.0, 0.7, 36);
    expect(hits).toHaveLength(2);
    expect(hits[0].args[1]).toBe(hits[1].args[1]); // 長さは同じ
    expect(hits[0].args[2]).toBe(1.0); // 開始時刻
    expect(hits[1].args[2]).toBe(2.0);
    expect(hits[0].args[3]).toBe(0.7); // ベロシティ
    expect(hits[0].cls).toBe("PolySynth");
  });

  it("同じ楽器の膜の音源は使い回し、別の楽器は別の音源にする", () => {
    const { Tone, made } = fakeTone();
    const voice = TIMBRES.drumkit.create(Tone, -3);
    voice.trigger(440, 0.1, 0, 0.8, 36);
    const polys = () => made.filter((m) => m.cls === "PolySynth").length;
    expect(polys()).toBe(1);
    voice.trigger(440, 0.1, 0.5, 0.8, 35); // 同じキック
    expect(polys()).toBe(1);
    voice.trigger(440, 0.1, 1, 0.8, 41); // タム
    expect(polys()).toBe(2);
  });

  it("ノイズと金属の楽器も、楽器ごとに 1 つの音源を使い回す(1 打ごとに作ると、数千打で描画が終わらない)", () => {
    const { Tone, made } = fakeTone();
    const voice = TIMBRES.drumkit.create(Tone, -3);
    const count = (cls: string) => made.filter((m) => m.cls === cls).length;
    voice.trigger(440, 0.1, 0, 0.8, 42);
    voice.trigger(440, 0.1, 0.1, 0.8, 44); // 同じクローズドハイハット
    expect(count("NoiseSynth")).toBe(1);
    expect(count("Filter")).toBe(1);
    voice.trigger(440, 0.1, 0.2, 0.8, 46); // オープンハイハットは別の楽器
    expect(count("NoiseSynth")).toBe(2);
    voice.trigger(440, 0.1, 0.3, 0.8, 49);
    voice.trigger(440, 0.1, 0.4, 0.8, 57); // 同じクラッシュ
    expect(count("MetalSynth")).toBe(1);
    voice.trigger(440, 0.1, 0.5, 0.8, 51); // ライドは別の楽器
    expect(count("MetalSynth")).toBe(2);
  });

  it("和音で同じ楽器を同じ時刻に 2 回鳴らしても、例外を投げず、1 回だけ鳴らす(38 と 40 はどちらもスネア)", () => {
    const pairs: [number, number, string][] = [
      [38, 40, "NoiseSynth"],
      [42, 44, "NoiseSynth"],
      [49, 57, "MetalSynth"],
      [51, 59, "MetalSynth"],
    ];
    for (const [a, b, cls] of pairs) {
      const { Tone, hits } = fakeTone();
      const voice = TIMBRES.drumkit.create(Tone, -3);
      expect(() => {
        voice.trigger(440, 0.1, 1.0, 0.8, a);
        voice.trigger(440, 0.1, 1.0, 0.8, b);
      }, `${a}+${b}`).not.toThrow();
      expect(hits.filter((h) => h.cls === cls), `${a}+${b}`).toHaveLength(1);
    }
  });

  it("前のクラップの打ち出しと重なる次のクラップは、例外を投げず、重なった打ち出しだけ飛ばす。離れていれば 3 回とも鳴らす", () => {
    const { Tone, hits } = fakeTone();
    const voice = TIMBRES.drumkit.create(Tone, -3);
    voice.trigger(440, 0.1, 1.0, 0.8, 39); // 1.000 / 1.012 / 1.024
    expect(hits).toHaveLength(3);
    // 次のクラップは 1.010 / 1.022 / 1.034。前の最後(1.024)以前の 2 つを飛ばし、1.034 だけ鳴らす
    expect(() => voice.trigger(440, 0.1, 1.01, 0.8, 39)).not.toThrow();
    expect(hits).toHaveLength(4);
    voice.trigger(440, 0.1, 1.2, 0.8, 39);
    expect(hits).toHaveLength(7);
  });

  it("ハンドクラップは短い間隔で 3 回叩く", () => {
    const { Tone, hits } = fakeTone();
    const voice = TIMBRES.drumkit.create(Tone, -3);
    voice.trigger(440, 0.1, 1.0, 0.8, 39);
    const times = hits.map((h) => h.args[1] as number);
    expect(times).toHaveLength(3);
    expect(times[0]).toBe(1.0);
    expect(times[1]).toBeGreaterThan(times[0]);
    expect(times[2]).toBeGreaterThan(times[1]);
  });

  it("出力の音量は、渡した volumeDb になる", () => {
    const { Tone, made } = fakeTone();
    TIMBRES.drumkit.create(Tone, -7.5);
    const volume = made.find((m) => m.cls === "Volume")!;
    expect(volume.args[0]).toBe(-7.5);
  });
});

describe("音色のパラメーター(Issue #499)", () => {
  const MELODIC: Timbre[] = ["fm", "square", "pulse", "triangle", "saw", "sine", "organ", "epiano", "bell", "am", "lead", "bass"];
  const ENV = { attackMs: 10, decayMs: 200, sustain: 60, releaseMs: 300 };
  const params = (p: Partial<{ envelope: typeof ENV | null; vibrato: { rateHz: number; depth: number } | null; detune: number }>) => ({
    envelope: null,
    vibrato: null,
    detune: 0,
    ...p,
  });
  const polyOf = (made: ReturnType<typeof fakeTone>["made"]) => made.find((m) => m.cls === "PolySynth")!.node as unknown as { sets: unknown[]; destinations: number; connected: unknown[] };

  it("打楽器だけが percussive で、パラメーターを無視する", () => {
    expect(TIMBRE_LIST.filter((p) => p.percussive).map((p) => p.name)).toEqual(["drum", "noise", "drumkit"]);
  });

  it("パラメーターなしでは、set もビブラートも使わず、そのまま出力に繋ぐ", () => {
    for (const name of MELODIC) {
      const { Tone, made } = fakeTone();
      TIMBRES[name].create(Tone, -3);
      TIMBRES[name].create(Tone, -3, null);
      expect(made.filter((m) => m.cls === "Vibrato"), name).toHaveLength(0);
      for (const m of made.filter((x) => x.cls === "PolySynth")) {
        const node = m.node as unknown as { sets: unknown[]; destinations: number };
        expect(node.sets, name).toEqual([]);
        expect(node.destinations, name).toBe(1);
      }
    }
  });

  it("エンベロープは、秒とサステインの割合にして set する(modulationEnvelope は触らない)", () => {
    const { Tone, made } = fakeTone();
    TIMBRES.fm.create(Tone, -3, params({ envelope: ENV }));
    expect(polyOf(made).sets).toEqual([{ envelope: { attack: 0.01, decay: 0.2, sustain: 0.6, release: 0.3 } }]);
  });

  it("lead(DuoSynth)は、2 つの声の両方のエンベロープを置き換える", () => {
    const { Tone, made } = fakeTone();
    TIMBRES.lead.create(Tone, -3, params({ envelope: ENV }));
    const e = { attack: 0.01, decay: 0.2, sustain: 0.6, release: 0.3 };
    expect(polyOf(made).sets).toEqual([{ voice0: { envelope: e }, voice1: { envelope: e } }]);
  });

  it("デチューンは音源に渡さない(描画側が、鳴らす周波数の倍率にする。音源の組に数えないため)", () => {
    const { Tone, made } = fakeTone();
    TIMBRES.square.create(Tone, -3, params({ detune: -12 }));
    expect(polyOf(made).sets).toEqual([]);
    expect(polyOf(made).destinations).toBe(1);
  });

  it("ビブラートは Tone.Vibrato を出力に挟み、音源を出力に二重に繋がない。深さは、速さに依らず、片側の揺れの幅(セント)になる", () => {
    const { Tone, made } = fakeTone();
    TIMBRES.saw.create(Tone, -3, params({ vibrato: { rateHz: 5, depth: 30 } }));
    const vibrato = made.find((m) => m.cls === "Vibrato")!;
    // Tone.Vibrato の揺れは「速さ × 深さ × 約 27 セント」(実測)なので、深さ(セント)を速さで割って渡す
    expect(vibrato.args[0]).toBe(5);
    expect(vibrato.args[1]).toBeCloseTo(30 / (27 * 5), 10);
    const synth = polyOf(made);
    expect(synth.connected).toEqual([vibrato.node]);
    expect(synth.destinations).toBe(0);
    expect((vibrato.node as unknown as { destinations: number }).destinations).toBe(1);
  });

  it("ビブラートの深さは、速さが違っても同じセントの揺れになるよう割る。遅い速さでは上限(1)で頭打ち", () => {
    const depthFor = (rateHz: number, depth: number): number => {
      const { Tone, made } = fakeTone();
      TIMBRES.sine.create(Tone, -3, params({ vibrato: { rateHz, depth } }));
      return made.find((m) => m.cls === "Vibrato")!.args[1] as number;
    };
    // 同じ深さでも、速さが 4 倍なら Tone に渡す値は 1/4(揺れの大きさ = 渡す値 × 速さ で一定)
    expect(depthFor(20, 40) * 20).toBeCloseTo(depthFor(5, 40) * 5, 10);
    expect(depthFor(5, 100)).toBeCloseTo(100 / 135, 10);
    expect(depthFor(1, 100)).toBe(1); // 1 Hz では ±27 セントまで
    expect(depthFor(20, 1)).toBeCloseTo(1 / 540, 10);
  });

  it("全旋律音色が、3 つのパラメーターを渡されても動く", () => {
    for (const name of MELODIC) {
      const { Tone, made } = fakeTone();
      expect(() => TIMBRES[name].create(Tone, -3, params({ envelope: ENV, vibrato: { rateHz: 5, depth: 30 }, detune: 7 })), name).not.toThrow();
      expect(polyOf(made).sets.length, name).toBe(1); // エンベロープだけ。デチューンは音源に渡さない
    }
  });

  it("打楽器は、パラメーターを渡されても何もしない", () => {
    for (const name of ["drum", "drumkit"] as const) {
      const { Tone, made } = fakeTone();
      TIMBRES[name].create(Tone, -3, params({ envelope: ENV, vibrato: { rateHz: 5, depth: 30 }, detune: 7 }));
      expect(made.filter((m) => m.cls === "Vibrato"), name).toHaveLength(0);
      for (const m of made.filter((x) => x.cls === "PolySynth")) expect((m.node as unknown as { sets: unknown[] }).sets, name).toEqual([]);
    }
  });
});

describe("エフェクトのバスへのつなぎ(Issue #499)", () => {
  const MELODIC: Timbre[] = ["fm", "square", "pulse", "triangle", "saw", "sine", "organ", "epiano", "bell", "am", "lead", "bass"];
  const out = { name: "bus" } as unknown as VoiceOutput;
  type Wired = { destinations: number; connected: unknown[] };
  const wired = (made: ReturnType<typeof fakeTone>["made"], cls: string) => made.find((m) => m.cls === cls)!.node as unknown as Wired;

  it("out を渡すと、旋律の 12 種は出力でなく out に繋ぐ", () => {
    for (const name of MELODIC) {
      const { Tone, made } = fakeTone();
      TIMBRES[name].create(Tone, -3, null, out);
      const poly = wired(made, "PolySynth");
      expect(poly.destinations, name).toBe(0);
      expect(poly.connected, name).toEqual([out]);
    }
  });

  it("ビブラートがあるときは、ビブラートが out に繋ぎ、音源はビブラートに繋ぐ", () => {
    const { Tone, made } = fakeTone();
    TIMBRES.fm.create(Tone, -3, { envelope: null, vibrato: { rateHz: 5, depth: 30 }, detune: 0 }, out);
    const vibrato = wired(made, "Vibrato");
    expect(vibrato.connected).toEqual([out]);
    expect(vibrato.destinations).toBe(0);
    const poly = wired(made, "PolySynth");
    expect(poly.destinations).toBe(0);
    expect(poly.connected).toEqual([made.find((m) => m.cls === "Vibrato")!.node]);
  });

  it("打楽器(drum・noise・drumkit)も out に繋ぐ", () => {
    const percussion: [Timbre, string][] = [["drum", "PolySynth"], ["noise", "NoiseSynth"], ["drumkit", "Volume"]];
    for (const [name, cls] of percussion) {
      const { Tone, made } = fakeTone();
      TIMBRES[name].create(Tone, -3, null, out);
      const node = wired(made, cls);
      expect(node.destinations, name).toBe(0);
      expect(node.connected, name).toEqual([out]);
    }
  });

  it("out を渡さなければ、全 15 種が今までどおり出力へ繋ぐ", () => {
    const all: [Timbre, string][] = [
      ...MELODIC.map((n): [Timbre, string] => [n, "PolySynth"]),
      ["drum", "PolySynth"],
      ["noise", "NoiseSynth"],
      ["drumkit", "Volume"],
    ];
    for (const [name, cls] of all) {
      const { Tone, made } = fakeTone();
      TIMBRES[name].create(Tone, -3);
      const node = wired(made, cls);
      expect(node.destinations, name).toBe(1);
      expect(node.connected, name).toEqual([]);
    }
  });
});
