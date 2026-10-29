// MML のドラム音色 `@drumkit`(Issue #499)の、GM のノート番号 → 楽器の割り当てと、楽器ごとの音の作り方の定数。
// Tone.js には触れない(音源を作るのは mmlTimbres.ts)。割り当ては General MIDI のパーカッションマップ
// (docs/superpowers/specs/2026-10-30-issue-499-mml-drumkit-design.md)。`midi = 12 × (オクターブ + 1) + 半音` で、`o2 c` = 36。
// 表にない高さは鳴らさない。音の細かい作り(減衰・フィルタ・周波数)と trimDb は、実機で描画して測って決めた値。

export type DrumPiece =
  | "kick"
  | "sidestick"
  | "snare"
  | "clap"
  | "tomLowFloor"
  | "tomHighFloor"
  | "tomLow"
  | "tomLowMid"
  | "tomHiMid"
  | "tomHigh"
  | "hihatClosed"
  | "hihatOpen"
  | "crash"
  | "ride"
  | "rideBell"
  | "cowbell";

/** GM のノート番号 → 楽器。表にない番号は鳴らさない */
export const DRUM_MAP: Readonly<Record<number, DrumPiece>> = {
  35: "kick",
  36: "kick",
  37: "sidestick",
  38: "snare",
  39: "clap",
  40: "snare",
  41: "tomLowFloor",
  42: "hihatClosed",
  43: "tomHighFloor",
  44: "hihatClosed",
  45: "tomLow",
  46: "hihatOpen",
  47: "tomLowMid",
  48: "tomHiMid",
  49: "crash",
  50: "tomHigh",
  51: "ride",
  53: "rideBell",
  56: "cowbell",
  57: "crash",
  59: "ride",
};

export function drumPieceFor(midi: number): DrumPiece | null {
  return Object.hasOwn(DRUM_MAP, midi) ? DRUM_MAP[midi] : null;
}

/** 膜の音(キック・タム)。`hz` は叩く高さで、`pitchDecay` と `octaves` で下降の幅を決める */
export interface MembraneSpec {
  kind: "membrane";
  hz: number;
  pitchDecay: number;
  octaves: number;
  decay: number;
  release: number;
  trimDb: number;
}

/** ノイズの音(スネア・クラップ・ハイハット・サイドスティック)。`bursts` は、叩く時刻のずれ(秒)。`body` は重ねる膜の音 */
export interface NoiseSpec {
  kind: "noise";
  filter: "highpass" | "bandpass";
  cutoff: number;
  bursts: number[];
  decay: number;
  release: number;
  trimDb: number;
  body?: MembraneSpec;
}

/** 金属の音(シンバル・ベル・カウベル) */
export interface MetalSpec {
  kind: "metal";
  hz: number;
  harmonicity: number;
  modulationIndex: number;
  resonance: number;
  octaves: number;
  decay: number;
  release: number;
  trimDb: number;
}

export type PieceSpec = MembraneSpec | NoiseSpec | MetalSpec;

const tom = (hz: number, trimDb: number): MembraneSpec => ({
  kind: "membrane",
  hz,
  pitchDecay: 0.06,
  octaves: 2,
  decay: 0.25,
  release: 0.15,
  trimDb,
});

export const PIECE_SPECS: Record<DrumPiece, PieceSpec> = {
  kick: { kind: "membrane", hz: 55, pitchDecay: 0.05, octaves: 4, decay: 0.3, release: 0.2, trimDb: -6 },
  sidestick: { kind: "noise", filter: "bandpass", cutoff: 2200, bursts: [0], decay: 0.03, release: 0.02, trimDb: -2.5 },
  snare: {
    kind: "noise",
    filter: "highpass",
    cutoff: 1800,
    bursts: [0],
    decay: 0.16,
    release: 0.05,
    trimDb: -14,
    body: { kind: "membrane", hz: 190, pitchDecay: 0.02, octaves: 2, decay: 0.1, release: 0.05, trimDb: -14 },
  },
  clap: { kind: "noise", filter: "bandpass", cutoff: 1500, bursts: [0, 0.012, 0.024], decay: 0.08, release: 0.05, trimDb: -6 },
  tomLowFloor: tom(70, -6),
  tomHighFloor: tom(85, -6),
  tomLow: tom(105, -6),
  tomLowMid: tom(130, -6),
  tomHiMid: tom(160, -6),
  tomHigh: tom(200, -6),
  hihatClosed: { kind: "noise", filter: "highpass", cutoff: 7000, bursts: [0], decay: 0.04, release: 0.03, trimDb: -4.5 },
  hihatOpen: { kind: "noise", filter: "highpass", cutoff: 7000, bursts: [0], decay: 0.3, release: 0.2, trimDb: -7.5 },
  crash: { kind: "metal", hz: 300, harmonicity: 5.1, modulationIndex: 32, resonance: 4000, octaves: 1.5, decay: 0.4, release: 0.15, trimDb: -13 },
  ride: { kind: "metal", hz: 400, harmonicity: 5.1, modulationIndex: 20, resonance: 5000, octaves: 1.5, decay: 0.3, release: 0.15, trimDb: -6 },
  rideBell: { kind: "metal", hz: 520, harmonicity: 3.1, modulationIndex: 12, resonance: 5500, octaves: 1, decay: 0.25, release: 0.1, trimDb: -11.5 },
  cowbell: { kind: "metal", hz: 560, harmonicity: 2.5, modulationIndex: 8, resonance: 800, octaves: 1, decay: 0.12, release: 0.05, trimDb: -17.5 },
};

/** 1 打の固定の長さ(最後の打ち出しの時刻 + 減衰 + リリース)。`TAIL_SEC` 以下にする */
export function pieceLengthSec(spec: PieceSpec): number {
  const lastBurst = spec.kind === "noise" ? Math.max(...spec.bursts) : 0;
  return lastBurst + spec.decay + spec.release;
}
