// MML の音色(@番号 / @名前)ごとの音源の作り方(Issue #494)。
// 音色の番号と名前は Rust の Timbre::ALL(src-tauri/src/domain/mml.rs)と同じにする。
// gainDb は、C4 の単音の聞こえる大きさ(RMS)を fm(現行の音)にそろえる補正。実際の renderScore を WebKitGTK 2.54.1(Xvfb)で測って校正した値で、noise だけはピーク上限(0.36)を優先して小さめにしている(docs/superpowers/specs/2026-10-16-issue-494-mml-timbre-design.md の「校正結果」)。
import type { Timbre, TimbreParams } from "../bindings/tauri.gen";
import { PIECE_SPECS, drumPieceFor, type DrumPiece, type MembraneSpec, type MetalSpec, type NoiseSpec } from "./mmlDrumkit";

export type ToneModule = typeof import("tone");

/** Offline コンテキスト内に作った音源 1 つ。 */
export interface Voice {
  /** `midi` はノート番号。打楽器の `drumkit` だけが使う(音の高さで楽器を選ぶ) */
  trigger(hz: number, durationSec: number, startSec: number, velocity: number, midi?: number): void;
}

export interface TimbrePreset {
  /** MML の `@<番号>` */
  number: number;
  /** MML の `@<名前>`(`Timbre` と同じ文字列) */
  name: Timbre;
  /** 聞こえる大きさをそろえる補正(dB) */
  gainDb: number;
  /** リリース時間(秒)。`TAIL_SEC` 以下にする */
  releaseSec: number;
  /** true なら 1 音ごとに音源を作る(単音用で、同じ音源に時刻の重なる音を鳴らせない) */
  perNote?: boolean;
  /** true なら打楽器で、`y` `m` `k` のパラメーターを無視する(描画は、パラメーターが違っても音源を使い回す) */
  percussive?: boolean;
  /** `Tone.Offline` のコールバック内で呼ぶ。出力先と音量(`volumeDb`)を設定済みの音源を返す。`params` は旋律の音色だけが使う。 */
  create(Tone: ToneModule, volumeDb: number, params?: TimbreParams | null): Voice;
}

interface PolyLike {
  volume: { value: number };
  toDestination(): unknown;
  triggerAttackRelease(note: number, duration: number, time: number, velocity: number): unknown;
}

