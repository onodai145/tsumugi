// ピン留め絵文字/リアクションのキー形式共通ヘルパ。Unicode絵文字はそのまま、
// 自インスタンスのカスタム絵文字は ":name@.:" 形式で表す。Misskey本家の
// ReactionService#decodeReaction が note.reactions / myReaction で返す正規形と揃えないと、
// 自分のリアクションキーだけ別表記になり重複バッジや取り消し判定ズレを起こす(Issue #152)。
export function isCustomEmojiKey(key: string): boolean {
  return key.startsWith(":") && key.endsWith(":") && key.length > 2;
}

export function customEmojiKey(name: string): string {
  return `:${name}@.:`;
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

// ReactionPicker.onpick が返すリアクションキー形式(Unicode文字 or ":name@host:")を、
// 投稿本文へ挿入するMFMショートコード形式に変換する。カスタム絵文字はhost部分を捨てて
// ":name:" にする(投稿本文中のショートコードにhostは含めない)。Unicodeはそのまま返す。
export function emojiKeyToInsertText(key: string): string {
  if (!isCustomEmojiKey(key)) return key;
  const { name } = parseCustomEmojiPinKey(key);
  return `:${name}:`;
}
