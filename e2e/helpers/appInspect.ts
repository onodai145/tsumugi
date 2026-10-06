// 実アプリ(WebDriver)の状態検査ヘルパー: キャッシュDB(読み取り専用コピー)・設定JSON・IPCのトレース・カラムのDOM。
// 再起動をまたぐシナリオ(specs-*-restart)と、競合を狙うシナリオ(specs-column-edit)が使う。
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { MiauthBridge } from "./miauthBridge";
import { clickThroughAccountSelect } from "./accountSelect";
import { debugLog } from "./debugLog";

export const MISSKEY_HOST = "misskey.local:8443";

/** run-app.shが使っているTMP_HOME(E2E_REUSE_HOME_FILEの中身)。 */
export function tmpHome(): string {
  const f = process.env.E2E_REUSE_HOME_FILE;
  if (!f) throw new Error("E2E_REUSE_HOME_FILE is not set");
  return readFileSync(f, "utf-8").trim();
}

/** 稼働中のcache.dbを、読み取り専用の`.backup`でコピーしてからクエリする(稼働中DBは直接開かない)。 */
export function queryCache(sql: string): string[][] {
  const src = join(tmpHome(), "cache", "com.onodai.tsumugi", "cache.db");
  if (!existsSync(src)) throw new Error(`cache.db not found: ${src}`);
  const dir = mkdtempSync(join(tmpdir(), "e2e-cache-"));
  const copy = join(dir, "copy.db");
  execFileSync("sqlite3", ["-readonly", src, `.backup '${copy}'`]);
  const out = execFileSync("sqlite3", ["-readonly", "-separator", "\t", copy, sql], { encoding: "utf-8" });
  return out
    .split("\n")
    .filter((l) => l.length > 0)
    .map((l) => l.split("\t"));
}

export function readSettingsJson(): Record<string, unknown> {
  return JSON.parse(readFileSync(join(tmpHome(), "config", "com.onodai.tsumugi", "settings.json"), "utf-8"));
}

export async function addAccountViaUi(bridge: MiauthBridge, accountLabel: string): Promise<void> {
  const hostInput = await $('[data-testid="add-account-host-input"]');
  await hostInput.waitForDisplayed({ timeout: 15000 });
  await hostInput.setValue(MISSKEY_HOST);
  const startButton = await $('[data-testid="add-account-start"]');
  await Promise.all([
    bridge.approveNext(),
    clickThroughAccountSelect(bridge.cdpPort, { logTag: "appInspect", accountLabel }),
    startButton.click(),
  ]);
  const completeButton = await $('[data-testid="add-account-complete"]');
  await completeButton.waitForDisplayed({ timeout: 15000 });
  await completeButton.click();
}

async function waitStableHeight(): Promise<void> {
  await browser.waitUntil(
    async () => {
      const h1 = await browser.execute(() => window.innerHeight);
      await browser.pause(150);
      const h2 = await browser.execute(() => window.innerHeight);
      return h1 === h2 && h1 > 300;
    },
    { timeout: 10000, interval: 200 },
  );
}

export async function setWindow(): Promise<void> {
  await browser.setWindowSize(1280, 1024).catch((e) => debugLog("appInspect", `setWindowSize failed: ${String(e)}`));
}

/** TQL(エキスパート)のカラムを追加する。accountIdを渡すと、そのアカウントを選ぶ。 */
export async function addTqlColumn(tql: string, accountId?: string): Promise<void> {
  const menuTrigger = await $('[data-testid="app-menu-trigger"]');
  await menuTrigger.waitForDisplayed({ timeout: 15000 });
  await menuTrigger.click();
  const addColumnItem = await $('[data-testid="app-menu-add-column"]');
  await addColumnItem.waitForDisplayed({ timeout: 15000 });
  await addColumnItem.click();
  await waitStableHeight();
  if (accountId) {
    const scope = await $('[data-testid="add-column-account-select"]');
    await scope.waitForDisplayed({ timeout: 15000 });
    const trigger = await scope.$('[data-testid="account-select-trigger"]');
    await trigger.waitForClickable({ timeout: 15000 });
    await trigger.click();
    const option = await $(`[data-testid="account-select-option-${accountId}"]`);
    await option.waitForDisplayed({ timeout: 15000 });
    await option.click();
  }
  const expertTab = await $("button=エキスパート(TQL)");
  await expertTab.waitForDisplayed({ timeout: 15000 });
  await expertTab.click();
  const ta = await $('[data-testid="add-column-tql-textarea"]');
  await ta.waitForDisplayed({ timeout: 15000 });
  await ta.setValue(tql);
  const submit = await $('[data-testid="add-column-submit"]');
  await submit.scrollIntoView();
  await submit.waitForClickable({ timeout: 15000 });
  await submit.click();
}

