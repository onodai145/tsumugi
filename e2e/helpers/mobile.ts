import type { MiauthBridge } from "./miauthBridge";
import { clickThroughAccountSelect } from "./accountSelect";

const MISSKEY_HOST = "misskey.local:8443";

export const MOBILE_VIEWPORT = { width: 390, height: 844 } as const;
export const SAFE_AREA = { top: 47, right: 0, bottom: 34, left: 0 } as const;

type Insets = { top: number; right: number; bottom: number; left: number };
export type Rect = { top: number; right: number; bottom: number; left: number; width: number; height: number };

export async function setSafeArea(insets: Insets): Promise<void> {
  await browser.execute((i) => {
    const s = document.documentElement.style;
    s.setProperty("--safe-top", `${i.top}px`);
    s.setProperty("--safe-right", `${i.right}px`);
    s.setProperty("--safe-bottom", `${i.bottom}px`);
    s.setProperty("--safe-left", `${i.left}px`);
  }, insets);
}

export async function rect(selector: string): Promise<Rect | null> {
  return browser.execute((sel) => {
    const el = document.querySelector(sel);
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { top: r.top, right: r.right, bottom: r.bottom, left: r.left, width: r.width, height: r.height };
  }, selector);
}

export async function addAccountAndEnableMobile(bridge: MiauthBridge, logTag: string): Promise<void> {
  const hostInput = await $('[data-testid="add-account-host-input"]');
  await hostInput.waitForDisplayed({ timeout: 15000 });
  await hostInput.setValue(MISSKEY_HOST);
  const startButton = await $('[data-testid="add-account-start"]');
  await Promise.all([
    bridge.approveNext(),
    clickThroughAccountSelect(bridge.cdpPort, { logTag, accountLabel: "e2etestadmin" }),
    startButton.click(),
  ]);
  const complete = await $('[data-testid="add-account-complete"]');
  await complete.waitForDisplayed({ timeout: 15000 });
  await complete.click();

  // 設定→レイアウト→UIモード「モバイル版」→保存。
  const menuTrigger = await $('[data-testid="app-menu-trigger"]');
  await menuTrigger.waitForDisplayed({ timeout: 15000 });
  await menuTrigger.click();
  const openSettings = await $('[data-testid="app-menu-open-settings"]');
  await openSettings.waitForDisplayed({ timeout: 15000 });
  await openSettings.click();
  const layoutTab = await $('[data-testid="settings-tab-layout"]');
  await layoutTab.waitForDisplayed({ timeout: 15000 });
  await layoutTab.click();
  const mobileBtn = await $("button=モバイル版");
  await mobileBtn.waitForClickable({ timeout: 15000 });
  await mobileBtn.click();
  const saveBtn = await $("button=保存");
  await saveBtn.scrollIntoView();
  await saveBtn.click();
  await $("span=保存しました").waitForDisplayed({ timeout: 15000 });
  // 設定モーダルを閉じる(Modal.svelteの閉じるボタン)。開いたままだとFABが隠れる。
  await $('[data-testid="modal-close"]').click();

  await browser.setWindowSize(MOBILE_VIEWPORT.width, MOBILE_VIEWPORT.height);
  await browser.waitUntil(async () => (await browser.execute(() => window.innerWidth)) <= MOBILE_VIEWPORT.width + 20, {
    timeout: 10000,
    timeoutMsg: "window did not shrink to mobile viewport",
  });
  await setSafeArea(SAFE_AREA);
}

export async function addHomeColumn(): Promise<void> {
  await $('[data-testid="app-menu-trigger"]').click();
  await $('[data-testid="app-menu-add-column"]').click();
  const submit = await $('[data-testid="add-column-submit"]');
  await submit.scrollIntoView();
  await submit.waitForClickable({ timeout: 15000 });
  await submit.click();
  await $(".column-root").waitForDisplayed({ timeout: 15000 });
}
