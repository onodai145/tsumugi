import { describe, expect, it } from "vitest";
import { formatBytes, progressPercent, statusText } from "./localModel";

describe("formatBytes", () => {
  it("MB 単位で小数1桁（1MB = 1,000,000 バイト）", () => {
    expect(formatBytes(573_126_848)).toBe("573.1 MB");
    expect(formatBytes(0)).toBe("0.0 MB");
  });
});

describe("progressPercent", () => {
  it("0〜100 に収め、total が 0 なら 0", () => {
    expect(progressPercent(0, 100)).toBe(0);
    expect(progressPercent(50, 100)).toBe(50);
    expect(progressPercent(100, 100)).toBe(100);
    expect(progressPercent(150, 100)).toBe(100);
    expect(progressPercent(1, 0)).toBe(0);
  });
});

describe("statusText", () => {
  it("状態ごとの文言", () => {
    expect(statusText({ type: "notInstalled" })).toBe("未取得");
    expect(statusText({ type: "ready" })).toBe("取得済み");
    expect(statusText({ type: "verifying" })).toBe("検証中…");
    expect(statusText({ type: "downloading", downloaded: 286_563_424, total: 573_126_848 })).toBe(
      "ダウンロード中 286.6 / 573.1 MB（50%）",
    );
    expect(statusText({ type: "failed", reason: "通信が途切れました" })).toBe("失敗: 通信が途切れました");
  });
});
