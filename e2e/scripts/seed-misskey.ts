// テスト用Misskeyインスタンスに管理者アカウントを1件だけ作成する。
// 既に作成済み(2回目以降の実行)は admin/accounts/create が
// ACCESS_DENIED を返すため、その場合は成功とみなしスキップする。
import { writeFileSync } from "node:fs";
import { join } from "node:path";

const BASE_URL = process.env.E2E_MISSKEY_URL ?? "https://misskey.local:8443";
const USERNAME = "e2etestadmin";
const PASSWORD = "e2e-test-user-password";
const SETUP_PASSWORD = "e2e-test-setup-password";
const OUT_FILE = join(__dirname, "..", "certs", "seeded-account.json");

async function main() {
  const res = await fetch(`${BASE_URL}/api/admin/accounts/create`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      username: USERNAME,
      password: PASSWORD,
      setupPassword: SETUP_PASSWORD,
    }),
  });

  if (res.ok) {
    const body = (await res.json()) as { token: string };
    writeFileSync(
      OUT_FILE,
      JSON.stringify({ username: USERNAME, password: PASSWORD, token: body.token }, null, 2),
    );
    console.log(`seed-misskey: created admin account, token written to ${OUT_FILE}`);
    return;
  }

  const errBody = (await res.json()) as { error?: { code?: string } };
  if (errBody.error?.code === "ACCESS_DENIED") {
    console.log("seed-misskey: instance already set up, skipping (idempotent)");
    return;
  }

  throw new Error(`seed-misskey: unexpected response ${res.status}: ${JSON.stringify(errBody)}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
