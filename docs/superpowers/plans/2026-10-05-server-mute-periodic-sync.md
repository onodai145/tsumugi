# サーバー側ミュートの定期同期 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Web 側でのサーバー側ミュート/ワードミュートの変更を、次の起動や再認証を待たずに反映するため、フロントのタイマーで、6時間ごとに全アカウントを同期する(#456 の P4)。

**Architecture:** `frontend/src/lib/store.svelte.ts` の `boot()` に、既存の定期処理(新バージョン確認、キャッシュ間引き)と同じ作りで `setInterval` を足す。発火時に、その時点の `this.accounts` の全アカウントで `#syncServerMutes(accountId, { quiet: true })` を呼ぶ。`quiet` は、成功時のログを出さない。Rust 側は変えない(P2 までの同期の本体と、アカウント単位のロックが、そのまま働く)。

**Tech Stack:** Svelte 5(ルーン)、TypeScript、Vitest(フェイクタイマー)。

**Spec:** `docs/superpowers/specs/2026-10-05-server-mute-periodic-sync-design.md`(実行者は spec も読むこと)

## Global Constraints

- ブランチは `feat/server-mute-periodic-sync-456`(作成済み)。`main` に直接コミットしない。
- コミットメッセージは**件名のみ**(本文なし)。末尾の `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` は別段落として付ける(`-m` を2回)。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら、リトライせず報告する(GPG 署名のタイムアウトが起きたことがある)。
- 変更は `frontend/src/lib/store.svelte.ts` と `frontend/src/lib/store.svelte.test.ts` だけ。Rust、Tauri コマンドの署名、`frontend/src/bindings/tauri.gen.ts`(生成物。手で編集しない)、ユーザーガイドは変えない。
- 間隔は **6時間**(`SERVER_MUTE_SYNC_INTERVAL_MS = 6 * 60 * 60 * 1000`)、設定項目にしない(spec の「判断」)。
- フロントのテストは `cd frontend && pnpm exec vitest run src/lib/store.svelte.test.ts -t "<名前>"`、全体は `cd frontend && pnpm test`、型は `cd frontend && pnpm check`。長いコマンドはバックグラウンドで流して出力をファイルに落とし、末尾だけ読む。
- 変異確認では、**変異の前に、テストの追加・修正を必ずコミットする**。変異を戻すときは `git checkout -- <file>` を使うので、未コミットの変更が消える(P3 で起きた事故)。
- `pkill` / `killall` は使わない。ユーザーの実アプリ(`cargo tauri dev`、`vite`)のプロセスには触らない。`cargo tauri dev` は起動しない。
- `pnpm check` の警告数は変更前と同じ(基準: 既存の `UrlPreviewCard.svelte` の警告1件のみ、エラー0)。増やさない。

## Review Focus

- `boot()` が再実行されても、定期同期が多重にならないこと(1周期に、1アカウントあたり1回。Task 1 のテスト)。
- アカウントが0件でも、何も呼ばず、例外も出ないこと(Task 1 のテスト)。
- 発火時の `this.accounts` を使うこと(`boot()` 時点の写しではない)。後から追加したアカウントを含み、削除したアカウントを含まないこと(Task 1 のテスト)。
- 定期実行は、成功してもログを出さず、失敗は警告ログを出すこと。起動時の同期は、これまでどおり、成功のログを出すこと(Task 1 のテスト)。
- `teardown()` の後は、同期が呼ばれないこと(Task 1 のテスト)。

---

## File Structure

| ファイル | 責任 |
|---|---|
| `frontend/src/lib/store.svelte.ts` | 定数 `SERVER_MUTE_SYNC_INTERVAL_MS`、タイマー `#serverMuteSyncTimer`、`#syncAllServerMutes`、`#syncServerMutes` の `quiet` オプション、`boot()` / `teardown()` の変更 |
| `frontend/src/lib/store.svelte.test.ts` | 定期同期のテスト(新しい `describe`) |

## Task 1: 6時間ごとのサーバー側ミュート同期

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`
- Test: `frontend/src/lib/store.svelte.test.ts`

**Interfaces:**
- Consumes: 既存の `#syncServerMutes(accountId: string)`、`commands.syncServerMutes(accountId)`(戻り値 `{ blockedUsers: number; wordRules: number }`)、`unwrapAcc`、`#log(level, text, reauthAccountId?)`、`this.accounts`、`app.logs`(`{ id, at, level, text }[]`)、既存のテストファイルの `invokeMock`、`app`。
- Produces:
  - `const SERVER_MUTE_SYNC_INTERVAL_MS = 6 * 60 * 60 * 1000;`
  - `#serverMuteSyncTimer: ReturnType<typeof setInterval> | null`
  - `async #syncAllServerMutes(): Promise<void>`(private)
  - `async #syncServerMutes(accountId: string, options: { quiet?: boolean } = {}): Promise<void>`(第2引数が増える。既存の2か所の呼び出しは、引数なしのまま、これまでの挙動)

