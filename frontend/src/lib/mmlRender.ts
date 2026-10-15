// MmlScore を Tone.js の Tone.Offline で WAV にする(Issue #75)。
// ここでは音を出さない。出力は data: URL で、再生は Rust(play_notify_sound)が行う。
// Tone.js は重いので遅延 import し、通常の起動コストに乗せない。
import type { MmlScore } from "../bindings/tauri.gen";
import { encodeWavMono16, wavToDataUrl } from "./wav";

const SAMPLE_RATE = 22050;
/** FM 音源の余韻(リリース)が切れないように足す秒数 */
const TAIL_SEC = 0.6;

export async function renderScore(score: MmlScore): Promise<string> {
  const Tone = await import("tone");
  const buffer = await Tone.Offline(
    () => {
      const synths = Array.from({ length: score.trackCount }, () => {
        const synth = new Tone.PolySynth(Tone.FMSynth, {
          harmonicity: 3,
          modulationIndex: 8,
          envelope: { attack: 0.005, decay: 0.1, sustain: 0.4, release: 0.1 },
        }).toDestination();
        // 1 音源のピークは約 0.316。複数トラックを足すと 1.0 を超えて WAV 化で頭打ちになるので、
        // トラック数で割って揃える(単一トラックは従来どおり)。
        if (score.trackCount > 1) synth.volume.value = Tone.gainToDb(1 / score.trackCount);
        return synth;
      });
      for (const n of score.notes) {
        const hz = Tone.Frequency(n.midi, "midi").toFrequency();
        synths[n.track].triggerAttackRelease(hz, n.durationSec ?? 0, n.startSec ?? 0, n.velocity ?? 0);
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
