# メンションアバターアイコン Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ノート本文中の `@mention` 表記（`MfmNode.svelte` の `mention` ノード）に、対象ユーザーのアバターアイコンをインライン表示する。

**Architecture:** 既存の `resolve_user_acct` コマンド（Rust側追加なし）を `app.resolveUserSilently()` 経由で呼び、`MfmNode.svelte` にセッション内インメモリキャッシュ（新規モジュール `mentionAvatar.svelte.ts`）を介して結果を渡す。`accountId` は `app.defaultAccountId()` にフォールバックし、`Mfm`/`MfmNode` へのprop配線は増やさない。

**Tech Stack:** Svelte 5（runes: `$state`/`$effect`）、TypeScript、Vitest + @testing-library/svelte。

## Global Constraints

- 新規Rustコマンドは追加しない。既存の `resolve_user_acct`（`src-tauri/src/commands/column.rs`）をフロント側から `app.resolveUserSilently(accountId, acct)` 経由で呼ぶ。
- `Mfm.svelte` / `MfmNode.svelte` に `accountId` propは追加しない。`app.defaultAccountId()` にフォールバックする（既存の mentionクリック→`openProfile` と同じ慣例）。
- キャッシュはセッション内インメモリのみ（永続化しない）。DB/Tauri側の永続層は使わない。
- アイコンは16px（`h-4 w-4`）・`rounded-md`（スタイルガイド§2のアバター規約）。取得前・失敗時はアイコンなしにフォールバックする（エラーを投げない）。
- 失敗結果（`null`）もキャッシュし、同一acctへの再フェッチを防ぐ。
- コメントは日本語、既存コードの密度・スタイルに合わせる。
- 参照spec: `docs/superpowers/specs/2026-08-25-mention-avatar-icon-design.md`

---

## Task 1: mentionAvatarキャッシュモジュール

**Files:**
- Create: `frontend/src/lib/mentionAvatar.svelte.ts`
- Test: `frontend/src/lib/mentionAvatar.svelte.test.ts`

**Interfaces:**
- Consumes: `app.defaultAccountId(): string`、`app.resolveUserSilently(accountId: string, acct: string): Promise<User>`（`User.avatarUrl: string | null`）— いずれも `frontend/src/lib/store.svelte.ts` の既存メソッド。
- Produces:
  - `cachedAvatarUrl(username: string, host: string | null): string | null | undefined`（`undefined`=未取得、`null`=解決失敗/アバター未設定、`string`=URL）
  - `fetchAvatarUrl(username: string, host: string | null): Promise<string | null>`
  - これらはTask 2の `MfmNode.svelte` から `import { cachedAvatarUrl, fetchAvatarUrl } from "../lib/mentionAvatar.svelte"` として使われる。

- [ ] **Step 1: 失敗するテストを書く（初期状態は未キャッシュ）**

`frontend/src/lib/mentionAvatar.svelte.test.ts` を新規作成:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";

const resolveUserSilently = vi.fn();
const defaultAccountId = vi.fn();

vi.mock("./store.svelte", () => ({
  app: {
    defaultAccountId: (...args: unknown[]) => defaultAccountId(...args),
    resolveUserSilently: (...args: unknown[]) => resolveUserSilently(...args),
  },
}));

const { cachedAvatarUrl, fetchAvatarUrl } = await import("./mentionAvatar.svelte");

afterEach(() => {
  resolveUserSilently.mockReset();
  defaultAccountId.mockReset();
});

