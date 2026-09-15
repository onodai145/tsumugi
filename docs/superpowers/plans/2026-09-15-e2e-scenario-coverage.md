# E2Eシナリオ網羅確認・拡充 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Issue #223のギャップを埋める5本の新規E2Eシナリオ(TQLフィルタ・複数アカウント・ストリーミングイベント・クライアント側ユーザーミュート・設定永続化)を追加し、そのために必要な共通インフラ(bridge/helper/フロントエンドtestid/起動スクリプト)を拡張する。

**Architecture:** 既存の`e2e/specs/*.e2e.ts`と同じ構成(wdio + tauri-driver + Playwright CDPブリッジ)を踏襲する。既存2本の`clickThroughAccountSelect`重複を`e2e/helpers/accountSelect.ts`へ抽出してから、それを土台に新規シナリオを積む。フロントエンドへの変更は既存2本と同じ方針(表示に影響しない`data-testid`/`data-*`追加のみ)。

**Tech Stack:** TypeScript(wdio/mocha, Playwright), Svelte 5, Rust(Tauri v2)、bash(`run-app.sh`)。

## Global Constraints

- フロントエンドへの変更は表示に影響しない追加のみ(既存の`data-testid`方針を踏襲)。
- 新規specファイルは`e2e/specs/**/*.e2e.ts`に置く(`wdio.conf.ts`の`specs`globにそのまま拾われる)。
- コミットメッセージは件名のみ(本文・箇条書き無し)。
- 各タスク実装前に、変更対象ファイルが属するfeatureブランチ(`feature/223-e2e-scenario-coverage`、既にチェックアウト済み)上で作業すること。mainには直接コミットしない。
- E2Eの実行手順は`e2e/README.md`のとおり(`gen-ca.sh` → `docker compose up` → `pnpm seed` → `xvfb-run -a pnpm e2e`)。各タスクの「テストを実行する」ステップはこの手順を前提にする。

---

## File Structure

| ファイル | 責務 |
|---|---|
| `e2e/helpers/accountSelect.ts`(新規) | MiAuthの「アカウントを選択してください」画面を通過させる共通ロジック(既存2本の重複を抽出) |
| `e2e/helpers/miauthBridge.ts`(変更) | `startMiauthBridge()`に任意の`{username, password}`引数を追加 |
| `e2e/helpers/misskeyApi.ts`(変更) | `signUp` / `renoteNote` / `deleteNote` を追加 |
| `e2e/scripts/run-app.sh`(変更) | `E2E_REUSE_HOME_FILE`環境変数によるHOME再利用サポート |
| `e2e/specs/account-post-reaction.e2e.ts`(変更) | 抽出した`accountSelect.ts`を使うようリファクタ |
| `e2e/specs/server-word-mute.e2e.ts`(変更) | 同上 |
| `e2e/specs/tql-column-filter.e2e.ts`(新規) | シナリオ1 |
| `e2e/specs/multi-account.e2e.ts`(新規) | シナリオ2 |
| `e2e/specs/streaming-events.e2e.ts`(新規) | シナリオ3 |
| `e2e/specs/client-user-mute.e2e.ts`(新規) | シナリオ4 |
| `e2e/specs/settings-persistence-restart.part1.e2e.ts`(新規) | シナリオ5前半 |
| `e2e/specs/settings-persistence-restart.part2.e2e.ts`(新規) | シナリオ5後半 |
| `frontend/src/input/TqlCompletionField.svelte`(変更) | `testid`propを追加しtextareaへ転送 |
| `frontend/src/ui/AddColumnModal.svelte`(変更) | 上記`testid`propを配線 |
| `frontend/src/ui/Settings.svelte`(変更) | タブボタンに`data-testid` |
| `frontend/src/ui/AppMenu.svelte`(変更) | 「設定」メニュー項目に`data-testid` |
| `frontend/src/ui/settings/MuteSection.svelte`(変更) | NGユーザー欄・保存ボタンに`data-testid` |
| `frontend/src/ui/Column.svelte`(変更) | カラムルートに`data-account-id`(既存の`data-group-id`と同じ流儀) |
| `frontend/src/ui/AccountSelect.svelte`(変更) | トリガー/各選択肢に`data-testid` |

---

### Task 1: `clickThroughAccountSelect`の共通化

**Files:**
- Create: `e2e/helpers/accountSelect.ts`
- Modify: `e2e/specs/account-post-reaction.e2e.ts`
- Modify: `e2e/specs/server-word-mute.e2e.ts`

**Interfaces:**
- Produces: `clickThroughAccountSelect(cdpPort: number, opts: { logTag: string; accountLabel: string }): Promise<void>` — 全ての新規specから使う。`logTag`はdebugLogのタグ接頭辞(例: `"tqlColumnFilter"`)、`accountLabel`はアカウント選択画面でクリックする行の部分一致文字列(既存2本は`"e2etestadmin"`)。

- [ ] **Step 1: `e2e/helpers/accountSelect.ts`を作成する**

既存2本(`account-post-reaction.e2e.ts`の`attachPageDiagnostics`/`dumpFailureArtifacts`/`clickThroughAccountSelect`、`server-word-mute.e2e.ts`の同名3関数)の内容をマージし、ハードコードされていた`"clickThroughAccountSelect"`ログタグと`"e2etestadmin"`を引数化する:

```typescript
// e2e/helpers/accountSelect.ts
//
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
```

- [ ] **Step 2: `account-post-reaction.e2e.ts`をリファクタする**

ファイル冒頭の`import { chromium, type Page } from "playwright";`と、`attachPageDiagnostics`/`dumpFailureArtifacts`/`clickThroughAccountSelect`の3関数定義(ファイル先頭〜`describe(...)`の直前まで)を削除し、代わりに以下を追加する:

```typescript
import { clickThroughAccountSelect } from "../helpers/accountSelect";
```

呼び出し箇所を:

```typescript
await Promise.all([
  bridge.approveNext(),
  clickThroughAccountSelect(bridge.cdpPort, { logTag: "accountPostReaction", accountLabel: "e2etestadmin" }),
  startButton.click(),
]);
```

に変更する。

- [ ] **Step 3: `server-word-mute.e2e.ts`を同様にリファクタする**

同じく重複していた3関数を削除し`import { clickThroughAccountSelect } from "../helpers/accountSelect";`に置き換え、呼び出しを:

```typescript
await Promise.all([
  bridge.approveNext(),
  clickThroughAccountSelect(bridge.cdpPort, { logTag: "serverWordMute", accountLabel: "e2etestadmin" }),
  startButton.click(),
]);
```

に変更する。

- [ ] **Step 4: `pnpm check`相当の型チェックを実行する**

Run: `cd e2e && pnpm exec tsc --noEmit -p tsconfig.json`
Expected: エラー無し(未使用importが残っていないこと・型が一致することを確認)。

- [ ] **Step 5: 既存2本のE2Eを実行し、リファクタで壊れていないことを確認する**

Run: `cd e2e && xvfb-run -a pnpm e2e`(`e2e/README.md`の前提手順を先に済ませておくこと)
Expected: 2 specとも全テストPASS。

- [ ] **Step 6: Commit**

```bash
git add e2e/helpers/accountSelect.ts e2e/specs/account-post-reaction.e2e.ts e2e/specs/server-word-mute.e2e.ts
git commit -m "test: MiAuthアカウント選択画面通過処理を共通ヘルパーに抽出"
```

---

### Task 2: `miauthBridge.ts`のアカウントパラメータ化

**Files:**
- Modify: `e2e/helpers/miauthBridge.ts`

**Interfaces:**
- Consumes: なし(Task 1と独立)
- Produces: `startMiauthBridge(credentials?: { username: string; password: string }): Promise<MiauthBridge>` — 省略時は現行どおり`certs/seeded-account.json`を読む。Task 7(複数アカウント)・Task 8(ストリーミング)・Task 9(クライアントミュート)が2人目ユーザーのブリッジ起動に使う。

- [ ] **Step 1: `startMiauthBridge`のシグネチャと本体を変更する**

`e2e/helpers/miauthBridge.ts`内の以下の箇所を変更する。まず関数シグネチャ:

