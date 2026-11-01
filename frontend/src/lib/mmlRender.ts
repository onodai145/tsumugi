// MmlScore を Tone.js の Tone.Offline で WAV にする(Issue #75、音色は Issue #494)。
// ここでは音を出さない。出力は data: URL で、再生は Rust(play_notify_sound)が行う。
// Tone.js は重いので遅延 import し、通常の起動コストに乗せない。
// エフェクト(リバーブ・ディレイ、Issue #499)のバスと送りは mmlEffects.ts。
import type { MmlScore, Timbre, TimbreParams } from "../bindings/tauri.gen";
import { createTrackEffects, effectTailSec } from "./mmlEffects";
import { TIMBRES, type Voice } from "./mmlTimbres";
import { encodeWavMono16, wavToDataUrl } from "./wav";

const SAMPLE_RATE = 22050;
/** 音色の余韻(リリース)が切れないように足す秒数。全プリセットの releaseSec はこれ以下にする */
export const TAIL_SEC = 0.6;

/**
 * `Tone.Offline` は、使うコンテキストを Tone 全体の変数で切り替える。リバーブの準備を待つ間に別の描画が始まると、
 * 音源が別の描画のコンテキストに作られて壊れる(Issue #499)ので、描画は 1 つずつ順に走らせる。
 */
let queue: Promise<unknown> = Promise.resolve();

export function renderScore(score: MmlScore): Promise<string> {
  const run = queue.then(() => renderScoreNow(score));
  queue = run.catch(() => undefined);
  return run;
}

async function renderScoreNow(score: MmlScore): Promise<string> {
  const Tone = await import("tone");
  // 描画の前のコンテキスト。`Tone.Offline` は途中で失敗するとコンテキストを戻さないので、成功・失敗にかかわらず自分で戻す
  const originalContext = Tone.getContext();
  // 音源の余韻(TAIL_SEC)の後に、エフェクトの尾(使わなければ 0)が続く
  const durationSec = (score.totalSec ?? 0) + TAIL_SEC + effectTailSec(score.notes);
  const rendering = Tone.Offline(
    async () => {
      // 複数トラックを足すと 1.0 を超えて WAV 化で頭打ちになるので、トラック数で割って揃える(単一トラックは補正なし)。
      const trackDb = score.trackCount > 1 ? Tone.gainToDb(1 / score.trackCount) : 0;
      // エフェクト(Issue #499)。使うトラックにだけバスを作る(使わないトラックは、今までどおり音源を出力へ直接つなぐ)。
      // リバーブのインパルス応答の生成が終わるまで待ってから、音源を作って鳴らす。
      const effects = createTrackEffects(Tone, score.notes, durationSec);
      await effects.ready();
      // 音源は (トラック, 音色) ごとに最初に使うときに作る。
      const voices = new Map<string, Voice>();
      const voiceFor = (track: number, timbre: Timbre, noteParams: TimbreParams | null): Voice => {
        const preset = TIMBRES[timbre];
        const volumeDb = preset.gainDb + trackDb;
        // 打楽器は、パラメーターを無視する(パラメーターが違っても 1 つの音源を使い回す)。
        // デチューンは音源に渡さず、鳴らす周波数の倍率にする(下)。音源の組は、エンベロープとビブラートだけで決める
        const params =
          preset.percussive || !noteParams || (!noteParams.envelope && !noteParams.vibrato)
            ? null
            : { envelope: noteParams.envelope, vibrato: noteParams.vibrato, detune: 0 };
        // つなぎ先はトラックごとに決まるので、音源の組(下の key)には入れない
        const out = effects.outFor(track);
        if (preset.perNote) return preset.create(Tone, volumeDb, params, out);
        // PolySynth は描画中に設定を時刻つきで変えられないので、パラメーターの組ごとに別の音源を作る
        const key = `${track}:${timbre}:${params === null ? "" : JSON.stringify(params)}`;
        let voice = voices.get(key);
        if (!voice) {
          voice = preset.create(Tone, volumeDb, params, out);
          voices.set(key, voice);
        }
        return voice;
      };
      for (const n of score.notes) {
        // 音の始まりの時刻に、エフェクトへの送りを切り替える(音は、トラックごとに時刻の昇順)
        effects.apply(n);
        const percussive = TIMBRES[n.timbre].percussive;
        // デチューン(セント)は、周波数に 2^(セント/1200) を掛ける。打楽器は無視する
        const cents = percussive ? 0 : (n.params?.detune ?? 0);
        const ratio = cents === 0 ? 1 : 2 ** (cents / 1200);
        const hz = Tone.Frequency(n.midi, "midi").toFrequency() * ratio;
        // ポルタメント: 滑りの長さは音の長さ以下に縮める。滑り始めにも、同じ倍率を掛ける
        const durationSec = n.durationSec ?? 0;
        const slideSec = n.glide ? Math.min(n.glide.ms / 1000, durationSec) : 0;
        const glide =
          n.glide && !percussive && slideSec > 0
            ? { fromHz: Tone.Frequency(n.glide.fromMidi, "midi").toFrequency() * ratio, sec: slideSec }
            : undefined;
        voiceFor(n.track, n.timbre, n.params ?? null).trigger(hz, durationSec, n.startSec ?? 0, n.velocity ?? 0, n.midi, glide);
      }
    },
    durationSec,
    1,
    SAMPLE_RATE,
  );
  let buffer: Awaited<typeof rendering>;
  try {
    buffer = await rendering;
  } finally {
    Tone.setContext(originalContext);
  }
  const audio = buffer.get();
  if (!audio) throw new Error("レンダリング結果が空でした");
  return wavToDataUrl(encodeWavMono16([audio.getChannelData(0)], audio.sampleRate));
}
