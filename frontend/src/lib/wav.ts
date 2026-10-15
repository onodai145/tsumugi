// PCM サンプルを 16bit モノラル WAV にする(Issue #75)。Tone.Offline の出力を data: URL にして
// 既存の play_notify_sound(Rust/rodio)に渡すために使う。

export function encodeWavMono16(channels: Float32Array[], sampleRate: number): Uint8Array {
  const length = channels[0]?.length ?? 0;
  const dataBytes = length * 2;
  const view = new DataView(new ArrayBuffer(44 + dataBytes));
  const writeStr = (offset: number, s: string) => {
    for (let i = 0; i < s.length; i++) view.setUint8(offset + i, s.charCodeAt(i));
  };
  writeStr(0, "RIFF");
  view.setUint32(4, 36 + dataBytes, true);
  writeStr(8, "WAVE");
  writeStr(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true); // PCM
  view.setUint16(22, 1, true); // mono
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  writeStr(36, "data");
  view.setUint32(40, dataBytes, true);
  for (let i = 0; i < length; i++) {
    let s = 0;
    for (const ch of channels) s += ch[i] ?? 0;
    s /= channels.length;
    const clamped = Math.max(-1, Math.min(1, s));
    view.setInt16(44 + i * 2, Math.round(clamped * 0x7fff), true);
  }
  return new Uint8Array(view.buffer);
}

export function wavToDataUrl(wav: Uint8Array): string {
  let bin = "";
  const CHUNK = 0x4000;
  for (let i = 0; i < wav.length; i += CHUNK) {
    bin += String.fromCharCode(...wav.subarray(i, i + CHUNK));
  }
  return `data:audio/wav;base64,${btoa(bin)}`;
}