/** パラメーター(Issue #499)を音源に反映するための材料。`duo` は DuoSynth(エンベロープが 2 つの声の中にある) */
interface Fx {
  Tone: ToneModule;
  params: TimbreParams | null | undefined;
  duo?: boolean;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyPoly = import("tone").PolySynth<any>;

/**
 * 出力に繋いで音量を設定する。`fx.params` があれば、エンベロープ(`envelope` だけ。modulationEnvelope は変えない)を `set` で反映し、
 * ビブラートは `Tone.Vibrato` を出力に挟む。デチューンは `renderScore` が鳴らす周波数の倍率にするので、ここでは扱わない。
 * `PolySynth` は描画中に設定を時刻つきで変えられないので、パラメーターの組(エンベロープ・ビブラート)ごとに別の音源を作る
 * (呼び出し側 `renderScore` の責任)。
 */
function poly(synth: AnyPoly, volumeDb: number, fx?: Fx): Voice {
  synth.volume.value = volumeDb;
  const params = fx?.params;
  if (fx && params) {
    if (params.envelope) {
      const e = {
        attack: params.envelope.attackMs / 1000,
        decay: params.envelope.decayMs / 1000,
        sustain: params.envelope.sustain / 100,
        release: params.envelope.releaseMs / 1000,
      };
      // DuoSynth のエンベロープは 2 つの声の中にある。`AnyPoly` の `set` の型には voice0/voice1 が無いので、`never` に通す
      const options = fx.duo ? { voice0: { envelope: e }, voice1: { envelope: e } } : { envelope: e };
      synth.set(options as never);
    }
  }
  if (fx && params?.vibrato) {
    const vibrato = new fx.Tone.Vibrato(params.vibrato.rateHz, Math.min(1, params.vibrato.depth / 100));
    vibrato.toDestination();
    synth.connect(vibrato);
  } else {
    synth.toDestination();
  }
  return { trigger: (hz, d, t, v) => synth.triggerAttackRelease(hz, d, t, v) };
}

/** `@drumkit`。音の高さ(midi)から GM の楽器を選んで鳴らす。割り当てのない高さは鳴らさない。書いた長さは無視する */
function drumkit(Tone: ToneModule, volumeDb: number): Voice {
  const out = new Tone.Volume(volumeDb).toDestination();
  // 音源は楽器ごとに 1 つ作って使い回す(1 打ごとに作ると、数千打で描画が終わらない)。同じ楽器を続けて叩くと、前の余韻は切れる。
  // 単音用の音源(ノイズ・金属)は、時刻の昇順に鳴らす(`Voice.trigger` は、トラック内で時刻の昇順に呼ばれる)。
  const membranes = new Map<DrumPiece, PolyLike & { connect(to: unknown): unknown }>();
  const noises = new Map<DrumPiece, InstanceType<ToneModule["NoiseSynth"]>>();
  const metals = new Map<DrumPiece, InstanceType<ToneModule["MetalSynth"]>>();
  // 単音用の音源は、前の開始時刻より後でないと Tone が例外を投げる(和音で同じ楽器を同時に鳴らす、打ち出しが重なる、など)。
  // 同じ楽器の、前の開始時刻以前の打ち出しは鳴らさない(同じ音なので、同時の重ね打ちは聞こえ方が変わらない)。
  const lastStart = new Map<DrumPiece, number>();
  const startable = (piece: DrumPiece, time: number): boolean => {
    const prev = lastStart.get(piece);
    if (prev !== undefined && time - prev <= 1e-3) return false;
    lastStart.set(piece, time);
    return true;
  };
  const membraneFor = (piece: DrumPiece, spec: MembraneSpec) => {
    let synth = membranes.get(piece);
    if (!synth) {
      synth = new Tone.PolySynth(Tone.MembraneSynth, {
        pitchDecay: spec.pitchDecay,
        octaves: spec.octaves,
        envelope: { attack: 0.001, decay: spec.decay, sustain: 0, release: spec.release },
      });
      synth.volume.value = spec.trimDb;
      synth.connect(out);
      membranes.set(piece, synth);
    }
    return synth;
  };
  const noiseFor = (piece: DrumPiece, spec: NoiseSpec) => {
    let noise = noises.get(piece);
    if (!noise) {
      noise = new Tone.NoiseSynth({
        noise: { type: "white" },
        envelope: { attack: 0.001, decay: spec.decay, sustain: 0, release: spec.release },
      });
      const filter = new Tone.Filter(spec.cutoff, spec.filter);
      noise.volume.value = spec.trimDb;
      noise.connect(filter);
      filter.connect(out);
      noises.set(piece, noise);
    }
    return noise;
  };
  const metalFor = (piece: DrumPiece, spec: MetalSpec) => {
    let metal = metals.get(piece);
    if (!metal) {
      metal = new Tone.MetalSynth({
        harmonicity: spec.harmonicity,
        modulationIndex: spec.modulationIndex,
        octaves: spec.octaves,
        resonance: spec.resonance,
        envelope: { attack: 0.001, decay: spec.decay, release: spec.release },
      });
      metal.volume.value = spec.trimDb;
      metal.connect(out);
      metals.set(piece, metal);
    }
    return metal;
  };
  return {
    trigger(_hz, _d, t, v, midi) {
      const piece = midi === undefined ? null : drumPieceFor(midi);
      if (!piece) return;
      const spec = PIECE_SPECS[piece];
      if (spec.kind === "membrane") {
        membraneFor(piece, spec).triggerAttackRelease(spec.hz, spec.decay, t, v);
      } else if (spec.kind === "noise") {
        const noise = noiseFor(piece, spec);
        for (const offset of spec.bursts) {
          if (startable(piece, t + offset)) noise.triggerAttackRelease(spec.decay, t + offset, v);
        }
        if (spec.body) membraneFor(piece, spec.body).triggerAttackRelease(spec.body.hz, spec.body.decay, t, v);
      } else {
        if (startable(piece, t)) metalFor(piece, spec).triggerAttackRelease(spec.hz, spec.decay, t, v);
      }
    },
  };
}

const BASIC_ENV = { attack: 0.005, decay: 0.1, sustain: 0.5, release: 0.1 };
type SynthOscillator = NonNullable<NonNullable<ConstructorParameters<ToneModule["Synth"]>[0]>["oscillator"]>;
const basic =
  (oscillator: SynthOscillator) =>
  (Tone: ToneModule, db: number, params?: TimbreParams | null): Voice =>
    poly(new Tone.PolySynth(Tone.Synth, { oscillator, envelope: BASIC_ENV }), db, { Tone, params });

export const TIMBRES: Record<Timbre, TimbrePreset> = {
  fm: {
    number: 0,
    name: "fm",
    gainDb: 0,
    releaseSec: 0.1,
    create: (Tone, db, params) =>
      poly(
        new Tone.PolySynth(Tone.FMSynth, {
          harmonicity: 3,
          modulationIndex: 8,
          envelope: { attack: 0.005, decay: 0.1, sustain: 0.4, release: 0.1 },
        }),
        db,
        { Tone, params },
      ),
  },
  square: { number: 1, name: "square", gainDb: -13.1, releaseSec: 0.1, create: basic({ type: "square" }) },
  pulse: { number: 2, name: "pulse", gainDb: -14.6, releaseSec: 0.1, create: basic({ type: "pulse", width: 0.25 }) },
  triangle: { number: 3, name: "triangle", gainDb: -9.8, releaseSec: 0.1, create: basic({ type: "triangle" }) },
  saw: { number: 4, name: "saw", gainDb: -8.3, releaseSec: 0.1, create: basic({ type: "sawtooth" }) },
  sine: { number: 5, name: "sine", gainDb: -11.6, releaseSec: 0.1, create: basic({ type: "sine" }) },
  organ: {
    number: 6,
    name: "organ",
    gainDb: -14.2,
    releaseSec: 0.05,
    create: (Tone, db, params) =>
      poly(
        new Tone.PolySynth(Tone.Synth, {
          oscillator: { type: "custom", partials: [1, 0.6, 0.4, 0.25, 0.15] },
          envelope: { attack: 0.01, decay: 0.05, sustain: 1, release: 0.05 },
        }),
        db,
        { Tone, params },
      ),
  },
  epiano: {
    number: 7,
    name: "epiano",
    gainDb: -0.5,
    releaseSec: 0.3,
    create: (Tone, db, params) =>
      poly(
        new Tone.PolySynth(Tone.FMSynth, {
          harmonicity: 2,
          modulationIndex: 3,
          envelope: { attack: 0.005, decay: 0.6, sustain: 0.2, release: 0.3 },
          modulationEnvelope: { attack: 0.005, decay: 0.3, sustain: 0.3, release: 0.3 },
        }),
        db,
        { Tone, params },
      ),
  },
  bell: {
    number: 8,
    name: "bell",
    gainDb: 0.1,
    releaseSec: 0.5,
    create: (Tone, db, params) =>
      poly(
        new Tone.PolySynth(Tone.FMSynth, {
          harmonicity: 5.07,
          modulationIndex: 12,
          envelope: { attack: 0.001, decay: 0.8, sustain: 0.05, release: 0.5 },
          modulationEnvelope: { attack: 0.001, decay: 0.4, sustain: 0.1, release: 0.4 },
        }),
        db,
        { Tone, params },
      ),
  },
  am: {
    number: 9,
    name: "am",
    gainDb: 4.3,
    releaseSec: 0.2,
    create: (Tone, db, params) =>
      poly(
        new Tone.PolySynth(Tone.AMSynth, {
          envelope: { attack: 0.005, decay: 0.1, sustain: 0.5, release: 0.2 },
          modulationEnvelope: { attack: 0.005, decay: 0.1, sustain: 0.5, release: 0.2 },
        }),
        db,
        { Tone, params },
      ),
  },
  lead: {
    number: 10,
    name: "lead",
    gainDb: -15.9,
    releaseSec: 0.15,
    create: (Tone, db, params) => {
      const voice = { envelope: { attack: 0.01, decay: 0.1, sustain: 0.6, release: 0.15 } };
      return poly(
        new Tone.PolySynth(Tone.DuoSynth, {
          harmonicity: 1,
          vibratoAmount: 0.3,
          vibratoRate: 5,
          voice0: voice,
          voice1: voice,
        }),
        db,
        { Tone, params, duo: true },
      );
    },
  },
  bass: {
    number: 11,
    name: "bass",
    gainDb: -10.0,
    releaseSec: 0.2,
    create: (Tone, db, params) =>
      poly(
        new Tone.PolySynth(Tone.MonoSynth, {
          oscillator: { type: "sawtooth" },
          filter: { Q: 1, type: "lowpass", rolloff: -24 },
          envelope: { attack: 0.01, decay: 0.2, sustain: 0.5, release: 0.2 },
          filterEnvelope: { attack: 0.01, decay: 0.2, sustain: 0.5, release: 0.2, baseFrequency: 200, octaves: 3 },
        }),
        db,
        { Tone, params },
      ),
  },
  drum: {
    number: 12,
    name: "drum",
    gainDb: -6.1,
    releaseSec: 0.3,
    percussive: true,
    create: (Tone, db) =>
      poly(
        new Tone.PolySynth(Tone.MembraneSynth, {
          envelope: { attack: 0.001, decay: 0.3, sustain: 0, release: 0.3 },
        }),
        db,
      ),
  },
  noise: {
    number: 13,
    name: "noise",
    gainDb: -5.0,
    releaseSec: 0.05,
    percussive: true,
    perNote: true,
    create: (Tone, db) => {
      // NoiseSynth は単音用で PolySynth に包めない。音程は無視する。
      const synth = new Tone.NoiseSynth({
        noise: { type: "white" },
        envelope: { attack: 0.001, decay: 0.15, sustain: 0, release: 0.05 },
      }).toDestination();
      synth.volume.value = db;
      return { trigger: (_hz, d, t, v) => void synth.triggerAttackRelease(d, t, v) };
    },
  },
  drumkit: {
    number: 14,
    name: "drumkit",
    gainDb: 0,
    releaseSec: 0.2,
    percussive: true,
    create: (Tone, db) => drumkit(Tone, db),
  },
};

/** 番号順の一覧(ヘルプ表示用)。 */
export const TIMBRE_LIST: TimbrePreset[] = Object.values(TIMBRES).sort((a, b) => a.number - b.number);
