// MML の音色(@番号 / @名前)ごとの音源の作り方(Issue #494)。
// 音色の番号と名前は Rust の Timbre::ALL(src-tauri/src/domain/mml.rs)と同じにする。
// gainDb は、C4 の単音の聞こえる大きさ(RMS)を fm(現行の音)にそろえる補正。実際の renderScore を WebKitGTK 2.54.1(Xvfb)で測って校正した値で、noise だけはピーク上限(0.36)を優先して小さめにしている(docs/superpowers/specs/2026-10-16-issue-494-mml-timbre-design.md の「校正結果」)。
import type { Timbre } from "../bindings/tauri.gen";
import { PIECE_SPECS, drumPieceFor, type DrumPiece, type MembraneSpec } from "./mmlDrumkit";

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

/** `@drumkit`。音の高さ(midi)から GM の楽器を選んで鳴らす。割り当てのない高さは鳴らさない。書いた長さは無視する */
function drumkit(Tone: ToneModule, volumeDb: number): Voice {
  const out = new Tone.Volume(volumeDb).toDestination();
  // 膜の音源(キック・タム・スネアの胴)は、楽器ごとに 1 つ作って使い回す。ノイズと金属は単音用なので 1 打ごとに作る
  const membranes = new Map<DrumPiece, PolyLike & { connect(to: unknown): unknown }>();
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
  return {
    trigger(_hz, _d, t, v, midi) {
      const piece = midi === undefined ? null : drumPieceFor(midi);
      if (!piece) return;
      const spec = PIECE_SPECS[piece];
      if (spec.kind === "membrane") {
        membraneFor(piece, spec).triggerAttackRelease(spec.hz, spec.decay, t, v);
      } else if (spec.kind === "noise") {
        for (const offset of spec.bursts) {
          const noise = new Tone.NoiseSynth({
            noise: { type: "white" },
            envelope: { attack: 0.001, decay: spec.decay, sustain: 0, release: spec.release },
          });
          const filter = new Tone.Filter(spec.cutoff, spec.filter);
          noise.volume.value = spec.trimDb;
          noise.connect(filter);
          filter.connect(out);
          noise.triggerAttackRelease(spec.decay, t + offset, v);
        }
        if (spec.body) membraneFor(piece, spec.body).triggerAttackRelease(spec.body.hz, spec.body.decay, t, v);
      } else {
        const metal = new Tone.MetalSynth({
          harmonicity: spec.harmonicity,
          modulationIndex: spec.modulationIndex,
          octaves: spec.octaves,
          resonance: spec.resonance,
          envelope: { attack: 0.001, decay: spec.decay, release: spec.release },
        });
        metal.volume.value = spec.trimDb;
        metal.connect(out);
        metal.triggerAttackRelease(spec.hz, spec.decay, t, v);
      }
    },
  };
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
    gainDb: -0.5,
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
    gainDb: 0.1,
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
    gainDb: 4.3,
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
    gainDb: -15.9,
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
    gainDb: -10.0,
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
    gainDb: -6.1,
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
    gainDb: -5.0,
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
  drumkit: {
    number: 14,
    name: "drumkit",
    gainDb: 0,
    releaseSec: 0.2,
    create: (Tone, db) => drumkit(Tone, db),
  },
};

/** 番号順の一覧(ヘルプ表示用)。 */
export const TIMBRE_LIST: TimbrePreset[] = Object.values(TIMBRES).sort((a, b) => a.number - b.number);
