// MML の音色(@番号 / @名前)ごとの音源の作り方(Issue #494)。
// 音色の番号と名前は Rust の Timbre::ALL(src-tauri/src/domain/mml.rs)と同じにする。
// gainDb は、C4 の単音の聞こえる大きさを fm(現行の音)にそろえる補正。実描画で測って決めている(docs/superpowers/specs/2026-10-16-issue-494-mml-timbre-design.md)。
import type { Timbre } from "../bindings/tauri.gen";

export type ToneModule = typeof import("tone");

/** Offline コンテキスト内に作った音源 1 つ。 */
export interface Voice {
  trigger(hz: number, durationSec: number, startSec: number, velocity: number): void;
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
  /** `Tone.Offline` のコールバック内で呼ぶ。出力先と音量(`volumeDb`)を設定済みの音源を返す。 */
  create(Tone: ToneModule, volumeDb: number): Voice;
}

interface PolyLike {
  volume: { value: number };
  toDestination(): unknown;
  triggerAttackRelease(note: number, duration: number, time: number, velocity: number): unknown;
}

function poly(synth: PolyLike, volumeDb: number): Voice {
  synth.toDestination();
  synth.volume.value = volumeDb;
  return { trigger: (hz, d, t, v) => synth.triggerAttackRelease(hz, d, t, v) };
}

const BASIC_ENV = { attack: 0.005, decay: 0.1, sustain: 0.5, release: 0.1 };
type SynthOscillator = NonNullable<NonNullable<ConstructorParameters<ToneModule["Synth"]>[0]>["oscillator"]>;
const basic =
  (oscillator: SynthOscillator) =>
  (Tone: ToneModule, db: number): Voice =>
    poly(new Tone.PolySynth(Tone.Synth, { oscillator, envelope: BASIC_ENV }), db);

export const TIMBRES: Record<Timbre, TimbrePreset> = {
  fm: {
    number: 0,
    name: "fm",
    gainDb: 0,
    releaseSec: 0.1,
    create: (Tone, db) =>
      poly(
        new Tone.PolySynth(Tone.FMSynth, {
          harmonicity: 3,
          modulationIndex: 8,
          envelope: { attack: 0.005, decay: 0.1, sustain: 0.4, release: 0.1 },
        }),
        db,
      ),
  },
  square: { number: 1, name: "square", gainDb: -10.2, releaseSec: 0.1, create: basic({ type: "square" }) },
  pulse: { number: 2, name: "pulse", gainDb: -11.6, releaseSec: 0.1, create: basic({ type: "pulse", width: 0.25 }) },
  triangle: { number: 3, name: "triangle", gainDb: -6.9, releaseSec: 0.1, create: basic({ type: "triangle" }) },
  saw: { number: 4, name: "saw", gainDb: -5.3, releaseSec: 0.1, create: basic({ type: "sawtooth" }) },
  sine: { number: 5, name: "sine", gainDb: -8.7, releaseSec: 0.1, create: basic({ type: "sine" }) },
  organ: {
    number: 6,
    name: "organ",
    gainDb: -14.3,
    releaseSec: 0.05,
    create: (Tone, db) =>
      poly(
        new Tone.PolySynth(Tone.Synth, {
          oscillator: { type: "custom", partials: [1, 0.6, 0.4, 0.25, 0.15] },
          envelope: { attack: 0.01, decay: 0.05, sustain: 1, release: 0.05 },
        }),
        db,
      ),
  },
  epiano: {
    number: 7,
    name: "epiano",
    gainDb: 0,
    releaseSec: 0.3,
    create: (Tone, db) =>
      poly(
        new Tone.PolySynth(Tone.FMSynth, {
          harmonicity: 2,
          modulationIndex: 3,
          envelope: { attack: 0.005, decay: 0.6, sustain: 0.2, release: 0.3 },
          modulationEnvelope: { attack: 0.005, decay: 0.3, sustain: 0.3, release: 0.3 },
        }),
        db,
      ),
  },
  bell: {
    number: 8,
    name: "bell",
    gainDb: 0,
    releaseSec: 0.5,
    create: (Tone, db) =>
      poly(
        new Tone.PolySynth(Tone.FMSynth, {
          harmonicity: 5.07,
          modulationIndex: 12,
          envelope: { attack: 0.001, decay: 0.8, sustain: 0.05, release: 0.5 },
          modulationEnvelope: { attack: 0.001, decay: 0.4, sustain: 0.1, release: 0.4 },
        }),
        db,
      ),
  },
  am: {
    number: 9,
    name: "am",
    gainDb: -1.4,
    releaseSec: 0.2,
    create: (Tone, db) =>
      poly(
        new Tone.PolySynth(Tone.AMSynth, {
          envelope: { attack: 0.005, decay: 0.1, sustain: 0.5, release: 0.2 },
          modulationEnvelope: { attack: 0.005, decay: 0.1, sustain: 0.5, release: 0.2 },
        }),
        db,
      ),
  },
  lead: {
    number: 10,
    name: "lead",
    gainDb: -20.3,
    releaseSec: 0.15,
    create: (Tone, db) => {
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
      );
    },
  },
  bass: {
    number: 11,
    name: "bass",
    gainDb: -14.5,
    releaseSec: 0.2,
    create: (Tone, db) =>
      poly(
        new Tone.PolySynth(Tone.MonoSynth, {
          oscillator: { type: "sawtooth" },
          filter: { Q: 1, type: "lowpass", rolloff: -24 },
          envelope: { attack: 0.01, decay: 0.2, sustain: 0.5, release: 0.2 },
          filterEnvelope: { attack: 0.01, decay: 0.2, sustain: 0.5, release: 0.2, baseFrequency: 200, octaves: 3 },
        }),
        db,
      ),
  },
  drum: {
    number: 12,
    name: "drum",
    gainDb: -6.8,
    releaseSec: 0.3,
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
    gainDb: -4.5,
    releaseSec: 0.05,
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
};

/** 番号順の一覧(ヘルプ表示用)。 */
export const TIMBRE_LIST: TimbrePreset[] = Object.values(TIMBRES).sort((a, b) => a.number - b.number);
