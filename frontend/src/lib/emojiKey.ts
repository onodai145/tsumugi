// ピン留め絵文字/リアクションのキー形式共通ヘルパ。Unicode絵文字はそのまま、
// カスタム絵文字は ":name:" 形式で表す(UiPrefs.pinnedEmojis, リアクション送信と同じ規約)。
export function isCustomEmojiKey(key: string): boolean {
  return key.startsWith(":") && key.endsWith(":") && key.length > 2;
}

export function customEmojiNameFromKey(key: string): string {
  return key.slice(1, -1);
}

export function customEmojiKey(name: string): string {
  return `:${name}:`;
}
