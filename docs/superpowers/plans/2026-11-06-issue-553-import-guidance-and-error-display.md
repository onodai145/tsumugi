# 設定インポート後の案内・再認証画面の文言・エラー表示の整理(Issue #553) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 設定インポート直後に再認証の案内を出し、トークンなしアカウントへの無駄な通信とエラーログをなくし、再認証画面の文言とエラー文の接頭辞を整える。

**Architecture:** フロントは store に `needsLogin: Set<string>` を持たせ、起動時と定期同期でそのアカウントの通信をスキップし、メイン画面に `ReauthBanner` を出す。エラー文は `ipc.ts` の `AppError`(`toString()` が `message` だけを返す)で一括して直す。Rust は確認ダイアログの件数を「取り込み後に実際にトークンを持たないアカウント数」に直すだけで、型の形は変えない。

**Tech Stack:** Rust(tauri v2, tauri-specta), Svelte 5(runes), Vitest + @testing-library/svelte, Tailwind

**Spec:** `docs/superpowers/specs/2026-11-05-issue-553-import-guidance-and-error-display-design.md`

## Global Constraints

- ブランチは `fix/issue-553-import-guidance-and-error-display`(作成済み)。`main` では編集・コミットしない。
- コミットメッセージは件名1行のみ(本文なし)。Conventional Commits 形式の日本語件名。`Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` のトレーラーは別の `-m` で付ける: `git commit -m "<件名>" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"`。`--no-verify` は使わない。コミットが失敗・タイムアウトしたら止めて報告する(再試行しない)。
- `frontend/src/bindings/tauri.gen.ts` は生成物。手で編集せず、`cd src-tauri && cargo test generates_frontend_bindings` で再生成する。
- UIは `docs/design/style-guide.md` に従う。角丸は `rounded-md` などのスケール、文字は `text-xs` / `text-sm`、アイコンは `size={12}` か `size={16}`。`rounded-[Npx]` などの即値を新設しない。
- 画面の文言はすべて日本語。
- 型やコマンドのシグネチャは変えない(`ImportPreview.accountsNeedingLogin` の意味とdocコメントだけ変わる)。
- `cargo tauri dev` や `cargo run` を直接起動して確認しない。実画面の確認は Task 6 の手順(Xvfb と隔離)だけで行い、自分で起動したプロセスは自分で止める。`pkill` / `killall` は使わない。
- テストの実行: Rust は `cd src-tauri && cargo test <name>`、フロントは `cd frontend && npx vitest run <path>`(`pnpm test -- <path>` はパスを無視して全体を走らせる)、型検査は `cd frontend && pnpm check`。

## Review Focus

- `accounts_without_token` が失敗(keyring 不調など)したとき、起動が止まらず、従来どおり全アカウントを対象にする(Task 3 でテスト)。
- 再認証が完了したアカウントは、`needsLogin` から外れ、バナーの件数が減り、0件でバナーが消える(Task 3・5 でテスト)。
- アカウントを削除したとき、`needsLogin` に ID が残らず、バナーの件数がずれない(Task 3 でテスト)。
- 端末側の一致したアカウントがトークンありのとき、確認ダイアログの件数に入らない(Task 1 でテスト)。
- IPC を経由しない通常の JS 例外は、従来どおり `Error: …` のまま出る(Task 2 でテスト。バグと見分けるため)。

---

## File Structure

| ファイル | 責務 |
|---|---|
| `src-tauri/src/commands/settings_transfer.rs` | `plan_checked` が、取り込み後にトークンを持たない全アカウントを数える(Task 1) |
| `src-tauri/src/store/settings_transfer.rs` | `ImportPreview` / `ImportResult` のdocコメントを新しい意味に直す(Task 1) |
| `frontend/src/bindings/tauri.gen.ts` | 生成物。docコメントの変更だけ反映(Task 1) |
| `frontend/src/lib/ipc.ts` | `AppError` / `ForbiddenError` / `formatError`(Task 2) |
| `frontend/src/lib/ipc.errors.test.ts` | 上のテスト(新規、Task 2) |
| `frontend/src/lib/store.svelte.ts` | `needsLogin` と、起動・定期同期のスキップ、再認証・削除での更新(Task 3) |
| `frontend/src/lib/store.svelte.needsLogin.test.ts` | 上のテスト(新規、Task 3) |
| `frontend/src/lib/reauthMessage.ts` | 再認証画面の文言を選ぶ純粋関数(新規、Task 4) |
| `frontend/src/lib/reauthMessage.test.ts` | 上のテスト(新規、Task 4) |
| `frontend/src/ui/AddAccount.svelte` | `reauthMessage` を使う(Task 4) |
| `frontend/src/ui/AddAccount.test.ts` | 再認証の文言のテストを足す(Task 4) |
| `frontend/src/ui/ReauthBanner.svelte` | メイン画面の案内バナー(新規、Task 5) |
| `frontend/src/ui/ReauthBanner.test.ts` | 上のテスト(新規、Task 5) |
| `frontend/src/App.svelte` | バナーを置く(Task 5) |
| `docs/guide/user-guide.md` | インポート後の案内の説明を直す(Task 6) |

---

### Task 1: 確認ダイアログの件数を、取り込み後にトークンを持たない全アカウントにする(Rust)

