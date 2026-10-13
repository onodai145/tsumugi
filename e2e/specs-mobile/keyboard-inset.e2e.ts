import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { addAccountAndEnableMobile, rect, SAFE_AREA, setKeyboardInset } from "../helpers/mobile";

describe("mobile compose page with the soft keyboard", () => {
  let bridge: MiauthBridge;

  before(async function () {
    this.timeout(120000);
    bridge = await startMiauthBridge();
    await addAccountAndEnableMobile(bridge, "mobileKeyboardInset");
  });

  after(async () => {
    await setKeyboardInset(0);
    await bridge.teardown();
  });

  it("grows the input down to the keyboard top and keeps the buttons above the keyboard", async () => {
    const KB = 300;
    await $('button[title="投稿"]').click();
    const textarea = await $('[data-testid="compose-textarea"]');
    await textarea.waitForDisplayed({ timeout: 15000 });
    const vh = await browser.execute(() => window.innerHeight);

    // キーボード無し: 入力欄は最小の高さ(96px)を大きく超えて広がる。
    await setKeyboardInset(0);
    const closed = await rect('[data-testid="compose-textarea"]');
    expect(closed!.height).toBeGreaterThan(200);

    // キーボード300px: 本文の下余白が max(セーフエリア, 300+8px) になり、入力欄がその分だけ縮む。
    await setKeyboardInset(KB);
    const open = await rect('[data-testid="compose-textarea"]');
    const submit = await rect('[data-testid="compose-submit"]');
    expect(submit!.bottom).toBeLessThanOrEqual(vh - KB);
    const expectedShrink = KB + 8 - SAFE_AREA.bottom;
    expect(Math.abs(closed!.height - open!.height - expectedShrink)).toBeLessThanOrEqual(2);

    // キーボードを閉じると元の高さに戻る。
    await setKeyboardInset(0);
    const back = await rect('[data-testid="compose-textarea"]');
    expect(Math.abs(back!.height - closed!.height)).toBeLessThanOrEqual(1);
    await browser.back();
    await textarea.waitForDisplayed({ reverse: true, timeout: 15000 });
  });

  // ボタン群が画面下端(キーボード直上)にあるため、メニューは上に出て、見える領域に収まる必要がある。
  for (const KB of [0, 300]) {
    it(`opens the attach and visibility menus inside the visible area (keyboard ${KB}px)`, async () => {
      await $('button[title="投稿"]').click();
      await $('[data-testid="compose-textarea"]').waitForDisplayed({ timeout: 15000 });
      await setKeyboardInset(KB);
      const vh = await browser.execute(() => window.innerHeight);

      const closeMenu = (role: string) =>
        browser.execute((r) => {
          // メニューは全面のバックドロップの中にある。バックドロップのクリックで閉じる。
          (document.querySelector(`[role="${r}"]`)?.parentElement as HTMLElement | null)?.click();
        }, role);

      await $('button[title="画像を添付"]').click();
      const attach = await rect('[role="menu"]');
      expect(attach).not.toBeNull();
      expect(attach!.top).toBeGreaterThanOrEqual(0);
      expect(attach!.bottom).toBeLessThanOrEqual(vh - KB);
      await closeMenu("menu");

      await $('button[title^="公開範囲"]').click();
      const visibility = await rect('[role="listbox"]');
      expect(visibility).not.toBeNull();
      expect(visibility!.top).toBeGreaterThanOrEqual(0);
      expect(visibility!.bottom).toBeLessThanOrEqual(vh - KB);
      await closeMenu("listbox");

      await setKeyboardInset(0);
      await browser.back();
      await $('[data-testid="compose-textarea"]').waitForDisplayed({ reverse: true, timeout: 15000 });
    });
  }
});