/** 表示中の全ノート本文。 */
export async function noteTexts(): Promise<string[]> {
  return browser.execute(() => Array.from(document.querySelectorAll('[data-testid="note-text"]')).map((e) => (e as HTMLElement).textContent ?? ""));
}

export async function waitForNote(needle: string, timeout = 20000): Promise<void> {
  await browser.waitUntil(async () => (await noteTexts()).some((t) => t.includes(needle)), {
    timeout,
    interval: 300,
    timeoutMsg: `note containing "${needle}" did not appear`,
  });
}

/** IPC(fetch経由のipc://)の開始/終了を window.__trace に記録する。__TAURI_INTERNALS__.invokeは書き換え不可のため、fetchを包む。 */
export async function installTrace(): Promise<void> {
  await browser.execute(() => {
    const w = window as unknown as { __trace?: unknown[]; fetch: typeof fetch };
    if (w.__trace) return;
    const trace: unknown[] = (w.__trace = []);
    const orig = w.fetch.bind(window);
    const watch = new Set(["fetch_backfill", "update_column", "close_column", "set_mute", "resume_column", "add_column", "sync_server_mutes", "rename_column"]);
    w.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : (input as Request).url;
      const m = /^(?:ipc:\/\/localhost|https?:\/\/ipc\.localhost)\/([a-z_]+)/.exec(url);
      const cmd = m?.[1];
      const p = orig(input as RequestInfo, init);
      if (cmd && watch.has(cmd)) {
        trace.push({ cmd, ev: "start", t: Math.round(performance.now()) });
        p.then(
          () => trace.push({ cmd, ev: "ok", t: Math.round(performance.now()) }),
          (e) => trace.push({ cmd, ev: "err", t: Math.round(performance.now()), e: String(e) }),
        );
      }
      return p;
    }) as typeof fetch;
  });
}

export async function takeTrace(): Promise<{ cmd: string; ev: string; t: number; e?: string }[]> {
  return browser.execute(() => {
    const w = window as unknown as { __trace?: unknown[] };
    // fetchの包みが配列への参照を持つので、差し替えずに中身だけ取り出す
    return (w.__trace ?? []).splice(0) as { cmd: string; ev: string; t: number; e?: string }[];
  });
}

/** 先頭のノートを含む縦スクロール要素の最下部までスクロールし、scrollイベントを同期で発火する(=loadMoreのトリガ)。 */
export const SCROLL_ACTIVE_JS = `
  const first = document.querySelector('[data-testid="note-text"]');
  let el = first;
  while (el && !(getComputedStyle(el).overflowY === 'auto' && el.scrollHeight > el.clientHeight)) el = el.parentElement;
  if (!el) return false;
  el.scrollTop = el.scrollHeight;
  el.dispatchEvent(new Event('scroll'));
  return true;
`;

export async function scrollActiveToBottom(): Promise<boolean> {
  return browser.execute(new Function(SCROLL_ACTIVE_JS) as () => boolean);
}

export async function openEditActiveTab(): Promise<void> {
  const tab = await $('[data-testid="column-tab-name"]');
  await tab.waitForDisplayed({ timeout: 15000 });
  await tab.doubleClick();
  await waitStableHeight();
}

