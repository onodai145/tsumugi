import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { addAccountAndEnableMobile } from "../helpers/mobile";

// html/body/ルートdivが overflow:hidden のため documentElement.scrollWidth は常に0になり空振りする。
// 代わりに、カラム列(意図した横スクロール=scroll-snap, [data-columns-scroll])の外にある可視要素
// (モーダルは overflow-y:auto で overflow-x も auto 扱いになるため、overflowによる除外はせず右端座標で判定する)の右端がviewportを超えないことを検証する。
const findOverflowing = () =>
  browser.execute(() => {
    const vw = window.innerWidth;
    const out: string[] = [];
    for (const el of Array.from(document.body.querySelectorAll("*"))) {
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) continue;
      const cs = getComputedStyle(el);
      if (cs.visibility === "hidden" || cs.display === "none") continue;
      if (el.closest("[data-columns-scroll]")) continue;
      if (r.right > vw + 1) {
        out.push(`${el.tagName.toLowerCase()}.${String(el.getAttribute("class") ?? "").slice(0, 60)} right=${Math.round(r.right)} vw=${vw}`);
      }
    }
    return out.slice(0, 10);
  });

describe("mobile horizontal overflow", () => {
  let bridge: MiauthBridge;

  before(async function () {
    this.timeout(120000);
    bridge = await startMiauthBridge();
    await addAccountAndEnableMobile(bridge, "mobileOverflow");
  });

  after(async () => {
    await bridge.teardown();
  });

  it("has no page-level horizontal overflow", async () => {
    expect(await findOverflowing()).toEqual([]);
  });

  it("has no horizontal overflow while the compose modal is open", async () => {
    await $('button[title="投稿"]').click();
    const textarea = await $('[data-testid="compose-textarea"]');
    await textarea.waitForDisplayed({ timeout: 15000 });
    try {
      expect(await findOverflowing()).toEqual([]);
    } finally {
      await browser.keys("Escape");
      await textarea.waitForDisplayed({ reverse: true, timeout: 15000 });
    }
  });
});
