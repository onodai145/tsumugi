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

// Unicode絵文字を画像化するファイル名部分。本家 packages/frontend-shared/js/emoji-base.ts を移植。
// 画像自体は本家と同じ @misskey-dev/emoji-assets をビルド時に public/twemoji, public/fluent-emoji へ
// 同梱しているため、インスタンスへの通信もCDNも使わずアプリ単体で解決できる。
function char2twemojiFileName(char: string): string {
  let codes = Array.from(char, (x) => x.codePointAt(0)?.toString(16)).filter((x): x is string => !!x);
  if (!codes.includes("200d")) codes = codes.filter((x) => x !== "fe0f");
  return codes.join("-");
}

function isFlagEmoji(char: string): boolean {
  const first = Array.from(char)[0]?.codePointAt(0)?.toString(16);
  return !!first?.startsWith("1f1");
}

function char2fluentEmojiFileName(char: string): string {
  let codes = Array.from(char, (x) => x.codePointAt(0)?.toString(16)).filter((x): x is string => !!x);
  if (!codes.includes("200d")) codes = codes.filter((x) => x !== "fe0f");
  return codes.join("-");
}

export type EmojiStyle = "native" | "twemoji" | "fluentEmoji";

/// Unicode絵文字1文字の画像URL(相対パス、アプリに同梱)を解決する。style="native"なら null（OSフォント任せ）。
export function unicodeEmojiUrl(char: string, style: EmojiStyle): string | null {
  if (style === "native") return null;
  // Fluent Emojiは国旗非対応のため twemoji にフォールバック（本家と同じ制約）
  // https://github.com/microsoft/fluentui-emoji/issues/25
  if (style === "twemoji" || isFlagEmoji(char)) {
    return `/twemoji/${char2twemojiFileName(char)}.svg`;
  }
  return `/fluent-emoji/${char2fluentEmojiFileName(char)}.png`;
}