```typescript
export async function startMiauthBridge(
  credentials?: { username: string; password: string },
): Promise<MiauthBridge> {
```

次に、ファイル冒頭で`seeded`を読んでいる箇所:

```typescript
  const seeded: SeededAccount = JSON.parse(
    readFileSync(join(__dirname, "..", "certs", "seeded-account.json"), "utf-8"),
  );
```

を:

```typescript
  const seeded: SeededAccount =
    credentials ??
    JSON.parse(readFileSync(join(__dirname, "..", "certs", "seeded-account.json"), "utf-8"));
```

に変更する(以降の`seeded.username`/`seeded.password`を参照する箇所は無変更のまま動く)。

- [ ] **Step 2: JSDocコメントに引数の説明を追記する**

`startMiauthBridge()`直前のJSDocコメント末尾に1行追加する:

```typescript
 * `credentials`を渡すと、シードアカウントではなくそのユーザーとしてMiAuthの
 * 同意画面を(既にサインイン済み扱いで)通過できる。複数アカウントを扱う
 * シナリオ(2人目以降のユーザー)向け。省略時は従来どおりシードアカウント。
 */
```

- [ ] **Step 3: 型チェックを実行する**

Run: `cd e2e && pnpm exec tsc --noEmit -p tsconfig.json`
Expected: エラー無し。

- [ ] **Step 4: 既存2本のE2Eを実行し、引数省略時の後方互換を確認する**

Run: `cd e2e && xvfb-run -a pnpm e2e`
Expected: 2 specとも全テストPASS(どちらも`startMiauthBridge()`を引数無しで呼んでいるため、変更前と同じ挙動になるはず)。

- [ ] **Step 5: Commit**

```bash
git add e2e/helpers/miauthBridge.ts
git commit -m "test: startMiauthBridgeに任意の認証情報を渡せるようにする"
```

---

### Task 3: `misskeyApi.ts`にsignUp/renoteNote/deleteNoteを追加

**Files:**
- Modify: `e2e/helpers/misskeyApi.ts`

**Interfaces:**
- Produces:
  - `signUp(username: string, password: string): Promise<{ token: string }>`
  - `renoteNote(token: string, noteId: string): Promise<string>`(作成されたリノートのnote idを返す)
  - `deleteNote(token: string, noteId: string): Promise<void>`

- [ ] **Step 1: `/api/signup`の実レスポンス形状を確認する**

`e2e/README.md`の手順で`docker compose up -d --wait`済みの状態で、実際に叩いて確認する:

```bash
curl -sk https://misskey.local:8443/api/signup \
  -H 'Content-Type: application/json' \
  -d '{"username":"e2etestuser2","password":"e2eTestPassword2!"}' | python3 -m json.tool
```

(`/etc/hosts`にmisskey.localが無い場合は`--resolve misskey.local:8443:127.0.0.1`を付ける。)
レスポンスに`token`フィールドが含まれるか、含まれない場合はどのフィールド名でアクセストークン相当の値が返るかをここで確認し、次のステップの実装をそれに合わせる。

- [ ] **Step 2: `signUp`を実装する**

Step 1で確認した実際のレスポンス形状に合わせて、`e2e/helpers/misskeyApi.ts`の`createNote`の直後に追加する(以下はレスポンスが`signin-flow`と同様`{..., token}`型である場合の実装。Step 1の結果がこれと異なった場合は`body.token`の参照先をStep 1で確認した実際のフィールド名に置き換える):

```typescript
/**
 * `/api/signup` で新規の一般ユーザーを作成し、アクセストークンを返す。
 * `seed-misskey.ts` が使う `/api/admin/accounts/create`(管理者作成)とは別の、
 * 管理者権限不要のセルフサインアップ経路。複数アカウントを扱うE2Eシナリオの
 * 2人目以降のユーザー作成に使う。
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
```

- [ ] **Step 3: `renoteNote`と`deleteNote`を実装する**

`createNote`の直後(または`signUp`の直後)に追加する:

```typescript
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
```

- [ ] **Step 4: 動作確認用の使い捨てスクリプトで疎通確認する**

`docker compose up -d --wait`済みの状態で、`e2e/`ディレクトリ直下に一時ファイル`/tmp/verify-misskey-api.ts`を作らず、Node REPLではなくTypeScriptの一時テストとして以下をその場で実行し、削除する:

```bash
cd e2e && pnpm exec tsx -e '
import { signUp, createNote, renoteNote, deleteNote } from "./helpers/misskeyApi";
(async () => {
  const { token } = await signUp("e2eapiverifyuser", "e2eTestPassword2!");
  const noteId = await createNote(token, "verify note");
  const renoteId = await renoteNote(token, noteId);
  console.log("renoted as", renoteId);
  await deleteNote(token, noteId);
  console.log("deleted", noteId);
})();
'
```

Expected: エラー無く`renoted as <id>` `deleted <id>`が出力される。

- [ ] **Step 5: 型チェックを実行する**

Run: `cd e2e && pnpm exec tsc --noEmit -p tsconfig.json`
Expected: エラー無し。

- [ ] **Step 6: Commit**

```bash
git add e2e/helpers/misskeyApi.ts
git commit -m "test: misskeyApiヘルパーにsignUp/renoteNote/deleteNoteを追加"
```

---

### Task 4: フロントエンドの`data-testid`/`data-account-id`追加

**Files:**
- Modify: `frontend/src/input/TqlCompletionField.svelte`
- Modify: `frontend/src/ui/AddColumnModal.svelte:404-414`
- Modify: `frontend/src/ui/Settings.svelte:60-69`
- Modify: `frontend/src/ui/AppMenu.svelte:88`
- Modify: `frontend/src/ui/settings/MuteSection.svelte:47,56`
- Modify: `frontend/src/ui/Column.svelte:361-368`
- Modify: `frontend/src/ui/AccountSelect.svelte`
- Test: `frontend`で`pnpm check`と既存のVitestを実行して既存動作を壊していないことを確認する(このタスク自体は表示に影響しない属性追加のみなので、新規ユニットテストは書かない)。

**Interfaces:**
- Produces(新規specが依存するDOM契約):
  - TQLテキストエリア: `[data-testid="add-column-tql-textarea"]`
  - 設定モーダルのタブ: `[data-testid="settings-tab-mute"]`
  - 設定を開くメニュー項目: `[data-testid="app-menu-open-settings"]`
  - NGユーザー欄: `[data-testid="mute-ng-users-textarea"]`、保存ボタン: `[data-testid="mute-save"]`
  - カラムルート: `[data-account-id="<accountId>"]`(既存の`data-group-id`と同じ流儀の生DOM属性。testidではない)
  - アカウント切り替えのトリガー: `[data-testid="account-select-trigger"]`、各選択肢: `[data-testid="account-select-option-<accountId>"]`

- [ ] **Step 1: `TqlCompletionField.svelte`に`testid`propを追加する**

`let { ... }: { ... } = $props();`のプロパティ一覧(21-38行目)に追加:

```typescript
  let {
    mode,
    value = $bindable(),
    placeholder = "",
    rows,
    invalid = false,
    oninput,
    lists = [],
    antennas = [],
    channels = [],
    testid,
  }: {
    mode: TqlEditMode;
    value: string;
    placeholder?: string;
    rows?: number;
    invalid?: boolean;
    oninput?: () => void;
    lists?: UserList[];
    antennas?: SourceItem[];
    channels?: SourceItem[];
    testid?: string;
  } = $props();
```

`<textarea>`要素(177行目付近)に`data-testid={testid}`を追加:

```svelte
  <textarea
    class={invalid
      ? 'rounded-lg border border-destructive bg-muted px-2.5 py-2 font-[ui-monospace,"Cascadia_Code","SF_Mono",monospace] text-sm text-foreground resize-y'
      : 'rounded-lg border border-border bg-muted px-2.5 py-2 font-[ui-monospace,"Cascadia_Code","SF_Mono",monospace] text-sm text-foreground resize-y'}
    {rows}
    {placeholder}
    data-testid={testid}
    bind:value
```

(以下`bind:this={el}`以降は無変更)