- [ ] **Step 1: 失敗するテストを書く(`store.svelte.test.ts` の末尾に、新しい `describe`)**

先頭の `import type` を確認し、`Account` が無ければ足す:

```ts
import type { Account, ColumnKind, FilterQuery, Note, Notification, User } from "../bindings/tauri.gen";
```

末尾に足す:

```ts
describe("定期的なサーバー側ミュート同期(Issue #456)", () => {
  const SIX_HOURS = 6 * 60 * 60 * 1000;
  const account = (id: string): Account => ({
    id,
    host: "misskey.test",
    username: `user_${id}`,
    userId: `uid_${id}`,
    displayName: id,
    avatarUrl: null,
  });
  /// 呼ばれた `sync_server_mutes` の accountId(呼ばれた順)。
  const syncedAccounts = () =>
    invokeMock.mock.calls.filter(([cmd]) => cmd === "sync_server_mutes").map(([, args]) => (args as { accountId: string }).accountId);
  const syncLogs = () => app.logs.filter((l) => l.text.includes("サーバのミュート/ブロックを同期"));
  let syncResult: { blockedUsers: number; wordRules: number } | Error;

  beforeEach(() => {
    syncResult = { blockedUsers: 3, wordRules: 1 };
    invokeMock.mockClear();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_accounts") return [];
      if (cmd === "sync_server_mutes") {
        if (syncResult instanceof Error) throw syncResult;
        return syncResult;
      }
      return { status: "ok", data: null };
    });
    app.logs = [];
  });

  afterEach(() => {
    app.teardown();
    vi.useRealTimers();
  });

  /// フェイクタイマーで boot() し、その間の呼び出しを数えないよう、記録を空にする。
  async function bootAndForgetCalls() {
    vi.useFakeTimers();
    const bootPromise = app.boot();
    await vi.advanceTimersByTimeAsync(0);
    await bootPromise.catch(() => {});
    invokeMock.mockClear();
    app.logs = [];
  }

  it("boot()の6時間後に、その時点の全アカウントで同期を呼ぶ(後から追加したアカウントも含む)", async () => {
    await bootAndForgetCalls();
    app.accounts = [account("acc1"), account("acc2")]; // boot() の後に増えた

    await vi.advanceTimersByTimeAsync(SIX_HOURS + 1_000);

    expect(syncedAccounts().sort()).toEqual(["acc1", "acc2"]);
  });

  it("発火時のアカウント一覧を使う(boot()の後に削除したアカウントは対象外)", async () => {
    await bootAndForgetCalls();
    app.accounts = [account("acc2")]; // boot() の時点で acc1 が居ても、発火時に居なければ対象外

    await vi.advanceTimersByTimeAsync(SIX_HOURS + 1_000);

    expect(syncedAccounts()).toEqual(["acc2"]);
  });

  it("アカウントが0件でも、何も呼ばず、例外も出ない", async () => {
    await bootAndForgetCalls();
    app.accounts = [];

    await vi.advanceTimersByTimeAsync(SIX_HOURS + 1_000);

    expect(syncedAccounts()).toEqual([]);
  });

  it("boot()を2回呼んでも、定期同期は多重にならない(1周期に、1アカウントあたり1回)", async () => {
    vi.useFakeTimers();
    for (let i = 0; i < 2; i++) {
      const bootPromise = app.boot();
      await vi.advanceTimersByTimeAsync(0);
      await bootPromise.catch(() => {});
    }
    invokeMock.mockClear();
    app.accounts = [account("acc1")];

    await vi.advanceTimersByTimeAsync(SIX_HOURS + 1_000);

    expect(syncedAccounts()).toEqual(["acc1"]);
  });

  it("定期実行は、成功しても、同期のログを出さない", async () => {
    await bootAndForgetCalls();
    app.accounts = [account("acc1")];

    await vi.advanceTimersByTimeAsync(SIX_HOURS + 1_000);

    expect(syncedAccounts()).toEqual(["acc1"]); // 同期は実際に走っている
    expect(syncLogs()).toEqual([]);
  });

  it("定期実行の失敗は、警告ログを出す", async () => {
    await bootAndForgetCalls();
    app.accounts = [account("acc1")];
    syncResult = new Error("boom");

    await vi.advanceTimersByTimeAsync(SIX_HOURS + 1_000);

    const warns = app.logs.filter((l) => l.level === "warn" && l.text.includes("サーバミュート同期に失敗"));
    expect(warns).toHaveLength(1);
  });

  it("起動時の同期は、これまでどおり、成功のログを出す(quiet は定期実行だけ)", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_accounts") return [account("acc1")];
      if (cmd === "get_ui_prefs") return { ...app.ui }; // boot() が、同期まで進めるように、妥当な設定を返す
      if (cmd === "sync_server_mutes") return syncResult;
      return { status: "ok", data: null };
    });
    vi.useFakeTimers();

    const bootPromise = app.boot();
    await vi.advanceTimersByTimeAsync(0);
    await bootPromise.catch(() => {});

    expect(syncedAccounts()).toEqual(["acc1"]); // boot() が、起動時の同期まで進んだ
    expect(syncLogs().map((l) => l.text)).toEqual(["サーバのミュート/ブロックを同期: ユーザ3件・ワード1件"]);
  });

  it("teardown()の後は、同期が呼ばれない", async () => {
    await bootAndForgetCalls();
    app.accounts = [account("acc1")];

    app.teardown();
    await vi.advanceTimersByTimeAsync(SIX_HOURS + 1_000);

    expect(syncedAccounts()).toEqual([]);
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && pnpm exec vitest run src/lib/store.svelte.test.ts -t "定期的なサーバー側ミュート同期" 2>&1 | tail -40`
Expected(想定の理由で失敗するもの):
- FAIL: 「全アカウントで同期を呼ぶ」「発火時のアカウント一覧を使う」「boot()を2回呼んでも…」「定期実行は、成功しても…」(`syncedAccounts()` が空)、「定期実行の失敗は、警告ログを出す」(警告が0件)。
- PASS(最初から通る。既存の挙動を固定する回帰テスト): 「アカウントが0件でも…」「起動時の同期は、これまでどおり…」「teardown()の後は…」。
- 「起動時の同期は…」が FAIL する場合は、`boot()` が起動時の同期まで進んでいない(`get_ui_prefs` 等のモックの形が足りない)。`syncedAccounts()` の最初のアサートで落ちるので、`boot()` の前半(`this.ui = {...}` から `#applyBackground` など)が例外を投げていないかを `app.logs` で確認し、`get_ui_prefs` / `get_mute` / `get_notify` の返す形を、妥当な値(`{ ...app.ui }`、`app.mute`、`app.notify`)に直す。

