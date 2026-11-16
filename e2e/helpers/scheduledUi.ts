// 予約投稿(Issue #60)の E2E で使う、予約 UI の操作 helper。セレクタは実際のコンポーネント
// (frontend/src/ui/ComposeBar.svelte、ScheduledModal.svelte)の data-testid。
// 日時は、カレンダーの操作ではなく flatpickr のインスタンスへ直接入れる(カレンダーのクリックは
// E2E では不安定になりやすく、この E2E の目的は予約の動作であってピッカーの操作ではないため)。

import { debugLog, debugLogPath } from "./debugLog";

/** 予約の日時入力を開く(開いていれば何もしない)。 */
export async function openPicker(): Promise<void> {
  const input = await $('[data-testid="compose-schedule-input"]');
  if (!(await input.isDisplayed().catch(() => false))) {
    const toggle = await $('[data-testid="compose-schedule-toggle"]');
    await toggle.waitForClickable({ timeout: 15000 });
    await toggle.click();
  }
  await (await $('[data-testid="compose-schedule-input"]')).waitForDisplayed({ timeout: 10000 });
}

/** 本文と予約日時(ミリ秒)を入れて「予約」を押し、作成欄が空になる(=成功する)まで待つ。 */
export async function scheduleAt(text: string, atMs: number): Promise<void> {
  await openPicker();
  await browser.execute((ms: number) => {
    const el = document.querySelector('[data-testid="compose-schedule-input"]') as unknown as {
      _flatpickr: { setDate(d: Date, triggerChange: boolean): void };
    };
    el._flatpickr.setDate(new Date(ms), true);
  }, atMs);
  const textarea = await $('[data-testid="compose-textarea"]');
  await textarea.setValue(text);
  const submit = await $('[data-testid="compose-submit"]');
  await submit.waitForClickable({ timeout: 15000 });
  await submit.click();
  try {
    await browser.waitUntil(async () => (await (await $('[data-testid="compose-textarea"]')).getValue()) === "", {
      timeout: 20000,
      interval: 300,
    });
  } catch (e) {
    const detail = await dumpScheduleFailure(text, atMs);
    throw new Error(
      `compose box did not clear after scheduling "${text}" (the app rejects a past date, so the due time may be too close — slow runner?)\n${detail}\n${String(e)}`,
    );
  }
}

/**
 * 予約後に作成欄が空にならなかったとき、アプリが出したエラー(`role="dialog"` のモーダル本文)と
 * 送信まわりの状態を集め、スクリーンショットを `e2e/wdio-logs/` に残す(Issue #641。CI は
 * アプリ自身のログを回収しないため、画面に出たエラー文が無いと原因を追えない)。
 * 診断の失敗で本来のエラーを隠さないよう、ここでは例外を投げない。
 */
async function dumpScheduleFailure(text: string, atMs: number): Promise<string> {
  let detail = "";
  try {
    const state = await browser.execute(() => {
      const val = (sel: string) => (document.querySelector(sel) as HTMLInputElement | null)?.value ?? null;
      const submit = document.querySelector('[data-testid="compose-submit"]') as HTMLButtonElement | null;
      return {
        dialog: document.querySelector('[role="dialog"]')?.textContent?.trim() ?? null,
        textareaLength: val('[data-testid="compose-textarea"]')?.length ?? null,
        scheduleInput: val('[data-testid="compose-schedule-input"]'),
        submitLabel: submit?.textContent?.trim() ?? null,
        submitDisabled: submit?.disabled ?? null,
        localHint: document.querySelector('[data-testid="compose-schedule-local-hint"]') !== null,
        browserNow: new Date().toISOString(),
      };
    });
    detail = `diagnostics: ${JSON.stringify({ ...state, requestedAt: new Date(atMs).toISOString() })}`;
  } catch (err) {
    detail = `diagnostics unavailable: ${String(err)}`;
  }
  debugLog("scheduleAt", `failed for "${text}": ${detail}`);
  try {
    const safe = text.replace(/[^a-zA-Z0-9_-]+/g, "_");
    const path = debugLogPath(`schedule-failure-${safe}-${Date.now()}.png`);
    await browser.saveScreenshot(path);
    debugLog("scheduleAt", `saved screenshot: ${path}`);
  } catch (err) {
    debugLog("scheduleAt", `failed to save screenshot: ${String(err)}`);
  }
  return detail;
}

/** 予約一覧を開く。 */
export async function openList(): Promise<void> {
  await openPicker();
  const btn = await $('[data-testid="compose-scheduled-list"]');
  await btn.waitForClickable({ timeout: 15000 });
  await btn.click();
  await (await $('[data-testid^="scheduled-item-"], [data-testid="scheduled-empty"]')).waitForDisplayed({ timeout: 15000 });
}

export interface ListRow {
  testid: string;
  text: string;
}

/** 予約一覧の行(testid と表示テキスト)。 */
export async function listRows(): Promise<ListRow[]> {
  return browser.execute(() =>
    Array.from(document.querySelectorAll('[data-testid^="scheduled-item-"]')).map((e) => ({
      testid: e.getAttribute("data-testid") ?? "",
      text: (e as HTMLElement).textContent ?? "",
    })),
  );
}

/** 行の testid(`scheduled-item-<id>`)から予約の ID を取り出す。 */
export const rowId = (testid: string): string => testid.replace("scheduled-item-", "");

/** 一覧のうち、本文に `needle` を含む行。 */
export async function rowContaining(needle: string): Promise<ListRow | undefined> {
  return (await listRows()).find((r) => r.text.includes(needle));
}

/** 予約日時に使う時刻(ミリ秒): 少なくとも `marginMs` 先の、次の分の 0 秒。予約は分単位のため。 */
export function nextMinuteAfter(marginMs: number): number {
  return Math.ceil((Date.now() + marginMs) / 60_000) * 60_000;
}
