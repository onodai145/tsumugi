# E2E操作テスト自動化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ログイン(MiAuth)→投稿→リアクションの一連の操作を、tauri-driver(WebdriverIO)でtsumugiの実バイナリを操作し、使い捨てのDocker Misskeyインスタンスに対して自動テストできるようにする。

**Architecture:** `e2e/`（新規pnpmパッケージ）に、テスト対象アプリを操作するWebdriverIOスペック、Docker ComposeのMisskeyスタック（Traefik + Misskey + PostgreSQL + Redis）、MiAuthのブラウザ同意画面をPlaywright(CDP)でブリッジするヘルパーをまとめる。本番の実行バイナリ・実キーリング・実設定ディレクトリには一切触れず、`XDG_CONFIG_HOME`/`XDG_CACHE_HOME`と`dbus-run-session`による一時Secret Serviceでプロセス単位に分離する。

**Tech Stack:** WebdriverIO + `@wdio/tauri-service`（tauri-driverを内部で起動）、Playwright（`chromium.connectOverCDP`によるMiAuthブリッジ専用）、Docker Compose（Traefik v3 + `misskey/misskey:2026.7.0` + postgres:18-alpine + redis:7-alpine）。

## Global Constraints

- `cargo tauri dev` は使わない。テスト対象バイナリは `cargo tauri build --debug` で作る（`frontendDist`埋め込み、vite devサーバー不要）。CLAUDE.md記載の既知の罠。
- `session/miauth.rs` / `api/client.rs` は `https://{host}` をハードコードしている。テスト用Misskeyは必ずhttps経由で提供する。
- `session/secrets.rs` の `KeyringStore` は本番コードで常時使用されており、切り替え機構は無い。本番コード変更はしない。プロセス起動方法（`dbus-run-session` + 一時DBusセッション）でのみ分離する。
- CA証明書は使い捨て生成せず `e2e/certs/`（`.gitignore`対象）に永続化し、信頼ストアへの登録は冪等にする（フィンガープリント/ファイル名で既登録チェック）。
- リバースプロキシはTraefik（Caddyより優先、ユーザーの選好）。
- MiAuthのブラウザ同意フォーム入力は自動化しない。`/api/signin`で取得したセッションCookieを注入し、「許可」ボタンのクリックのみをCDPで自動化する。
- 初回テストシナリオは「アカウント追加→投稿→自分の投稿にリアクション」の1本の連続フロー。

---

## ファイル構成

```
e2e/
├─ package.json
├─ tsconfig.json
├─ .gitignore                     # certs/, node_modules/, wdio-logs/ 等
├─ wdio.conf.ts                   # WebdriverIO + @wdio/tauri-service 設定
├─ docker-compose.yml             # Traefik + Misskey + PostgreSQL + Redis
├─ traefik/
│   └─ dynamic.yml                # Traefikの動的設定（TLS証明書パス指定）
├─ misskey-config/
│   └─ default.yml                # Misskeyの設定ファイル（url/db/redis/setupPassword）
├─ scripts/
│   ├─ gen-ca.sh                  # CA/サーバー証明書の初回生成（冪等）
│   ├─ install-ca.sh              # OS信頼ストアへの登録（冪等）
│   ├─ seed-misskey.ts            # /admin/accounts/create でテスト管理者作成
│   └─ run-app.sh                 # dbus-run-session + XDG_* + tauri-driver起動ラッパー
├─ helpers/
│   ├─ browser-open.sh            # BROWSER環境変数で差し替える起動スクリプト
│   └─ miauthBridge.ts            # Playwright CDP接続・Cookie注入・許可ボタンクリック
└─ specs/
    └─ account-post-reaction.e2e.ts
```

---

### Task 1: tauri-driver スパイク（go/no-go判断）

このタスクはE2E基盤全体の前提が成立するかを確認する最小検証。CI組み込みや他タスクより先に実施し、ここで実用に耐えないと分かった場合は速やかにユーザーへ報告して方針を相談する（設計スペックのリスク節参照）。

