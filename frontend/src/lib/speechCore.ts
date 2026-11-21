// 内蔵 VOICEVOX CORE(Issue #625 段階2)の、表示用の純粋な関数。状態は Rust が持ち、ここは文面とクレジットの引き当てだけ。
import type { SpeechCoreInfo, Voice, VoicePackInfo } from "../bindings/tauri.gen";

/// 読み上げの状態のエラー文(識別コードを含む)を、利用者向けの日本語にする。知らない文面はそのまま返す。
const ERROR_TEXT: [string, string][] = [
  ["SPEECH_CORE_NOT_READY", "内蔵エンジンまたは選んだ声が未取得です。設定 → 読み上げ で取得してください"],
  ["SPEECH_CORE_LOAD_FAILED", "内蔵エンジンを起動できません。エンジン一式を取得し直してください"],
  ["SPEECH_CORE_UNSUPPORTED", "この環境では内蔵エンジンを使えません"],
  ["SPEECH_NO_VOICE", "声が選ばれていません"],
];

export function speechErrorText(message: string): string {
  for (const [code, text] of ERROR_TEXT) if (message.includes(code)) return text;
  return message;
}

/// 声の ID(スタイル ID)から、そのキャラのクレジット表記を引く。未取得の声でも引ける。見つからなければ null。
export function creditFor(info: SpeechCoreInfo | null | undefined, voiceId: string): string | null {
  if (!info || !voiceId) return null;
  for (const v of info.voices) for (const c of v.characters) if (c.styles.some((s) => s.id === voiceId)) return c.credit;
  return null;
}

/// 取得済みの VVM の声(設定画面の選択肢)。Rust の `CoreEngine::voices()` と同じ並び・同じ表示名。
export function installedVoices(info: SpeechCoreInfo | null | undefined): Voice[] {
  if (!info) return [];
  const out: Voice[] = [];
  for (const v of info.voices) {
    if (v.pack.status.type !== "ready") continue;
    for (const c of v.characters) for (const s of c.styles) out.push({ id: s.id, label: `${c.name}（${s.name}）` });
  }
  return out;
}

/// 1 つの VVM に入っているキャラ名(重複なし)。
export function voiceCharactersLabel(v: VoicePackInfo): string {
  return [...new Set(v.characters.map((c) => c.name))].join("、");
}
