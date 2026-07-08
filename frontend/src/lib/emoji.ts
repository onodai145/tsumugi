// リアクション絵文字の解決。キーは :name: / :name@.:（ローカル）/ :name@host:（リモート）。
// ローカルの `@.` はローカル絵文字マップ（キー=name）に合わせて剥がす。
export function reactionEmoji(
  key: string,
  emojiMap: Record<string, string>,
): { name: string; url?: string } {
  const raw = key.replace(/^:|:$/g, "");
  const local = raw.endsWith("@.") ? raw.slice(0, -2) : raw;
  return { name: local, url: emojiMap[raw] ?? emojiMap[local] };
}
