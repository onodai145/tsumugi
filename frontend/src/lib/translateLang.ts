// ノート翻訳（Issue #440）の翻訳先言語。コードはサーバーの notes/translate の targetLang へ
// そのまま渡す（変換しない）。地域付きコード(EN-US 等)が必要な言語は、実インスタンスで
// 通ることを確認してからここへ足す。
export const DEFAULT_TRANSLATE_LANG = "ja";

export const TRANSLATE_LANG_PRESETS: { code: string; label: string }[] = [
  { code: "ja", label: "日本語" },
  { code: "en", label: "English" },
  { code: "zh", label: "中文" },
  { code: "ko", label: "한국어" },
  { code: "fr", label: "Français" },
  { code: "de", label: "Deutsch" },
  { code: "es", label: "Español" },
  { code: "pt", label: "Português" },
  { code: "ru", label: "Русский" },
];

/// 保存値を整える。空・空白のみ・未設定は既定（ja）へ戻す。
export function normalizeTranslateLang(v: string | null | undefined): string {
  const t = (v ?? "").trim();
  return t === "" ? DEFAULT_TRANSLATE_LANG : t;
}