- [ ] **Step 2: `AddColumnModal.svelte`から`testid`を渡す**

`<TqlCompletionField ... />`(404-414行目)に`testid="add-column-tql-textarea"`を追加する:

```svelte
      <TqlCompletionField
        mode="query"
        bind:value={tqlText}
        rows={4}
        placeholder={'from home, list("...") where has_files && !cw'}
        invalid={!!tqlErr}
        oninput={onTqlInput}
        testid="add-column-tql-textarea"
        {lists}
        {antennas}
        {channels}
      />
```

- [ ] **Step 3: `Settings.svelte`のタブボタンに`data-testid`を追加する**

60-69行目の`{#each nav as item (item.id)}`ブロック内の`<button>`に追加:

```svelte
        {#each nav as item (item.id)}
          <button
            type="button"
            class={active === item.id
              ? "rounded-md bg-primary px-2.5 py-2 text-left text-sm text-primary-foreground"
              : "rounded-md px-2.5 py-2 text-left text-sm text-foreground hover:bg-background"}
            data-testid={`settings-tab-${item.id}`}
            onclick={() => (active = item.id)}
          >
            {item.label}
          </button>
        {/each}
```

- [ ] **Step 4: `AppMenu.svelte`の「設定」項目に`data-testid`を追加する**

88行目付近、`onclick={() => pick(onOpenSettings)}`を持つ`<button>`に`data-testid="app-menu-open-settings"`を追加する。

- [ ] **Step 5: `MuteSection.svelte`にNGユーザー欄・保存ボタンの`data-testid`を追加する**

47行目のNGユーザーtextareaに`data-testid="mute-ng-users-textarea"`、56行目の保存`<Button>`に`data-testid="mute-save"`を追加する:

```svelte
<label class="mb-2.5 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">NGユーザ(@user@host。@は省略可)</span>
  <textarea class="resize-y rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground" rows="2" placeholder={"@spammer@example.com"} bind:value={users} data-testid="mute-ng-users-textarea"></textarea>
</label>
```

```svelte
  <Button type="button" disabled={busy} onclick={save} data-testid="mute-save">{busy ? "保存中…" : "保存"}</Button>
```

- [ ] **Step 6: `Column.svelte`のカラムルートに`data-account-id`を追加する**

361-368行目の`<section>`要素に、既存の`data-group-id={group.id}`と並べて追加する:

```svelte
<section
  class="column-root relative flex flex-none flex-col h-full border-r border-border col-bg"
  style={app.useMobileUi() ? "flex:0 0 100%;width:100%;min-width:0" : stretch ? "flex:1 1 0;min-width:0" : group.auto ? "flex:1 1 0;min-width:220px" : `width:${group.width}px`}
  class:opacity-55={app.draggingGroupId === group.id}
  class:focused={app.focusedGroupId === group.id}
  data-group-id={group.id}
  data-account-id={activeTab?.accountId}
  style:scroll-snap-align={app.useMobileUi() ? "start" : undefined}
  style:scroll-snap-stop={app.useMobileUi() ? "always" : undefined}
  ondragover={(e) => {
```

- [ ] **Step 7: `AccountSelect.svelte`にトリガー/選択肢の`data-testid`を追加する**

トリガーの`<Button ...>`(60-69行目)に`data-testid="account-select-trigger"`を追加:

```svelte
<Button
  type="button"
  variant="outline"
  size={large ? "lg" : "sm"}
  class={showLabel ? "w-full justify-start" : ""}
  onclick={toggle}
  {disabled}
  title={selected ? handle(selected) : "アカウントを選択"}
  bind:ref={trigger}
  data-testid="account-select-trigger"
>
```

各選択肢の`<button>`(116-122行目)に`data-testid={`account-select-option-${a.id}`}`を追加:

```svelte
      {#each accounts as a (a.id)}
        <button
          type="button"
          class={a.id === value
            ? "active flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left font-[inherit] text-foreground hover:bg-muted"
            : "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left font-[inherit] text-foreground hover:bg-muted"}
          data-testid={`account-select-option-${a.id}`}
          onclick={() => choose(a.id)}
        >
```

同一コンポーネントが複数箇所(ComposeBar・AddColumnModal)から使われるため、`data-testid`は画面上に複数出現しうる。specから使う際はスコープを絞って`$$`で扱う(Task 7で対応)。

- [ ] **Step 8: 型チェック・既存フロントエンドテストを実行する**

Run: `cd frontend && pnpm check`
Expected: エラー無し。

Run: `cd frontend && pnpm test`
Expected: 既存のVitestが全てPASS(表示に影響しない属性追加のみのため、既存テストのスナップショット等に影響しないはず。もし失敗するテストがあれば、その属性追加が原因かを確認し、無関係な既存の不安定テストでないことを切り分ける)。

- [ ] **Step 9: Commit**

```bash
git add frontend/src/input/TqlCompletionField.svelte frontend/src/ui/AddColumnModal.svelte frontend/src/ui/Settings.svelte frontend/src/ui/AppMenu.svelte frontend/src/ui/settings/MuteSection.svelte frontend/src/ui/Column.svelte frontend/src/ui/AccountSelect.svelte
git commit -m "test: E2E新規シナリオ向けにdata-testid/data-account-idを追加"
```

---

### Task 5: `run-app.sh`に`E2E_REUSE_HOME_FILE`サポートを追加

**Files:**
- Modify: `e2e/scripts/run-app.sh`

**Interfaces:**
- Produces: 環境変数`E2E_REUSE_HOME_FILE`(ファイルパス)。設定時、`run-app.sh`はそのファイルに書かれたディレクトリパスを`TMP_HOME`として再利用し(無ければ新規作成してパスを書き込む)、プロセス終了時にディレクトリを削除しない。Task 10(part1/part2)が利用する。

- [ ] **Step 1: `TMP_HOME`決定ロジックを変更する**

`e2e/scripts/run-app.sh`冒頭の以下の行:

```bash
TMP_HOME="$(mktemp -d /tmp/tsumugi-e2e-XXXXXX)"
```

を、次のブロックに置き換える:

```bash
# E2E_REUSE_HOME_FILE が指定されている場合、そのファイルに書かれたディレクトリを
# TMP_HOME として再利用する(無ければ新規作成してパスを書き込む)。
# settings-persistence-restart シナリオ(Issue #223)が、アプリを一度正常終了させた後に
# 同じ設定ディレクトリで再起動して永続化を検証するために使う。この変数が未設定の
# 既存の全シナリオは従来どおり毎回mktempする(挙動変化なし)。
if [ -n "${E2E_REUSE_HOME_FILE:-}" ]; then
  if [ -s "$E2E_REUSE_HOME_FILE" ]; then
    TMP_HOME="$(cat "$E2E_REUSE_HOME_FILE")"
    mkdir -p "$TMP_HOME"
  else
    TMP_HOME="$(mktemp -d /tmp/tsumugi-e2e-XXXXXX)"
    mkdir -p "$(dirname "$E2E_REUSE_HOME_FILE")"
    printf '%s' "$TMP_HOME" > "$E2E_REUSE_HOME_FILE"
  fi
else
  TMP_HOME="$(mktemp -d /tmp/tsumugi-e2e-XXXXXX)"
fi
```

- [ ] **Step 2: `set -u`との整合を確認する**

ファイル冒頭に`set -euo pipefail`があるため、`${E2E_REUSE_HOME_FILE:-}`のように未設定時のデフォルト展開を必ず使うこと(Step 1のコードは既に対応済み)。

- [ ] **Step 3: シェル構文チェックを実行する**

Run: `bash -n e2e/scripts/run-app.sh`
Expected: エラー無し(構文エラーが無いことのみを確認。実際の起動確認はTask 10で行う)。

- [ ] **Step 4: Commit**

```bash
git add e2e/scripts/run-app.sh
git commit -m "test: run-app.shにHOME再利用による再起動サポートを追加"
```

---

### Task 6: TQLフィルタE2E(`tql-column-filter.e2e.ts`)

**Files:**
- Create: `e2e/specs/tql-column-filter.e2e.ts`

