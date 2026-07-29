import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { relativeTime } from "./time";

describe("relativeTime", () => {
  const NOW = new Date("2026-07-29T12:00:00Z").getTime();

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns an empty string for epochSec 0", () => {
    expect(relativeTime(0)).toBe("");
  });

  it("returns seconds for under a minute", () => {
    const epochSec = NOW / 1000 - 30;
    expect(relativeTime(epochSec)).toBe("30s");
  });

  it("returns minutes for under an hour", () => {
    const epochSec = NOW / 1000 - 5 * 60;
    expect(relativeTime(epochSec)).toBe("5m");
  });

  it("returns hours for under a day", () => {
    const epochSec = NOW / 1000 - 3 * 3600;
    expect(relativeTime(epochSec)).toBe("3h");
  });

  it("returns days for under a week", () => {
    const epochSec = NOW / 1000 - 2 * 86400;
    expect(relativeTime(epochSec)).toBe("2d");
  });

  it("returns a localized date for a week or more", () => {
    const epochSec = NOW / 1000 - 8 * 86400;
    expect(relativeTime(epochSec)).toBe(new Date(epochSec * 1000).toLocaleDateString());
  });
});
