import { describe, expect, it } from "vitest";
import type { DriveFile } from "../bindings/tauri.gen";
import {
  deriveViewItems,
  initialImageTransform,
  isImageOverflowing,
  isRevealed,
  nextIndex,
  playerSrc,
  prevIndex,
  reveal,
  rotateCCW,
  rotateCW,
  toggleFlipH,
  toggleFlipV,
} from "./mediaViewer.svelte";

function file(overrides: Partial<DriveFile>): DriveFile {
  return {
    id: "f1",
    mimeType: "image/png",
    isSensitive: false,
    url: "https://example.com/f1.png",
    thumbnailUrl: null,
    ...overrides,
  };
}

describe("deriveViewItems", () => {
  it("画像・動画・音声のみを残し、その他のファイルは除外する", () => {
    const files = [
      file({ id: "img", mimeType: "image/png" }),
      file({ id: "vid", mimeType: "video/mp4" }),
      file({ id: "aud", mimeType: "audio/mpeg" }),
      file({ id: "pdf", mimeType: "application/pdf" }),
    ];
    expect(deriveViewItems(files).map((f) => f.id)).toEqual(["img", "vid", "aud"]);
  });
});

describe("nextIndex / prevIndex", () => {
  it("nextIndexは末尾で先頭に折り返す", () => {
    expect(nextIndex(2, 3)).toBe(0);
    expect(nextIndex(0, 3)).toBe(1);
  });

  it("prevIndexは先頭で末尾に折り返す", () => {
    expect(prevIndex(0, 3)).toBe(2);
    expect(prevIndex(2, 3)).toBe(1);
  });
});

describe("閲覧注意ゲーティング", () => {
  it("isSensitiveでなければ常にrevealed扱い", () => {
    const f = file({ id: "img", isSensitive: false });
    expect(isRevealed({}, f)).toBe(true);
  });

  it("isSensitiveかつrevealed未登録ならfalse", () => {
    const f = file({ id: "img", isSensitive: true });
    expect(isRevealed({}, f)).toBe(false);
  });

  it("isSensitiveでもrevealedに登録済みならtrue", () => {
    const f = file({ id: "img", isSensitive: true });
    expect(isRevealed({ img: true }, f)).toBe(true);
  });

  it("revealで対象ファイルのidだけをtrueにした新しいRecordを返す", () => {
    const f = file({ id: "img", isSensitive: true });
    const result = reveal({ other: true }, f);
    expect(result).toEqual({ other: true, img: true });
  });
});

describe("画像のrotation/flip状態", () => {
  it("初期状態はrotation:0, flipH/flipV:false", () => {
    expect(initialImageTransform).toEqual({ rotation: 0, flipH: false, flipV: false });
  });

  it("rotateCWは90度ずつ進み、270の次は0に戻る", () => {
    let t = initialImageTransform;
    t = rotateCW(t);
    expect(t.rotation).toBe(90);
    t = rotateCW(rotateCW(t));
    expect(t.rotation).toBe(270);
    t = rotateCW(t);
    expect(t.rotation).toBe(0);
  });

  it("rotateCCWは90度ずつ戻り、0の前は270になる", () => {
    let t = initialImageTransform;
    t = rotateCCW(t);
    expect(t.rotation).toBe(270);
  });

  it("toggleFlipH/toggleFlipVはbooleanを反転する", () => {
    let t = initialImageTransform;
    t = toggleFlipH(t);
    expect(t.flipH).toBe(true);
    t = toggleFlipV(t);
    expect(t).toEqual({ rotation: 0, flipH: true, flipV: true });
  });
});

describe("playerSrc", () => {
  it("Vidstackが認識しない動画MIME(video/quicktime等)はvideo/mp4に正規化する", () => {
    for (const mimeType of ["video/quicktime", "video/x-m4v", "video/x-matroska", "video/3gpp"]) {
      expect(playerSrc(file({ mimeType, url: "https://example.com/files/webpublic-abc" }))).toEqual({
        src: "https://example.com/files/webpublic-abc",
        type: "video/mp4",
      });
    }
  });

  it("Vidstackが認識する動画MIMEと音声MIMEはそのまま渡す", () => {
    expect(playerSrc(file({ mimeType: "video/webm", url: "u" }))).toEqual({ src: "u", type: "video/webm" });
    expect(playerSrc(file({ mimeType: "audio/mpeg", url: "u" }))).toEqual({ src: "u", type: "audio/mpeg" });
  });
});

describe("isImageOverflowing", () => {
  const natural = { width: 4000, height: 3000 };
  const container = { width: 1200, height: 800 };
  // Cropper.jsのCSS行列は[a, b, c, d, e, f]。a=d=s, b=c=0がscale s。
  const scale = (s: number) => [s, 0, 0, s, 0, 0];
  const rotate90 = (s: number) => [0, s, -s, 0, 0, 0];

  it("contain でフィットした初期状態(倍率が1未満)ははみ出していない", () => {
    expect(isImageOverflowing(scale(800 / 3000), natural, container)).toBe(false);
  });

  it("フィットより大きくなったら、倍率が1未満でもはみ出している", () => {
    expect(isImageOverflowing(scale(0.4), natural, container)).toBe(true);
  });

  it("自然サイズより小さい画像を拡大してフィットした状態(倍率が1超)もはみ出していない", () => {
    const small = { width: 300, height: 200 };
    expect(isImageOverflowing(scale(4), small, container)).toBe(false);
  });

  it("90度回転後は縦横が入れ替わった外接サイズで判定する(a=0でもはみ出し判定できる)", () => {
    expect(isImageOverflowing(rotate90(1), natural, container)).toBe(true);
    expect(isImageOverflowing(rotate90(800 / 4000), natural, container)).toBe(false);
  });

  it("反転(aが負)でも符号に依らず判定する", () => {
    expect(isImageOverflowing([-0.4, 0, 0, 0.4, 0, 0], natural, container)).toBe(true);
    expect(isImageOverflowing([-800 / 3000, 0, 0, 800 / 3000, 0, 0], natural, container)).toBe(false);
  });

  it("フィット時の丸め誤差(1px以内)ははみ出しとみなさない", () => {
    expect(isImageOverflowing(scale(800.5 / 3000), natural, container)).toBe(false);
  });

  it("自然サイズが未取得(0)の間は常にはみ出していない", () => {
    expect(isImageOverflowing(scale(5), { width: 0, height: 0 }, container)).toBe(false);
  });
});
