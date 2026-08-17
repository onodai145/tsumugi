// MiAuthの同意画面を自動承認するためのブリッジ。
// tsumugi本体がMiAuth URLをシステムブラウザで開こうとすると、opener差し替え
// (browser-open.sh)がこのブリッジのCDPセッションに新規タブとしてURLを渡す。
// approveNext()はその新規タブを検知し、「許可」ボタンをクリックする。
import { chromium, type Browser, type BrowserContext } from "playwright";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// このファイル自体はESM (package.json "type": "module") で実行されるため、
// CommonJSの__dirnameは使えない。import.meta.urlから同等のものを導出する。
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const CDP_PORT = 9333;
const MISSKEY_URL = process.env.E2E_MISSKEY_URL ?? "https://misskey.local:8443";

export interface MiauthBridge {
  cdpPort: number;
  approveNext(): Promise<void>;
  teardown(): Promise<void>;
}

interface SeededAccount {
  username: string;
  password: string;
}

/**
 * MiAuthの同意画面を自動承認するためのブリッジを起動する。
 * 1. CDPデバッグポート付きでChromiumを起動する。
 * 2. /api/signin でテストユーザーのセッションCookieを取得し、ブラウザに注入する。
 * 3. approveNext() が呼ばれたら、新規タブでMiAuth URLが開かれるのを待ち、
 *    「許可」ボタンをクリックする。
 *
 * KNOWN BROKEN (Misskey 2026.7.0, verified 2026-08-17 — see task-6-report.md):
 * - `/api/signin` no longer exists; the current endpoint is
 *   `/api/signin-flow`, and even that returns `{finished, id, i: token}`
 *   with NO `Set-Cookie` header at all. This Misskey version's web frontend
 *   does not use a server session cookie for login — `packages/frontend/src/
 *   accounts.ts` stores the access token client-side (pizzax store, backed
 *   by IndexedDB, with a legacy localStorage migration path). The
 *   cookie-injection step below (`signinRes.headers.get("set-cookie")`)
 *   will always throw. A correct implementation would call
 *   `/api/signin-flow`, take the returned `i` token, and use
 *   `context.addInitScript()` (or `page.addInitScript()`, since a
 *   CDP-opened tab lands in the browser's default context — see next
 *   paragraph) to seed the frontend's account store before the MiAuth
 *   consent page's own script runs, instead of `addCookies()`.
 * - Separately (independent of the above): a tab opened via CDP's
 *   `/json/new` lands in the browser's default context, not the
 *   `browser.newContext()` created here — `context.waitForEvent("page")`
 *   in `approveNext()` never fires for it. Confirmed empirically. Fix is to
 *   use `chromium.launchPersistentContext()` instead of `launch()` +
 *   `newContext()`, so there is only one context and CDP-opened tabs are
 *   visible to it.
 */
export async function startMiauthBridge(): Promise<MiauthBridge> {
  const seeded: SeededAccount = JSON.parse(
    readFileSync(join(__dirname, "..", "certs", "seeded-account.json"), "utf-8"),
  );

  const browser: Browser = await chromium.launch({
    args: [
      `--remote-debugging-port=${CDP_PORT}`,
      // このブリッジ専用のChromiumインスタンスに限定した措置。
      // e2eサンドボックスには /etc/hosts への misskey.local エントリが無いため、
      // このChromiumだけがmisskey.localを127.0.0.1へ解決できるようにする。
      "--host-resolver-rules=MAP misskey.local 127.0.0.1",
    ],
  });
  const context: BrowserContext = await browser.newContext({ ignoreHTTPSErrors: true });

  const signinRes = await fetch(`${MISSKEY_URL}/api/signin`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ username: seeded.username, password: seeded.password }),
  });
  if (!signinRes.ok) {
    throw new Error(`miauthBridge: /api/signin failed with ${signinRes.status}`);
  }
  const setCookie = signinRes.headers.get("set-cookie");
  if (!setCookie) {
    throw new Error("miauthBridge: /api/signin did not return a session cookie");
  }
  const [nameValue] = setCookie.split(";");
  const [cookieName, cookieValue] = nameValue.split("=");
  const url = new URL(MISSKEY_URL);
  await context.addCookies([
    {
      name: cookieName,
      value: cookieValue,
      domain: url.hostname,
      path: "/",
      secure: true,
    },
  ]);

  return {
    cdpPort: CDP_PORT,
    async approveNext() {
      const page = await context.waitForEvent("page", { timeout: 30000 });
      await page.waitForLoadState("domcontentloaded");
      await page.getByRole("button", { name: "許可" }).click();
    },
    async teardown() {
      await context.close();
      await browser.close();
    },
  };
}