**Interfaces:**
- Consumes: `startMiauthBridge`(Task 2)、`clickThroughAccountSelect`(Task 1)、`signInAsSeededUser`/`createNote`(既存`misskeyApi.ts`)、`[data-testid="add-column-tql-textarea"]`(Task 4)

- [ ] **Step 1: specファイルを作成する**

`e2e/specs/server-word-mute.e2e.ts`と同じ全体構成(before/after/afterEachのパターン)に倣い、以下を作成する:

```typescript
// TQLエキスパートモードで作成したカラムが、実データに対して正しく絞り込まれることを
// 検証する(Issue #223)。初期表示(REST fetch_and_filter_multi)とストリーミング受信
// (eval pipeline)の両方でフィルタが効くことを確認する。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { clickThroughAccountSelect } from "../helpers/accountSelect";
import { signInAsSeededUser, createNote } from "../helpers/misskeyApi";
import { debugLog, debugLogPath } from "../helpers/debugLog";

const MISSKEY_HOST = "misskey.local:8443";
const MARKER = "tsumugie2etqlmatch";

describe("TQL column filter matches real data", () => {
  let bridge: MiauthBridge;
  let token: string;
  let initialMatchText: string;
  let initialNoMatchText: string;

  before(async function () {
    this.timeout(30000);
    token = await signInAsSeededUser();
    const runId = Date.now();
    initialMatchText = `tsumugi e2e tql initial match ${runId} ${MARKER}`;
    initialNoMatchText = `tsumugi e2e tql initial no-match ${runId}`;
    // アカウント追加(→カラムのREST初期ロード)より先に投稿しておく。
    await createNote(token, initialMatchText);
    await createNote(token, initialNoMatchText);

    // startMiauthBridge() 自身も内部でsignin-flowを叩くため、直前のsignin連続で
    // レート制限(429)に当たることがある(既存2本と同じ既知の回避策)。
    await new Promise((resolve) => setTimeout(resolve, 10000));
    bridge = await startMiauthBridge();
  });

  after(async () => {
    await bridge?.teardown();
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

  it("adds an account via MiAuth", async function () {
    this.timeout(90000);

    const hostInput = await $('[data-testid="add-account-host-input"]');
    await hostInput.waitForDisplayed({ timeout: 15000 });
    await hostInput.setValue(MISSKEY_HOST);

    const startButton = await $('[data-testid="add-account-start"]');
    await Promise.all([
      bridge.approveNext(),
      clickThroughAccountSelect(bridge.cdpPort, { logTag: "tqlColumnFilter", accountLabel: "e2etestadmin" }),
      startButton.click(),
    ]);

    const completeButton = await $('[data-testid="add-account-complete"]');
    await completeButton.waitForDisplayed({ timeout: 15000 });
    await completeButton.click();

    const composeTextarea = await $('[data-testid="compose-textarea"]');
    await composeTextarea.waitForDisplayed({ timeout: 15000 });
  });

  it("shows only the initial note matching the TQL filter", async () => {
    await browser.setWindowSize(1280, 1024).catch((err) => debugLog("tqlColumnFilter", `setWindowSize failed (continuing anyway): ${String(err)}`));

    const menuTrigger = await $('[data-testid="app-menu-trigger"]');
    await menuTrigger.waitForDisplayed({ timeout: 15000 });
    await menuTrigger.click();

    const addColumnItem = await $('[data-testid="app-menu-add-column"]');
    await addColumnItem.waitForDisplayed({ timeout: 15000 });
    await addColumnItem.click();

    // account-post-reaction.e2e.tsと同じレイアウト安定待ち(Xvfb初回ペイントの揺れ対策)。
    await browser.waitUntil(
      async () => {
        const h1 = await browser.execute(() => window.innerHeight);
        await browser.pause(150);
        const h2 = await browser.execute(() => window.innerHeight);
        return h1 === h2 && h1 > 300;
      },
      { timeout: 10000, interval: 200 },
    );

    // guided/expertタブの切り替え(AddColumnModal.svelte、"エキスパート(TQL)"ボタン)。
    const expertTab = await $("button=エキスパート(TQL)");
    await expertTab.waitForDisplayed({ timeout: 15000 });
    await expertTab.click();

    const tqlTextarea = await $('[data-testid="add-column-tql-textarea"]');
    await tqlTextarea.waitForDisplayed({ timeout: 15000 });
    await tqlTextarea.setValue(`from home where text -> "${MARKER}"`);

    const addColumnSubmit = await $('[data-testid="add-column-submit"]');
    await addColumnSubmit.waitForDisplayed({ timeout: 15000 });
    await addColumnSubmit.scrollIntoView();
    await addColumnSubmit.waitForClickable({ timeout: 15000 });
    await addColumnSubmit.click();

    // マーカー入りノートが表示されるまで待つ(=初期ロード完了の合図)。
    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        for (const el of noteTexts) {
          const text = await el.getText().catch(() => "");
          if (text.includes(initialMatchText)) return true;
        }
        return false;
      },
      { timeout: 20000, interval: 300, timeoutMsg: `matching note "${initialMatchText}" did not appear` },
    );

    // マーカー無しノートは一切表示されていないことを確認する。
    const noteTexts = await $$('[data-testid="note-text"]');
    const visibleTexts: string[] = [];
    for (const el of noteTexts) {
      visibleTexts.push(await el.getText().catch(() => ""));
    }
    const noMatchVisible = visibleTexts.some((t) => t.includes(initialNoMatchText));
    if (noMatchVisible) {
      debugLog("tqlColumnFilter", `non-matching note unexpectedly visible; all visible note texts: ${JSON.stringify(visibleTexts)}`);
    }
    expect(noMatchVisible).toBe(false);
  });

  it("filters live-streamed notes the same way", async () => {
    const runId = Date.now();
    const liveMatchText = `tsumugi e2e tql live match ${runId} ${MARKER}`;
    const liveNoMatchText = `tsumugi e2e tql live no-match ${runId}`;

    await createNote(token, liveMatchText);
    await createNote(token, liveNoMatchText);

    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        for (const el of noteTexts) {
          const text = await el.getText().catch(() => "");
          if (text.includes(liveMatchText)) return true;
        }
        return false;
      },
      { timeout: 20000, interval: 300, timeoutMsg: `live matching note "${liveMatchText}" did not appear` },
    );

    const noteTexts = await $$('[data-testid="note-text"]');
    const visibleTexts: string[] = [];
    for (const el of noteTexts) {
      visibleTexts.push(await el.getText().catch(() => ""));
    }
    const liveNoMatchVisible = visibleTexts.some((t) => t.includes(liveNoMatchText));
    if (liveNoMatchVisible) {
      debugLog("tqlColumnFilter", `live non-matching note unexpectedly visible; all visible note texts: ${JSON.stringify(visibleTexts)}`);
    }
    expect(liveNoMatchVisible).toBe(false);
  });
});
```

- [ ] **Step 2: E2Eを実行する**

Run: `cd e2e && xvfb-run -a pnpm e2e`
Expected: `tql-column-filter.e2e.ts`の3テストが全てPASS。失敗した場合は`e2e/wdio-logs/`のスクリーンショット・ログで原因(セレクタ不一致・TQL構文エラー等)を確認し、修正する。

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/tql-column-filter.e2e.ts
git commit -m "test: TQLフィルタカラムのE2Eシナリオを追加"
```

---

### Task 7: 複数アカウントE2E(`multi-account.e2e.ts`)

**Files:**
- Create: `e2e/specs/multi-account.e2e.ts`

**Interfaces:**
- Consumes: `startMiauthBridge`(Task 2、`credentials`引数)、`clickThroughAccountSelect`(Task 1)、`signUp`/`createNote`(Task 3・既存)、`[data-account-id]`(Task 4)、`[data-testid="account-select-trigger"]`/`account-select-option-<id>`(Task 4)

- [ ] **Step 1: specファイルを作成する**

```typescript
// 2アカウントを同時に追加し、それぞれのカラムが互いのデータを混同しないこと、
// ComposeBarのアカウント切り替えが正しく機能することを検証する(Issue #223)。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { clickThroughAccountSelect } from "../helpers/accountSelect";
import { signUp, createNote } from "../helpers/misskeyApi";
import { debugLog, debugLogPath } from "../helpers/debugLog";