**Files:**
- Modify: `src-tauri/src/commands/settings_transfer.rs`(`plan_checked`、テスト)
- Modify: `src-tauri/src/store/settings_transfer.rs:49-66`(docコメントのみ)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: `AppState::has_token(&self, account_id: &str) -> bool`、`ImportPlan { data: SettingsData, preview: ImportPreview, accounts_needing_login: Vec<String> }`(`plan.data.accounts: Vec<Account>`)
- Produces: `plan_checked` が返す `ImportPlan` の `preview.accounts_needing_login`(件数)と `accounts_needing_login`(ID一覧)が、取り込み後にトークンを持たない全アカウントを指す。`inspect_settings_bytes` / `import_settings_bytes` のシグネチャは変わらない。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/settings_transfer.rs` の `mod tests` に、`state_with_local_account` の直後へヘルパーを、末尾の `a_rejected_file_changes_nothing` の後へテストを足す。

ヘルパー(`state_with_local_account` の直後):

```rust
    /// 端末側にあるが、トークンを持たないアカウント(以前のインポートの取り込み直後など)。
    fn state_with_tokenless_local_account() -> AppState {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.accounts.lock().unwrap().upsert(account("local1", "misskey.test", "u1"));
        state.settings.upsert_account(&account("local1", "misskey.test", "u1")).unwrap();
        state
    }
