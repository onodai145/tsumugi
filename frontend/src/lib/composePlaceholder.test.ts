import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pickComposePlaceholder, COMPOSE_PLACEHOLDER_BANDS } from "./composePlaceholder";

function setHour(hour: number, minute = 0) {
  const d = new Date("2026-07-29T00:00:00");
  d.setHours(hour, minute, 0, 0);
  vi.setSystemTime(d);
}

describe("pickComposePlaceholder", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  const cases: { hour: number; band: keyof typeof COMPOSE_PLACEHOLDER_BANDS }[] = [
    { hour: 0, band: "midnight" },
    { hour: 3, band: "midnight" },
    { hour: 4, band: "earlyMorning" },
    { hour: 6, band: "earlyMorning" },
    { hour: 7, band: "morning" },
    { hour: 9, band: "morning" },
    { hour: 10, band: "noon" },
    { hour: 16, band: "noon" },
    { hour: 17, band: "evening" },
    { hour: 18, band: "evening" },
    { hour: 19, band: "night" },
    { hour: 23, band: "night" },
  ];

  for (const { hour, band } of cases) {
    it(`returns a phrase from the "${band}" band at hour ${hour}`, () => {
      setHour(hour);
      const result = pickComposePlaceholder();
      expect(COMPOSE_PLACEHOLDER_BANDS[band]).toContain(result);
    });
  }

  it("uses the passed-in date instead of the system clock", () => {
    setHour(2); // 深夜のはずだが、引数の時刻(朝8時)を優先すべき
    const morning = new Date("2026-07-29T08:00:00");
    const result = pickComposePlaceholder(morning);
    expect(COMPOSE_PLACEHOLDER_BANDS.morning).toContain(result);
  });

  it("has exactly 7 phrases in each band", () => {
    for (const phrases of Object.values(COMPOSE_PLACEHOLDER_BANDS)) {
      expect(phrases).toHaveLength(7);
    }
  });

  it("only ever returns phrases defined in one of the bands", () => {
    const allPhrases = new Set<string>(Object.values(COMPOSE_PLACEHOLDER_BANDS).flat());
    for (let hour = 0; hour < 24; hour++) {
      setHour(hour);
      expect(allPhrases.has(pickComposePlaceholder())).toBe(true);
    }
  });
});