const MISSKEY_HOST = "misskey.local:8443";
const SECOND_USERNAME = "e2etestuser2";
const SECOND_PASSWORD = "e2eTestPassword2!";

describe("multiple accounts used simultaneously", () => {
  let bridgeA: MiauthBridge;
  let bridgeB: MiauthBridge;
  let tokenB: string;

  before(async function () {
    this.timeout(30000);
    const signUpResult = await signUp(SECOND_USERNAME, SECOND_PASSWORD);
    tokenB = signUpResult.token;

    bridgeA = await startMiauthBridge();
    // 直前のsignUpのsignin相当処理と連続するため、既存シナリオと同じくレート制限を避ける。
    await new Promise((resolve) => setTimeout(resolve, 10000));
    bridgeB = await startMiauthBridge({ username: SECOND_USERNAME, password: SECOND_PASSWORD });
  });

  after(async () => {
    await bridgeA?.teardown();
    await bridgeB?.teardown();
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

  async function addAccount(bridge: MiauthBridge, accountLabel: string): Promise<void> {
    const hostInput = await $('[data-testid="add-account-host-input"]');
    await hostInput.waitForDisplayed({ timeout: 15000 });
    await hostInput.setValue(MISSKEY_HOST);

    const startButton = await $('[data-testid="add-account-start"]');
    await Promise.all([
      bridge.approveNext(),
      clickThroughAccountSelect(bridge.cdpPort, { logTag: "multiAccount", accountLabel }),
      startButton.click(),
    ]);

    const completeButton = await $('[data-testid="add-account-complete"]');
    await completeButton.waitForDisplayed({ timeout: 15000 });
    await completeButton.click();
  }

  it("adds two accounts via MiAuth", async function () {
    this.timeout(120000);

    // 1アカウント目: 起動直後は自動的にAddAccount画面(App.svelte: accounts.length === 0)。
    await addAccount(bridgeA, "e2etestadmin");

    const composeTextarea = await $('[data-testid="compose-textarea"]');
    await composeTextarea.waitForDisplayed({ timeout: 15000 });

    // 2アカウント目: AppMenu経由でアカウント追加画面を再度開く必要がある想定だが、
    // App.svelteは`accounts.length === 0`のときのみ自動表示するため、既存動線を
    // 使う(Settings→アカウント→追加、または同等のUI)。ここではAppMenuの設定タブ
    // からアカウント管理を開く。
    const menuTrigger = await $('[data-testid="app-menu-trigger"]');
    await menuTrigger.waitForDisplayed({ timeout: 15000 });
    await menuTrigger.click();
    const openSettings = await $('[data-testid="app-menu-open-settings"]');
    await openSettings.waitForDisplayed({ timeout: 15000 });
    await openSettings.click();

    const accountsTab = await $('[data-testid="settings-tab-accounts"]');
    await accountsTab.waitForDisplayed({ timeout: 15000 });
    await accountsTab.click();

    // Settings > アカウント タブ内の「追加」導線を開く。具体的なセレクタは
    // Settings.svelteのaccountsセクション実装を確認してから実装時に確定する。
    const addAccountFromSettings = await $('[data-testid="settings-accounts-add"]');
    await addAccountFromSettings.waitForDisplayed({ timeout: 15000 });
    await addAccountFromSettings.click();

    await addAccount(bridgeB, SECOND_USERNAME);
  });

  it("adds a Home column for each account and keeps their notes separate", async function () {
    this.timeout(60000);
    await browser.setWindowSize(1280, 1024).catch((err) => debugLog("multiAccount", `setWindowSize failed (continuing anyway): ${String(err)}`));

    // アカウントAのHomeカラム(1アカウント目追加時に既定で作られる想定が無ければ、
    // ここで明示的に追加する)。account-post-reaction.e2e.tsと同じ手順。
    async function addHomeColumnFor(accountId: string): Promise<void> {
      const menuTrigger = await $('[data-testid="app-menu-trigger"]');
      await menuTrigger.waitForDisplayed({ timeout: 15000 });
      await menuTrigger.click();
      const addColumnItem = await $('[data-testid="app-menu-add-column"]');
      await addColumnItem.waitForDisplayed({ timeout: 15000 });
      await addColumnItem.click();

      await browser.waitUntil(
        async () => {
          const h1 = await browser.execute(() => window.innerHeight);
          await browser.pause(150);
          const h2 = await browser.execute(() => window.innerHeight);
          return h1 === h2 && h1 > 300;
        },
        { timeout: 10000, interval: 200 },
      );

      // AddColumnModal内のアカウント選択(AccountSelect)で目的のアカウントを選ぶ。
      const accountTrigger = await $('[data-testid="account-select-trigger"]');
      await accountTrigger.waitForDisplayed({ timeout: 15000 });
      await accountTrigger.click();
      const accountOption = await $(`[data-testid="account-select-option-${accountId}"]`);
      await accountOption.waitForDisplayed({ timeout: 15000 });
      await accountOption.click();

      const addColumnSubmit = await $('[data-testid="add-column-submit"]');
      await addColumnSubmit.waitForDisplayed({ timeout: 15000 });
      await addColumnSubmit.scrollIntoView();
      await addColumnSubmit.waitForClickable({ timeout: 15000 });
      await addColumnSubmit.click();
    }

    // アカウントidはUI上のacct表示(note-acct等)からではなく、Settings>アカウント
    // タブに並ぶアカウント行のdata属性から拾う想定。実装時にSettings.svelteの
    // アカウント一覧の実マークアップを確認し、確実にidを取得できるセレクタに置き換える。
    const accountIds = await browser.execute(() => {
      return Array.from(document.querySelectorAll<HTMLElement>("[data-account-row-id]")).map(
        (el) => el.dataset.accountRowId!,
      );
    });
    expect(accountIds.length).toBe(2);
    const [accountIdA, accountIdB] = accountIds;

    await addHomeColumnFor(accountIdA);
    await addHomeColumnFor(accountIdB);

    const runId = Date.now();
    const noteFromA = `tsumugi e2e multi-account note from A ${runId}`;
    await createNote(await (async () => {
      // アカウントAのトークンは既にbridgeA起動時にsignin-flowで取得済みだが、
      // MiauthBridge自体はトークンを公開していないため、ここではUI経由(ComposeBar)
      // で投稿する。
      return "";
    })(), noteFromA).catch(() => {
      // 上のcreateNote呼び出しはプレースホルダのトークン取得に失敗するため、
      // 実装時はUI経由の投稿に置き換える(下記参照)。
    });

    // UI経由でアカウントAとして投稿する(ComposeBarのAccountSelectをAに合わせてから投稿)。
    const composeAccountTrigger = await $('[data-testid="account-select-trigger"]');
    await composeAccountTrigger.waitForDisplayed({ timeout: 15000 });
    await composeAccountTrigger.click();
    const composeAccountOptionA = await $(`[data-testid="account-select-option-${accountIdA}"]`);
    await composeAccountOptionA.waitForDisplayed({ timeout: 15000 });
    await composeAccountOptionA.click();

    const textarea = await $('[data-testid="compose-textarea"]');
    await textarea.waitForDisplayed({ timeout: 15000 });
    await textarea.setValue(noteFromA);
    const submitButton = await $('[data-testid="compose-submit"]');
    await submitButton.click();

    // アカウントAのカラム(data-account-id=accountIdA)にだけ現れることを確認する。
    const columnA = await $(`[data-account-id="${accountIdA}"]`);
    await browser.waitUntil(
      async () => {
        const noteTexts = await columnA.$$('[data-testid="note-text"]');
        for (const el of noteTexts) {
          const text = await el.getText().catch(() => "");
          if (text.includes(noteFromA)) return true;
        }
        return false;
      },
      { timeout: 20000, interval: 300, timeoutMsg: `note "${noteFromA}" did not appear in account A's column` },
    );

    const columnB = await $(`[data-account-id="${accountIdB}"]`);
    const columnBNoteTexts = await columnB.$$('[data-testid="note-text"]');
    const columnBVisible: string[] = [];
    for (const el of columnBNoteTexts) {
      columnBVisible.push(await el.getText().catch(() => ""));
    }
    expect(columnBVisible.some((t) => t.includes(noteFromA))).toBe(false);
  });
});
```

- [ ] **Step 2: E2Eを実行し、プレースホルダ箇所を実マークアップに合わせて修正する**

Run: `cd e2e && xvfb-run -a pnpm e2e`

このテストは`Settings.svelte`のアカウント一覧・アカウント追加導線の実際のマークアップに
依存する箇所(`settings-accounts-add`・`data-account-row-id`・上記の未使用になった
`createNote`呼び出し)を仮置きしているため、実行前に`frontend/src/ui/Settings.svelte`
のaccountsセクション(既存の`AddAccount.svelte`呼び出し箇所)を確認し、実在する
testid/属性に置き換える。既存にtestidが無ければ、Task 4と同じ方針(表示に影響しない
追加のみ)で`Settings.svelte`にも追記してよい。プレースホルダの`createNote`呼び出し
ブロック(空文字トークンで呼ぶ箇所)は削除する。

Expected: 修正後、`multi-account.e2e.ts`の全テストがPASS。

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/multi-account.e2e.ts frontend/src/ui/Settings.svelte
git commit -m "test: 複数アカウント同時使用のE2Eシナリオを追加"
```

---

### Task 8: ストリーミングイベントE2E(`streaming-events.e2e.ts`)

**Files:**
- Create: `e2e/specs/streaming-events.e2e.ts`

**Interfaces:**
- Consumes: `startMiauthBridge`(Task 2)、`clickThroughAccountSelect`(Task 1)、`signUp`/`createNote`/`renoteNote`/`deleteNote`(Task 3)、`[data-testid="notification-actor"]`/`notification-note-preview`(既存`NotificationCard.svelte`)

- [ ] **Step 1: specファイルを作成する**

```typescript
// main channel経由の通知・リノート・ノート削除が、既に開いているカラム/通知カラムへ
// リアルタイムに反映されることを検証する(Issue #223、stream/connection.rs)。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { clickThroughAccountSelect } from "../helpers/accountSelect";
import { signUp, createNote, renoteNote, deleteNote } from "../helpers/misskeyApi";
import { signInAsSeededUser } from "../helpers/misskeyApi";
import { debugLog, debugLogPath } from "../helpers/debugLog";

const MISSKEY_HOST = "misskey.local:8443";
const SECOND_USERNAME = "e2etestuser3";
const SECOND_PASSWORD = "e2eTestPassword3!";

describe("streaming events reflect in real time", () => {
  let bridge: MiauthBridge;
  let tokenA: string;
  let tokenB: string;

  before(async function () {
    this.timeout(30000);
    tokenA = await signInAsSeededUser();
    const signUpResult = await signUp(SECOND_USERNAME, SECOND_PASSWORD);
    tokenB = signUpResult.token;

    await new Promise((resolve) => setTimeout(resolve, 10000));
    bridge = await startMiauthBridge();
  });

  after(async () => {
    await bridge?.teardown();
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

  it("adds an account, a Home column, and a Notifications column", async function () {
    this.timeout(90000);

    const hostInput = await $('[data-testid="add-account-host-input"]');
    await hostInput.waitForDisplayed({ timeout: 15000 });
    await hostInput.setValue(MISSKEY_HOST);

    const startButton = await $('[data-testid="add-account-start"]');
    await Promise.all([
      bridge.approveNext(),
      clickThroughAccountSelect(bridge.cdpPort, { logTag: "streamingEvents", accountLabel: "e2etestadmin" }),
      startButton.click(),
    ]);

    const completeButton = await $('[data-testid="add-account-complete"]');
    await completeButton.waitForDisplayed({ timeout: 15000 });
    await completeButton.click();

    await browser.setWindowSize(1280, 1024).catch((err) => debugLog("streamingEvents", `setWindowSize failed (continuing anyway): ${String(err)}`));

    async function addColumn(sourceValueLabel: string): Promise<void> {
      const menuTrigger = await $('[data-testid="app-menu-trigger"]');
      await menuTrigger.waitForDisplayed({ timeout: 15000 });
      await menuTrigger.click();
      const addColumnItem = await $('[data-testid="app-menu-add-column"]');
      await addColumnItem.waitForDisplayed({ timeout: 15000 });
      await addColumnItem.click();

      await browser.waitUntil(
        async () => {
          const h1 = await browser.execute(() => window.innerHeight);
          await browser.pause(150);
          const h2 = await browser.execute(() => window.innerHeight);
          return h1 === h2 && h1 > 300;
        },
        { timeout: 10000, interval: 200 },
      );

      if (sourceValueLabel !== "home") {
        // AddColumnModal.svelteのソース選択(guidedモード)から"Notifications（通知）"を選ぶ。
        // 実装時にソース選択セレクトの実マークアップ(nativeの<select>かカスタムか)を
        // 確認し、対応するwdioコマンド(selectByVisibleText等)に置き換える。
        const sourceSelect = await $('[data-testid="add-column-source-select"]');
        await sourceSelect.waitForDisplayed({ timeout: 15000 });
        await sourceSelect.selectByAttribute("value", sourceValueLabel);
      }

      const addColumnSubmit = await $('[data-testid="add-column-submit"]');
      await addColumnSubmit.waitForDisplayed({ timeout: 15000 });
      await addColumnSubmit.scrollIntoView();
      await addColumnSubmit.waitForClickable({ timeout: 15000 });
      await addColumnSubmit.click();
    }

    // 1つ目: 既定のHomeカラム。
    await addColumn("home");
    // 2つ目: 通知カラム。
    await addColumn("notifications");

    const composeTextarea = await $('[data-testid="compose-textarea"]');
    await composeTextarea.waitForDisplayed({ timeout: 15000 });
  });

  it("shows a mention notification live", async () => {
    // アカウントAのusernameは"e2etestadmin"固定(certs/seeded-account.json)。
    const mentionText = `@e2etestadmin tsumugi e2e streaming mention ${Date.now()}`;
    await createNote(tokenB, mentionText);

    await browser.waitUntil(
      async () => {
        const previews = await $$('[data-testid="notification-note-preview"]');
        for (const el of previews) {
          const text = await el.getText().catch(() => "");
          if (text.includes("tsumugi e2e streaming mention")) return true;
        }
        return false;
      },
      { timeout: 20000, interval: 300, timeoutMsg: "mention notification did not appear live" },
    );
  });

  it("shows a renote live in the timeline", async () => {
    const runId = Date.now();
    const originalText = `tsumugi e2e streaming renote source ${runId}`;
    const originalNoteId = await createNote(tokenB, originalText);
    await renoteNote(tokenB, originalNoteId);

    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        for (const el of noteTexts) {
          const text = await el.getText().catch(() => "");
          if (text.includes(originalText)) return true;
        }
        return false;
      },
      { timeout: 20000, interval: 300, timeoutMsg: "renote did not appear live" },
    );
  });

  it("removes a deleted note live", async () => {
    const runId = Date.now();
    const deleteTargetText = `tsumugi e2e streaming delete target ${runId}`;
    const noteId = await createNote(tokenB, deleteTargetText);

    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        for (const el of noteTexts) {
          const text = await el.getText().catch(() => "");
          if (text.includes(deleteTargetText)) return true;
        }
        return false;
      },
      { timeout: 20000, interval: 300, timeoutMsg: `note "${deleteTargetText}" did not appear before delete` },
    );

    await deleteNote(tokenB, noteId);

    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        for (const el of noteTexts) {
          const text = await el.getText().catch(() => "");
          if (text.includes(deleteTargetText)) return false;
        }
        return true;
      },
      { timeout: 20000, interval: 300, timeoutMsg: `note "${deleteTargetText}" was not removed after delete` },
    );
  });
});
```

- [ ] **Step 2: E2Eを実行し、通知カラム追加導線のプレースホルダを実マークアップに合わせて修正する**

Run: `cd e2e && xvfb-run -a pnpm e2e`

`add-column-source-select`は仮のセレクタ名。実装時に`AddColumnModal.svelte`の
ソース選択部(guidedモード、`sourceType`にbindする要素)の実マークアップを確認し、
必要であれば`data-testid`をTask 4と同じ方針で追加してから実装を合わせる。

Expected: 修正後、`streaming-events.e2e.ts`の全テストがPASS。

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/streaming-events.e2e.ts frontend/src/ui/AddColumnModal.svelte
git commit -m "test: ストリーミングイベント即時反映のE2Eシナリオを追加"
```

---

### Task 9: クライアント側ユーザーミュートE2E(`client-user-mute.e2e.ts`)

**Files:**
- Create: `e2e/specs/client-user-mute.e2e.ts`

**Interfaces:**
- Consumes: `startMiauthBridge`(Task 2)、`clickThroughAccountSelect`(Task 1)、`signUp`/`createNote`(Task 3)、`[data-testid="settings-tab-mute"]`/`mute-ng-users-textarea`/`mute-save`(Task 4)

- [ ] **Step 1: specファイルを作成する**

```typescript
// クライアント側NGユーザー設定(app.mute.ngUsers)がタイムライン表示に反映される
// ことを検証する(Issue #223)。server-word-mute.e2e.tsのクライアント側版。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { clickThroughAccountSelect } from "../helpers/accountSelect";
import { signUp, createNote } from "../helpers/misskeyApi";
import { debugLog, debugLogPath } from "../helpers/debugLog";

const MISSKEY_HOST = "misskey.local:8443";
const MUTED_USERNAME = "e2etestuser4";
const MUTED_PASSWORD = "e2eTestPassword4!";

describe("client-side user mute hides the muted user's notes", () => {
  let bridge: MiauthBridge;
  let controlNoteText: string;
  let targetNoteText: string;

  before(async function () {
    this.timeout(30000);
    const { token } = await signUp(MUTED_USERNAME, MUTED_PASSWORD);

    const runId = Date.now();
    controlNoteText = `tsumugi e2e user-mute control ${runId}`;
    targetNoteText = `tsumugi e2e user-mute target ${runId}`;
    // ミュート設定より先に、対象ユーザーとは別の制御ノートも含めて投稿しておく。
    await createNote(token, targetNoteText);

    await new Promise((resolve) => setTimeout(resolve, 10000));
    bridge = await startMiauthBridge();
  });

  after(async () => {
    await bridge?.teardown();
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

  it("adds an account and posts a control note", async function () {
    this.timeout(90000);

    const hostInput = await $('[data-testid="add-account-host-input"]');
    await hostInput.waitForDisplayed({ timeout: 15000 });
    await hostInput.setValue(MISSKEY_HOST);

    const startButton = await $('[data-testid="add-account-start"]');
    await Promise.all([
      bridge.approveNext(),
      clickThroughAccountSelect(bridge.cdpPort, { logTag: "clientUserMute", accountLabel: "e2etestadmin" }),
      startButton.click(),
    ]);

    const completeButton = await $('[data-testid="add-account-complete"]');
    await completeButton.waitForDisplayed({ timeout: 15000 });
    await completeButton.click();

    await browser.setWindowSize(1280, 1024).catch((err) => debugLog("clientUserMute", `setWindowSize failed (continuing anyway): ${String(err)}`));

    const menuTrigger = await $('[data-testid="app-menu-trigger"]');
    await menuTrigger.waitForDisplayed({ timeout: 15000 });
    await menuTrigger.click();
    const addColumnItem = await $('[data-testid="app-menu-add-column"]');
    await addColumnItem.waitForDisplayed({ timeout: 15000 });
    await addColumnItem.click();

    await browser.waitUntil(
      async () => {
        const h1 = await browser.execute(() => window.innerHeight);
        await browser.pause(150);
        const h2 = await browser.execute(() => window.innerHeight);
        return h1 === h2 && h1 > 300;
      },
      { timeout: 10000, interval: 200 },
    );

    const addColumnSubmit = await $('[data-testid="add-column-submit"]');
    await addColumnSubmit.waitForDisplayed({ timeout: 15000 });
    await addColumnSubmit.scrollIntoView();
    await addColumnSubmit.waitForClickable({ timeout: 15000 });
    await addColumnSubmit.click();

    const textarea = await $('[data-testid="compose-textarea"]');
    await textarea.waitForDisplayed({ timeout: 15000 });
    await textarea.setValue(controlNoteText);
    const submitButton = await $('[data-testid="compose-submit"]');
    await submitButton.click();

    // ミュート設定前は両方表示されていることを確認する(制御ノート:自分の投稿でライブ反映、
    // 対象ノート:REST初期ロードで表示済みのはず)。
    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        const visible = await Promise.all(noteTexts.map((el) => el.getText().catch(() => "")));
        return visible.some((t) => t.includes(controlNoteText)) && visible.some((t) => t.includes(targetNoteText));
      },
      { timeout: 20000, interval: 300, timeoutMsg: "control/target notes did not both appear before muting" },
    );
  });

  it("hides the target user's note after registering a client-side NG user, keeping the control note visible", async () => {
    const menuTrigger = await $('[data-testid="app-menu-trigger"]');
    await menuTrigger.waitForDisplayed({ timeout: 15000 });
    await menuTrigger.click();
    const openSettings = await $('[data-testid="app-menu-open-settings"]');
    await openSettings.waitForDisplayed({ timeout: 15000 });
    await openSettings.click();

    const muteTab = await $('[data-testid="settings-tab-mute"]');
    await muteTab.waitForDisplayed({ timeout: 15000 });
    await muteTab.click();

    const ngUsersTextarea = await $('[data-testid="mute-ng-users-textarea"]');
    await ngUsersTextarea.waitForDisplayed({ timeout: 15000 });
    await ngUsersTextarea.setValue(`@${MUTED_USERNAME}`);

    const saveButton = await $('[data-testid="mute-save"]');
    await saveButton.click();

    // 設定モーダルを閉じてタイムラインへ戻る(Modal.svelteの閉じるボタンを踏襲)。
    const closeModal = await $('button[aria-label="閉じる"]');
    await closeModal.waitForDisplayed({ timeout: 15000 });
    await closeModal.click();

    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        const visible = await Promise.all(noteTexts.map((el) => el.getText().catch(() => "")));
        return !visible.some((t) => t.includes(targetNoteText));
      },
      { timeout: 20000, interval: 300, timeoutMsg: `target note "${targetNoteText}" was not hidden after muting` },
    );

    const noteTexts = await $$('[data-testid="note-text"]');
    const visible = await Promise.all(noteTexts.map((el) => el.getText().catch(() => "")));
    expect(visible.some((t) => t.includes(controlNoteText))).toBe(true);
  });
});
```

- [ ] **Step 2: E2Eを実行し、設定モーダルの「閉じる」ボタンのセレクタを実マークアップに合わせて修正する**

Run: `cd e2e && xvfb-run -a pnpm e2e`

`button[aria-label="閉じる"]`は`Modal.svelte`の実装を見て正しい`aria-label`/セレクタに
合わせる(無ければTask 4と同じ方針で追加する)。

Expected: 修正後、`client-user-mute.e2e.ts`の全テストがPASS。

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/client-user-mute.e2e.ts
git commit -m "test: クライアント側ユーザーミュートのE2Eシナリオを追加"
```

