import { describe, expect, it } from "vitest";
import type { Timbre } from "../bindings/tauri.gen";
import { TAIL_SEC } from "./mmlRender";
import { TIMBRES, TIMBRE_LIST, type ToneModule } from "./mmlTimbres";

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
    toDestination() {
      return this;
    }
    triggerAttackRelease(...args: unknown[]) {
      hits.push({ cls: this.cls, args, node: this });
      return this;
    }
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
  } as unknown as ToneModule;
  return { Tone, made, hits };
}

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

  it("ノイズと金属の楽器は、1 打ごとに音源を作る", () => {
    const { Tone, made } = fakeTone();
    const voice = TIMBRES.drumkit.create(Tone, -3);
    const count = (cls: string) => made.filter((m) => m.cls === cls).length;
    voice.trigger(440, 0.1, 0, 0.8, 42);
    voice.trigger(440, 0.1, 0.1, 0.8, 42);
    expect(count("NoiseSynth")).toBe(2);
    voice.trigger(440, 0.1, 0.2, 0.8, 49);
    voice.trigger(440, 0.1, 0.3, 0.8, 57);
    expect(count("MetalSynth")).toBe(2);
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
