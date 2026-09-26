// メディアビューワーのセーフエリアは未カバー(画像アップロード用のE2Eヘルパーが無いため)。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { addAccountAndEnableMobile, rect, SAFE_AREA } from "../helpers/mobile";

describe("mobile safe-area", () => {
  let bridge: MiauthBridge;

  before(async function () {
    this.timeout(120000);
    bridge = await startMiauthBridge();
    await addAccountAndEnableMobile(bridge, "mobileSafeArea");
  });

  after(async () => {
    await bridge.teardown();
  });

  it("keeps the compose FAB inside the bottom/right safe area", async () => {
    const fab = await rect('button[title="投稿"]');
    expect(fab).not.toBeNull();
    const vh = await browser.execute(() => window.innerHeight);
    const vw = await browser.execute(() => window.innerWidth);
    expect(fab!.bottom).toBeLessThanOrEqual(vh - SAFE_AREA.bottom);
    expect(fab!.right).toBeLessThanOrEqual(vw - SAFE_AREA.right);
  });

  it("keeps the compose modal content below the top safe area", async () => {
    await $('button[title="投稿"]').click();
    const textarea = await $('[data-testid="compose-textarea"]');
    await textarea.waitForDisplayed({ timeout: 15000 });
    const r = await rect('[data-testid="compose-textarea"]');
    expect(r!.top).toBeGreaterThanOrEqual(SAFE_AREA.top);
    await browser.keys("Escape");
  });

  it("keeps the bottom menu bar above the bottom safe area padding", async () => {
    // AppMenu/Backstage の下端パディングが --safe-bottom 分確保されているか。
    const trigger = await rect('[data-testid="app-menu-trigger"]');
    const vh = await browser.execute(() => window.innerHeight);
    expect(trigger!.bottom).toBeLessThanOrEqual(vh - SAFE_AREA.bottom);
  });
});