**Files:**
- Create: `e2e/package.json`
- Create: `e2e/tsconfig.json`
- Create: `e2e/wdio.conf.ts`
- Create: `e2e/.gitignore`
- Test: `e2e/specs/spike-window-title.e2e.ts`

**Interfaces:**
- Produces: `e2e/wdio.conf.ts` の `config`（後続タスクのspecはこのファイルの設定をそのまま使う）

- [ ] **Step 1: WebKitWebDriverの前提を確認する**

```bash
which WebKitWebDriver || (sudo apt-get update && sudo apt-get install -y webkit2gtk-driver)
```

Expected: `WebKitWebDriver` のパスが出力される。

- [ ] **Step 2: `e2e/` パッケージを初期化する**

```bash
mkdir -p /home/onodai145/repos/github.com/onodai145/tsumugi/e2e/specs
cd /home/onodai145/repos/github.com/onodai145/tsumugi/e2e
pnpm init
pnpm add -D @wdio/cli @wdio/local-runner @wdio/mocha-framework @wdio/spec-reporter @wdio/tauri-service typescript ts-node @types/node
```

- [ ] **Step 3: `e2e/tsconfig.json` を作成する**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "commonjs",
    "moduleResolution": "node",
    "esModuleInterop": true,
    "strict": true,
    "skipLibCheck": true,
    "types": ["node", "@wdio/mocha-framework"]
  },
  "include": ["wdio.conf.ts", "specs/**/*.ts", "helpers/**/*.ts", "scripts/**/*.ts"]
}
```

- [ ] **Step 4: `e2e/.gitignore` を作成する**

```
node_modules/
certs/
wdio-logs/
misskey-data/
```

- [ ] **Step 5: `e2e/wdio.conf.ts` を作成する**

```typescript
import type { Options } from "@wdio/types";

export const config: Options.Testrunner = {
  runner: "local",
  specs: ["./specs/**/*.e2e.ts"],
  maxInstances: 1,
  services: ["tauri"],
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application: "../src-tauri/target/debug/tsumugi",
      },
    } as WebdriverIO.Capabilities,
  ],
  logLevel: "info",
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 60000,
  },
};
```

- [ ] **Step 6: デバッグビルドを作成する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm build
cd /home/onodai145/repos/github.com/onodai145/tsumugi && cargo tauri build --debug
```

Expected: `src-tauri/target/debug/tsumugi` が生成される。

- [ ] **Step 7: スパイク用スペックを書く**

```typescript
// e2e/specs/spike-window-title.e2e.ts
describe("tauri-driver spike", () => {
  it("gets the window title", async () => {
    const title = await browser.getTitle();
    expect(title).toBeDefined();
    expect(title.length).toBeGreaterThan(0);
  });
});
```

- [ ] **Step 8: `xvfb-run` 配下で実行する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/e2e
xvfb-run -a pnpm wdio run wdio.conf.ts
```

Expected: `1 passing` でテストが通る。

**Go/No-Go判断:** ここで失敗する場合（`WebKitWebDriver`が接続できない、`@wdio/tauri-service`がバイナリを起動できない等）は、以降のタスクに進まずユーザーに報告し、CI組み込みを別issueへ切り出すかどうかを相談する。

- [ ] **Step 9: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add e2e/package.json e2e/tsconfig.json e2e/wdio.conf.ts e2e/.gitignore e2e/specs/spike-window-title.e2e.ts e2e/pnpm-lock.yaml
git commit -m "test: tauri-driverスパイクでE2E基盤の疎通を確認"
```

---

### Task 2: Traefik CA証明書の生成・登録スクリプト（冪等）

**Files:**
- Create: `e2e/scripts/gen-ca.sh`
- Create: `e2e/scripts/install-ca.sh`
- Test: 手動実行での確認（シェルスクリプトのため単体テストフレームワークは使わない）

**Interfaces:**
- Produces: `e2e/certs/ca.pem`（CA証明書）、`e2e/certs/misskey.local.pem` / `e2e/certs/misskey.local-key.pem`（サーバー証明書・秘密鍵）。Task 3のTraefik設定がこれらのパスを参照する。

