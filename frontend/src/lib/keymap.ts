// キーバインド定義。将来のカスタマイズに備え action ↔ キー(chord) を分離して持つ。
export type KeyAction =
  | "note.next"
  | "note.prev"
  | "note.reply"
  | "note.renote"
  | "note.quote"
  | "note.react"
  | "note.open"
  | "column.prev"
  | "column.next"
  | "compose.new";

/// アクションの表示ラベルと既定キー。設定画面のキー操作一覧もこれを使う。
export const ACTIONS: { action: KeyAction; label: string; default: string }[] = [
  { action: "note.next", label: "次のノートを選択", default: "j" },
  { action: "note.prev", label: "前のノートを選択", default: "k" },
  { action: "note.reply", label: "選択ノートに返信", default: "r" },
  { action: "note.renote", label: "選択ノートを Renote", default: "t" },
  { action: "note.quote", label: "選択ノートを引用", default: "q" },
  { action: "note.react", label: "選択ノートにリアクション", default: "e" },
  { action: "note.open", label: "選択ノートをブラウザで開く", default: "o" },
  { action: "column.prev", label: "左のカラムへフォーカス", default: "h" },
  { action: "column.next", label: "右のカラムへフォーカス", default: "l" },
  { action: "compose.new", label: "新規投稿", default: "n" },
];

/// KeyboardEvent を正規化した chord 文字列へ（例 "j" / "shift+r" / "ctrl+enter"）。
export function eventToChord(e: KeyboardEvent): string {
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("ctrl");
  if (e.metaKey) parts.push("meta");
  if (e.altKey) parts.push("alt");
  if (e.shiftKey) parts.push("shift");
  let key = e.key;
  if (key === " ") key = "space";
  if (key.length === 1) key = key.toLowerCase();
  parts.push(key);
  return parts.join("+");
}

/// 既定キーマップ（chord → action）。
export function defaultKeymap(): Map<string, KeyAction> {
  const m = new Map<string, KeyAction>();
  for (const a of ACTIONS) m.set(a.default, a.action);
  return m;
}
