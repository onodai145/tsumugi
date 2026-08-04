// リアクションピッカーの使用履歴（Issue #108）。キー形式は pinnedEmojis と同じ
// （Unicode絵文字はそのまま、カスタム絵文字は ":name@host:" 形式）。
export const RECENT_EMOJIS_MAX = 16;

// 使用のたびに呼ぶ。既存の同一キーを除去してから先頭に追加し、最大件数に切り詰める
// (タイムスタンプは持たず、配列の並び順で最新度を表す)。
export function withRecentEmojiUsage(list: string[], key: string): string[] {
  return [key, ...list.filter((k) => k !== key)].slice(0, RECENT_EMOJIS_MAX);
}
