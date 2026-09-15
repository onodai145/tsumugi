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

/**
 * `/api/signup` で新規の一般ユーザーを作成し、アクセストークンを返す。
 * `seed-misskey.ts` が使う `/api/admin/accounts/create`(管理者作成)とは別の、
 * 管理者権限不要のセルフサインアップ経路。複数アカウントを扱うE2Eシナリオの
 * 2人目以降のユーザー作成に使う。
 *
 * 実機確認済み: レスポンスは`/api/i`同様のユーザーオブジェクト全体で、その中の
 * `token`フィールドがアクセストークン(`signin-flow`の`i`とは別名)。また、この
 * Misskeyインスタンスは`disableRegistration: true`がデフォルトのため、呼び出し側は
 * 事前に`admin/update-meta`で`disableRegistration: false`にしておく必要がある
 * (`seed-misskey.ts`はこれを行っていない)。
 */
export async function signUp(username: string, password: string): Promise<{ token: string }> {
  const res = await fetch(`${BASE_URL}/api/signup`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ username, password }),
  });
  if (!res.ok) {
    throw new Error(`signUp: signup failed ${res.status}: ${await res.text()}`);
  }
  const body = (await res.json()) as { token: string };
  return { token: body.token };
}

/** `notes/create` に `renoteId` を渡してリノートし、作成されたリノートのidを返す。 */
export async function renoteNote(token: string, noteId: string): Promise<string> {
  const res = await fetch(`${BASE_URL}/api/notes/create`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ i: token, renoteId: noteId }),
  });
  if (!res.ok) {
    throw new Error(`renoteNote: notes/create(renote) failed ${res.status}: ${await res.text()}`);
  }
  const body = (await res.json()) as { createdNote: { id: string } };
  return body.createdNote.id;
}

/** `notes/delete` でノートを削除する。 */
export async function deleteNote(token: string, noteId: string): Promise<void> {
  const res = await fetch(`${BASE_URL}/api/notes/delete`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ i: token, noteId }),
  });
  if (!res.ok) {
    throw new Error(`deleteNote: notes/delete failed ${res.status}: ${await res.text()}`);
  }
}