- [ ] **Step 3: `store.svelte.ts` を実装する**

定数を、`PRUNE_INTERVAL_MS` の直後に足す:

```ts
const SERVER_MUTE_SYNC_INTERVAL_MS = 6 * 60 * 60 * 1000; // サーバー側ミュートの定期同期の間隔（6時間。Issue #456）
```

フィールドを、`#pruneTimer` の直後に足す:

```ts
  // サーバー側ミュート/ブロック・ワードミュートの定期同期（Issue #456）。Web側での変更を、次の起動や
  // 再認証を待たずに反映する。ミュートはめったに変えないので、間隔は数時間。
  #serverMuteSyncTimer: ReturnType<typeof setInterval> | null = null;
```

`teardown()` の、`#updateCheckTimer` の解除ブロックの直後に足す:

```ts
    if (this.#serverMuteSyncTimer !== null) {
      clearInterval(this.#serverMuteSyncTimer);
      this.#serverMuteSyncTimer = null;
    }
```

`boot()` の、`#pruneTimer` を張る3行の直後(`#clockTimer` の前)に足す:

```ts
    if (this.#serverMuteSyncTimer !== null) clearInterval(this.#serverMuteSyncTimer);
    this.#serverMuteSyncTimer = setInterval(() => void this.#syncAllServerMutes(), SERVER_MUTE_SYNC_INTERVAL_MS);
```

`#syncServerMutes` を、次に置き換える(直前に `#syncAllServerMutes` を足す):

