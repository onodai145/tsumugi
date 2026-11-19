import { describe, expect, it } from "vitest";
import {
  DEFAULT_COLUMN_SPEECH,
  DEFAULT_SPEECH_SETTINGS,
  withColumnSpeechDefaults,
  withSpeechSettingsDefaults,
} from "./speechDefaults";

// Rust 側の `#[serde(default)]` のフィールドは、生成される TS の型では省略可能になる。
// 古い設定ファイルや IPC のモックで欠けていても、フロントでは既定値で補う。
describe("withColumnSpeechDefaults", () => {
  it("undefined / null / 空のオブジェクトは既定値になり、既定値そのものは共有しない", () => {
    expect(withColumnSpeechDefaults(undefined)).toEqual(DEFAULT_COLUMN_SPEECH);
    expect(withColumnSpeechDefaults(null)).toEqual(DEFAULT_COLUMN_SPEECH);
    expect(withColumnSpeechDefaults({})).toEqual(DEFAULT_COLUMN_SPEECH);
    expect(withColumnSpeechDefaults(undefined)).not.toBe(DEFAULT_COLUMN_SPEECH);
  });

  it("ある項目はそのまま、無い項目だけ補う", () => {
    expect(withColumnSpeechDefaults({ enabled: true, queueLimit: 9 })).toEqual({
      enabled: true,
      speakAuthor: true,
      queueLimit: 9,
      voiceId: "",
    });
    expect(withColumnSpeechDefaults({ speakAuthor: false }).speakAuthor).toBe(false);
  });
});

describe("withSpeechSettingsDefaults", () => {
  it("欠けた項目は既定値。話速・音量の null も既定値(1)に戻す", () => {
    expect(withSpeechSettingsDefaults(undefined)).toEqual(DEFAULT_SPEECH_SETTINGS);
    expect(withSpeechSettingsDefaults({ speedScale: null, volume: null })).toMatchObject({ speedScale: 1, volume: 1 });
    expect(withSpeechSettingsDefaults({ enabled: true, externalUrl: "http://h:1", speedScale: 1.5 })).toEqual({
      ...DEFAULT_SPEECH_SETTINGS,
      enabled: true,
      externalUrl: "http://h:1",
      speedScale: 1.5,
    });
  });

  it("既知の項目だけを拾い、想定外のキー(IPC の応答の包みなど)は混ぜない", () => {
    const got = withSpeechSettingsDefaults({ enabled: true, status: "ok", data: null } as never);
    expect(Object.keys(got).sort()).toEqual(["defaultVoiceId", "enabled", "engine", "externalUrl", "speedScale", "volume"]);
    expect(got.enabled).toBe(true);
    const col = withColumnSpeechDefaults({ enabled: true, extra: 1 } as never);
    expect(Object.keys(col).sort()).toEqual(["enabled", "queueLimit", "speakAuthor", "voiceId"]);
  });

  it("既定は、オフ・外部エンジン・URL なし・話速と音量は 1", () => {
    expect(DEFAULT_SPEECH_SETTINGS).toEqual({
      enabled: false,
      engine: "external",
      externalUrl: "",
      defaultVoiceId: "",
      speedScale: 1,
      volume: 1,
    });
  });
});
