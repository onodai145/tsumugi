// MmlScore を Tone.js の Tone.Offline で WAV にする(Issue #75、音色は Issue #494)。
// ここでは音を出さない。出力は data: URL で、再生は Rust(play_notify_sound)が行う。
// Tone.js は重いので遅延 import し、通常の起動コストに乗せない。
import type { MmlScore, Timbre, TimbreParams } from "../bindings/tauri.gen";
import { TIMBRES, type Voice } from "./mmlTimbres";
import { encodeWavMono16, wavToDataUrl } from "./wav";

const SAMPLE_RATE = 22050;
/** 音色の余韻(リリース)が切れないように足す秒数。全プリセットの releaseSec はこれ以下にする */
export const TAIL_SEC = 0.6;

export async function renderScore(score: MmlScore): Promise<string> {
  const Tone = await import("tone");
  const buffer = await Tone.Offline(
    () => {
      // 複数トラックを足すと 1.0 を超えて WAV 化で頭打ちになるので、トラック数で割って揃える(単一トラックは補正なし)。
      const trackDb = score.trackCount > 1 ? Tone.gainToDb(1 / score.trackCount) : 0;
      // 音源は (トラック, 音色) ごとに最初に使うときに作る。
      const voices = new Map<string, Voice>();
      const voiceFor = (track: number, timbre: Timbre, noteParams: TimbreParams | null): Voice => {
        const preset = TIMBRES[timbre];
        const volumeDb = preset.gainDb + trackDb;
        // 打楽器は、パラメーターを無視する(パラメーターが違っても 1 つの音源を使い回す)
        const params = preset.percussive ? null : noteParams;
        if (preset.perNote) return preset.create(Tone, volumeDb, params);
        // PolySynth は描画中に設定を時刻つきで変えられないので、パラメーターの組ごとに別の音源を作る
        const key = `${track}:${timbre}:${params === null ? "" : JSON.stringify(params)}`;
        let voice = voices.get(key);
        if (!voice) {
          voice = preset.create(Tone, volumeDb, params);
          voices.set(key, voice);
        }
        return voice;
      };
      for (const n of score.notes) {
        const hz = Tone.Frequency(n.midi, "midi").toFrequency();
        voiceFor(n.track, n.timbre, n.params ?? null).trigger(hz, n.durationSec ?? 0, n.startSec ?? 0, n.velocity ?? 0, n.midi);
      }
    },
    (score.totalSec ?? 0) + TAIL_SEC,
    1,
    SAMPLE_RATE,
  );
  const audio = buffer.get();
  if (!audio) throw new Error("レンダリング結果が空でした");
  return wavToDataUrl(encodeWavMono16([audio.getChannelData(0)], audio.sampleRate));
}
