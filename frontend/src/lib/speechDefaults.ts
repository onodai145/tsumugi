// 読み上げ(Issue #625)の設定の既定値と、欠けた項目の補完。
// Rust 側の `#[serde(default)]` のフィールドは、生成される TS の型では省略可能(`enabled?: boolean`)になる。
// 古い設定ファイル・IPC のモックなどで項目が欠けていても、フロントでは既定値で補ってから使う。
import type { ColumnSpeech, SpeechSettings } from "../bindings/tauri.gen";

/// カラムの読み上げ設定(全項目あり)。Rust の `ColumnSpeech::default()` と揃える。
export type ColumnSpeechValue = Required<ColumnSpeech>;
/// 全体の読み上げ設定(全項目あり)。
export type SpeechSettingsValue = Required<Omit<SpeechSettings, "speedScale" | "volume">> & {
  speedScale: number;
  volume: number;
};

export const DEFAULT_COLUMN_SPEECH: ColumnSpeechValue = {
  enabled: false,
  speakAuthor: true,
  queueLimit: 5,
  voiceId: "",
};

export const DEFAULT_SPEECH_SETTINGS: SpeechSettingsValue = {
  enabled: false,
  engine: "external",
  externalUrl: "",
  defaultVoiceId: "",
  speedScale: 1,
  volume: 1,
};

/// 欠けた項目を既定値で補う。常に新しいオブジェクトを返す(既定値を共有しない)。
/// 既知の項目だけを拾い、想定外のキーは混ぜない。
export function withColumnSpeechDefaults(s: ColumnSpeech | null | undefined): ColumnSpeechValue {
  return {
    enabled: s?.enabled ?? DEFAULT_COLUMN_SPEECH.enabled,
    speakAuthor: s?.speakAuthor ?? DEFAULT_COLUMN_SPEECH.speakAuthor,
    queueLimit: s?.queueLimit ?? DEFAULT_COLUMN_SPEECH.queueLimit,
    voiceId: s?.voiceId ?? DEFAULT_COLUMN_SPEECH.voiceId,
  };
}

/// 欠けた項目を既定値で補う。話速・音量の `null` も既定値に戻す。既知の項目だけを拾う。
export function withSpeechSettingsDefaults(s: SpeechSettings | null | undefined): SpeechSettingsValue {
  return {
    enabled: s?.enabled ?? DEFAULT_SPEECH_SETTINGS.enabled,
    engine: s?.engine ?? DEFAULT_SPEECH_SETTINGS.engine,
    externalUrl: s?.externalUrl ?? DEFAULT_SPEECH_SETTINGS.externalUrl,
    defaultVoiceId: s?.defaultVoiceId ?? DEFAULT_SPEECH_SETTINGS.defaultVoiceId,
    speedScale: s?.speedScale ?? DEFAULT_SPEECH_SETTINGS.speedScale,
    volume: s?.volume ?? DEFAULT_SPEECH_SETTINGS.volume,
  };
}