- [ ] **Step 1: `e2e/scripts/gen-ca.sh` を作成する**

```bash
#!/usr/bin/env bash
# CA・サーバー証明書を初回のみ生成する。既に存在する場合は何もしない（冪等）。
set -euo pipefail
CERT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/certs"
mkdir -p "$CERT_DIR"

if [[ -f "$CERT_DIR/ca.pem" ]]; then
  echo "gen-ca: already exists, skip: $CERT_DIR/ca.pem"
  exit 0
fi

openssl genrsa -out "$CERT_DIR/ca-key.pem" 4096
openssl req -x509 -new -nodes -key "$CERT_DIR/ca-key.pem" -sha256 -days 3650 \
  -subj "/CN=tsumugi-e2e-test-CA" -out "$CERT_DIR/ca.pem"

openssl genrsa -out "$CERT_DIR/misskey.local-key.pem" 2048
openssl req -new -key "$CERT_DIR/misskey.local-key.pem" \
  -subj "/CN=misskey.local" -out "$CERT_DIR/misskey.local.csr"

cat > "$CERT_DIR/misskey.local.ext" <<EOF
subjectAltName = DNS:misskey.local
EOF

openssl x509 -req -in "$CERT_DIR/misskey.local.csr" \
  -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca-key.pem" -CAcreateserial \
  -out "$CERT_DIR/misskey.local.pem" -days 3650 -sha256 \
  -extfile "$CERT_DIR/misskey.local.ext"

rm -f "$CERT_DIR/misskey.local.csr" "$CERT_DIR/misskey.local.ext"
echo "gen-ca: generated $CERT_DIR/ca.pem and $CERT_DIR/misskey.local.pem"
```

- [ ] **Step 2: `e2e/scripts/install-ca.sh` を作成する**

```bash
#!/usr/bin/env bash
# 生成済みCAをOS信頼ストアへ登録する。既に登録済みならスキップする（冪等）。
set -euo pipefail
CERT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/certs"
CA_FILE="$CERT_DIR/ca.pem"
DEST="/usr/local/share/ca-certificates/tsumugi-e2e-test-ca.crt"

if [[ ! -f "$CA_FILE" ]]; then
  echo "install-ca: $CA_FILE not found. Run gen-ca.sh first." >&2
  exit 1
fi

NEW_FP="$(openssl x509 -in "$CA_FILE" -noout -fingerprint -sha256)"
if [[ -f "$DEST" ]]; then
  EXISTING_FP="$(openssl x509 -in "$DEST" -noout -fingerprint -sha256 2>/dev/null || true)"
  if [[ "$NEW_FP" == "$EXISTING_FP" ]]; then
    echo "install-ca: already installed with matching fingerprint, skip"
    exit 0
  fi
fi

sudo cp "$CA_FILE" "$DEST"
sudo update-ca-certificates
echo "install-ca: installed $DEST"
```

- [ ] **Step 3: 実行権限を付与し、動作確認する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/e2e
chmod +x scripts/gen-ca.sh scripts/install-ca.sh
./scripts/gen-ca.sh
./scripts/install-ca.sh
./scripts/gen-ca.sh   # 2回目は何もしないことを確認
./scripts/install-ca.sh   # 2回目は「already installed」と出て終わることを確認
```

Expected: 2回目実行時にそれぞれ `already exists, skip` / `already installed with matching fingerprint, skip` と出力される。

- [ ] **Step 4: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add e2e/scripts/gen-ca.sh e2e/scripts/install-ca.sh
git commit -m "test: E2E用CA証明書の生成・登録スクリプトを追加"
```

---

### Task 3: Docker Compose Misskeyスタック

**Files:**
- Create: `e2e/docker-compose.yml`
- Create: `e2e/traefik/dynamic.yml`
- Create: `e2e/misskey-config/default.yml`