---

### Task 10: 設定・カラム構成の永続化E2E(`settings-persistence-restart.part1/2.e2e.ts`)

**Files:**
- Create: `e2e/specs/settings-persistence-restart.part1.e2e.ts`
- Create: `e2e/specs/settings-persistence-restart.part2.e2e.ts`
- Modify: `e2e/wdio.conf.ts`(`E2E_REUSE_HOME_FILE`をrun-app.shへ渡すための`env`設定)

**Interfaces:**
- Consumes: `run-app.sh`の`E2E_REUSE_HOME_FILE`サポート(Task 5)

- [ ] **Step 1: `wdio.conf.ts`に`env`設定を追加する**

`capabilities`の`"tauri:options"`に、`E2E_REUSE_HOME_FILE`を固定パスで渡す設定を追加する:

```typescript
export const config: Options.Testrunner = {
  runner: "local",
  specs: ["./specs/**/*.e2e.ts"],
  maxInstances: 1,
  services: [["tauri", { driverProvider: "external", autoInstallTauriDriver: true }]],
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application: "./scripts/run-app.sh",
      },
    } as WebdriverIO.Capabilities,
  ],
  logLevel: "info",
  outputDir: "./wdio-logs",
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 60000,
  },
};
```

`@wdio/tauri-service`が`application`に指定したスクリプトへ環境変数をどう渡すか
(プロセス全体の`env`をそのまま継承するのか、`tauri:options`に`env`キーを取るのか)は
`e2e/patches/@wdio__tauri-service@1.3.0.patch`と`node_modules/@wdio/tauri-service`の
実装を確認してから確定する。素直な経路が無い場合は、`part1`/`part2`の各specの
`before()`で`process.env.E2E_REUSE_HOME_FILE`をspecファイル自身が設定してから
セッションを開始する方式(wdio自体のプロセス環境変数を、`run-app.sh`を起動する
子プロセスが継承する)に切り替える。いずれの経路でも、両ファイルが同じ固定パス
(`wdio-logs/persistence-home-path.txt`)を指すことが要件。

