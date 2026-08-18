// MiAuthの同意画面を自動承認するためのブリッジ。
// tsumugi本体がMiAuth URLをシステムブラウザで開こうとすると、opener差し替え
// (browser-open.sh)がこのブリッジのCDPセッションに新規タブとしてURLを渡す。
// approveNext()はその新規タブを検知し、「許可」ボタンをクリックする。
import { chromium, type BrowserContext } from "playwright";
import { mkdtempSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import dns from "node:dns";

// このファイル自体はESM (package.json "type": "module") で実行されるため、
// CommonJSの__dirnameは使えない。import.meta.urlから同等のものを導出する。
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const CDP_PORT = 9333;
const MISSKEY_URL = process.env.E2E_MISSKEY_URL ?? "https://misskey.local:8443";

// e2eサンドボックスの/etc/hostsにはmisskey.localのエントリが無い(実機検証済み、
// sudoにパスワードが必要でCI/エージェントからは書き換えられない)。このファイル内で
// launchPersistentContext()に渡すChromiumは`--host-resolver-rules`で自前解決できるが、
// このファイル自身がNodeの標準fetch()で直接叩く/api/signin-flowや/api/iは
// misskey.localを解決できず`TypeError: fetch failed`になる(実機確認済み)。
// undiciベースのNode fetch()はデフォルトでnode:dnsのdns.lookup()を経由して名前解決する
// ため、ここをプロセス内でパッチしてmisskey.localだけ127.0.0.1へ固定する
// (Chromiumの--host-resolver-rulesと同じ発想を、Node側のfetch()にも適用するもの)。
const originalLookup = dns.lookup;
// @ts-expect-error - overload signatures make a single reassignment awkward; behavior is verified above
dns.lookup = (hostname: string, options: unknown, callback?: unknown) => {
  const cb = (typeof options === "function" ? options : callback) as (
    err: NodeJS.ErrnoException | null,
    address: string | dns.LookupAddress[],
    family?: number,
  ) => void;
  if (hostname === "misskey.local") {
    if (typeof options === "object" && options !== null && (options as { all?: boolean }).all) {
      return cb(null, [{ address: "127.0.0.1", family: 4 }]);
    }
    return cb(null, "127.0.0.1", 4);
  }
  // @ts-expect-error - passthrough to the original overloaded signature
  return originalLookup(hostname, options, callback);
};

export interface MiauthBridge {
  cdpPort: number;
  approveNext(): Promise<void>;
  teardown(): Promise<void>;
}

interface SeededAccount {
  username: string;
  password: string;
}

// SigninFlowResponse (packages/backend/src/server/api/SigninApiService.ts /
// misskey-js `entities.SigninFlowResponse`) when signin completes in a
// single step (no 2FA/captcha configured on this instance).
interface SigninFlowFinished {
  finished: true;
  id: string;
  i: string; // access token
}

/**
 * MiAuthの同意画面を自動承認するためのブリッジを起動する。
 * 1. 単一の永続コンテキスト付きでChromiumを起動する（CDP `/json/new` で
 *    開かれるタブも、このブリッジが観測するタブも同じコンテキストに
 *    属させるため — 詳細は下記コメント参照）。
 * 2. /api/signin-flow でテストユーザーのアクセストークンを取得し、/api/i
 *    でそのユーザーの詳細情報を取得したうえで、フロントエンドが起動時に
 *    参照する localStorage の "account" キーへ addInitScript() で注入する。
 *    これにより、MiAuth同意画面がロードされた時点でフロントエンドは
 *    「既にサインイン済み」として扱う。
 * 3. approveNext() が呼ばれたら、新規タブでMiAuth URLが開かれるのを待ち、
 *    「許可」ボタンをクリックする。
 *
 * --- なぜCookieではなくlocalStorageなのか (Misskey 2026.7.0で検証済み) ---
 * このMisskeyバージョンに `/api/signin` は存在しない
 * (`Route POST:/api/signin not found`)。実際のエンドポイントは
 * `/api/signin-flow` で、これも `Set-Cookie` を一切返さない
 * (`packages/backend/src/server/api/SigninService.ts` の `signin()` は
 * `{finished, id, i}` を返すのみで `reply.setCookie` を呼んでいない)。
 * このバージョンのWebフロントエンドはサーバーセッションCookieを
 * 使っておらず、ログイン状態はクライアント側の
 * `window.localStorage.getItem('account')` (JSON文字列、
 * `MeDetailed & {token: string}` 形状) を起動時に同期的に読んで
 * `$i` を初期化することで判定している
 * (`packages/frontend/src/i.ts`: `const accountData =
 * miLocalStorage.getItem('account'); export const $i = accountData ? ... :
 * null;`)。MiAuth同意画面 (`packages/frontend/src/components/
 * MkAuthConfirm.vue`) の `init()` も `$i` が非nullなら
 * `users.value.set($i.id, $i)` としてアカウント選択候補に加える。
 * したがって `addCookies()` ではなく、ページ読み込み前に
 * `localStorage['account']` へ正しい形状のJSONを書き込む
 * `addInitScript()` が正しい注入手段となる。
 *
 * --- なぜ`launch()`+`newContext()`ではなく`launchPersistentContext()`か ---
 * CDPの `PUT /json/new` で開いたタブは、ブラウザの「デフォルトの
 * コンテキスト」に属する。`chromium.launch()` + `browser.newContext()` で
 * 別途作成したコンテキストの `waitForEvent("page")` は、デフォルト
 * コンテキストに属するそのタブを検知できない (実機で確認済み:
 * `browserContext.waitForEvent: Timeout 5000ms exceeded`)。
 * `launchPersistentContext()` はコンテキストが1つしか存在しないため、
 * CDPで開いたタブも `addInitScript()` によるlocalStorage注入も
 * 同じコンテキストに属し、両方が機能する。
 */
export async function startMiauthBridge(): Promise<MiauthBridge> {
  const seeded: SeededAccount = JSON.parse(
    readFileSync(join(__dirname, "..", "certs", "seeded-account.json"), "utf-8"),
  );

  const userDataDir = mkdtempSync(join(tmpdir(), "tsumugi-e2e-miauth-bridge-"));

  const context: BrowserContext = await chromium.launchPersistentContext(userDataDir, {
    ignoreHTTPSErrors: true,
    args: [
      `--remote-debugging-port=${CDP_PORT}`,
      // このブリッジ専用のChromiumインスタンスに限定した措置。
      // e2eサンドボックスには /etc/hosts への misskey.local エントリが無いため、
      // このChromiumだけがmisskey.localを127.0.0.1へ解決できるようにする。
      "--host-resolver-rules=MAP misskey.local 127.0.0.1",
    ],
  });

  const signinRes = await fetch(`${MISSKEY_URL}/api/signin-flow`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ username: seeded.username, password: seeded.password }),
  });
  if (!signinRes.ok) {
    throw new Error(`miauthBridge: /api/signin-flow failed with ${signinRes.status}`);
  }
  const signinBody = (await signinRes.json()) as { finished?: boolean; i?: string };
  if (!signinBody.finished || !signinBody.i) {
    throw new Error(
      `miauthBridge: /api/signin-flow did not complete in one step (got ${JSON.stringify(signinBody)}); ` +
        "2FA/captcha may be enabled on this instance, which this bridge does not handle",
    );
  }
  const token = (signinBody as SigninFlowFinished).i;

  const meRes = await fetch(`${MISSKEY_URL}/api/i`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ i: token }),
  });
  if (!meRes.ok) {
    throw new Error(`miauthBridge: /api/i failed with ${meRes.status}`);
  }
  const me = await meRes.json();

  // packages/frontend/src/i.ts reads this synchronously at module-load time
  // (before any app code runs), so it must land in localStorage before the
  // page's own scripts execute — addInitScript() runs on every subsequent
  // document in this context, exactly what's needed here.
  const accountJson = JSON.stringify({ ...me, token });
  await context.addInitScript(
    (value: string) => {
      window.localStorage.setItem("account", value);
    },
    accountJson,
  );

  return {
    cdpPort: CDP_PORT,
    async approveNext() {
      const page = await context.waitForEvent("page", { timeout: 30000 });
      await page.waitForLoadState("domcontentloaded");
      await page.getByRole("button", { name: "許可" }).click();
    },
    async teardown() {
      await context.close();
    },
  };
}
