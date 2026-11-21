import { describe, expect, it } from "vitest";
import type { SpeechCoreInfo } from "../bindings/tauri.gen";
import { creditFor, installedVoices, speechErrorText, voiceCharactersLabel } from "./speechCore";

const pack = (status: SpeechCoreInfo["engine"]["status"] = { type: "notInstalled" }) => ({ status, sizeBytes: 1000, partialBytes: 0 });

const info = (): SpeechCoreInfo => ({
  supported: true,
  termsVersion: "v1",
  termsAccepted: true,
  engine: pack({ type: "ready" }),
  voices: [
    {
      id: "0",
      pack: pack({ type: "ready" }),
      characters: [
        { name: "四国めたん", credit: "VOICEVOX:四国めたん", styles: [{ id: "2", name: "ノーマル" }, { id: "0", name: "あまあま" }] },
        { name: "ずんだもん", credit: "VOICEVOX:ずんだもん", styles: [{ id: "3", name: "ノーマル" }] },
      ],
    },
    { id: "1", pack: pack(), characters: [{ name: "冥鳴ひまり", credit: "VOICEVOX:冥鳴ひまり", styles: [{ id: "14", name: "ノーマル" }] }] },
  ],
});

describe("speechErrorText", () => {
  it("内蔵エンジンの識別コードを、利用者向けの日本語にする", () => {
    expect(speechErrorText("SPEECH_CORE_NOT_READY")).toContain("未取得");
    expect(speechErrorText("invalid: SPEECH_CORE_LOAD_FAILED")).toContain("起動できません");
    expect(speechErrorText("SPEECH_CORE_UNSUPPORTED")).toContain("この環境");
    expect(speechErrorText("SPEECH_NO_VOICE")).toContain("声");
  });
  it("知らない文面は、そのまま返す", () => {
    expect(speechErrorText("SPEECH_ENGINE_UNREACHABLE: connect")).toBe("SPEECH_ENGINE_UNREACHABLE: connect");
    expect(speechErrorText("")).toBe("");
  });
});

describe("creditFor", () => {
  it("声の ID(スタイル ID)から、そのキャラのクレジットを引く", () => {
    expect(creditFor(info(), "3")).toBe("VOICEVOX:ずんだもん");
    expect(creditFor(info(), "0")).toBe("VOICEVOX:四国めたん");
  });
  it("未取得の声でも引ける。見つからない・空・情報なしは null", () => {
    expect(creditFor(info(), "14")).toBe("VOICEVOX:冥鳴ひまり");
    expect(creditFor(info(), "999")).toBeNull();
    expect(creditFor(info(), "")).toBeNull();
    expect(creditFor(null, "3")).toBeNull();
    expect(creditFor(undefined, "3")).toBeNull();
  });
});

describe("installedVoices", () => {
  it("取得済みの VVM の声だけを、Rust と同じ並び・同じ表示名で返す", () => {
    expect(installedVoices(info())).toEqual([
      { id: "2", label: "四国めたん（ノーマル）" },
      { id: "0", label: "四国めたん（あまあま）" },
      { id: "3", label: "ずんだもん（ノーマル）" },
    ]);
  });
  it("情報なし・取得中・失敗の VVM は含めない", () => {
    expect(installedVoices(null)).toEqual([]);
    const i = info();
    i.voices[0].pack = pack({ type: "downloading", downloaded: 1, total: 2 });
    expect(installedVoices(i)).toEqual([]);
    i.voices[0].pack = pack({ type: "failed", reason: "x" });
    expect(installedVoices(i)).toEqual([]);
  });
});

describe("voiceCharactersLabel", () => {
  it("キャラ名を、重複なしで「、」でつなぐ", () => {
    expect(voiceCharactersLabel(info().voices[0])).toBe("四国めたん、ずんだもん");
    const v = info().voices[0];
    v.characters.push({ ...v.characters[0] });
    expect(voiceCharactersLabel(v)).toBe("四国めたん、ずんだもん");
  });
});