**Interfaces:**
- Consumes: `e2e/certs/ca.pem`, `e2e/certs/misskey.local.pem`, `e2e/certs/misskey.local-key.pem`（Task 2で生成済み）
- Produces: `https://misskey.local:8443` でリッスンするMisskeyインスタンス。`setupPassword: "e2e-test-setup-password"`（Task 4の seedスクリプトが使う）

- [ ] **Step 1: `e2e/misskey-config/default.yml` を作成する**

```yaml
url: https://misskey.local:8443/
port: 3000

db:
  host: db
  port: 5432
  db: misskey
  user: misskey
  pass: misskey

redis:
  host: redis
  port: 6379

setupPassword: "e2e-test-setup-password"

id: "aidx"
```

- [ ] **Step 2: `e2e/traefik/dynamic.yml` を作成する**

```yaml
tls:
  certificates:
    - certFile: /certs/misskey.local.pem
      keyFile: /certs/misskey.local-key.pem
```

- [ ] **Step 3: `e2e/docker-compose.yml` を作成する**

```yaml
services:
  traefik:
    image: traefik:v3.3
    command:
      - --providers.docker=true
      - --providers.docker.exposedbydefault=false
      - --providers.file.filename=/etc/traefik/dynamic.yml
      - --entrypoints.websecure.address=:443
    ports:
      - "8443:443"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - ./traefik/dynamic.yml:/etc/traefik/dynamic.yml:ro
      - ./certs:/certs:ro
    depends_on:
      web:
        condition: service_healthy

  web:
    image: misskey/misskey:2026.7.0
    restart: "no"
    environment:
      NODE_ENV: production
    volumes:
      - ./misskey-config/default.yml:/misskey/.config/default.yml:ro
      - misskey_files:/misskey/files
    depends_on:
      db:
        condition: service_healthy
      redis:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "wget", "-qO-", "http://localhost:3000/api/meta", "--post-data={}", "--header=Content-Type: application/json"]
      interval: 5s
      timeout: 5s
      retries: 40
    labels:
      - traefik.enable=true
      - traefik.http.routers.misskey.rule=Host(`misskey.local`)
      - traefik.http.routers.misskey.entrypoints=websecure
      - traefik.http.routers.misskey.tls=true
      - traefik.http.services.misskey.loadbalancer.server.port=3000

  db:
    image: postgres:18-alpine
    environment:
      POSTGRES_DB: misskey
      POSTGRES_USER: misskey
      POSTGRES_PASSWORD: misskey
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U misskey"]
      interval: 5s
      timeout: 5s
      retries: 20

  redis:
    image: redis:7-alpine
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 5s
      timeout: 5s
      retries: 20

volumes:
  misskey_files:
```

- [ ] **Step 4: `/etc/hosts` に `misskey.local` を解決させる（ローカル実行のみ）**

```bash
grep -q "misskey.local" /etc/hosts || echo "127.0.0.1 misskey.local" | sudo tee -a /etc/hosts
```

- [ ] **Step 5: スタックを起動して疎通確認する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/e2e
./scripts/gen-ca.sh
./scripts/install-ca.sh
docker compose up -d --wait
curl -sf https://misskey.local:8443/api/meta -X POST -H "Content-Type: application/json" -d '{}'
```

Expected: JSONレスポンス（Misskeyのmeta情報）が返る。TLSエラーが出ないこと（`install-ca.sh`実行後であること）。

- [ ] **Step 6: スタックを停止する**

```bash
docker compose down
```

- [ ] **Step 7: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add e2e/docker-compose.yml e2e/traefik/dynamic.yml e2e/misskey-config/default.yml
git commit -m "test: E2E用Docker Compose Misskeyスタックを追加"
```

---

### Task 4: Misskey seedスクリプト（テスト管理者アカウント作成）

**Files:**
- Create: `e2e/scripts/seed-misskey.ts`

**Interfaces:**
- Consumes: `https://misskey.local:8443`（Task 3のスタック）
- Produces: テスト管理者アカウント（`username: "e2etestadmin"`, `password: "e2e-test-user-password"`）と、そのアクセストークン。トークンは標準出力とファイル（`e2e/certs/seeded-account.json`、`.gitignore`対象に追加）の両方に出力する。Task 6の`miauthBridge.ts`がこのファイルを読む。

