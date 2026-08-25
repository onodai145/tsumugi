#!/usr/bin/env python3
"""通知音プリセット(beep/chime/ping/pop)のWAVアセットを生成する。

frontend/src/lib/store.svelte.ts の playTone()/playPreset() (Web Audio API
オシレーター合成) と同じパラメータで、事前レンダリングしたWAVを
src-tauri/assets/sounds/ に書き出す(Issue #12: 実行時合成をやめてRust側で
ネイティブ再生する設計に伴う一度きりのアセット生成)。

再生成する場合: python3 scripts/render_notify_sounds.py
"""

import math
import os
import struct
import wave

SAMPLE_RATE = 44100
FLOOR = 0.0001
ATTACK = 0.01


def synth_tone(freq: float, delay: float, dur: float, wave_type: str = "sine", peak: float = 0.15) -> list[float]:
    n_delay = int(delay * SAMPLE_RATE)
    n_dur = int(dur * SAMPLE_RATE)
    n_attack = max(1, min(n_dur, int(ATTACK * SAMPLE_RATE)))
    samples = [0.0] * (n_delay + n_dur)
    for i in range(n_dur):
        t = i / SAMPLE_RATE
        if i < n_attack:
            amp = FLOOR * (peak / FLOOR) ** (i / n_attack)
        else:
            remain = n_dur - n_attack
            frac = (i - n_attack) / remain if remain > 0 else 1.0
            amp = peak * (FLOOR / peak) ** frac
        if wave_type == "sine":
            wave_val = math.sin(2 * math.pi * freq * t)
        else:  # triangle
            wave_val = (2 / math.pi) * math.asin(math.sin(2 * math.pi * freq * t))
        samples[n_delay + i] = amp * wave_val
    return samples


def mix(*tracks: list[float]) -> list[float]:
    length = max(len(t) for t in tracks)
    out = [0.0] * length
    for t in tracks:
        for i, v in enumerate(t):
            out[i] += v
    return [max(-1.0, min(1.0, v)) for v in out]


def write_wav(path: str, samples: list[float]) -> None:
    with wave.open(path, "w") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SAMPLE_RATE)
        frames = b"".join(struct.pack("<h", int(v * 32767)) for v in samples)
        w.writeframes(frames)


def main() -> None:
    out_dir = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "assets", "sounds")
    os.makedirs(out_dir, exist_ok=True)

    presets = {
        "beep.wav": synth_tone(880, 0, 0.18),
        "chime.wav": mix(synth_tone(660, 0, 0.12), synth_tone(880, 0.1, 0.16)),
        "ping.wav": synth_tone(1300, 0, 0.09, "sine", 0.12),
        "pop.wav": synth_tone(220, 0, 0.09, "triangle", 0.2),
    }
    for name, samples in presets.items():
        write_wav(os.path.join(out_dir, name), samples)
        print(f"wrote {name} ({len(samples)} samples)")


if __name__ == "__main__":
    main()
