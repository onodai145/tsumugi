// MiAuth同意フローには、単一アカウントしか無くても必ず「アカウントを選択してください」
// 画面(MkAuthConfirm.vueのaccountSelectフェーズ)が先に出る。miauthBridge.tsの
// approveNext()は「許可」ボタン(consentフェーズ)しか押さないため、先にこの画面を
// 通過させないとapproveNext()はタイムアウトする。
//
// account-post-reaction.e2e.ts / server-word-mute.e2e.ts にほぼ同一の実装が
// 重複していたため、ここへ抽出した(Issue #223)。ログタグ・クリック対象の
// アカウント行文字列を引数化し、複数アカウントを扱うシナリオでも再利用できるようにした。
import { chromium, type Page } from "playwright";
import { debugLog, debugLogPath } from "./debugLog";

function attachPageDiagnostics(page: Page, logTag: string, label: string): void {
  page.on("console", (msg) => debugLog(`${logTag}:${label}:console`, `${msg.type()}: ${msg.text()}`));
  page.on("pageerror", (err) => debugLog(`${logTag}:${label}:pageerror`, err.stack ?? err.message));
}

async function dumpFailureArtifacts(page: Page, logTag: string, stage: string): Promise<void> {
  try {
    const bodyText = await page.innerText("body").catch((e) => `<failed to read body: ${String(e)}>`);
    debugLog(`${logTag}:failure`, `stage=${stage} url=${page.url()} bodyText(先頭2000文字)=${bodyText.slice(0, 2000)}`);
  } catch (err) {
    debugLog(`${logTag}:failure`, `stage=${stage} failed to dump body text: ${String(err)}`);
  }
  try {
    const screenshotPath = debugLogPath(`${logTag}-failure-${stage}.png`);
    await page.screenshot({ path: screenshotPath });
    debugLog(`${logTag}:failure`, `screenshot saved: ${screenshotPath}`);
  } catch (err) {
    debugLog(`${logTag}:failure`, `stage=${stage} failed to save screenshot: ${String(err)}`);
  }
}

/**
 * MiAuthの「アカウントを選択してください」画面を通過させ、「続ける」を押す。
 * bridgeが公開しているCDPポートに対して別のPlaywright CDPクライアントで接続し、
 * 同じブラウザコンテキスト上のMiAuthタブを見つけて操作する
 * (approveNext()自体は「許可」ボタンしか押さないため、これと並行に呼ぶ設計)。
 */
export async function clickThroughAccountSelect(
  cdpPort: number,
  opts: { logTag: string; accountLabel: string },
): Promise<void> {
  const { logTag, accountLabel } = opts;
  debugLog(logTag, `connecting over CDP to 127.0.0.1:${cdpPort}`);
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${cdpPort}`);
  try {
    const context = browser.contexts()[0];
    if (!context) throw new Error(`${logTag}: no browser context found via CDP`);
    debugLog(logTag, `connected; existing pages=${context.pages().map((p) => p.url()).join(", ") || "(none)"}`);
    const existing = context.pages().find((p) => p.url().includes("/miauth/"));
    const page = existing ?? (await context.waitForEvent("page", { timeout: 30000 }));
    attachPageDiagnostics(page, logTag, existing ? "existing" : "waited");
    debugLog(logTag, `page acquired (${existing ? "was already open" : "via waitForEvent"}): ${page.url()}`);

    await page.waitForLoadState("domcontentloaded");
    debugLog(logTag, `domcontentloaded: url=${page.url()} title=${await page.title().catch(() => "?")}`);

    await page
      .getByText(accountLabel, { exact: false })
      .first()
      .click({ timeout: 5000 })
      .then(
        () => debugLog(logTag, "account row click: succeeded"),
        (err) => debugLog(logTag, `account row click: FAILED (continuing anyway): ${String(err)}`),
      );

    const continueButton = page.getByRole("button", { name: "続ける" });
    try {
      await continueButton.waitFor({ state: "attached", timeout: 15000 });
      const [count, visible, enabled] = await Promise.all([
        continueButton.count(),
        continueButton.isVisible().catch(() => "?"),
        continueButton.isEnabled().catch(() => "?"),
      ]);
      debugLog(logTag, `続ける button attached: count=${count} visible=${visible} enabled=${enabled}`);
    } catch (err) {
      debugLog(logTag, `続ける button never attached to DOM: ${String(err)}`);
      await dumpFailureArtifacts(page, logTag, "button-not-attached");
      throw err;
    }

    try {
      await continueButton.click({ timeout: 15000 });
      debugLog(logTag, "clicked 続ける button");
    } catch (err) {
      debugLog(logTag, `FAILED to click 続ける button: ${String(err)}`);
      await dumpFailureArtifacts(page, logTag, "button-click-failed");
      throw err;
    }
  } finally {
    await browser.close();
  }
}