- [ ] **Step 1: `.gitignore` に seed 出力ファイルを追加する**

```
e2e/.gitignore に certs/ が既に含まれているため、出力先を certs/ 配下にすることで追加設定不要。
```

（このステップは確認のみ。`certs/` は既にTask 1で `.gitignore` 済み。）

- [ ] **Step 2: `e2e/scripts/seed-misskey.ts` を作成する**

```typescript
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
```

- [ ] **Step 3: `e2e/package.json` に実行スクリプトを追加する**

```json
{
  "scripts": {
    "seed": "ts-node scripts/seed-misskey.ts"
  }
}
```

- [ ] **Step 4: Misskeyスタックを起動してseedを実行し、動作確認する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/e2e
docker compose up -d --wait
pnpm seed
cat certs/seeded-account.json
pnpm seed   # 2回目実行、"already set up, skipping" になることを確認
docker compose down
```

Expected: 初回は `certs/seeded-account.json` に `{username, password, token}` が書き出される。2回目は「skipping」で正常終了する。

- [ ] **Step 5: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add e2e/scripts/seed-misskey.ts e2e/package.json
git commit -m "test: E2Eテスト管理者アカウントのseedスクリプトを追加"
```

---

### Task 5: アプリ起動ラッパースクリプト（本番環境からの分離）

**Files:**
- Create: `e2e/scripts/run-app.sh`
- Modify: `e2e/wdio.conf.ts`

**Interfaces:**
- Consumes: `src-tauri/target/debug/tsumugi`（Task 1のデバッグビルド）
- Produces: `dbus-run-session` + 一時 `XDG_CONFIG_HOME`/`XDG_CACHE_HOME` 配下でアプリを起動するコマンド。`@wdio/tauri-service`の`application`にこのラッパーを指定する。

- [ ] **Step 1: `e2e/scripts/run-app.sh` を作成する**

```bash
#!/usr/bin/env bash
# tsumugi本体を、本番の設定ディレクトリ・OS keyringから完全に分離した状態で起動する。
# 引数はそのままtsumugiバイナリへ渡す（tauri-driver/wdio-tauri-serviceが付与する引数を透過）。
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BINARY="$REPO_ROOT/src-tauri/target/debug/tsumugi"

TMP_HOME="$(mktemp -d /tmp/tsumugi-e2e-XXXXXX)"
export XDG_CONFIG_HOME="$TMP_HOME/config"
export XDG_CACHE_HOME="$TMP_HOME/cache"
export XDG_DATA_HOME="$TMP_HOME/data"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME"

export BROWSER="$REPO_ROOT/e2e/helpers/browser-open.sh"

exec dbus-run-session -- "$BINARY" "$@"
```

- [ ] **Step 2: `e2e/helpers/browser-open.sh` のプレースホルダを作成する（Task 6で実装を埋める）**

```bash
#!/usr/bin/env bash
# Task 6で実装する。現時点ではエラーで止め、未実装のまま呼ばれたことが分かるようにする。
echo "browser-open.sh: not implemented yet (Task 6)" >&2
exit 1
```

- [ ] **Step 3: 実行権限を付与する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/e2e
chmod +x scripts/run-app.sh helpers/browser-open.sh
```

- [ ] **Step 4: `e2e/wdio.conf.ts` の `application` をラッパー経由に変更する**

```typescript
// 変更前:
//   application: "../src-tauri/target/debug/tsumugi",
// 変更後:
      "tauri:options": {
        application: "./scripts/run-app.sh",
      },
```

- [ ] **Step 5: Task 1のスパイクを再実行し、分離環境でも動くことを確認する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/e2e
xvfb-run -a pnpm wdio run wdio.conf.ts
```

Expected: `1 passing`（Task 1のスパイクspecがラッパー経由でも通る）。

