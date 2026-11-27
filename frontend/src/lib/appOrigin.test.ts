import { describe, expect, it } from "vitest";
import { isFramed, isSafeUrl } from "./appOrigin";

describe("isSafeUrl", () => {
  it.each(["https://example.com/embed", "http://example.com:8080/x", "https://localhost.example.com/", "https://notlocalhost/"])(
    "accepts the third-party URL %s",
    (url) => expect(isSafeUrl(url)).toBe(true),
  );

  it.each(["javascript:alert(1)", "data:text/html,x", "tauri://localhost/", "/relative", "https://", ""])(
    "rejects %j, which is not an absolute http(s) URL",
    (url) => expect(isSafeUrl(url)).toBe(false),
  );

  // Issue #613: Windows / Android ではアプリ本体が http://tauri.localhost で配信される。
  it.each([
    "http://tauri.localhost/",
    "https://tauri.localhost/index.html",
    "http://TAURI.LOCALHOST/",
    "http://tauri.localhost./",
    "http://%74auri.localhost/",
    "http://user@tauri.localhost/",
    "http://asset.localhost/x",
    "http://ipc.localhost/x",
    "http://localhost:3000/",
  ])("rejects %s, a localhost name used by the app itself", (url) => expect(isSafeUrl(url)).toBe(false));

  // dev(http://127.0.0.1:5173)のように、アプリのオリジンが localhost 名でない場合。
  it("rejects the app's own origin", () => {
    const own = "http://127.0.0.1:5173";
    expect(isSafeUrl(`${own}/index.html`, own)).toBe(false);
    expect(isSafeUrl("http://127.0.0.1:8080/", own)).toBe(true);
  });
});

describe("isFramed", () => {
  it("is false for a top-level window", () => {
    expect(isFramed(window)).toBe(false);
  });

  it("is true inside an iframe", () => {
    const frame = document.createElement("iframe");
    document.body.appendChild(frame);
    try {
      expect(isFramed(frame.contentWindow!)).toBe(true);
    } finally {
      frame.remove();
    }
  });
});
