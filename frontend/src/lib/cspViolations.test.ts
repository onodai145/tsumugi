import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CSP_VIOLATIONS_LIMIT, installCspViolationRecorder, type CspViolation } from "./cspViolations";

type Target = EventTarget & { __cspViolations?: CspViolation[] };

function violate(target: Target, blockedURI = "https://cdn.example/x.js") {
  const e = new Event("securitypolicyviolation");
  Object.assign(e, {
    effectiveDirective: "script-src-elem",
    blockedURI,
    sourceFile: "tauri://localhost/assets/a.js",
    lineNumber: 3,
  });
  target.dispatchEvent(e);
}

describe("installCspViolationRecorder", () => {
  beforeEach(() => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("設置した時点で空の配列を用意する", () => {
    const target: Target = new EventTarget();
    installCspViolationRecorder(target);
    expect(target.__cspViolations).toEqual([]);
  });

  it("違反を配列に積み、1行の警告を出す", () => {
    const target: Target = new EventTarget();
    installCspViolationRecorder(target);
    violate(target);
    expect(target.__cspViolations).toEqual([
      {
        directive: "script-src-elem",
        blockedUri: "https://cdn.example/x.js",
        source: "tauri://localhost/assets/a.js",
        line: 3,
      },
    ]);
    expect(console.warn).toHaveBeenCalledWith(
      "[csp] blocked script-src-elem https://cdn.example/x.js (tauri://localhost/assets/a.js:3)",
    );
  });

  it("上限を超えた分は積まない", () => {
    const target: Target = new EventTarget();
    installCspViolationRecorder(target);
    for (let i = 0; i < CSP_VIOLATIONS_LIMIT + 10; i++) violate(target, `https://e.example/${i}`);
    expect(target.__cspViolations).toHaveLength(CSP_VIOLATIONS_LIMIT);
    // 上限分の警告と、捨て始めたことの警告 1 回
    expect(console.warn).toHaveBeenCalledTimes(CSP_VIOLATIONS_LIMIT + 1);
    expect(console.warn).toHaveBeenLastCalledWith(
      `[csp] ${CSP_VIOLATIONS_LIMIT} violations recorded; further ones are dropped`,
    );
  });

  it("記録を空にした後は、また積み、次に溢れたときにもう一度知らせる", () => {
    const target: Target = new EventTarget();
    installCspViolationRecorder(target);
    for (let i = 0; i <= CSP_VIOLATIONS_LIMIT; i++) violate(target);
    target.__cspViolations!.length = 0;
    vi.mocked(console.warn).mockClear();
    for (let i = 0; i <= CSP_VIOLATIONS_LIMIT; i++) violate(target);
    expect(target.__cspViolations).toHaveLength(CSP_VIOLATIONS_LIMIT);
    expect(console.warn).toHaveBeenCalledTimes(CSP_VIOLATIONS_LIMIT + 1);
  });
});
