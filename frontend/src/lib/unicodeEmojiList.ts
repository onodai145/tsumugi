// Unicode絵文字ピッカー用データ(Issue #21: 絵文字ピッカーにすべての絵文字を表示する)。
// 本家 Misskey と同じ @misskey-dev/emoji-data(emoji-assets の姉妹パッケージ、CDN不要でバンドル)から
// [char, name, categoryIndex][] を読み込む。name はいずれも英語スラッグ(本家と同じデータソースのため)。
import rawList from "@misskey-dev/emoji-data/emojilist.json";

export interface UnicodeEmojiEntry {
  char: string;
  name: string;
  category: number;
}

export const UNICODE_EMOJIS: UnicodeEmojiEntry[] = rawList.map(([char, name, category]) => ({
  char,
  name,
  category,
}));

// カテゴリ順は @misskey-dev/emoji-data の categoryIndex に対応(本家 emojilist.ts 準拠)。
export const UNICODE_EMOJI_CATEGORIES: { index: number; label: string }[] = [
  { index: 0, label: "Face" },
  { index: 1, label: "People" },
  { index: 2, label: "Animals & Nature" },
  { index: 3, label: "Food & Drink" },
  { index: 4, label: "Activity" },
  { index: 5, label: "Travel & Places" },
  { index: 6, label: "Objects" },
  { index: 7, label: "Symbols" },
  { index: 8, label: "Flags" },
];

// ピン留め絵文字の既定値(追加前にハードコードされていた8種、src-tauri/src/domain/ui.rs と一致させる)。
export const DEFAULT_PINNED_EMOJIS: string[] = ["👍", "❤️", "😆", "🎉", "🤔", "😢", "😮", "🙏"];
