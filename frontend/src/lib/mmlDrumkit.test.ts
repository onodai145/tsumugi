import { describe, expect, it } from "vitest";
import { DRUM_MAP, PIECE_SPECS, drumPieceFor, pieceLengthSec, type DrumPiece } from "./mmlDrumkit";
import { TAIL_SEC } from "./mmlRender";

// GM のパーカッションマップ(docs/superpowers/specs/2026-10-30-issue-499-mml-drumkit-design.md の表)
const EXPECTED: [number, DrumPiece][] = [
  [35, "kick"],
  [36, "kick"],
  [37, "sidestick"],
  [38, "snare"],
  [39, "clap"],
  [40, "snare"],
  [41, "tomLowFloor"],
  [42, "hihatClosed"],
  [43, "tomHighFloor"],
  [44, "hihatClosed"],
  [45, "tomLow"],
  [46, "hihatOpen"],
  [47, "tomLowMid"],
  [48, "tomHiMid"],
  [49, "crash"],
  [50, "tomHigh"],
  [51, "ride"],
  [53, "rideBell"],
  [56, "cowbell"],
  [57, "crash"],
  [59, "ride"],
];

describe("DRUM_MAP / drumPieceFor", () => {
  it("GM の番号が、仕様の表の楽器に対応する", () => {
    for (const [midi, piece] of EXPECTED) expect(drumPieceFor(midi), `midi ${midi}`).toBe(piece);
    expect(Object.keys(DRUM_MAP)).toHaveLength(EXPECTED.length);
  });

  it("表にない高さは null(0〜127 のうち、表にある番号以外すべて)", () => {
    const mapped = new Set(EXPECTED.map(([m]) => m));
    for (let midi = 0; midi <= 127; midi++) {
      if (!mapped.has(midi)) expect(drumPieceFor(midi), `midi ${midi}`).toBeNull();
    }
    for (const midi of [34, 52, 54, 55, 58, 60, 81, 82]) expect(drumPieceFor(midi)).toBeNull();
  });

  it("タムの 6 段は、互いに別の楽器で、高さが低い順に並ぶ", () => {
    const toms = [41, 43, 45, 47, 48, 50].map((m) => drumPieceFor(m)!);
    expect(new Set(toms).size).toBe(6);
    const hz = toms.map((p) => {
      const spec = PIECE_SPECS[p];
      if (spec.kind !== "membrane") throw new Error(`${p} は膜の音源ではない`);
      return spec.hz;
    });
    expect([...hz].sort((a, b) => a - b)).toEqual(hz);
  });
});

describe("PIECE_SPECS", () => {
  it("割り当てた全楽器に仕様があり、仕様のある楽器はすべて割り当てられている", () => {
    const used = new Set(Object.values(DRUM_MAP));
    expect(Object.keys(PIECE_SPECS).sort()).toEqual([...used].sort());
  });

  it("固定の長さ(最後の打ち出し + 減衰 + リリース)は TAIL_SEC 以下", () => {
    for (const [piece, spec] of Object.entries(PIECE_SPECS)) {
      expect(pieceLengthSec(spec), piece).toBeGreaterThan(0);
      expect(pieceLengthSec(spec), piece).toBeLessThanOrEqual(TAIL_SEC);
    }
  });

  it("trimDb は有限で、極端でない", () => {
    for (const [piece, spec] of Object.entries(PIECE_SPECS)) {
      expect(Number.isFinite(spec.trimDb), piece).toBe(true);
      expect(Math.abs(spec.trimDb), piece).toBeLessThanOrEqual(24);
    }
  });
});
