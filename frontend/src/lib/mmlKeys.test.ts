import { describe, expect, it } from "vitest";
import { MML_KEY_GROUPS } from "./mmlKeys";
import { lexMml } from "./mmlLexer";

const allKeys = MML_KEY_GROUPS.flatMap((g) => g.keys);
const byId = (id: string) => {
  const key = allKeys.find((k) => k.id === id);
  if (!key) throw new Error(`キー ${id} が無い`);
  return key;
};

describe("MML_KEY_GROUPS(入力補助キー)", () => {
  it("グループは 音符・修飾・オクターブ・長さ・構造 の順", () => {
    expect(MML_KEY_GROUPS.map((g) => g.id)).toEqual(["notes", "modifiers", "octave", "lengths", "structure"]);
  });

  it("音符は c d e f g a b と休符 r", () => {
    const notes = MML_KEY_GROUPS.find((g) => g.id === "notes")!;
    expect(notes.keys.map((k) => k.insert)).toEqual(["c", "d", "e", "f", "g", "a", "b", "r"]);
  });

  it("修飾は # - . &、オクターブは < >、構造は [ ] ;", () => {
    const inserts = (id: string) => MML_KEY_GROUPS.find((g) => g.id === id)!.keys.map((k) => k.insert);
    expect(inserts("modifiers")).toEqual(["#", "-", ".", "&"]);
    expect(inserts("octave")).toEqual(["<", ">"]);
    expect(inserts("structure")).toEqual(["[", "]", ";"]);
  });

  it("長さは 1 2 4 8 16 32", () => {
    const lengths = MML_KEY_GROUPS.find((g) => g.id === "lengths")!;
    expect(lengths.keys.map((k) => k.insert)).toEqual(["1", "2", "4", "8", "16", "32"]);
  });

  it("id は重複せず、挿入文字は空で空白を含まず、読み上げ名がある", () => {
    const ids = allKeys.map((k) => k.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const k of allKeys) {
      expect(k.insert).not.toBe("");
      expect(k.insert).not.toMatch(/\s/);
      expect(k.label).not.toBe("");
      expect(k.aria).not.toBe("");
    }
  });

  it("キーだけで打った列(c# 8. & c 16 > [ d ] 2 ; r 4 <)に、読めない文字が残らない", () => {
    const text = ["c", "sharp", "len-8", "dot", "tie", "c", "len-16", "oct-up", "loop-open", "d", "loop-close", "len-2", "track", "r", "len-4", "oct-down"]
      .map((id) => byId(id).insert)
      .join("");
    expect(lexMml(text).filter((t) => t.kind === "unknown")).toEqual([]);
  });
});
