// MML の入力補助キー(Issue #500)。ソフトキーボードで切り替えが要る記号と、音符・長さをまとめて押せるようにする。
// insert はカーソル位置にそのまま入る文字。構文は検証しない(検証は Rust の parse_mml)。

export interface MmlKey {
  id: string;
  /// キーに出す文字
  label: string;
  /// 入る文字
  insert: string;
  /// 読み上げ名
  aria: string;
}

export interface MmlKeyGroup {
  id: string;
  /// グループの読み上げ名
  label: string;
  keys: readonly MmlKey[];
}

export const MML_KEY_GROUPS: readonly MmlKeyGroup[] = [
  {
    id: "notes",
    label: "音符",
    keys: [
      { id: "c", label: "c", insert: "c", aria: "ド c" },
      { id: "d", label: "d", insert: "d", aria: "レ d" },
      { id: "e", label: "e", insert: "e", aria: "ミ e" },
      { id: "f", label: "f", insert: "f", aria: "ファ f" },
      { id: "g", label: "g", insert: "g", aria: "ソ g" },
      { id: "a", label: "a", insert: "a", aria: "ラ a" },
      { id: "b", label: "b", insert: "b", aria: "シ b" },
      { id: "r", label: "r", insert: "r", aria: "休符 r" },
    ],
  },
  {
    id: "modifiers",
    label: "半音・付点・タイ",
    keys: [
      { id: "sharp", label: "#", insert: "#", aria: "半音上げ #" },
      { id: "flat", label: "-", insert: "-", aria: "半音下げ -" },
      { id: "dot", label: ".", insert: ".", aria: "付点 ." },
      { id: "tie", label: "&", insert: "&", aria: "タイ &" },
      { id: "tie-len", label: "^", insert: "^", aria: "長さを足すタイ ^" },
    ],
  },
  {
    id: "octave",
    label: "オクターブ",
    keys: [
      { id: "oct-down", label: "<", insert: "<", aria: "オクターブを下げる <" },
      { id: "oct-up", label: ">", insert: ">", aria: "オクターブを上げる >" },
    ],
  },
  {
    id: "lengths",
    label: "長さ",
    keys: ["1", "2", "4", "8", "16", "32"].map((n) => ({ id: `len-${n}`, label: n, insert: n, aria: `長さ ${n}` })),
  },
  {
    id: "structure",
    label: "繰り返し・連符・トラック",
    keys: [
      { id: "loop-open", label: "[", insert: "[", aria: "繰り返しの始め [" },
      { id: "loop-close", label: "]", insert: "]", aria: "繰り返しの終わり ]" },
      { id: "loop-escape", label: ":", insert: ":", aria: "繰り返しの脱出 :" },
      { id: "tuplet-open", label: "{", insert: "{", aria: "連符の始め {" },
      { id: "tuplet-close", label: "}", insert: "}", aria: "連符の終わり }" },
      { id: "chord", label: "'", insert: "'", aria: "和音の始め・終わり '" },
      { id: "track", label: ";", insert: ";", aria: "トラック区切り ;" },
    ],
  },
];
