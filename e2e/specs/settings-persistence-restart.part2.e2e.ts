// アプリを一度正常終了させ、同じ設定ディレクトリで再起動しても構成が復元されることを
// 検証するシナリオの後半(Issue #223)。part1が作ったTMP_HOMEを再利用して起動するため、
// このプロセスはアカウント追加画面を経由せずいきなり以前のカラム構成が復元されるはず。
//
// このspecは"pnpm e2e"(既定の全spec実行)からは除外されている。理由は
// part1.e2e.ts冒頭のコメントを参照。
import { readFileSync } from "node:fs";
import { debugLog, debugLogPath } from "../helpers/debugLog";
import { PERSISTENCE_HOME_FILE } from "../helpers/persistenceHome";

describe("settings persist across restart (part 2: verify after restart)", () => {
  before(function () {
    // part1のログと同じTMP_HOMEパスが記録されていれば、run-app.shが正しく
    // 同一ディレクトリを再利用したことの直接証拠になる(part1側のログと比較する)。
    try {
      debugLog("persistenceRestartPart2", `TMP_HOME (from ${PERSISTENCE_HOME_FILE}): ${readFileSync(PERSISTENCE_HOME_FILE, "utf8")}`);
    } catch (err) {
      debugLog("persistenceRestartPart2", `failed to read PERSISTENCE_HOME_FILE (continuing anyway): ${String(err)}`);
    }
  });

  afterEach(async function () {
    if (this.currentTest?.state !== "failed") return;
    const safeTitle = (this.currentTest.title ?? "unknown").replace(/[^a-zA-Z0-9_-]+/g, "_");
    const path = debugLogPath(`app-window-failure-${safeTitle}-${Date.now()}.png`);
    try {
      await browser.saveScreenshot(path);
      debugLog("afterEach", `saved WebDriver screenshot: ${path}`);
    } catch (err) {
      debugLog("afterEach", `failed to save WebDriver screenshot: ${String(err)}`);
    }
  });

  it("restores the previously added account and Home column without re-adding an account", async function () {
    this.timeout(30000);

    // アカウントが0件ならApp.svelteはAddAccount画面を自動表示する
    // (`showAdd || reauthAccount || app.accounts.length === 0`)。復元が効いていれば
    // これは表示されず、既存のカラム/ComposeBarがいきなり見えるはず。
    const addAccountHostInput = await $('[data-testid="add-account-host-input"]');
    const addAccountShown = await addAccountHostInput.isExisting();
    if (addAccountShown) {
      debugLog("persistenceRestartPart2", "add-account screen unexpectedly shown; persistence did not restore the account");
    }
    expect(addAccountShown).toBe(false);

    const composeTextarea = await $('[data-testid="compose-textarea"]');
    await composeTextarea.waitForDisplayed({ timeout: 15000 });

    // part1で追加したHomeカラムが復元されていることを確認する(タイムラインの
    // 表示領域が存在すること。REST初期ロード自体は別シナリオで検証済みのため
    // ここではカラム構成の復元のみを見る)。
    const columnRoot = await $('[data-group-id]');
    await columnRoot.waitForDisplayed({ timeout: 15000 });
  });
});
