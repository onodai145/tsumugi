// メディアビューワーのセーフエリアは未カバー(画像アップロード用のE2Eヘルパーが無いため)。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { addAccountAndEnableMobile, addHomeColumn, rect, SAFE_AREA, setSafeArea } from "../helpers/mobile";

describe("mobile safe-area", () => {
  let bridge: MiauthBridge;

  before(async function () {
    this.timeout(120000);
    bridge = await startMiauthBridge();
    await addAccountAndEnableMobile(bridge, "mobileSafeArea");
    await addHomeColumn();
  });

  after(async () => {
    await bridge.teardown();
  });

  it("keeps the compose FAB inside the bottom/right safe area", async () => {
    // right は既定 0 だと検証が空振りするため、非ゼロのinsetを与える。
    const RIGHT = 24;
    await setSafeArea({ ...SAFE_AREA, right: RIGHT });
    try {
      const fab = await rect('button[title="投稿"]');
      expect(fab).not.toBeNull();
      const vh = await browser.execute(() => window.innerHeight);
      const vw = await browser.execute(() => window.innerWidth);
      expect(fab!.bottom).toBeLessThanOrEqual(vh - SAFE_AREA.bottom);
      expect(fab!.right).toBeLessThanOrEqual(vw - RIGHT);
    } finally {
      await setSafeArea(SAFE_AREA);
    }
  });

  it("keeps the compose modal content below the top safe area", async () => {
    // 6vh(約50px)より十分大きいinsetにして var(--safe-top) の効きを検出する。
    const TOP = 120;
    await setSafeArea({ ...SAFE_AREA, top: TOP });
    try {
      await $('button[title="投稿"]').click();
      const textarea = await $('[data-testid="compose-textarea"]');
      await textarea.waitForDisplayed({ timeout: 15000 });
      const r = await rect('[data-testid="compose-textarea"]');
      expect(r!.top).toBeGreaterThanOrEqual(TOP);
      await browser.keys("Escape");
      await textarea.waitForDisplayed({ reverse: true, timeout: 15000 });
    } finally {
      await setSafeArea(SAFE_AREA);
    }
  });

  it("keeps the bottom menu bar above the bottom safe area padding", async () => {
    // AppMenu/Backstage の下端パディングが --safe-bottom 分確保されているか。
    const trigger = await rect('[data-testid="app-menu-trigger"]');
    const vh = await browser.execute(() => window.innerHeight);
    expect(trigger!.bottom).toBeLessThanOrEqual(vh - SAFE_AREA.bottom);
  });

  it("keeps the column area below the top safe area", async () => {
    // モバイルUIにはheaderが無く、main自身が pt-[var(--safe-top)] でステータスバー分を確保する(Issue #257)。
    const col = await rect(".column-root");
    expect(col).not.toBeNull();
    expect(col!.top).toBeGreaterThanOrEqual(SAFE_AREA.top);
  });

  it("keeps the bottom menu button inside the left safe area", async () => {
    // 既存パディング(max(8px, var(--safe-left)))より十分大きいinsetで var(--safe-left) の効きを検出する。
    const LEFT = 30;
    await setSafeArea({ ...SAFE_AREA, left: LEFT });
    try {
      const trigger = await rect('[data-testid="app-menu-trigger"]');
      expect(trigger).not.toBeNull();
      expect(trigger!.left).toBeGreaterThanOrEqual(LEFT);
    } finally {
      await setSafeArea(SAFE_AREA);
    }
  });
});
