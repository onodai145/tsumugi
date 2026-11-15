import { describe, expect, it } from "vitest";
import { downloadWarning, formatBytes, progressPercent, statusText, UNKNOWN_NETWORK } from "./localModel";
import type { NetworkStatus } from "./ipc";

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

const net = (transport: NetworkStatus["transport"], metered: NetworkStatus["metered"]): NetworkStatus => ({ transport, metered });
const SIZE = 573_126_848;
const UNSURE = "回線の種類を判定できませんでした。約 573.1 MB を取得します。モバイル通信の場合はご注意ください。続けますか?";

describe("downloadWarning", () => {
  it("モバイル通信なら、従量制の値に関わらずモバイル通信の警告", () => {
    for (const m of ["metered", "unmetered", "unknown"] as const) {
      expect(downloadWarning(net("cellular", m), SIZE)).toBe("モバイル通信を使用中です。約 573.1 MB を取得します。続けますか?");
    }
  });
  it("モバイル通信以外の従量制は、従量制の警告（VPN などで種類が other のときも）", () => {
    expect(downloadWarning(net("wifi", "metered"), SIZE)).toBe("従量制の接続です。約 573.1 MB を取得します。続けますか?");
    expect(downloadWarning(net("other", "metered"), SIZE)).toBe("従量制の接続です。約 573.1 MB を取得します。続けますか?");
  });
  it("非従量制なら、種類に関わらず警告なし", () => {
    for (const t of ["wifi", "ethernet", "other", "unknown"] as const) expect(downloadWarning(net(t, "unmetered"), SIZE)).toBeNull();
  });
  it("従量制が不明でも、Wi-Fi・Ethernet だと分かっていれば警告なし", () => {
    expect(downloadWarning(net("wifi", "unknown"), SIZE)).toBeNull();
    expect(downloadWarning(net("ethernet", "unknown"), SIZE)).toBeNull();
  });
  it("従量制も種類も当てにならない（other / unknown）なら、安全側に倒して注記つきの確認", () => {
    expect(downloadWarning(net("other", "unknown"), SIZE)).toBe(UNSURE);
    expect(downloadWarning(net("unknown", "unknown"), SIZE)).toBe(UNSURE);
    expect(downloadWarning(UNKNOWN_NETWORK, SIZE)).toBe(UNSURE);
  });
});
