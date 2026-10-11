import { describe, expect, it } from "vitest";
import type { DriveFile, Note } from "../bindings/tauri.gen";
import { replyPreviewBody } from "./replyPreview";

function file(mimeType: string): DriveFile {
  return {
    id: "f1",
    mimeType,
    isSensitive: false,
    url: "https://example.com/f",
    thumbnailUrl: null,
    name: "f",
    size: 1,
  };
}

function partial(overrides: Partial<Note>): Note {
  return {
    text: null,
    cw: null,
    files: [],
    renote: null,
    ...overrides,
  } as Note;
}

describe("replyPreviewBody", () => {
  it("collapses newlines and runs of whitespace in the text for one-line display", () => {
    expect(replyPreviewBody(partial({ text: "一行目\n\n二行目   三行目" }))).toEqual({
      kind: "text",
      text: "一行目 二行目 三行目",
    });
  });

  it("collapses newlines in the cw for one-line display", () => {
    expect(replyPreviewBody(partial({ cw: "注意\n  ネタバレ", text: "本文" }))).toEqual({
      kind: "cw",
      text: "注意 ネタバレ",
    });
  });

  it("returns the text when there is no cw", () => {
    expect(replyPreviewBody(partial({ text: "こんにちは" }))).toEqual({ kind: "text", text: "こんにちは" });
  });

  it("prefers the cw over the text", () => {
    expect(replyPreviewBody(partial({ cw: "ネタバレ注意", text: "本文" }))).toEqual({
      kind: "cw",
      text: "ネタバレ注意",
    });
  });

  it("labels image-only notes as (画像)", () => {
    expect(replyPreviewBody(partial({ files: [file("image/png"), file("image/jpeg")] }))).toEqual({
      kind: "label",
      label: "(画像)",
    });
  });

  it("labels notes with a non-image file as (ファイル)", () => {
    expect(replyPreviewBody(partial({ files: [file("image/png"), file("video/mp4")] }))).toEqual({
      kind: "label",
      label: "(ファイル)",
    });
  });

  it("labels a pure renote as (Renote)", () => {
    expect(replyPreviewBody(partial({ renote: {} as Note }))).toEqual({ kind: "label", label: "(Renote)" });
  });

  it("treats whitespace-only text as empty", () => {
    expect(replyPreviewBody(partial({ text: "  \n " }))).toBeNull();
  });

  it("returns null when there is nothing to show", () => {
    expect(replyPreviewBody(partial({}))).toBeNull();
  });
});