describe("mentionAvatar", () => {
  it("初期状態は未キャッシュ(undefined)", () => {
    expect(cachedAvatarUrl("fresh-user-1", null)).toBeUndefined();
  });

  it("解決に成功するとavatarUrlをキャッシュする", async () => {
    defaultAccountId.mockReturnValue("acc1");
    resolveUserSilently.mockResolvedValue({
      id: "u1",
      username: "alice",
      host: null,
      name: "Alice",
      avatarUrl: "https://example.com/alice.png",
      isBot: false,
      isCat: false,
      followersCount: 0,
      followingCount: 0,
      notesCount: 0,
    });

    const result = await fetchAvatarUrl("alice-success", null);

    expect(result).toBe("https://example.com/alice.png");
    expect(cachedAvatarUrl("alice-success", null)).toBe("https://example.com/alice.png");
    expect(resolveUserSilently).toHaveBeenCalledWith("acc1", "alice-success");
  });

  it("解決に失敗した場合はnullをキャッシュし、以後は再フェッチしない", async () => {
    defaultAccountId.mockReturnValue("acc1");
    resolveUserSilently.mockRejectedValue(new Error("network error"));

    const result = await fetchAvatarUrl("bob-fail", null);

    expect(result).toBeNull();
    expect(cachedAvatarUrl("bob-fail", null)).toBeNull();

    // 2回目はキャッシュから返るのでresolveUserSilentlyは呼ばれない
    resolveUserSilently.mockClear();
    const second = await fetchAvatarUrl("bob-fail", null);
    expect(second).toBeNull();
    expect(resolveUserSilently).not.toHaveBeenCalled();
  });

  it("同一acctへの同時呼び出しは1回のリクエストに集約する（重複排除）", async () => {
    defaultAccountId.mockReturnValue("acc1");
    let resolveFn!: (v: unknown) => void;
    resolveUserSilently.mockReturnValue(
      new Promise((r) => {
        resolveFn = r;
      }),
    );

    const p1 = fetchAvatarUrl("carol-dedup", "remote.example");
    const p2 = fetchAvatarUrl("carol-dedup", "remote.example");
    expect(resolveUserSilently).toHaveBeenCalledTimes(1);
    expect(resolveUserSilently).toHaveBeenCalledWith("acc1", "carol-dedup@remote.example");

    resolveFn({
      id: "u2",
      username: "carol-dedup",
      host: "remote.example",
      name: null,
      avatarUrl: "https://remote.example/carol.png",
      isBot: false,
      isCat: false,
      followersCount: 0,
      followingCount: 0,
      notesCount: 0,
    });

    const [r1, r2] = await Promise.all([p1, p2]);
    expect(r1).toBe("https://remote.example/carol.png");
    expect(r2).toBe("https://remote.example/carol.png");
  });

  it("アカウント未設定（defaultAccountIdが空文字）の場合はnullを返しリクエストしない", async () => {
    defaultAccountId.mockReturnValue("");

    const result = await fetchAvatarUrl("dave-noaccount", null);

    expect(result).toBeNull();
    expect(resolveUserSilently).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: テストを実行し失敗を確認**

Run: `cd frontend && pnpm vitest run src/lib/mentionAvatar.svelte.test.ts`
Expected: FAIL（`./mentionAvatar.svelte` が存在しない）

- [ ] **Step 3: 最小実装を書く**

`frontend/src/lib/mentionAvatar.svelte.ts` を新規作成:

```ts
import { app } from "./store.svelte";

// メンション本文中のアバターアイコン表示用: acctごとにセッション内でアバターURLをキャッシュする。
// 同一ユーザーへの複数メンション（同一ノート内・別ノート間）で resolve_user_acct を重複して
// 叩かないようにするため。値の意味:
//   - キャッシュ未登録（Map.get が undefined）: 未取得
//   - null: 解決失敗（リモート到達不可等）またはアバター未設定。以後リトライしない
//   - string: 解決済みのアバターURL
const cache = new Map<string, string | null>();
// 同一acctへの同時フェッチを1回のリクエストに集約するための in-flight Promise。
const inflight = new Map<string, Promise<string | null>>();

function acctKey(username: string, host: string | null): string {
  return host ? `${username}@${host}` : username;
}

/// キャッシュ済みなら即値を返す（同期的にレンダリング判定するため）。未取得ならundefined。
export function cachedAvatarUrl(username: string, host: string | null): string | null | undefined {
  return cache.get(acctKey(username, host));
}

/// acctからアバターURLを解決する。`app.defaultAccountId()` を使い、mentionクリック時の
/// openProfile と同じフォールバック慣例に倣う（呼び出し元でaccountIdを配線しない）。
/// 失敗時・アカウント未設定時はnullを返し、以後の再フェッチを避けるためnullをキャッシュする。
export async function fetchAvatarUrl(username: string, host: string | null): Promise<string | null> {
  const key = acctKey(username, host);
  const cached = cache.get(key);
  if (cached !== undefined) return cached;

  const existing = inflight.get(key);
  if (existing) return existing;

  const promise = resolve(key).finally(() => inflight.delete(key));
  inflight.set(key, promise);
  const result = await promise;
  cache.set(key, result);
  return result;
}

async function resolve(acct: string): Promise<string | null> {
  const accountId = app.defaultAccountId();
  if (!accountId) return null;
  try {
    const user = await app.resolveUserSilently(accountId, acct);
    return user.avatarUrl ?? null;
  } catch {
    return null;
  }
}
```

- [ ] **Step 4: テストを実行し成功を確認**

Run: `cd frontend && pnpm vitest run src/lib/mentionAvatar.svelte.test.ts`
Expected: PASS（5件全て）

- [ ] **Step 5: 型チェック**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 6: コミット**

```bash
git add frontend/src/lib/mentionAvatar.svelte.ts frontend/src/lib/mentionAvatar.svelte.test.ts
git commit -m "feat: メンションアバターのセッション内キャッシュモジュールを追加"
```

---

## Task 2: MfmNode.svelte にメンションアイコン表示を統合

**Files:**
- Modify: `frontend/src/render/MfmNode.svelte`
- Modify: `frontend/src/render/Mfm.test.ts`

**Interfaces:**
- Consumes: Task 1 の `cachedAvatarUrl(username: string, host: string | null): string | null | undefined` と `fetchAvatarUrl(username: string, host: string | null): Promise<string | null>`（`frontend/src/lib/mentionAvatar.svelte.ts`）。
- Produces: `mention` ノードのレンダリングに `img.mfm-mention-avatar`（`src`=解決済みアバターURL）が、アバター解決済みのときのみ `span.mfm-mention` の子として先頭に追加される。既存の `span.mfm-mention` のクリック・キーボード操作・`textContent`（`@acct`部分）には影響しない。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/render/Mfm.test.ts` の先頭付近、既存の `vi.mock("../lib/profileModal.svelte", ...)` の直後に追記:

```ts
vi.mock("../lib/mentionAvatar.svelte", () => ({
  cachedAvatarUrl: vi.fn(),
  fetchAvatarUrl: vi.fn(),
}));
```

ファイル先頭のimportに追加:

```ts
import { cachedAvatarUrl, fetchAvatarUrl } from "../lib/mentionAvatar.svelte";
```

`afterEach` の中（`cleanup(); vi.unstubAllGlobals();` の箇所）に、モックのリセットを追加:

```ts
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.mocked(cachedAvatarUrl).mockReset();
  vi.mocked(fetchAvatarUrl).mockReset();
});
```

既存の `it("renders a mention", ...)` テストの直後に以下を追加:

```ts
  it("キャッシュ済みアバターがあれば即座に表示する", () => {
    vi.mocked(cachedAvatarUrl).mockReturnValue("https://example.com/alice.png");
    const { container } = render(Mfm, { props: { text: "@alice@example.com" } });
    const img = container.querySelector("span.mfm-mention img.mfm-mention-avatar");
    expect(img?.getAttribute("src")).toBe("https://example.com/alice.png");
    expect(cachedAvatarUrl).toHaveBeenCalledWith("alice", "example.com");
    expect(fetchAvatarUrl).not.toHaveBeenCalled();
  });

  it("未キャッシュならfetchAvatarUrlを呼び、解決後にアバターを表示する", async () => {
    vi.mocked(cachedAvatarUrl).mockReturnValue(undefined);
    let resolveFn!: (url: string | null) => void;
    vi.mocked(fetchAvatarUrl).mockReturnValue(
      new Promise((r) => {
        resolveFn = r;
      }),
    );

    const { container } = render(Mfm, { props: { text: "@alice@example.com" } });
    expect(fetchAvatarUrl).toHaveBeenCalledWith("alice", "example.com");
    expect(container.querySelector("img.mfm-mention-avatar")).toBeNull();

    resolveFn("https://example.com/alice.png");

    await waitFor(() => {
      const img = container.querySelector("img.mfm-mention-avatar");
      expect(img?.getAttribute("src")).toBe("https://example.com/alice.png");
    });
  });

  it("解決に失敗した場合(null)はアバターを表示せずテキストのみのまま", async () => {
    vi.mocked(cachedAvatarUrl).mockReturnValue(undefined);
    vi.mocked(fetchAvatarUrl).mockResolvedValue(null);

    const { container } = render(Mfm, { props: { text: "@alice@example.com" } });

    await waitFor(() => expect(fetchAvatarUrl).toHaveBeenCalled());
    expect(container.querySelector("img.mfm-mention-avatar")).toBeNull();
    expect(container.querySelector("span.mfm-mention")?.textContent).toBe("@alice@example.com");
  });
```

`waitFor` を `@testing-library/svelte` のimportに追加（ファイル先頭）:

```ts
import { cleanup, render, waitFor } from "@testing-library/svelte";
```

- [ ] **Step 2: テストを実行し失敗を確認**

Run: `cd frontend && pnpm vitest run src/render/Mfm.test.ts`
Expected: FAIL（`../lib/mentionAvatar.svelte` が存在しない、または `img.mfm-mention-avatar` が見つからない）

- [ ] **Step 3: MfmNode.svelte を実装**

`frontend/src/render/MfmNode.svelte` のimport群（10-13行目付近）に追加:

```ts
  import { cachedAvatarUrl, fetchAvatarUrl } from "../lib/mentionAvatar.svelte";
```

`const unixLabel = ...` の直後（63行目 `</script>` の直前）に追加:

```ts
  // mentionノードのアバターURL。未取得はundefined、解決失敗/アバター無しはnull。
  let mentionAvatarUrl = $state<string | null | undefined>(undefined);

  $effect(() => {
    if (node.type !== "mention") return;
    const username = String(p.username ?? "");
    const host = (p.host as string | null | undefined) ?? null;
    const cached = cachedAvatarUrl(username, host);
    if (cached !== undefined) {
      mentionAvatarUrl = cached;
      return;
    }
    let cancelled = false;
    fetchAvatarUrl(username, host).then((url) => {
      if (!cancelled) mentionAvatarUrl = url;
    });
    return () => {
      cancelled = true;
    };
  });
```

`{:else if node.type === "mention"}` ブロック（108-118行目）を以下に置き換え:

```svelte
{:else if node.type === "mention"}
  <!-- role="button"だがButtonプリミティブ非経由のため、キーボードフォーカス時の視認性を
       Buttonのfocus-visibleパターン（スタイルガイド§7、border-ringは無枠のため省略）で個別に補う -->
  <span
    class="mfm-mention rounded-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
    onclick={() => openProfile({ username: p.username, host: p.host ?? null })}
    role="button"
    tabindex="0"
    onkeydown={(e) => e.key === "Enter" && openProfile({ username: p.username, host: p.host ?? null })}
    style="cursor: pointer"
  >{#if mentionAvatarUrl}<img
      class="mfm-mention-avatar mr-0.5 inline-block h-4 w-4 rounded-md object-cover align-middle"
      src={mentionAvatarUrl}
      alt=""
      loading="lazy"
    />{/if}{p.acct}</span>
{:else if node.type === "hashtag"}
```

（`{:else if node.type === "hashtag"}` 以降は変更なし。置き換え範囲は `mention` ブロックのみ。）

- [ ] **Step 4: テストを実行し成功を確認**

Run: `cd frontend && pnpm vitest run src/render/Mfm.test.ts`
Expected: PASS（既存テスト含め全件）

- [ ] **Step 5: フロントエンド全体のテストと型チェック**

Run: `cd frontend && pnpm test && pnpm check`
Expected: 全テストPASS、型エラーなし

- [ ] **Step 6: コミット**

```bash
git add frontend/src/render/MfmNode.svelte frontend/src/render/Mfm.test.ts
git commit -m "feat: 本文中のメンションにアバターアイコンを表示する"
```

---

## 完了確認

- [ ] `cd frontend && pnpm check` が通る
- [ ] `cd frontend && pnpm test` が通る
- [ ] Issue #102 の内容（本文中の@mentionへのアイコン表示）を満たしている
