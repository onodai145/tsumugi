import { describe, expect, it } from "vitest";
import type { DriveFile } from "../bindings/tauri.gen";
import {
  deriveViewItems,
  initialImageTransform,
  isRevealed,
  nextIndex,
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
