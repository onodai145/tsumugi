// テスト用MisskeyインスタンスへNode標準fetch()で直接REST APIを叩くための薄いヘルパー。
// miauthBridge.ts/seed-misskey.tsと同じ理由(e2eサンドボックスの/etc/hostsに
// misskey.localのエントリが無い)で、このファイル自身もdns.lookup()を
// プロセス内パッチしてmisskey.localを127.0.0.1へ固定解決する。
import dns from "node:dns";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const originalLookup = dns.lookup;
// @ts-expect-error - overload signatures make a single reassignment awkward; behavior is verified in miauthBridge.ts
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

const BASE_URL = process.env.E2E_MISSKEY_URL ?? "https://misskey.local:8443";

interface SeededAccount {
  username: string;
  password: string;
}

function readSeededAccount(): SeededAccount {
  return JSON.parse(readFileSync(join(__dirname, "..", "certs", "seeded-account.json"), "utf-8"));
}

// miauthBridge.tsのコメント参照: このMisskeyバージョンに/api/signinは無く、
// /api/signin-flowがCookie無しで{finished, id, i}を返す。
interface SigninFlowFinished {
  finished: true;
  id: string;
  i: string; // access token
}

/**
 * 種付けされたテストユーザー(certs/seeded-account.json)としてサインインし、
 * アクセストークンを取得する。tokenは初回作成時のみ書き込まれるファイルに
 * 依存しない(常にusername/passwordから毎回取り直す)。
 */
export async function signInAsSeededUser(): Promise<string> {
  const { username, password } = readSeededAccount();
  // このヘルパー自身の呼び出し(mutedWords設定前の下準備)と、直後に始まる
  // miauthBridge.tsの内部signin-flowが短時間に連続するため、Misskeyのサインイン
  // レート制限(429)に当たることがある(実機確認済み)。指数バックオフで数回だけ再試行する。
  const maxAttempts = 4;
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    const res = await fetch(`${BASE_URL}/api/signin-flow`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username, password }),
    });
    if (res.status === 429 && attempt < maxAttempts) {
      await new Promise((resolve) => setTimeout(resolve, attempt * 2000));
      continue;
    }
    if (!res.ok) {
      throw new Error(`signInAsSeededUser: signin-flow failed ${res.status}: ${await res.text()}`);
    }
    const body = (await res.json()) as SigninFlowFinished;
    if (!body.finished || !body.i) {
      throw new Error(`signInAsSeededUser: unexpected signin-flow response: ${JSON.stringify(body)}`);
    }
    return body.i;
  }
  throw new Error("signInAsSeededUser: unreachable");
}

/** `i/update` でサーバ側のワードミュート(`mutedWords`)を設定する。 */
export async function setMutedWords(token: string, mutedWords: (string | string[])[]): Promise<void> {
  const res = await fetch(`${BASE_URL}/api/i/update`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ i: token, mutedWords }),
  });
  if (!res.ok) {
    throw new Error(`setMutedWords: i/update failed ${res.status}: ${await res.text()}`);
  }
}

/** `notes/create` でノートを投稿し、投稿したノートのidを返す。 */
export async function createNote(token: string, text: string): Promise<string> {
  const res = await fetch(`${BASE_URL}/api/notes/create`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ i: token, text }),
  });
  if (!res.ok) {
    throw new Error(`createNote: notes/create failed ${res.status}: ${await res.text()}`);
  }
  const body = (await res.json()) as { createdNote: { id: string } };
  return body.createdNote.id;
}
