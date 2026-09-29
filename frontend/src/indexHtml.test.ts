import { describe, expect, it } from "vitest";
import indexHtml from "../index.html?raw";

describe("index.html", () => {
  // Issue #411: WebView は外部画像に `Referer: <アプリのOrigin>` を付けるが、Misskey.io 等
  // Cloudflare 配下のサーバーはそれを 403 で弾き、Instance Ticker のアイコンやアバターが
  // 表示されなくなる。このアプリのAPI通信はRust側でRefererに依存しないため、全体の既定を
  // no-referrer にする。
  it("sets the document-wide referrer policy to no-referrer so external images are not blocked by hotlink rules", () => {
    const doc = new DOMParser().parseFromString(indexHtml, "text/html");
    const meta = doc.querySelector('meta[name="referrer"]');
    expect(meta?.getAttribute("content")).toBe("no-referrer");
  });
});