```ts
  /// 定期同期の1回分。発火時点の全アカウントを同期する(アカウントの追加・削除に追従する)。
  /// 成功のログは出さない(quiet)。失敗の警告は、`#syncServerMutes` が出す。
  async #syncAllServerMutes() {
    await Promise.all(this.accounts.map((a) => this.#syncServerMutes(a.id, { quiet: true })));
  }

  /// サーバ側ミュート/ブロック・ワードミュート(mutedWords)を同期（失敗しても致命的でないのでログのみ）。
  /// `quiet` のときは、成功時のログを出さない(定期実行で、Backstage のログが埋まるのを避ける)。
  async #syncServerMutes(accountId: string, options: { quiet?: boolean } = {}) {
    try {
      const result = await unwrapAcc(accountId, commands.syncServerMutes(accountId));
      if (!options.quiet && (result.blockedUsers > 0 || result.wordRules > 0)) {
        this.#log(
          "info",
          `サーバのミュート/ブロックを同期: ユーザ${result.blockedUsers}件・ワード${result.wordRules}件`,
        );
      }
    } catch (e) {
      if (e instanceof ForbiddenError) {
        this.#log("warn", "サーバミュート同期: 権限不足。再認証してください", e.accountId);
      } else {
        this.#log("warn", `サーバミュート同期に失敗: ${String(e)}`);
      }
    }
  }
```

- [ ] **Step 4: テストを流して、全件が通ることを確認する**

Run: `cd frontend && pnpm test 2>&1 | tail -12`
Expected: すべてパス(追加8件を含む)。失敗があれば、`superpowers:systematic-debugging` で、テストの前提(フェイクタイマーと `boot()` の絡み)か、実装かを切り分ける。

- [ ] **Step 5: 型チェックを確認する**

Run: `cd frontend && pnpm check 2>&1 | tail -6`
Expected: `0 ERRORS`、警告は既存の `UrlPreviewCard.svelte` の1件のみ。

- [ ] **Step 6: コミットする**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "feat: サーバー側ミュートを6時間ごとに同期する" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 7: 変異確認(コミット後。変異のたびに、確認したら必ず元に戻す)**

各変異のあと `cd frontend && pnpm exec vitest run src/lib/store.svelte.test.ts -t "定期的なサーバー側ミュート同期" 2>&1 | tail -20` を流し、`git checkout -- frontend/src/lib/store.svelte.ts` で戻す。

1. **タイマーを張らない**: `boot()` の `this.#serverMuteSyncTimer = setInterval(...)` の行を削除する。Expected: 「全アカウントで同期を呼ぶ」「発火時のアカウント一覧を使う」「boot()を2回呼んでも…」「定期実行は、成功しても…」「定期実行の失敗は…」が FAIL。
2. **`quiet` を無視する**: `#syncServerMutes` の `!options.quiet &&` を削除する。Expected: 「定期実行は、成功しても、同期のログを出さない」が FAIL。
3. **`teardown()` で解除しない**: `teardown()` に足したブロック(`clearInterval(this.#serverMuteSyncTimer)` を含む)を削除する。Expected: 「teardown()の後は、同期が呼ばれない」が FAIL。
4. **`boot()` で、前のタイマーを解除しない**: `if (this.#serverMuteSyncTimer !== null) clearInterval(this.#serverMuteSyncTimer);` の行を削除する。Expected: 「boot()を2回呼んでも、定期同期は多重にならない」が FAIL(同期が2回呼ばれる)。
5. **`boot()` 時点のアカウントの写しを使う**: `boot()` の `setInterval(...)` の直前に `const accountsAtBoot = this.accounts;` を足し、`#syncAllServerMutes` を呼ぶ代わりに `() => void Promise.all(accountsAtBoot.map((a) => this.#syncServerMutes(a.id, { quiet: true })))` を渡す。Expected: 「全アカウントで同期を呼ぶ」「発火時のアカウント一覧を使う」が FAIL。

`git status --short` が空であること。

## 完了後(実行者が行う。ユーザーの承認を取ってから)

- 全体の確認: `cd frontend && pnpm check && pnpm test`、`git status --short`(`frontend/src/bindings/tauri.gen.ts` に差分が無いこと)。
- push して PR を作る。本文は `.github/pull_request_template.md` の構成に沿い、関連 Issue は `Refs #456`(これが #456 の最後の項目なので、ユーザーに、`Closes #456` にするかを確認する。自動クローズの判断は、ユーザーに任せる)。検証欄には、実機・実 UI の確認をしていない旨と、既知の挙動(追加したミュートは、表示済みのノートに効かない)を書く。
- マージ後、#456 の最後の項目(「定期同期が無い」)にチェックを入れる。
