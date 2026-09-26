import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { addAccountAndEnableMobile, rect, MOBILE_VIEWPORT } from "../helpers/mobile";

describe("mobile layout", () => {
  let bridge: MiauthBridge;

  before(async function () {
    this.timeout(120000);
    bridge = await startMiauthBridge();
    await addAccountAndEnableMobile(bridge, "mobileLayout");
  });

  after(async () => {
    await bridge.teardown();
  });

  it("shows the compose FAB and hides the always-on compose bar", async () => {
    const fab = await $('button[title="投稿"]');
    await fab.waitForDisplayed({ timeout: 15000 });
    expect(await $('[data-testid="compose-textarea"]').isDisplayed()).toBe(false);
  });

  it("opens compose as a modal when the FAB is tapped", async () => {
    await $('button[title="投稿"]').click();
    const textarea = await $('[data-testid="compose-textarea"]');
    await textarea.waitForDisplayed({ timeout: 15000 });
    await browser.keys("Escape");
    await textarea.waitForDisplayed({ reverse: true, timeout: 15000 });
  });

  it("keeps a column at full viewport width", async () => {
    // アカウント追加直後はカラムが無いため、Homeカラムを追加する。
    await $('[data-testid="app-menu-trigger"]').click();
    await $('[data-testid="app-menu-add-column"]').click();
    const submit = await $('[data-testid="add-column-submit"]');
    await submit.scrollIntoView();
    await submit.waitForClickable({ timeout: 15000 });
    await submit.click();

    await $(".column-root").waitForDisplayed({ timeout: 15000 });
    const col = await rect(".column-root");
    expect(col).not.toBeNull();
    expect(Math.round(col!.width)).toBeGreaterThanOrEqual(MOBILE_VIEWPORT.width - 2);
  });
});