- [ ] **Step 6: 起動後、実際の`XDG_CONFIG_HOME`配下に設定ファイルが作られ、本来のユーザー設定ディレクトリ（`~/.config/tsumugi` 等）が変化していないことを目視確認する**

```bash
ls /tmp/tsumugi-e2e-*/config
```

Expected: tsumugiの設定ファイル（`settings.json`等）が一時ディレクトリ配下に作成されている。

- [ ] **Step 7: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add e2e/scripts/run-app.sh e2e/helpers/browser-open.sh e2e/wdio.conf.ts
git commit -m "test: E2E実行時にアプリを本番環境から分離するラッパーを追加"
```

---

### Task 6: MiAuthブリッジ（Playwright CDP）

**Files:**
- Create: `e2e/helpers/miauthBridge.ts`
- Modify: `e2e/helpers/browser-open.sh`
- Modify: `e2e/package.json`（Playwright依存追加）

**Interfaces:**
- Consumes: `e2e/certs/seeded-account.json`（Task 4）の `{username, password}`
- Produces: `startMiauthBridge(): Promise<MiauthBridge>` — `{ cdpPort: number, teardown(): Promise<void> }`。`browser-open.sh`が起動時にこのブリッジのCDPポートへ新規タブを開かせる。Task 7のspecが`startMiauthBridge`/`teardown`を呼ぶ。

- [ ] **Step 1: Playwrightを依存に追加する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/e2e
pnpm add -D playwright
pnpm exec playwright install chromium
```

- [ ] **Step 2: `e2e/helpers/miauthBridge.ts` を作成する**

```typescript
import { chromium, type Browser, type BrowserContext } from "playwright";
import { readFileSync } from "node:fs";
import { join } from "node:path";

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
 */
export async function startMiauthBridge(): Promise<MiauthBridge> {
  const seeded: SeededAccount = JSON.parse(
    readFileSync(join(__dirname, "..", "certs", "seeded-account.json"), "utf-8"),
  );

  const browser: Browser = await chromium.launch({
    args: [`--remote-debugging-port=${CDP_PORT}`],
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
```

- [ ] **Step 3: `e2e/helpers/browser-open.sh` を実装する**

```bash
#!/usr/bin/env bash
# opener プラグインが呼ぶ「デフォルトブラウザ起動コマンド」の差し替え先。
# 実際にはブラウザを新規起動せず、miauthBridge.ts が立てたCDPセッションに
# 新規タブとしてURLを開かせる。
set -euo pipefail
URL="$1"
CDP_PORT="${E2E_MIAUTH_CDP_PORT:-9333}"
curl -sf "http://localhost:${CDP_PORT}/json/new?${URL}" > /dev/null
```

- [ ] **Step 4: `run-app.sh` から `E2E_MIAUTH_CDP_PORT` を透過させる**

`e2e/scripts/run-app.sh` の `export BROWSER=...` の下に以下を追記する:

```bash
export E2E_MIAUTH_CDP_PORT="${E2E_MIAUTH_CDP_PORT:-9333}"
```

- [ ] **Step 5: 単体で疎通確認する（Misskeyスタックが起動している状態で）**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/e2e
docker compose up -d --wait
pnpm seed
cat <<'EOF' > /tmp/bridge-smoke-test.ts
import { startMiauthBridge } from "./helpers/miauthBridge";
(async () => {
  const bridge = await startMiauthBridge();
  console.log("bridge started on port", bridge.cdpPort);
  await bridge.teardown();
})();
EOF
pnpm exec ts-node /tmp/bridge-smoke-test.ts
```

Expected: `bridge started on port 9333` が出力され、エラーなく終了する。

- [ ] **Step 6: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add e2e/helpers/miauthBridge.ts e2e/helpers/browser-open.sh e2e/scripts/run-app.sh e2e/package.json e2e/pnpm-lock.yaml
git commit -m "test: MiAuth同意画面のPlaywright CDPブリッジを追加"
```

---

### Task 7: E2Eシナリオ実装（アカウント追加→投稿→リアクション）