/** アプリが持つアカウント一覧(id, username)。 */
export async function listAccounts(): Promise<{ id: string; username: string }[]> {
  return browser.executeAsync((done: (v: { id: string; username: string }[]) => void) => {
    const inv = (window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string) => Promise<unknown> } }).__TAURI_INTERNALS__.invoke;
    inv("list_accounts").then((v) => done(v as { id: string; username: string }[]));
  });
}

export async function openSettingsAccountsAndAdd(): Promise<void> {
  const menuTrigger = await $('[data-testid="app-menu-trigger"]');
  await menuTrigger.waitForDisplayed({ timeout: 15000 });
  await menuTrigger.click();
  const openSettings = await $('[data-testid="app-menu-open-settings"]');
  await openSettings.waitForDisplayed({ timeout: 15000 });
  await openSettings.click();
  const accountsTab = await $('[data-testid="settings-tab-accounts"]');
  await accountsTab.waitForDisplayed({ timeout: 15000 });
  await accountsTab.click();
  const add = await $('[data-testid="settings-accounts-add"]');
  await add.waitForDisplayed({ timeout: 15000 });
  await add.click();
}

export async function closeModal(): Promise<void> {
  const close = await $('[data-testid="modal-close"]');
  if (await close.isExisting()) {
    await close.click();
    await browser.pause(500);
  }
}

/** 条件が成り立つまでポーリング。 */
export async function until<T>(fn: () => T | Promise<T>, ok: (v: T) => boolean, timeout = 45000, msg = "condition"): Promise<T> {
  const t0 = Date.now();
  for (;;) {
    const v = await fn();
    if (ok(v)) return v;
    if (Date.now() - t0 > timeout) throw new Error(`timeout waiting for ${msg}; last=${JSON.stringify(v)}`);
    await browser.pause(500);
  }
}

/** ノートを持つ縦スクロール要素(=カラム)を左から順に列挙し、各カラムのノート番号("v458 n<i> ")を返す。 */
export async function columnsInfo(): Promise<{ idx: number[] }[]> {
  return browser.execute(() => {
    const scrollers = new Set<HTMLElement>();
    document.querySelectorAll('[data-testid="note-text"]').forEach((n) => {
      let el: HTMLElement | null = n as HTMLElement;
      while (el && !(getComputedStyle(el).overflowY === "auto" && el.scrollHeight > el.clientHeight)) el = el.parentElement;
      if (el) scrollers.add(el);
    });
    const list = Array.from(scrollers).sort((a, b) => a.getBoundingClientRect().left - b.getBoundingClientRect().left);
    list.forEach((el, i) => el.setAttribute("data-e2e-col", String(i)));
    return list.map((el) => ({
      idx: Array.from(el.querySelectorAll('[data-testid="note-text"]'))
        .map((n) => /v458 n(\d+) /.exec((n as HTMLElement).textContent ?? ""))
        .filter((m): m is RegExpExecArray => !!m)
        .map((m) => Number(m[1])),
    }));
  });
}

export async function scrollColumnAt(i: number): Promise<void> {
  await browser.execute((k: number) => {
    const el = document.querySelector(`[data-e2e-col="${k}"]`) as HTMLElement | null;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    el.dispatchEvent(new Event("scroll"));
  }, i);
}

export async function saveNgUsers(value: string): Promise<void> {
  const menuTrigger = await $('[data-testid="app-menu-trigger"]');
  await menuTrigger.click();
  const openSettings = await $('[data-testid="app-menu-open-settings"]');
  await openSettings.waitForDisplayed({ timeout: 15000 });
  await openSettings.click();
  const muteTab = await $('[data-testid="settings-tab-mute"]');
  await muteTab.waitForDisplayed({ timeout: 15000 });
  await muteTab.click();
  const ta = await $('[data-testid="mute-ng-users-textarea"]');
  await ta.waitForDisplayed({ timeout: 15000 });
  await ta.setValue(value);
  await (await $('[data-testid="mute-save"]')).click();
  await browser.pause(1500);
  await (await $('[data-testid="modal-close"]')).click();
  await browser.pause(500);
}
