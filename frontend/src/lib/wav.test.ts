import { describe, expect, it } from "vitest";
import { encodeWavMono16, wavToDataUrl } from "./wav";

const ascii = (b: Uint8Array, from: number, to: number) => String.fromCharCode(...b.subarray(from, to));
const view = (b: Uint8Array) => new DataView(b.buffer, b.byteOffset, b.byteLength);

describe("encodeWavMono16", () => {
  it("書き出す WAV は 16bit モノラル PCM で、ヘッダと長さが正しい", () => {
    const wav = encodeWavMono16([new Float32Array(100)], 22050);
    const v = view(wav);
    expect(ascii(wav, 0, 4)).toBe("RIFF");
    expect(ascii(wav, 8, 12)).toBe("WAVE");
    expect(ascii(wav, 12, 16)).toBe("fmt ");
    expect(ascii(wav, 36, 40)).toBe("data");
    expect(v.getUint16(20, true)).toBe(1); // PCM
    expect(v.getUint16(22, true)).toBe(1); // mono
    expect(v.getUint32(24, true)).toBe(22050);
    expect(v.getUint16(34, true)).toBe(16);
    expect(v.getUint32(40, true)).toBe(200); // 100 サンプル × 2 バイト
    expect(v.getUint32(4, true)).toBe(wav.length - 8);
    expect(wav.length).toBe(44 + 200);
  });

  it("サンプルを 16bit に変換し、範囲外はクリップする", () => {
    const wav = encodeWavMono16([Float32Array.from([0, 0.25, -0.25, 1, -1, 2, -2])], 8000);
    const v = view(wav);
    const got = Array.from({ length: 7 }, (_, i) => v.getInt16(44 + i * 2, true));
    expect(got).toEqual([0, 8192, -8192, 32767, -32767, 32767, -32767]);
  });

  it("複数チャンネルは平均してモノラルにする", () => {
    const wav = encodeWavMono16([Float32Array.from([0.5, 0]), Float32Array.from([-0.5, 0.5])], 8000);
    const v = view(wav);
    expect(v.getInt16(44, true)).toBe(0);
    expect(v.getInt16(46, true)).toBe(8192); // (0 + 0.5) / 2 = 0.25
  });

  it("チャンネルが空でも例外にならず、データ長 0 の WAV を返す", () => {
    const wav = encodeWavMono16([], 8000);
    expect(view(wav).getUint32(40, true)).toBe(0);
  });
});

describe("wavToDataUrl", () => {
  it("base64 の data: URL にして、元のバイト列に戻せる", () => {
    const wav = encodeWavMono16([Float32Array.from([0.1, -0.2, 0.3])], 22050);
    const url = wavToDataUrl(wav);
    expect(url.startsWith("data:audio/wav;base64,")).toBe(true);
    const back = Uint8Array.from(atob(url.slice("data:audio/wav;base64,".length)), (c) => c.charCodeAt(0));
    expect(Array.from(back)).toEqual(Array.from(wav));
  });

  it("大きなバッファ(10 秒分)でも例外にならない", () => {
    const wav = encodeWavMono16([new Float32Array(22050 * 10)], 22050);
    expect(() => wavToDataUrl(wav)).not.toThrow();
  });
});