```

テスト(`mod tests` の最後の `}` の直前):

```rust
    #[test]
    fn inspect_counts_a_matched_local_account_that_has_no_token() {
        let state = state_with_tokenless_local_account();
        // ファイル側の file1 は、端末側の local1(host+userIdが同じ)に付け替わる。
        let bytes = file_bytes(data_with(vec![account("file1", "misskey.test", "u1")], vec![]));

        let preview = inspect_settings_bytes(&state, &bytes).unwrap();

        assert_eq!(preview.accounts_needing_login, 1, "端末側でトークンが無いアカウントも数える");
    }

    #[test]
    fn inspect_does_not_count_a_matched_local_account_that_has_a_token() {
        let state = state_with_local_account(); // local1 はトークンあり
        let bytes = file_bytes(data_with(vec![account("file1", "misskey.test", "u1")], vec![]));

        let preview = inspect_settings_bytes(&state, &bytes).unwrap();

        assert_eq!(preview.accounts_needing_login, 0);
    }

    #[tokio::test]
    async fn import_reports_every_account_left_without_a_token() {
        let state = state_with_tokenless_local_account();
        let bytes = file_bytes(data_with(
            vec![account("file1", "misskey.test", "u1"), account("file2", "other.example", "u9")],
            vec![],
        ));

        let preview = inspect_settings_bytes(&state, &bytes).unwrap();
        let result = import_settings_bytes(&state, &bytes).await.unwrap();

        assert_eq!(preview.accounts_needing_login, 2);
        let mut ids = result.accounts_needing_login.clone();
        ids.sort();
        assert_eq!(ids, vec!["file2".to_string(), "local1".to_string()]);
        let mut actual = accounts_without_token_ids(&state);
        actual.sort();
        assert_eq!(actual, ids, "取り込み結果は、実際にトークンの無いアカウントと一致する");
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib settings_transfer::`(コマンド層のテスト名は `commands::settings_transfer::tests::...`)
Expected: `inspect_counts_a_matched_local_account_that_has_no_token` と `import_reports_every_account_left_without_a_token` が FAIL(件数が 0 / 1)。`inspect_does_not_count_a_matched_local_account_that_has_a_token` は PASS。

- [ ] **Step 3: `plan_checked` を直す**

`src-tauri/src/commands/settings_transfer.rs` の `plan_checked` を、次に置き換える。

```rust
fn plan_checked(state: &AppState, current: &SettingsData, file: ExportFile) -> Result<ImportPlan> {
    let mut plan = plan_import(current, file)?;
    if plan.accounts_needing_login.iter().any(|id| state.has_token(id)) {
        return Err(Error::Invalid("アカウントのIDがこの端末の保存済みトークンと衝突しています".into()));
    }
    // 取り込み後にトークンを持たないアカウント全体。ファイルにだけあって新しく追加されるものに加え、
    // 端末側の一致したアカウントのうちトークンが無いものも含める(Issue #553)。
    // 確認ダイアログの件数と、取り込み結果(ImportResult)の両方に使う。
    let tokenless: Vec<String> = plan
        .data
        .accounts
        .iter()
        .filter(|a| !state.has_token(&a.id))
        .map(|a| a.id.clone())
        .collect();
    plan.preview.accounts_needing_login = tokenless.len() as i32;
    plan.accounts_needing_login = tokenless;
    Ok(plan)
}
```

- [ ] **Step 4: docコメントを新しい意味に直す**

`src-tauri/src/store/settings_transfer.rs` の `ImportPreview` と `ImportResult` のコメントを直す。

`ImportPreview.accounts_needing_login` の上のコメント `/// ファイルにだけあり、トークンなしで追加されるアカウントの数。` を、次に置き換える。

```rust
    /// 取り込み後にトークンを持たないアカウントの数。ファイルにだけあって新しく追加されるものに加え、
    /// 端末側の一致したアカウントのうちトークンが無いものも含む(コマンド層で数える。`plan_import` が返す値は、
    /// 前者だけ)。
```

`ImportResult.accounts_needing_login` の上のコメント `/// トークンなしで追加したアカウントのID。再認証が必要。` を、次に置き換える。

```rust
    /// 取り込み後にトークンを持たないアカウントのID。再認証が必要。
```

`ImportPlan.accounts_needing_login`(同じファイルの `pub struct ImportPlan` 内)の上のコメント `/// ファイルにだけあり、トークンなしで追加するアカウントのID。` は、`plan_import` が返す値の説明としてそのまま正しい。末尾に1行足して、コマンド層での上書きを知らせる。

```rust
    /// ファイルにだけあり、トークンなしで追加するアカウントのID。
    /// コマンド層の `plan_checked` は、これを「取り込み後にトークンを持たない全アカウントのID」で上書きする。
```

- [ ] **Step 5: テストが通ることを確認し、bindings を再生成する**

Run: `cd src-tauri && cargo test --lib settings_transfer`
Expected: すべて PASS(既存の `inspect_does_not_change_anything` と `import_replaces_settings_keeps_the_local_account_and_reports_new_ones` も PASS)。

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`git diff frontend/src/bindings/tauri.gen.ts` に、`accountsNeedingLogin` の2か所のdocコメントの変更だけがあること(それ以外の差分があれば、原因を調べる)。

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/commands/settings_transfer.rs src-tauri/src/store/settings_transfer.rs frontend/src/bindings/tauri.gen.ts
git commit -m "fix(import): 確認ダイアログの再認証件数を取り込み後のトークンなし全体にする (#553)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: エラー文から `kind` と `Error:` の接頭辞を外す(フロント)

**Files:**
- Modify: `frontend/src/lib/ipc.ts:1-36`
- Create: `frontend/src/lib/ipc.errors.test.ts`

**Interfaces:**
- Consumes: `bindings/tauri.gen.ts` の `Error as ApiError`(`{kind: "network"|"unauthorized"|"forbidden"|"notFound"|"api"|"secret"|"db"|"invalid"; message: string} | {kind: "rateLimited"}`)
- Produces: `export class AppError extends Error`(`toString()` が `message` だけを返す)、`export class ForbiddenError extends AppError`(コンストラクタは従来どおり `(accountId: string, message: string)`)、`export function formatError(e: ApiError): string`(接頭辞なし)。`unwrap` / `unwrapAcc` は `AppError` / `ForbiddenError` を投げる。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/ipc.errors.test.ts` を作る。

```ts
import { describe, expect, it, vi } from "vitest";

vi.mock("../bindings/tauri.gen", () => ({ commands: {} }));
vi.mock("./soundChoice", () => ({ resolveSoundChoice: vi.fn() }));

import { AppError, ForbiddenError, formatError, unwrap, unwrapAcc } from "./ipc";

const err = (error: unknown) => Promise.resolve({ status: "error" as const, error: error as never });

async function caught(p: Promise<unknown>): Promise<unknown> {
  try {
    await p;
  } catch (e) {
    return e;
  }
  throw new Error("rejectされなかった");
}

describe("formatError", () => {
  it("kind の接頭辞を付けず、message だけを返す", () => {
    expect(formatError({ kind: "invalid", message: "tsumugiの設定ファイルではありません" })).toBe(
      "tsumugiの設定ファイルではありません",
    );
    expect(formatError({ kind: "network", message: "connect: refused" })).toBe("connect: refused");
  });

  it("message を持たない rateLimited は日本語にする", () => {
    expect(formatError({ kind: "rateLimited" })).toBe("リクエストが多すぎます。しばらく待ってからやり直してください");
  });
});

describe("unwrap / unwrapAcc が投げるエラー", () => {
  it("unwrap: String(e) が message だけになる(Error: も kind も付かない)", async () => {
    const e = await caught(unwrap(err({ kind: "invalid", message: "tsumugiの設定ファイルではありません" })));
    expect(e).toBeInstanceOf(AppError);
    expect(String(e)).toBe("tsumugiの設定ファイルではありません");
  });

  it("unwrapAcc: 403 は ForbiddenError(AppError の一種)で、accountId を持つ。String(e) は message だけ", async () => {
    const e = await caught(unwrapAcc("acc1", err({ kind: "forbidden", message: "権限が足りません" })));
    expect(e).toBeInstanceOf(ForbiddenError);
    expect(e).toBeInstanceOf(AppError);
    expect((e as ForbiddenError).accountId).toBe("acc1");
    expect(String(e)).toBe("権限が足りません");
  });

  it("unwrapAcc: 403 以外は AppError", async () => {
    const e = await caught(unwrapAcc("acc1", err({ kind: "network", message: "offline" })));
    expect(e).toBeInstanceOf(AppError);
    expect(e).not.toBeInstanceOf(ForbiddenError);
    expect(String(e)).toBe("offline");
  });

  it("IPC を経由しない通常のJS例外は、従来どおり Error: 付きで出る(バグと見分けるため)", () => {
    expect(String(new Error("boom"))).toBe("Error: boom");
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/lib/ipc.errors.test.ts`
Expected: FAIL(`AppError` が export されていない。`formatError` は `invalid: …` を返す)。

- [ ] **Step 3: `ipc.ts` を直す**

`frontend/src/lib/ipc.ts` の `unwrap` から `formatError` までの部分(`export async function unwrap` の行から `export function formatError` の関数の閉じ括弧まで)を、次に置き換える。

```ts
/// IPC(Rust 側の型付きエラー)から作るエラー。`String(e)` が `Error: ` を付けず、message だけを返す
/// (各画面は `String(e)` をそのまま表示するため。Issue #553)。
/// IPC を経由しない通常の JS 例外は従来どおり `Error: …` で出るので、バグと見分けられる。
export class AppError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AppError";
  }
  override toString(): string {
    return this.message;
  }
}

export async function unwrap<T>(p: Promise<Result<T>>): Promise<T> {
  const r = await p;
  if (r.status === "ok") return r.data;
  throw new AppError(formatError(r.error));
}

/// 403 (kind: "forbidden") を ForbiddenError として throw する unwrap。
/// 呼び出し元は accountId を渡し、store 側で「再認証」アクションをログに出せるようにする。
export class ForbiddenError extends AppError {
  accountId: string;
  constructor(accountId: string, message: string) {
    super(message);
    this.name = "ForbiddenError";
    this.accountId = accountId;
  }
}

export async function unwrapAcc<T>(accountId: string, p: Promise<Result<T>>): Promise<T> {
  const r = await p;
  if (r.status === "ok") return r.data;
  if (r.error.kind === "forbidden") throw new ForbiddenError(accountId, formatError(r.error));
  throw new AppError(formatError(r.error));
}

/// 利用者に見せるエラー文。`kind` の接頭辞は付けず、message だけを返す。
/// message を持たない種別は日本語の文にする。
export function formatError(e: ApiError): string {
  if ("message" in e) return e.message;
  switch (e.kind) {
    case "rateLimited":
      return "リクエストが多すぎます。しばらく待ってからやり直してください";
  }
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/lib/ipc.errors.test.ts`
Expected: PASS(5件)。

Run: `cd frontend && pnpm check`
Expected: エラーなし(`formatError` の `switch` は、`rateLimited` が唯一の message なし種別なので網羅的)。

- [ ] **Step 5: 既存テストへの影響を確認する**

Run: `cd frontend && pnpm test`
Expected: すべて PASS。接頭辞付きの文字列を期待して落ちるテストがあれば、期待を新しい出力(message のみ)に直す。`FollowListModal.test.ts` と `ProfileModal.test.ts` の `Error: network error` は、`new Error(...)` を直接 reject しているため変わらない(直さない)。

- [ ] **Step 6: コミット**

```bash
git add frontend/src/lib/ipc.ts frontend/src/lib/ipc.errors.test.ts
git commit -m "fix(ipc): エラー文から kind と Error: の接頭辞を外す (#553)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: store に `needsLogin` を持たせ、トークンなしアカウントへの通信をスキップする

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`(フィールド追加、`boot`、`#syncAllServerMutes`、`#refreshInstanceMeta`、`completeAccount`、`removeAccount`)
- Create: `frontend/src/lib/store.svelte.needsLogin.test.ts`

**Interfaces:**
- Consumes: `commands.accountsWithoutToken(): Promise<Result<string[]>>`(`unwrap` で包む)
- Produces: `app.needsLogin: Set<string>`(`$state`。更新は必ず新しい `Set` への差し替え)。Task 4・5 が読む。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/store.svelte.needsLogin.test.ts` を作る。

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Account, Column, OpenedColumn } from "../bindings/tauri.gen";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("./uiScale", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./uiScale")>()),
  applyUiScale: vi.fn().mockResolvedValue(undefined),
}));

const { app } = await import("./store.svelte");

const SIX_HOURS = 6 * 60 * 60 * 1000;

function account(id: string): Account {
  return {
    id,
    host: "misskey.test",
    username: id,
    userId: `u-${id}`,
    displayName: id,
    avatarUrl: null,
    instance: null,
    isCat: false,
    avatarBlurhash: null,
  };
}

function column(id: string, accountId: string): Column {
  return {
    id,
    accountId,
    kind: { type: "home" },
    order: 0,
    filter: { kind: "keywords", value: [] },
    notifySound: false,
    notifyDesktop: false,
    notifySoundChoice: "",
    groupId: "g1",
    title: null,
  };
}

function opened(c: Column): OpenedColumn {
  return { column: c, group: { id: "g1", order: 0, width: 300, auto: false }, notes: [], notifications: [] };
}

/// 呼ばれたコマンドの引数(呼ばれた順)。
const callsOf = (cmd: string) =>
  invokeMock.mock.calls.filter(([c]) => c === cmd).map(([, args]) => args as Record<string, string>);

let tokenless: string[] | Error;

beforeEach(() => {
  tokenless = ["acc2"];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, string>) => {
    switch (cmd) {
      case "list_accounts":
        return [account("acc1"), account("acc2")];
      case "accounts_without_token":
        if (tokenless instanceof Error) throw tokenless;
        return tokenless;
      case "get_ui_prefs":
        return { ...app.ui };
      case "sync_server_mutes":
        return { blockedUsers: 0, wordRules: 0 };
      case "refresh_instance_meta":
        return account(args!.accountId);
      case "list_groups":
        return [{ id: "g1", order: 0, width: 300, auto: false }];
      case "load_pane_layout":
        return null;
      case "list_columns":
        return [column("c1", "acc1"), column("c2", "acc2")];
      case "resume_column":
        return opened(column(args!.columnId, args!.columnId === "c1" ? "acc1" : "acc2"));
      default:
        return { status: "ok", data: null };
    }
  });
  app.logs = [];
  app.ui = { ...app.ui, theme: "auto" };
  app.needsLogin = new Set();
});

afterEach(() => {
  app.teardown();
  vi.useRealTimers();
  app.accounts = [];
  app.groups = [];
  app.needsLogin = new Set();
});

async function boot() {
  vi.useFakeTimers();
  const bootPromise = app.boot();
  await vi.advanceTimersByTimeAsync(0);
  await bootPromise.catch(() => {});
}

describe("トークンなしアカウントの扱い(Issue #553)", () => {
  it("起動時、トークンなしのアカウントには、ミュート同期・インスタンス情報取得・タブ復元をしない", async () => {
    await boot();

    expect([...app.needsLogin]).toEqual(["acc2"]);
    expect(callsOf("sync_server_mutes").map((a) => a.accountId)).toEqual(["acc1"]);
    expect(callsOf("refresh_instance_meta").map((a) => a.accountId)).toEqual(["acc1"]);
    expect(callsOf("resume_column").map((a) => a.columnId)).toEqual(["c1"]);
  });

  it("起動時に、エラーも警告もログに出さない", async () => {
    await boot();

    expect(app.logs.filter((l) => l.level === "error" || l.level === "warn")).toEqual([]);
  });

  it("accounts_without_token が失敗したら、従来どおり全アカウントを対象にする(起動は止めない)", async () => {
    tokenless = new Error("keyring unavailable");

    await boot();

    expect(app.needsLogin.size).toBe(0);
    expect(callsOf("sync_server_mutes").map((a) => a.accountId).sort()).toEqual(["acc1", "acc2"]);
    expect(callsOf("resume_column").map((a) => a.columnId).sort()).toEqual(["c1", "c2"]);
  });

  it("6時間ごとの定期同期でも、トークンなしのアカウントを同期しない", async () => {
    await boot();
    invokeMock.mockClear();

    await vi.advanceTimersByTimeAsync(SIX_HOURS + 1_000);

    expect(callsOf("sync_server_mutes").map((a) => a.accountId)).toEqual(["acc1"]);
  });

  it("再認証が完了したら needsLogin から外し、そのアカウントのミュート同期とカラム復元を行う", async () => {
    app.accounts = [account("acc2")];
    app.groups = [{ id: "g1", width: 300, auto: false, tabs: [], activeTabId: "" }];
    app.needsLogin = new Set(["acc2"]);
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, string>) => {
      switch (cmd) {
        case "complete_miauth":
          return account("acc2");
        case "sync_server_mutes":
          return { blockedUsers: 0, wordRules: 0 };
        case "list_columns":
          return [column("c2", "acc2")];
        case "resume_column":
          return opened(column(args!.columnId, "acc2"));
        default:
          return { status: "ok", data: null };
      }
    });

    await app.completeAccount("session1");

    expect(app.needsLogin.size).toBe(0);
    expect(callsOf("sync_server_mutes").map((a) => a.accountId)).toEqual(["acc2"]);
    expect(callsOf("resume_column").map((a) => a.columnId)).toEqual(["c2"]);
  });

  it("アカウントを削除したら needsLogin から外す", async () => {
    app.accounts = [account("acc1"), account("acc2")];
    app.needsLogin = new Set(["acc2"]);

    await app.removeAccount("acc2");

    expect(app.needsLogin.size).toBe(0);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/lib/store.svelte.needsLogin.test.ts`
Expected: FAIL(`needsLogin` がまだ無いので、`[...app.needsLogin]` が空、`sync_server_mutes` が `["acc1","acc2"]` など)。

- [ ] **Step 3: store にフィールドと補助メソッドを足す**

`frontend/src/lib/store.svelte.ts` の `accounts = $state<Account[]>([]);`(147行付近)の直後に足す。

```ts
  /// トークンを持たないアカウントのID(設定のインポート直後など。Issue #544/#553)。起動時に取得し、
  /// 再認証・削除で更新する。Set は $state の深い追跡の対象ではないため、更新は必ず新しい Set に差し替える。
  needsLogin = $state<Set<string>>(new Set());
```

`#myUserIds()` の直前(`#myUserIds(): Set<string> {` の行の前)に、補助メソッドを足す。

```ts
  /// トークンを持たないアカウントのIDを取り直す。取れなければ空にする(従来どおり全アカウントを対象にする)。
  /// 失敗してもログは出さない(AccountsSection の「要ログイン」バッジと同じ扱いで、補助情報のため)。
  async #loadNeedsLogin() {
    try {
      this.needsLogin = new Set(await unwrap(commands.accountsWithoutToken()));
    } catch {
      this.needsLogin = new Set();
    }
  }

  #clearNeedsLogin(accountId: string) {
    if (!this.needsLogin.has(accountId)) return;
    this.needsLogin = new Set([...this.needsLogin].filter((id) => id !== accountId));
  }

  /// 通信してよいアカウント(トークンを持つもの)。
  #loggedInAccounts(): Account[] {
    return this.accounts.filter((a) => !this.needsLogin.has(a.id));
  }
```

- [ ] **Step 4: 起動・定期同期・インスタンス情報のスキップを入れる**

`boot()` の中:

old:
```ts
      this.accounts = await unwrap(commands.listAccounts());
      void this.#refreshInstanceMeta();
```
new:
```ts
      this.accounts = await unwrap(commands.listAccounts());
      await this.#loadNeedsLogin();
      void this.#refreshInstanceMeta();
```

old:
```ts
      await Promise.all(this.accounts.map((a) => this.#syncServerMutes(a.id)));
      await this.#subscribe();
```
new:
```ts
      await Promise.all(this.#loggedInAccounts().map((a) => this.#syncServerMutes(a.id)));
      await this.#subscribe();
```

old:
```ts
      for (const tab of tabDefs) {
        try {
          const opened = await unwrap(commands.resumeColumn(tab.id));
```
new:
```ts
      for (const tab of tabDefs) {
        // トークンなしのアカウントのカラムは開かない(再認証の完了後に #resumeAccountColumns が開く)。
        if (this.needsLogin.has(tab.accountId)) continue;
        try {
          const opened = await unwrap(commands.resumeColumn(tab.id));
```

`#syncAllServerMutes` の中:

old: `await Promise.all(this.accounts.map((a) => this.#syncServerMutes(a.id, { quiet: true })));`
new: `await Promise.all(this.#loggedInAccounts().map((a) => this.#syncServerMutes(a.id, { quiet: true })));`

`#refreshInstanceMeta` の中:

old: `this.accounts.map((a) => unwrapAcc(a.id, commands.refreshInstanceMeta(a.id))),`
new: `this.#loggedInAccounts().map((a) => unwrapAcc(a.id, commands.refreshInstanceMeta(a.id))),`

(`#refreshInstanceMeta` の `for (const r of results)` では `updated` を使うだけなので、他は変えない。)

- [ ] **Step 5: 再認証・削除での更新を入れる**

`completeAccount` の中:

old:
```ts
    await this.#syncServerMutes(account.id);
    await this.#resumeAccountColumns(account.id);
```
new:
```ts
    this.#clearNeedsLogin(account.id); // 認証が通ったので、通信の対象に戻す
    await this.#syncServerMutes(account.id);
    await this.#resumeAccountColumns(account.id);
```

`removeAccount` の中:

old:
```ts
    this.accounts = this.accounts.filter((a) => a.id !== accountId);
    for (const t of this.#allTabs().filter((t) => t.accountId === accountId)) {
```
new:
```ts
    this.accounts = this.accounts.filter((a) => a.id !== accountId);
    this.#clearNeedsLogin(accountId);
    for (const t of this.#allTabs().filter((t) => t.accountId === accountId)) {
```

- [ ] **Step 6: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/lib/store.svelte.needsLogin.test.ts`
Expected: PASS(6件)。`boot` がタブ復元まで進まず「起動時、…」が落ちる場合は、`beforeEach` のモックに足りないコマンドがある。失敗メッセージの呼び出し一覧を見て、そのコマンドの戻り値を `switch` に足す(アサーションは変えない)。

Run: `cd frontend && npx vitest run src/lib/store.svelte.test.ts src/lib/store.svelte.resumeColumns.test.ts`
Expected: PASS(既存の boot のテストでは `accounts_without_token` が object を返して `new Set(...)` が例外になるが、`#loadNeedsLogin` が握って空集合に戻すため、従来どおりに動く)。

Run: `cd frontend && pnpm check`
Expected: エラーなし。

- [ ] **Step 7: コミット**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.needsLogin.test.ts
git commit -m "fix(store): トークンなしアカウントの同期・取得・復元を起動時にスキップする (#553)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 再認証画面の文言を、理由に応じて切り替える

**Files:**
- Create: `frontend/src/lib/reauthMessage.ts`
- Create: `frontend/src/lib/reauthMessage.test.ts`
- Modify: `frontend/src/ui/AddAccount.svelte`(import と、文言を選ぶ三項演算子)
- Modify: `frontend/src/ui/AddAccount.test.ts`(文言のテストを足す)

**Interfaces:**
- Consumes: `app.needsLogin: Set<string>`(Task 3)
- Produces: `export function reauthMessage(needsLogin: boolean): string`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/reauthMessage.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { reauthMessage } from "./reauthMessage";

describe("reauthMessage", () => {
  it("トークンなし(設定のインポートで追加された)アカウントは、ログインを促す文言", () => {
    const m = reauthMessage(true);
    expect(m).toContain("設定のインポートで追加され");
    expect(m).toContain("まだログインしていません");
    expect(m).not.toContain("スコープ");
  });

  it("トークンはあるが再認証する(スコープ更新)ときは、従来の文言", () => {
    expect(reauthMessage(false)).toBe(
      "スコープが更新されたトークンを取得します。ブラウザで認可を完了したら、下のボタンを押してください。",
    );
  });
});
```

`frontend/src/ui/AddAccount.test.ts` の `describe("AddAccount: 設定のインポート導線(Issue #544)"` ブロックの後ろ(ファイル末尾)に足す。`afterEach` に `app.needsLogin = new Set();` も足す(`app.accounts = [];` の次の行)。

```ts
describe("AddAccount: 再認証画面の文言(Issue #553)", () => {
  const reauth = { id: "a1", host: "misskey.test", username: "alice" };

  it("トークンなしのアカウントでは、インポート後の文脈の文言を出す", async () => {
    vi.spyOn(app, "addAccount").mockResolvedValue("session");
    app.needsLogin = new Set(["a1"]);

    render(AddAccount, { props: { reauthAccount: reauth } });

    await vi.waitFor(() => expect(screen.getByText(/まだログインしていません/)).toBeTruthy());
    expect(screen.queryByText(/スコープが更新/)).toBeNull();
  });

  it("トークンがあるアカウントの再認証では、スコープ更新の文言を出す", async () => {
    vi.spyOn(app, "addAccount").mockResolvedValue("session");
    app.needsLogin = new Set();

    render(AddAccount, { props: { reauthAccount: reauth } });

    await vi.waitFor(() => expect(screen.getByText(/スコープが更新されたトークンを取得します/)).toBeTruthy());
    expect(screen.queryByText(/まだログインしていません/)).toBeNull();
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/lib/reauthMessage.test.ts src/ui/AddAccount.test.ts`
Expected: `reauthMessage.test.ts` は import 失敗、`AddAccount.test.ts` の新しい1件目が FAIL(常にスコープ更新の文言)。

- [ ] **Step 3: 実装する**

`frontend/src/lib/reauthMessage.ts`:

```ts
/// 再認証画面の説明文(AddAccount.svelte の再認証フロー)。
/// needsLogin が true なら、設定のインポートで追加されたトークンなしのアカウント。
/// false なら、トークンはあるがスコープ更新などで取り直すアカウント。
export function reauthMessage(needsLogin: boolean): string {
  return needsLogin
    ? "このアカウントは設定のインポートで追加され、まだログインしていません。ブラウザで認可を完了したら、下のボタンを押してください。"
    : "スコープが更新されたトークンを取得します。ブラウザで認可を完了したら、下のボタンを押してください。";
}
```

`frontend/src/ui/AddAccount.svelte`:

import を足す(`import { app } from "../lib/store.svelte";` の次の行):

```ts
  import { reauthMessage } from "../lib/reauthMessage";
```

old:
```svelte
      {reauthAccount
        ? "スコープが更新されたトークンを取得します。ブラウザで認可を完了したら、下のボタンを押してください。"
        : "ブラウザで認可を完了したら、下のボタンを押してください。"}
```
new:
```svelte
      {reauthAccount
        ? reauthMessage(app.needsLogin.has(reauthAccount.id))
        : "ブラウザで認可を完了したら、下のボタンを押してください。"}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/lib/reauthMessage.test.ts src/ui/AddAccount.test.ts`
Expected: PASS。

Run: `cd frontend && pnpm check`
Expected: エラーなし。

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/reauthMessage.ts frontend/src/lib/reauthMessage.test.ts frontend/src/ui/AddAccount.svelte frontend/src/ui/AddAccount.test.ts
git commit -m "fix(account): 再認証画面の文言をトークンなしの場合に切り替える (#553)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: メイン画面に再認証の案内バナーを出す

**Files:**
- Create: `frontend/src/ui/ReauthBanner.svelte`
- Create: `frontend/src/ui/ReauthBanner.test.ts`
- Modify: `frontend/src/App.svelte`(import、`<main>` の直前にバナー、`<main>` のセーフエリア指定)

**Interfaces:**
- Consumes: `app.needsLogin: Set<string>`(Task 3)、`openSettings(section)`(`App.svelte` 内)
- Produces: `ReauthBanner.svelte`(props: `onOpenAccounts: () => void`)。`app.needsLogin.size > 0` のときだけ描画する。`data-testid="reauth-banner"` と、ボタンの `data-testid="reauth-banner-open"`。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/ReauthBanner.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { app } from "../lib/store.svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: ReauthBanner } = await import("./ReauthBanner.svelte");

afterEach(() => {
  cleanup();
  app.needsLogin = new Set();
});

describe("ReauthBanner(Issue #553)", () => {
  it("再認証が必要なアカウントが無いときは、何も描画しない", () => {
    app.needsLogin = new Set();
    render(ReauthBanner, { props: { onOpenAccounts: () => {} } });
    expect(screen.queryByTestId("reauth-banner")).toBeNull();
  });

  it("件数を表示する", () => {
    app.needsLogin = new Set(["a1", "a2", "a3"]);
    render(ReauthBanner, { props: { onOpenAccounts: () => {} } });
    expect(screen.getByTestId("reauth-banner").textContent).toContain("3件のアカウントは再認証が必要です");
  });

  it("ボタンで、設定のアカウントを開く導線を呼ぶ", async () => {
    app.needsLogin = new Set(["a1"]);
    const open = vi.fn();
    render(ReauthBanner, { props: { onOpenAccounts: open } });

    await fireEvent.click(screen.getByTestId("reauth-banner-open"));

    expect(open).toHaveBeenCalledTimes(1);
  });

  it("再認証が済んで0件になったら消える", async () => {
    app.needsLogin = new Set(["a1"]);
    render(ReauthBanner, { props: { onOpenAccounts: () => {} } });
    expect(screen.getByTestId("reauth-banner")).toBeTruthy();

    app.needsLogin = new Set();

    await vi.waitFor(() => expect(screen.queryByTestId("reauth-banner")).toBeNull());
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/ui/ReauthBanner.test.ts`
Expected: FAIL(`ReauthBanner.svelte` が無い)。

- [ ] **Step 3: コンポーネントを作る**

`frontend/src/ui/ReauthBanner.svelte`:

```svelte
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { Button } from "$lib/components/ui/button";
  import { TriangleAlert } from "@lucide/svelte";

  // 設定のインポートで追加された、トークンなしのアカウントがあるときの案内(Issue #553)。
  // 再認証が済んで件数が0になれば自然に消えるので、閉じるボタンは持たない。
  let { onOpenAccounts }: { onOpenAccounts: () => void } = $props();
</script>

{#if app.needsLogin.size > 0}
  <div
    class="flex items-center gap-2 border-b border-border bg-muted px-3 py-2 text-sm"
    role="status"
    data-testid="reauth-banner"
  >
    <TriangleAlert size={16} class="flex-none text-[var(--warning)]" />
    <span class="min-w-0 flex-1">{app.needsLogin.size}件のアカウントは再認証が必要です</span>
    <Button variant="outline" size="xs" onclick={onOpenAccounts} data-testid="reauth-banner-open">
      設定のアカウントを開く
    </Button>
  </div>
{/if}
```

- [ ] **Step 4: `App.svelte` に置く**

`frontend/src/App.svelte`:

import を足す(`import AddAccount from "./ui/AddAccount.svelte";` の次の行):

```ts
  import ReauthBanner from "./ui/ReauthBanner.svelte";
```

`let reauthAccount = $state<Account | null>(null);` の直後に足す:

```ts
  // 再認証が必要なアカウントの案内。アカウント追加/再認証の画面では出さない(その画面が案内そのもの)。
  const showReauthBanner = $derived(
    !app.booting && app.accounts.length > 0 && app.needsLogin.size > 0 && !showAdd && !reauthAccount,
  );
```

old:
```svelte
  <main class="min-h-0 min-w-0 flex-1" class:pt-[var(--safe-top)]={useMobileUi}>
```
new(バナーを `main` の直前に置き、バナーがある間は、スマホのセーフエリアをバナー側で確保する):
```svelte
  {#if showReauthBanner}
    <div class="flex-none" class:pt-[var(--safe-top)]={useMobileUi}>
      <ReauthBanner onOpenAccounts={() => openSettings("accounts")} />
    </div>
  {/if}

  <main class="min-h-0 min-w-0 flex-1" class:pt-[var(--safe-top)]={useMobileUi && !showReauthBanner}>
```

(`showAdd` は `App.svelte` で `$state(false)` として `reauthAccount` より後に宣言されているため、`$derived` の位置は `let showAdd = ...` の後になるようにする。宣言順で参照エラーになる場合は、`$derived` を `showAdd` の宣言の直後へ移す。)

- [ ] **Step 5: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/ReauthBanner.test.ts`
Expected: PASS(4件)。

Run: `cd frontend && pnpm check && pnpm test`
Expected: エラーなし、すべて PASS。

- [ ] **Step 6: コミット**

```bash
git add frontend/src/ui/ReauthBanner.svelte frontend/src/ui/ReauthBanner.test.ts frontend/src/App.svelte
git commit -m "feat(ui): 再認証が必要なアカウントがあるとき、メイン画面に案内を出す (#553)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: ユーザーガイドの更新と、全体検証、実画面の確認

**Files:**
- Modify: `docs/guide/user-guide.md`(「設定のバックアップ」の節、262行付近・264行付近)
- 実画面の確認用の使い捨て spec(コミットしない): `e2e/` 配下に置き、確認後に削除する

**Interfaces:**
- Consumes: Task 1〜5 の成果物すべて。

- [ ] **Step 1: ユーザーガイドを直す**

`docs/guide/user-guide.md` の「設定のバックアップ(エクスポート/インポート)」の節で、次の2か所を直す。

old: `  - ファイルにだけあるアカウントは「要ログイン」として追加されます。設定の「アカウント」で「再認証」を押してログインすると、そのアカウントのカラムが表示されます。`
new:
```
  - ファイルにだけあるアカウントは「要ログイン」として追加されます。取り込み後、メイン画面の上部に「N件のアカウントは再認証が必要です」と案内が出るので、「設定のアカウントを開く」から「再認証」を押してログインしてください。ログインすると、そのアカウントのカラムが表示され、案内の件数が減ります(0件になると案内は消えます)。ログインするまで、そのアカウントのカラムは空のままで、サーバーとの通信は行いません。
  - 確認ダイアログの「再認証が必要です」の件数には、この端末にすでにあってトークンを持たないアカウントも含まれます。
```

old: `取り込み後は、設定の「アカウント」で各アカウントを再認証してください。`
new: `取り込み後は、メイン画面の上部の案内から設定の「アカウント」を開き、各アカウントを再認証してください。`

- [ ] **Step 2: 全体のテストと型検査**

Run: `cd src-tauri && cargo test`
Expected: すべて PASS(`#[ignore]` のテストは走らない)。

Run: `cd frontend && pnpm check && pnpm test`
Expected: エラーなし、すべて PASS。

Run: `git status --short`
Expected: `frontend/src/bindings/tauri.gen.ts` に、Task 1 のコミット以外の差分が出ていないこと。

- [ ] **Step 3: 実画面の確認(受け入れ条件)**

確認の手順は、プロジェクトの `e2e/README.md` と、メモリの `reference-real-screen-check-via-e2e-compose` / `feedback-dev-server-verification-must-use-virtual-display` / `reference-isolated-instance-needs-short-xdg-path` に従う。要点:

1. `e2e/README.md` を読み、Compose の Misskey の起動方法と実行順を確認する。
2. 使い捨ての wdio spec(コミットしない)を `e2e/` に置き、隔離インスタンス(`/tmp` 直下の短い XDG パス、Xvfb、`WAYLAND_DISPLAY` を unset、`dbus-run-session`)で、次を撮影する。
   - トークンなしのアカウントを含む設定ファイルをインポートした直後のメイン画面(上部のバナー「N件のアカウントは再認証が必要です」と、ボタン)。
   - Backstage(ログ欄)に、赤いエラー(`unauthorized: no token for account: …`)も、インスタンス情報・サーバーミュート同期の警告も出ていないこと。
   - バナーの「設定のアカウントを開く」で、設定のアカウントが開くこと。
   - 「再認証」を押した画面の文言が「このアカウントは設定のインポートで追加され、まだログインしていません。…」になっていること。
   - 設定のバックアップで不正なファイルをインポートしたときの表示が、「Error: invalid: …」ではなく、message だけになっていること。
   - 確認ダイアログの件数が、端末側の一致したトークンなしアカウントを含んで数えられること(トークンなしのアカウントがすでにある端末で、同じアカウントを含むファイルをインポートする)。
3. 撮影した画像は、Read ツールで開いて自分の目で確認する(テストが通っただけでは完了にしない)。
4. 確認用に自分で起動した `Xvfb`、`dbus-run-session`、`gnome-keyring-daemon`、`tsumugi` は、PID を指定して自分で止める(`pkill` / `killall` は使わない)。Compose の Misskey も止める。使い捨ての spec とスクリーンショットは、リポジトリに残さない。

Expected: 上の6点がすべて期待どおり。ずれがあれば、該当するタスクに戻って直す。

- [ ] **Step 4: コミット**

```bash
git add docs/guide/user-guide.md
git commit -m "docs(guide): インポート後の再認証の案内を反映する (#553)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 5: 完了の扱い**

`superpowers:verification-before-completion` で、上の検証の結果を再確認してから、`superpowers:finishing-a-development-branch` に進む。PR は `Fixes #553` を本文に入れ、`.github/pull_request_template.md` の構成に沿って書く(`gh pr create --body` はテンプレートを適用しない)。push 後に CI を待たない。マージは通常のマージコミット(`gh pr merge --merge`)。
