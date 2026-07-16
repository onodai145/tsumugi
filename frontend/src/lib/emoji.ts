// リアクション絵文字の解決。キーは :name: / :name@.:（ローカル）/ :name@host:（リモート）。
// ローカルの `@.` はローカル絵文字マップ（キー=name）に合わせて剥がす。
//
// emojiMap（note.emojis）はノート取得時点のスナップショットのため、取得後に付いた
// リアクション（noteUpdated の reacted イベントには emoji の URL が含まれない）は
// map に無いことがある。その場合、Misskey が任意の絵文字を name(@host) から解決できる
// `/emoji/:name(@host).webp` プロキシへ instanceHost 経由でフォールバックする
// （本家クライアントも同じ仕組みでリアクション絵文字を解決している）。
export function reactionEmoji(
  key: string,
  emojiMap: Record<string, string>,
  instanceHost?: string,
): { name: string; url?: string } {
  const raw = key.replace(/^:|:$/g, "");
  const local = raw.endsWith("@.") ? raw.slice(0, -2) : raw;
  const known = emojiMap[raw] ?? emojiMap[local];
  if (known) return { name: local, url: known };
  if (!instanceHost) return { name: local };
  const at = raw.lastIndexOf("@");
  const remoteHost = at === -1 ? undefined : raw.slice(at + 1);
  const proxyName = remoteHost && remoteHost !== "." ? raw : local;
  return { name: local, url: `https://${instanceHost}/emoji/${proxyName}.webp` };
}

// 自インスタンスに存在しないカスタム絵文字のリアクション（:name@host: 形式でホストが
// ローカルでないもの）か判定する。Unicode絵文字・自インスタンスの絵文字（:name:/:name@.:）は false。
// 自インスタンスに無い絵文字は notes/reactions/create がエラーになるため、押せないようにする判定に使う。
export function isRemoteCustomEmoji(key: string): boolean {
  if (!key.startsWith(":")) return false;
  const raw = key.replace(/^:|:$/g, "");
  const at = raw.lastIndexOf("@");
  if (at === -1) return false;
  const host = raw.slice(at + 1);
  return host !== "" && host !== ".";
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
