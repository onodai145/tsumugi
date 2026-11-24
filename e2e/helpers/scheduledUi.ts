// 予約投稿(Issue #60)の E2E で使う、予約 UI の操作 helper。セレクタは実際のコンポーネント
// (frontend/src/ui/ComposeBar.svelte、ScheduledModal.svelte)の data-testid。
// 日時は、カレンダーの操作ではなく flatpickr のインスタンスへ直接入れる(カレンダーのクリックは
// E2E では不安定になりやすく、この E2E の目的は予約の動作であってピッカーの操作ではないため)。

import { debugLog, debugLogPath } from "./debugLog";

/**
 * 要素の矩形が `quietMs` のあいだ変わらなくなるまで待つ。
 *
 * 作成欄の textarea は、フォーカスと入力の有無で最小の高さが 34px ↔ 80px に変わり(`ComposeBar.svelte` の
 * `compact`)、0.12 秒かけて遷移する。その間はツールバーのボタンが縦に約 46px 動く。`waitForClickable` は
 * 「表示されていて有効」しか見ないので、遷移の途中で通ったあとのクリックは、動く前の座標に飛んで空振りする
 * (Issue #641。入力後の「予約」が textarea の上に落ちる、予約成功直後のトグルが空白に落ちる)。
 *
 * `browser.execute` の中には名前付きの関数を作らない(下の `dumpUiFailure` の注意を参照)。
 */
async function waitForStableRect(selector: string, quietMs = 300): Promise<void> {
  let last = "";
  let since = Date.now();
  try {
    await browser.waitUntil(
      async () => {
        const rect = await browser.execute((sel: string) => {
          const r = document.querySelector(sel)?.getBoundingClientRect();
          return r ? `${r.x},${r.y},${r.width},${r.height}` : "";
        }, selector);
        if (rect === "" || rect !== last) {
          last = rect;
          since = Date.now();
          return false;
        }
        return Date.now() - since >= quietMs;
      },
      { timeout: 15000, interval: 50 },
    );
  } catch (e) {
    // timeoutMsg は呼び出し時に評価されて last が空のままなので、ここで組み立てる
    throw new Error(`layout of ${selector} did not settle within 15s (last rect: ${last || "element not found"})\n${String(e)}`);
  }
}

/** 要素が押せて、位置が落ち着いてからクリックする(遷移中の座標ずれによる空振りを避ける)。 */
async function clickStable(testid: string): Promise<void> {
  const selector = `[data-testid="${testid}"]`;
  await (await $(selector)).waitForClickable({ timeout: 15000 });
  await waitForStableRect(selector);
  await (await $(selector)).click();
}

/** 予約の日時入力を開く(開いていれば何もしない)。 */
export async function openPicker(): Promise<void> {
  const input = await $('[data-testid="compose-schedule-input"]');
  if (!(await input.isDisplayed().catch(() => false))) {
    await clickStable("compose-schedule-toggle");
  }
  try {
    await (await $('[data-testid="compose-schedule-input"]')).waitForDisplayed({ timeout: 10000 });
  } catch (e) {
    const detail = await dumpUiFailure("openPicker");
    throw new Error(`schedule picker did not open after clicking the toggle\n${detail}\n${String(e)}`);
  }
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
  // setValue で textarea が伸び、ボタンが動く最中にクリックすると空振りする(waitForStableRect を参照)
  await clickStable("compose-submit");
  try {
    await browser.waitUntil(async () => (await (await $('[data-testid="compose-textarea"]')).getValue()) === "", {
      timeout: 20000,
      interval: 300,
    });
  } catch (e) {
    const detail = await dumpUiFailure(`scheduleAt-${text}`, { requestedAt: new Date(atMs).toISOString() });
    throw new Error(
      `compose box did not clear after scheduling "${text}" (the app rejects a past date, so the due time may be too close — slow runner?)\n${detail}\n${String(e)}`,
    );
  }
}

/**
 * 予約 UI の操作が失敗したとき、アプリが出したエラー(`role="dialog"` のモーダル本文)と作成欄・予約欄・
 * ボタンの状態を集め、スクリーンショットを `e2e/wdio-logs/` に残す(Issue #641。CI はアプリ自身のログを
 * 回収しないため、画面の状態が無いと原因を追えない)。診断の失敗で本来のエラーを隠さないよう、ここでは
 * 例外を投げない。
 *
 * `browser.execute` に渡す関数の中で、名前付きの関数(`const f = () => ...` や `function f() {}`)を作らない
 * こと。E2E の TS 変換(tsx/esbuild の keepNames)がそれを `__name(...)` で包み、WebView 側には
 * `__name` が無いので `Can't find variable: __name` で診断ごと失敗する(2026-11-16 の CI で実際に起きた)。
 * `.map((x) => ...)` のように引数へ直接渡す無名関数は包まれない。
 */
async function dumpUiFailure(label: string, extra: Record<string, unknown> = {}): Promise<string> {
  let detail = "";
  try {
    const state = await browser.execute(() => {
      const textarea = document.querySelector('[data-testid="compose-textarea"]') as HTMLTextAreaElement | null;
      const input = document.querySelector('[data-testid="compose-schedule-input"]') as HTMLInputElement | null;
      const toggle = document.querySelector('[data-testid="compose-schedule-toggle"]') as HTMLButtonElement | null;
      const submit = document.querySelector('[data-testid="compose-submit"]') as HTMLButtonElement | null;
      return {
        dialog: document.querySelector('[role="dialog"]')?.textContent?.trim() ?? null,
        textareaLength: textarea?.value.length ?? null,
        scheduleInputPresent: input !== null,
        scheduleInputRects: input?.getClientRects().length ?? null,
        scheduleInput: input?.value ?? null,
        toggleDisabled: toggle?.disabled ?? null,
        submitLabel: submit?.textContent?.trim() ?? null,
        submitDisabled: submit?.disabled ?? null,
        // クリックの空振り(レイアウトの遷移中のクリック)を疑うときに、ボタンの位置を見る(Issue #641)
        toggleY: toggle ? Math.round(toggle.getBoundingClientRect().y) : null,
        submitY: submit ? Math.round(submit.getBoundingClientRect().y) : null,
        textareaHeight: textarea ? Math.round(textarea.getBoundingClientRect().height) : null,
        localHint: document.querySelector('[data-testid="compose-schedule-local-hint"]') !== null,
        activeElement: document.activeElement?.getAttribute("data-testid") ?? document.activeElement?.tagName ?? null,
        browserNow: new Date().toISOString(),
      };
    });
    detail = `diagnostics: ${JSON.stringify({ ...state, ...extra })}`;
  } catch (err) {
    detail = `diagnostics unavailable: ${String(err)}`;
  }
  debugLog(label, `failed: ${detail}`);
  try {
    const safe = label.replace(/[^a-zA-Z0-9_-]+/g, "_");
    const path = debugLogPath(`schedule-failure-${safe}-${Date.now()}.png`);
    await browser.saveScreenshot(path);
    debugLog(label, `saved screenshot: ${path}`);
  } catch (err) {
    debugLog(label, `failed to save screenshot: ${String(err)}`);
  }
  return detail;
}

/** 予約一覧を開く。 */
export async function openList(): Promise<void> {
  await openPicker();
  await clickStable("compose-scheduled-list");
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