- [ ] **Step 2: 固定パスの定数を用意する**

`e2e/helpers/persistenceHome.ts`を新規作成し、両specファイルで共有する:

```typescript
// settings-persistence-restart.part1/2.e2e.ts が共有する、run-app.shへ渡す
// E2E_REUSE_HOME_FILE の固定パス。part1が新規作成したTMP_HOMEのパスをここに書き込み、
// part2が同じパスを読んで同じTMP_HOMEを再利用することでアプリ再起動を再現する。
import { join } from "node:path";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
export const PERSISTENCE_HOME_FILE = join(__dirname, "..", "wdio-logs", "persistence-home-path.txt");
```

- [ ] **Step 3: `part1`を作成する**

```typescript
// アプリを一度正常終了させ、同じ設定ディレクトリで再起動しても構成が復元されることを
// 検証するシナリオの前半(Issue #223)。ここでアカウント追加とカラム追加を行い、
// part2で同じTMP_HOMEを再利用して起動した際に復元されることを確認する。
import { rmSync } from "node:fs";
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { clickThroughAccountSelect } from "../helpers/accountSelect";
import { debugLog, debugLogPath } from "../helpers/debugLog";
import { PERSISTENCE_HOME_FILE } from "../helpers/persistenceHome";

const MISSKEY_HOST = "misskey.local:8443";

describe("settings persist across restart (part 1: set up)", () => {
  let bridge: MiauthBridge;

  before(async function () {
    this.timeout(30000);
    // 前回の失敗実行が残したファイルがあれば消し、必ず新規TMP_HOMEから始める。
    rmSync(PERSISTENCE_HOME_FILE, { force: true });
    bridge = await startMiauthBridge();
  });

  after(async () => {
    await bridge?.teardown();
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

  it("adds an account and a Home column, then the session ends normally", async function () {
    this.timeout(90000);

    const hostInput = await $('[data-testid="add-account-host-input"]');
    await hostInput.waitForDisplayed({ timeout: 15000 });
    await hostInput.setValue(MISSKEY_HOST);

    const startButton = await $('[data-testid="add-account-start"]');
    await Promise.all([
      bridge.approveNext(),
      clickThroughAccountSelect(bridge.cdpPort, { logTag: "persistenceRestartPart1", accountLabel: "e2etestadmin" }),
      startButton.click(),
    ]);

    const completeButton = await $('[data-testid="add-account-complete"]');
    await completeButton.waitForDisplayed({ timeout: 15000 });
    await completeButton.click();

    await browser.setWindowSize(1280, 1024).catch((err) => debugLog("persistenceRestartPart1", `setWindowSize failed (continuing anyway): ${String(err)}`));

    const menuTrigger = await $('[data-testid="app-menu-trigger"]');
    await menuTrigger.waitForDisplayed({ timeout: 15000 });
    await menuTrigger.click();
    const addColumnItem = await $('[data-testid="app-menu-add-column"]');
    await addColumnItem.waitForDisplayed({ timeout: 15000 });
    await addColumnItem.click();

    await browser.waitUntil(
      async () => {
        const h1 = await browser.execute(() => window.innerHeight);
        await browser.pause(150);
        const h2 = await browser.execute(() => window.innerHeight);
        return h1 === h2 && h1 > 300;
      },
      { timeout: 10000, interval: 200 },
    );

    const addColumnSubmit = await $('[data-testid="add-column-submit"]');
    await addColumnSubmit.waitForDisplayed({ timeout: 15000 });
    await addColumnSubmit.scrollIntoView();
    await addColumnSubmit.waitForClickable({ timeout: 15000 });
    await addColumnSubmit.click();

    // カラムが追加されたことを確認してからセッションを終える(wdioのafterフックが
    // アプリを正常終了させる)。
    const composeTextarea = await $('[data-testid="compose-textarea"]');
    await composeTextarea.waitForDisplayed({ timeout: 15000 });
  });
});
```

