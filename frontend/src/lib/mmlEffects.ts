// MML のエフェクト(リバーブ・ディレイ、Issue #499)。トラックごとのバス、送りの切り替え、エフェクトの尾の長さ。
// 設計: docs/superpowers/specs/2026-10-31-issue-499-mml-effects-design.md
import type { NoteEvent } from "../bindings/tauri.gen";
import type { ToneModule, VoiceOutput } from "./mmlTimbres";

/** `Tone.Reverb` のプリディレイ(秒)。Tone の既定と同じ。インパルス応答の長さは `decay + preDelay` */
export const REVERB_PREDELAY_SEC = 0.01;
/** エフェクトの尾(音源の余韻が終わった後に鳴り続ける時間)の上限(秒) */
export const MAX_EFFECT_TAIL_SEC = 5;
/** ディレイの繰り返しが、元の 1/1000(−60 dB)まで下がったら、尾を終える */
const DELAY_FLOOR = 0.001;
/** エフェクトの出力を、描画の終わりの手前から 0 までフェードさせる長さ(秒)。尾が上限で頭打ちになっても、急に切れない */
const FADE_OUT_SEC = 0.3;
/** エフェクトの出力の補正(dB)。リバーブは畳み込みで音量が変わるので、実測で決める(計画の Task 2 のゲート) */
export const REVERB_TRIM_DB = 0;
export const DELAY_TRIM_DB = 0;

export function reverbTailSec(decayMs: number): number {
  return Math.min(MAX_EFFECT_TAIL_SEC, decayMs / 1000 + REVERB_PREDELAY_SEC);
}

export function delayTailSec(timeMs: number, feedbackPercent: number): number {
  const feedback = feedbackPercent / 100;
  const repeats = feedback <= 0 ? 0 : Math.ceil(Math.log(DELAY_FLOOR) / Math.log(feedback));
  return Math.min(MAX_EFFECT_TAIL_SEC, (timeMs / 1000) * (repeats + 1));
}

/** 使ったエフェクトの尾の最大(秒)。何も使わなければ 0 */
export function effectTailSec(notes: NoteEvent[]): number {
  let tail = 0;
  for (const n of notes) {
    if (n.reverb) tail = Math.max(tail, reverbTailSec(n.reverb.decayMs));
    if (n.delay) tail = Math.max(tail, delayTailSec(n.delay.timeMs, n.delay.feedback));
  }
  return tail;
}

type GainNode = InstanceType<ToneModule["Gain"]>;
type ToneNode = import("tone").ToneAudioNode;

/** (エフェクトの種類, 長さや間隔・繰り返し)の組 1 つ。`send` を量にした音だけが、このエフェクトへ送る */
interface Group {
  send: GainNode;
  /** その音が、この組へ送る量(0〜1) */
  level(n: NoteEvent): number;
}

interface TrackFx {
  /** そのトラックの音源の合流点。出力へも(ドライ)、各組の送りへもつなぐ */
  bus: GainNode;
  groups: Map<string, Group>;
  /** 組ごとに、最後に設定した送りの量(初期値 0) */
  sent: Map<Group, number>;
}

export interface TrackEffects {
  /** そのトラックの音源のつなぎ先。エフェクトを使わないトラックは undefined(出力へ直接つなぐ) */
  outFor(track: number): VoiceOutput | undefined;
  /** リバーブのインパルス応答の生成が終わるまで待つ(終わる前に音を鳴らすと、残響が付かない) */
  ready(): Promise<unknown>;
  /**
   * 音の始まりの時刻に、その音の組の送りだけを量にし、ほかの組を 0 にする。
   * 音は、トラックごとに時刻の昇順で渡すこと(同じ状態を重ねて設定しないため、直前の設定を覚えている)。
   */
  apply(n: NoteEvent): void;
}

const gainOf = (db: number) => 10 ** (db / 20);

/**
 * エフェクトを使うトラックにだけ、バスと、組ごとの「送り → エフェクト → 補正 → 出力」を作る。
 * `endSec` は描画の終わりの時刻(秒)。エフェクトの出力は、その `FADE_OUT_SEC` 前から 0 までフェードする。
 * `Tone.Offline` のコールバックの中で、音源を作る前に呼ぶ。
 */
export function createTrackEffects(Tone: ToneModule, notes: NoteEvent[], endSec: number): TrackEffects {
  const tracks = new Map<number, TrackFx>();
  const pending: Promise<unknown>[] = [];

  const fxFor = (track: number): TrackFx => {
    let fx = tracks.get(track);
    if (!fx) {
      fx = { bus: new Tone.Gain(1).toDestination(), groups: new Map(), sent: new Map() };
      tracks.set(track, fx);
    }
    return fx;
  };

  const addGroup = (fx: TrackFx, key: string, effect: ToneNode, trimDb: number, level: Group["level"]) => {
    const send = new Tone.Gain(0);
    const trim = new Tone.Gain(gainOf(trimDb)).toDestination();
    trim.gain.setValueAtTime(gainOf(trimDb), Math.max(0, endSec - FADE_OUT_SEC));
    trim.gain.linearRampToValueAtTime(0, endSec);
    fx.bus.connect(send);
    send.connect(effect);
    effect.connect(trim);
    fx.groups.set(key, { send, level });
  };

  for (const n of notes) {
    const rv = n.reverb;
    if (rv) {
      const fx = fxFor(n.track);
      const key = `reverb:${rv.decayMs}`;
      if (!fx.groups.has(key)) {
        const reverb = new Tone.Reverb({ decay: rv.decayMs / 1000, preDelay: REVERB_PREDELAY_SEC });
        pending.push(reverb.ready);
        addGroup(fx, key, reverb, REVERB_TRIM_DB, (x) => (x.reverb && x.reverb.decayMs === rv.decayMs ? x.reverb.level / 100 : 0));
      }
    }
    const dl = n.delay;
    if (dl) {
      const fx = fxFor(n.track);
      const key = `delay:${dl.timeMs}:${dl.feedback}`;
      if (!fx.groups.has(key)) {
        const delay = new Tone.FeedbackDelay({ delayTime: dl.timeMs / 1000, feedback: dl.feedback / 100, maxDelay: 1 });
        addGroup(fx, key, delay, DELAY_TRIM_DB, (x) =>
          x.delay && x.delay.timeMs === dl.timeMs && x.delay.feedback === dl.feedback ? x.delay.level / 100 : 0,
        );
      }
    }
  }

  return {
    outFor: (track) => tracks.get(track)?.bus,
    ready: () => Promise.all(pending),
    apply(n) {
      const fx = tracks.get(n.track);
      if (!fx) return;
      const t = n.startSec ?? 0;
      for (const g of fx.groups.values()) {
        const level = g.level(n);
        if ((fx.sent.get(g) ?? 0) === level) continue;
        g.send.gain.setValueAtTime(level, t);
        fx.sent.set(g, level);
      }
    },
  };
}
