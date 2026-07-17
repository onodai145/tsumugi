// ピン留め絵文字/リアクションのキー形式共通ヘルパ。Unicode絵文字はそのまま、
// カスタム絵文字は ":name:" 形式で表す(リアクション送信時の規約)。
export function isCustomEmojiKey(key: string): boolean {
  return key.startsWith(":") && key.endsWith(":") && key.length > 2;
}

export function customEmojiKey(name: string): string {
  return `:${name}:`;
}

// UiPrefs.pinnedEmojis に保存するカスタム絵文字キーはインスタンス(host)を含める
// ":name@host:" 形式。ピン留めは全アカウント共通のグローバル設定のため、host無しだと
// 複数インスタンスのアカウントを使っている場合に同名の別絵文字と衝突し得る
// (lib/emoji.ts の :name@host: リモート絵文字表記と同じ規約を流用)。
export function customEmojiPinKey(name: string, host: string): string {
  return `:${name}@${host}:`;
}

// ピン留めキーから { name, host } を取り出す。host無し(旧形式/バグ由来)は host: null。
export function parseCustomEmojiPinKey(key: string): { name: string; host: string | null } {
  const raw = key.slice(1, -1);
  const at = raw.lastIndexOf("@");
  if (at === -1) return { name: raw, host: null };
  return { name: raw.slice(0, at), host: raw.slice(at + 1) };
}