**Files:**
- Create: `e2e/specs/account-post-reaction.e2e.ts`
- Delete: `e2e/specs/spike-window-title.e2e.ts`（役目を終えたため削除。Task 1の疎通確認は本テストに統合される）

**Interfaces:**
- Consumes: `startMiauthBridge`/`MiauthBridge`（Task 6）、`e2e/certs/seeded-account.json`（Task 4）

- [ ] **Step 1: `e2e/specs/account-post-reaction.e2e.ts` を作成する**

```typescript
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";

const MISSKEY_HOST = "misskey.local:8443";

describe("account → post → reaction", () => {
  let bridge: MiauthBridge;

  before(async () => {
    bridge = await startMiauthBridge();
  });

  after(async () => {
    await bridge.teardown();
  });

  it("adds an account via MiAuth", async () => {
    const addAccountButton = await $('button[aria-label="アカウントを追加"]');
    await addAccountButton.waitForDisplayed({ timeout: 15000 });
    await addAccountButton.click();

    const hostInput = await $('input[name="host"]');
    await hostInput.waitForDisplayed({ timeout: 15000 });
    await hostInput.setValue(MISSKEY_HOST);

    const startButton = await $('button[type="submit"]');
    await startButton.click();

    await bridge.approveNext();

    const completeButton = await $('button=認証完了');
    await completeButton.waitForDisplayed({ timeout: 15000 });
    await completeButton.click();

    const accountLabel = await $(`text/@e2etestadmin@${MISSKEY_HOST}`);
    await accountLabel.waitForDisplayed({ timeout: 15000 });
  });

  it("posts a note", async () => {
    const composeButton = await $('button[aria-label="投稿"]');
    await composeButton.waitForDisplayed({ timeout: 15000 });
    await composeButton.click();

    const textarea = await $('textarea[data-testid="compose-textarea"]');
    await textarea.waitForDisplayed({ timeout: 15000 });
    const noteText = `tsumugi e2e test note ${Date.now()}`;
    await textarea.setValue(noteText);

    const submitButton = await $('button[data-testid="compose-submit"]');
    await submitButton.click();

    const postedNote = await $(`text/${noteText}`);
    await postedNote.waitForDisplayed({ timeout: 15000 });
  });

  it("reacts to its own note", async () => {
    const noteReactionButton = await $('button[aria-label="リアクション"]');
    await noteReactionButton.waitForDisplayed({ timeout: 15000 });
    await noteReactionButton.click();

    const thumbsUp = await $('button[data-emoji="👍"]');
    await thumbsUp.waitForDisplayed({ timeout: 15000 });
    await thumbsUp.click();

    const reactionCount = await $('[data-testid="reaction-count-👍"]');
    await reactionCount.waitForDisplayed({ timeout: 15000 });
    await expect(reactionCount).toHaveText("1");
  });
});
```

**注記:** ここで使うセレクタ（`aria-label`, `data-testid`等）は現行UIの実装と合わせて実装時に調整する。存在しない`data-testid`がある場合は、対象コンポーネントに追加する（フロントエンドの表示に影響しない属性追加のみ）。

- [ ] **Step 2: `e2e/package.json` にE2E実行の一括スクリプトを追加する**

```json
{
  "scripts": {
    "seed": "ts-node scripts/seed-misskey.ts",
    "e2e": "wdio run wdio.conf.ts"
  }
}
```

- [ ] **Step 3: フルフローを手元で実行する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/e2e
./scripts/gen-ca.sh
./scripts/install-ca.sh
docker compose up -d --wait
pnpm seed
xvfb-run -a pnpm e2e
docker compose down
```

Expected: 3つのitがすべてpassする。失敗する場合はセレクタの不一致が最も疑わしいので、実際のUIのDOM（`browser.getPageSource()`や手動の`cargo tauri dev`起動での確認）に合わせて `e2e/specs/account-post-reaction.e2e.ts` のセレクタを調整する。

- [ ] **Step 4: 古いスパイクspecを削除する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
rm e2e/specs/spike-window-title.e2e.ts
```

