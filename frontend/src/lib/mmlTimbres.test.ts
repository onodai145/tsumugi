import { describe, expect, it } from "vitest";
import type { Timbre } from "../bindings/tauri.gen";
import { TAIL_SEC } from "./mmlRender";
import { TIMBRES, TIMBRE_LIST } from "./mmlTimbres";

// Rust 側(domain/mml.rs の Timbre::ALL と mml.rs の TIMBRE_TABLE)と同じ並び。変えるときは両方を直す。
const EXPECTED: [number, Timbre][] = [
  [0, "fm"],
  [1, "square"],
  [2, "pulse"],
  [3, "triangle"],
  [4, "saw"],
  [5, "sine"],
  [6, "organ"],
  [7, "epiano"],
  [8, "bell"],
  [9, "am"],
  [10, "lead"],
  [11, "bass"],
  [12, "drum"],
  [13, "noise"],
];

describe("TIMBRES", () => {
  it("番号と名前が Rust 側の表と一致する", () => {
    expect(TIMBRE_LIST.map((p) => [p.number, p.name])).toEqual(EXPECTED);
  });

  it("キーと name が一致し、全 Timbre にプリセットがある", () => {
    for (const [, name] of EXPECTED) expect(TIMBRES[name].name).toBe(name);
    expect(Object.keys(TIMBRES).sort()).toEqual(EXPECTED.map(([, n]) => n).sort());
  });

  it("全プリセットのリリースは TAIL_SEC 以下で、gainDb は有限の妥当な範囲", () => {
    for (const p of TIMBRE_LIST) {
      expect(p.releaseSec, p.name).toBeGreaterThan(0);
      expect(p.releaseSec, p.name).toBeLessThanOrEqual(TAIL_SEC);
      expect(Number.isFinite(p.gainDb), p.name).toBe(true);
      expect(p.gainDb, p.name).toBeGreaterThanOrEqual(-30);
      expect(p.gainDb, p.name).toBeLessThanOrEqual(6);
    }
  });

  it("fm は現行の音(補正 0 dB)のまま", () => {
    expect(TIMBRES.fm.gainDb).toBe(0);
    expect(TIMBRES.fm.releaseSec).toBe(0.1);
  });

  it("単音用の音源だけが perNote である", () => {
    expect(TIMBRE_LIST.filter((p) => p.perNote).map((p) => p.name)).toEqual(["noise"]);
  });
});