- [ ] **Step 4: `part2`を作成する**

```typescript
// アプリを一度正常終了させ、同じ設定ディレクトリで再起動しても構成が復元されることを
// 検証するシナリオの後半(Issue #223)。part1が作ったTMP_HOMEを再利用して起動するため、
// このプロセスはアカウント追加画面を経由せずいきなり以前のカラム構成が復元されるはず。
import { debugLog, debugLogPath } from "../helpers/debugLog";

describe("settings persist across restart (part 2: verify after restart)", () => {
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
```

- [ ] **Step 5: E2Eを実行する**

Run: `cd e2e && xvfb-run -a pnpm e2e`

`settings-persistence-restart.part1.e2e.ts` → `part2.e2e.ts`の順に実行されることを
wdioのログで確認する(specsのglob展開はアルファベット順なので、ファイル名の
`part1`/`part2`で自然に順序が決まる)。part1・part2ともに個別のwdioセッション
(=`run-app.sh`の個別起動)になるため、Step 1で確定した環境変数の受け渡し経路が
両方のプロセスに効いていることをログ(`wdio-logs/wdio.log`、`run-app.sh`が
`TMP_HOME`を出力するようdebug echoを一時的に足して確認してもよい)で確かめる。

Expected: part1・part2とも全テストPASS。part2でaddAccountShownがfalseになる
(=アカウント追加画面が出ない)ことが最も重要な確認ポイント。

- [ ] **Step 6: Commit**

```bash
git add e2e/wdio.conf.ts e2e/helpers/persistenceHome.ts e2e/specs/settings-persistence-restart.part1.e2e.ts e2e/specs/settings-persistence-restart.part2.e2e.ts
git commit -m "test: 設定・カラム構成の永続化E2Eシナリオを追加"
```

---

## Self-Review Notes

- **Spec coverage:** 設計ドキュメントの5シナリオ(TQL/複数アカウント/ストリーミング/クライアントミュート/永続化)にそれぞれTask 6〜10が対応する。共通インフラ(bridge/helper/testid/run-app.sh)はTask 1〜5でカバーしている。
- **既知の未確定要素:** Task 3の`signUp`レスポンス形状、Task 7のSettingsアカウント一覧・追加導線の実マークアップ、Task 8の通知カラム選択導線、Task 9のModal閉じるボタンのセレクタ、Task 10の`@wdio/tauri-service`への環境変数受け渡し経路は、いずれも実装時に実機/実コードを確認してから確定する箇所として各タスクのStepに明記した(設計ドキュメントの「リスク・不確定要素」節と対応)。
- **型/命名の一貫性:** `clickThroughAccountSelect(cdpPort, { logTag, accountLabel })`のシグネチャ、`startMiauthBridge(credentials?)`のシグネチャ、`misskeyApi.ts`の`signUp`/`renoteNote`/`deleteNote`の型はTask 1〜3で定義した後、Task 6〜10全てで同一のシグネチャのまま使用している。
