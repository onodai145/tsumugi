import { describe, expect, it } from "vitest";
import { formatFileSize } from "./fileSize";

describe("formatFileSize", () => {
  it("未指定・不正値は空文字", () => {
    expect(formatFileSize(undefined)).toBe("");
    expect(formatFileSize(null)).toBe("");
    expect(formatFileSize(-1)).toBe("");
    expect(formatFileSize(NaN)).toBe("");
  });
  it("1024未満はB", () => {
    expect(formatFileSize(0)).toBe("0 B");
    expect(formatFileSize(1023)).toBe("1023 B");
  });
  it("1024進でKB/MB/GB", () => {
    expect(formatFileSize(1024)).toBe("1.0 KB");
    expect(formatFileSize(1536)).toBe("1.5 KB");
    expect(formatFileSize(1024 * 1024 * 1.2)).toBe("1.2 MB");
    expect(formatFileSize(1024 ** 3 * 2)).toBe("2.0 GB");
  });
});