- [ ] **Step 5: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add e2e/specs/account-post-reaction.e2e.ts e2e/specs/spike-window-title.e2e.ts e2e/package.json
git commit -m "test: アカウント追加→投稿→リアクションのE2Eシナリオを実装"
```

---

### Task 8: CIジョブ追加

**Files:**
- Modify: `.github/workflows/test.yml`

**Interfaces:**
- Consumes: Task 1〜7の全成果物

- [ ] **Step 1: `.github/workflows/test.yml` の `android-build` ジョブの直前に `e2e` ジョブを追加する**

```yaml
  e2e:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4

      - name: Install system dependencies (Tauri v2 Linux + WebKitWebDriver + Xvfb)
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libwebkit2gtk-4.1-dev \
            libjavascriptcoregtk-4.1-dev \
            libsoup-3.0-dev \
            libgtk-3-dev \
            libayatana-appindicator3-dev \
            librsvg2-dev \
            patchelf \
            webkit2gtk-driver \
            xvfb \
            dbus-x11

      - uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@42dc69e1aa15d09112580998cf2ef0119e2e91ae # v2
        with:
          workspaces: src-tauri

      - uses: pnpm/action-setup@f40ffcd9367d9f12939873eb1018b921a783ffaa # v4
        with:
          version: 11.3.0

      - uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4
        with:
          node-version: 22
          cache: pnpm
          cache-dependency-path: |
            frontend/pnpm-lock.yaml
            e2e/pnpm-lock.yaml

      - name: pnpm install (frontend)
        working-directory: frontend
        run: pnpm install --frozen-lockfile

      - name: pnpm build (frontendDist生成)
        working-directory: frontend
        run: pnpm build

      - name: cargo tauri build --debug
        run: cargo tauri build --debug

      - name: pnpm install (e2e)
        working-directory: e2e
        run: pnpm install --frozen-lockfile

      - name: Install Playwright browsers
        working-directory: e2e
        run: pnpm exec playwright install --with-deps chromium

      - name: Generate and install test CA
        working-directory: e2e
        run: |
          ./scripts/gen-ca.sh
          ./scripts/install-ca.sh
          echo "127.0.0.1 misskey.local" | sudo tee -a /etc/hosts

      - name: Start Misskey stack
        working-directory: e2e
        run: docker compose up -d --wait

      - name: Seed test account
        working-directory: e2e
        run: pnpm seed

      - name: Run E2E tests
        working-directory: e2e
        run: xvfb-run -a pnpm e2e

      - name: Upload E2E artifacts on failure
        if: failure()
        uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa9 # v4
        with:
          name: e2e-failure-artifacts
          path: |
            e2e/wdio-logs/
            e2e/certs/seeded-account.json

      - name: Stop Misskey stack
        if: always()
        working-directory: e2e
        run: docker compose down -v
```

- [ ] **Step 2: ローカルでYAML構文を確認する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/test.yml'))" && echo "YAML OK"
```

Expected: `YAML OK` が出力される。

- [ ] **Step 3: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add .github/workflows/test.yml
git commit -m "ci: E2EテストジョブをGitHub Actionsに追加"
```

**注記:** 初回のCI実行はユーザー自身がGitHub上で確認する（push後にMonitorで待たない方針）。CI実行が不安定な場合（Docker Compose起動タイムアウト、WebKitWebDriverのバージョン差異等）は、ログを見てヘルスチェックのretries/intervalやタイムアウト値を調整する。

---

## 完了条件

- [ ] `xvfb-run -a pnpm e2e`（`e2e/`ディレクトリ内）がローカルで3件すべてpassする
- [ ] `.github/workflows/test.yml` の `e2e` ジョブがCI上でグリーンになる（ユーザーが確認）
- [ ] 実行後、ユーザーの実キーリング・実`~/.config/tsumugi`に変化がないことを確認済み
- [ ] `e2e/certs/`（CA・秘密鍵・seedトークン含む）が`.gitignore`されている
