import { describe, expect, it } from "vitest";
import { shouldInterceptPaste } from "./pasteIntent";

describe("shouldInterceptPaste", () => {
  it("text/plain のみ(types が空でも getData で取れる WebKitGTK のケース)は横取りしない", () => {
    expect(shouldInterceptPaste([], "hello")).toBe(false);
  });

  it("types に text/plain/text/html があり本文もあるなら横取りしない", () => {
    expect(shouldInterceptPaste(["text/plain", "text/html"], "hi")).toBe(false);
  });

  it("本文が空(スクリーンショット画像など)なら横取りする", () => {
    expect(shouldInterceptPaste([], "")).toBe(true);
  });

  it("text/uri-list があれば本文の有無に関わらず横取りする(WebKitGTK は text/plain を隠す)", () => {
    expect(shouldInterceptPaste(["text/uri-list"], "")).toBe(true);
    // text/plain も見える WebView(WebView2 / WKWebView)でもファイル参照を優先する
    expect(shouldInterceptPaste(["text/uri-list", "text/plain"], "a.mp4")).toBe(true);
  });

  it("Files があれば本文の有無に関わらず横取りする", () => {
    expect(shouldInterceptPaste(["Files"], "")).toBe(true);
    expect(shouldInterceptPaste(["Files", "text/plain"], "a.png")).toBe(true);
  });
});
