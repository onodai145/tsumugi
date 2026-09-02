# 相対時刻の自動更新 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `NoteCard.svelte` / `NotificationCard.svelte` の投稿時刻（相対時間表示）を、時間経過に合わせて自動的に再描画されるようにする。

**Architecture:** `AppStore`（`frontend/src/lib/store.svelte.ts`）に共有の `now`（`$state<number>`）を追加し、5秒ごとに更新するタイマーを既存の `#statsTimer` 等と同じパターンで `boot()`/`teardown()` に組み込む。各コンポーネントは `relativeTime(...)` の直呼びを `$derived.by` に置き換え、その中で `app.now` を読むことで tick のたびに再計算させる。

**Tech Stack:** Svelte 5（runes）, Vitest + @testing-library/svelte, TypeScript.

## Global Constraints

- 更新間隔は5秒（`docs/superpowers/specs/2026-09-01-relative-time-update-design.md` で確定）。
- タイマーはアプリ全体で1本のみ（カードごとに `setInterval` を作らない）。
- `relativeTime()`（`frontend/src/lib/time.ts`）自体のフォーマットロジックは変更しない。
- 対象は `NoteCard.svelte` / `NotificationCard.svelte` の2箇所のみ。
- dev HMR時の多重タイマー登録を防ぐため、既存の `#statsTimer` 等と同じ `boot()`/`teardown()` パターンに従う。

---

### Task 1: AppStoreに共有tickを追加する

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts:169`（フィールド宣言部）, `:194-209`（`teardown()`）, `:211-279`（`boot()`）
- Test: `frontend/src/lib/store.test.ts` があればそこに追加、なければ `frontend/src/lib/store.svelte.test.ts` を新規作成

**Interfaces:**
- Consumes: なし（既存の `AppStore` クラス構造のみ）
- Produces: `app.now: number`（`Date.now()` 由来のepochミリ秒。5秒ごとに更新される）。Task 2/3がこれを読んで再計算のトリガーにする。

まず既存テストの配置を確認する。

- [ ] **Step 1: 既存のstoreテストファイルの有無と場所を確認する**

Run: `ls frontend/src/lib/store*.test.ts 2>/dev/null || echo "not found"`

見つかった場合はそのファイルに追記する。見つからない場合は `frontend/src/lib/store.svelte.test.ts` を新規作成する（下記Step 2はそれを前提に書く。既存ファイルがあれば、そのファイルの先頭の `vi.mock(...)` 群と `beforeEach`/`afterEach` パターンを踏襲し、テスト本体だけをそこに追加すること）。

- [ ] **Step 2: 失敗するテストを書く**

`frontend/src/lib/store.svelte.test.ts`（新規、または既存storeテストファイルへの追記）:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { app } = await import("./store.svelte");

describe("AppStore.now (共有tick)", () => {
  afterEach(() => {
    app.teardown();
    vi.useRealTimers();
  });

  it("boot()後、5秒ごとにnowが更新される", async () => {
    vi.useFakeTimers();
    const before = app.now;
    // boot()はネットワーク呼び出しを含み失敗しうるが、失敗してもfinallyでタイマーは起動される
    const bootPromise = app.boot();
    await vi.advanceTimersByTimeAsync(0);
    await bootPromise.catch(() => {});

    vi.setSystemTime(Date.now() + 5_000);
    await vi.advanceTimersByTimeAsync(5_000);

    expect(app.now).toBeGreaterThan(before);
  });

  it("teardown()後はnowが更新されなくなる", async () => {
    vi.useFakeTimers();
    const bootPromise = app.boot();
    await vi.advanceTimersByTimeAsync(0);
    await bootPromise.catch(() => {});

    app.teardown();
    const after = app.now;
    await vi.advanceTimersByTimeAsync(5_000);

    expect(app.now).toBe(after);
  });
});
```

- [ ] **Step 3: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/lib/store.svelte.test.ts`
Expected: FAIL（`app.now` が存在しない、または5秒経過後も変化しない）

- [ ] **Step 4: `AppStore` に `now` と `#clockTimer` を実装する**

`frontend/src/lib/store.svelte.ts:169` の直前（`#statsTimer` フィールド宣言の並び）に追加:

```ts
  // ノート/通知カードの相対時刻表示を定期的に再計算させるための共有時計（Issue #256）。
  // カードごとにsetIntervalを持たせず、アプリ全体で1本のタイマーを共有する。
  now = $state(Date.now());
  #clockTimer: ReturnType<typeof setInterval> | null = null;
```

`teardown()`（`:194-209`）内、既存の `#statsTimer` 解除ブロックの前後どちらでもよいので追加:

```ts
    if (this.#clockTimer !== null) {
      clearInterval(this.#clockTimer);
      this.#clockTimer = null;
    }
```

`boot()`（`:211-279`）の末尾、既存の `#pruneTimer` セットアップの直後に追加:

```ts
    if (this.#clockTimer !== null) clearInterval(this.#clockTimer);
    this.#clockTimer = setInterval(() => (this.now = Date.now()), 5_000);
```

- [ ] **Step 5: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/lib/store.svelte.test.ts`
Expected: PASS

- [ ] **Step 6: コミット**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "feat: AppStoreに相対時刻更新用の共有tickを追加"
```

---

### Task 2: NoteCardの投稿時刻表示を共有tickに追従させる

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte:59`付近（`$derived` 宣言群）, `:353-355`（時刻表示テンプレート）
- Test: `frontend/src/ui/NoteCard.test.ts`

**Interfaces:**
- Consumes: `app.now: number`（Task 1で追加）, `relativeTime(epochSec: number): string`（`frontend/src/lib/time.ts`、既存）
- Produces: `displayTime: string`（NoteCard内のローカル `$derived.by`。テンプレートの時刻表示に使う）

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/NoteCard.test.ts` の末尾（`describe("instance ticker", ...)` ブロックの後）に追加:

```ts
describe("投稿時刻の自動更新", () => {
  it("app.nowが進むと相対時刻の表示が更新される", async () => {
    vi.useFakeTimers();
    try {
      const nowSec = Math.floor(Date.now() / 1000);
      const note = makeNote({ createdAt: nowSec - 30 }); // 30秒前
      const { getByTitle } = render(NoteCard, { props: { note } });

      const timeEl = getByTitle(new Date(note.createdAt * 1000).toLocaleString());
      expect(timeEl.textContent?.trim()).toBe("30s");

      // 90秒進める（分単位表示に切り替わるはず）
      vi.setSystemTime(Date.now() + 90_000);
      app.now = Date.now();
      await vi.advanceTimersByTimeAsync(0);

      expect(timeEl.textContent?.trim()).toBe("2m");
    } finally {
      vi.useRealTimers();
    }
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts -t "投稿時刻の自動更新"`
Expected: FAIL（表示が更新されず `"30s"` のまま、`"2m"` にならない）

- [ ] **Step 3: `NoteCard.svelte` を実装する**

`frontend/src/ui/NoteCard.svelte:59` の `const inner = $derived(...)` の直後に追加:

```ts
  // app.now（Issue #256、5秒ごとに更新される共有tick）を依存として読むことで、
  // 時間経過に合わせて相対時刻表示を再計算させる。
  const displayTime = $derived.by(() => {
    app.now;
    return relativeTime(inner.createdAt);
  });
```

`:353-355` のテンプレートを変更:

```svelte
        <span class="ml-auto text-xs text-muted-foreground" title={new Date(inner.createdAt * 1000).toLocaleString()}>
          {displayTime}
        </span>
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts`
Expected: PASS（新規テストを含め全件）

- [ ] **Step 5: コミット**

```bash
git add frontend/src/ui/NoteCard.svelte frontend/src/ui/NoteCard.test.ts
git commit -m "feat: NoteCardの投稿時刻表示を共有tickで自動更新する"
```

---

### Task 3: NotificationCardの通知時刻表示を共有tickに追従させる

**Files:**
- Modify: `frontend/src/ui/NotificationCard.svelte`（`$derived` 追加箇所、および `:110`付近の時刻表示テンプレート）
- Test: `frontend/src/ui/NotificationCard.test.ts`

**Interfaces:**
- Consumes: `app.now: number`（Task 1）, `relativeTime(epochSec: number): string`（既存）
- Produces: `displayTime: string`（NotificationCard内のローカル `$derived.by`）

- [ ] **Step 1: 既存テストファイルの構造を確認する**

Run: `cd frontend && sed -n '1,40p' src/ui/NotificationCard.test.ts`

`NoteCard.test.ts` と同様に `vi.mock(...)` 群・`app` のimport・`makeNotification`（または類似のファクトリ関数）があるはずなので、それに合わせてTask 2と同じ形のテストを追加する。ファクトリ関数名がplanと異なる場合は実際の名前を使うこと。

- [ ] **Step 2: 失敗するテストを書く**

`frontend/src/ui/NotificationCard.test.ts` の末尾に追加（`makeNotification` は既存のファクトリ関数名に合わせる。`n.createdAt` を持つ通知オブジェクトを組み立てられればよい）:

```ts
describe("通知時刻の自動更新", () => {
  it("app.nowが進むと相対時刻の表示が更新される", async () => {
    vi.useFakeTimers();
    try {
      const nowSec = Math.floor(Date.now() / 1000);
      const notification = makeNotification({ createdAt: nowSec - 30 });
      const { container } = render(NotificationCard, { props: { notification } });

      const timeEl = container.querySelector(".text-sm.text-muted-foreground") as HTMLElement;
      expect(timeEl.textContent?.trim()).toBe("30s");

      vi.setSystemTime(Date.now() + 90_000);
      app.now = Date.now();
      await vi.advanceTimersByTimeAsync(0);

      expect(timeEl.textContent?.trim()).toBe("2m");
    } finally {
      vi.useRealTimers();
    }
  });
});
```

`.text-sm.text-muted-foreground` セレクタが他要素と衝突して取得できない場合は、実装側（Step 3）で `data-testid="notification-time"` をそのspanに追加し、テストは `getByTestId("notification-time")` に変更すること。

- [ ] **Step 3: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/ui/NotificationCard.test.ts -t "通知時刻の自動更新"`
Expected: FAIL

- [ ] **Step 4: `NotificationCard.svelte` を実装する**

`<script>` 内、`n` （通知オブジェクト）が確定した後のいずれかの `$derived`/計算値の並びに追加（既存の `reaction` など他の派生値の近く）:

```ts
  // app.now（Issue #256、5秒ごとに更新される共有tick）を依存として読むことで、
  // 時間経過に合わせて相対時刻表示を再計算させる。
  const displayTime = $derived.by(() => {
    app.now;
    return relativeTime(n.createdAt);
  });
```

テンプレートの `<span class="text-sm text-muted-foreground">{relativeTime(n.createdAt)}</span>` を次に変更:

```svelte
    <span class="text-sm text-muted-foreground">{displayTime}</span>
```

（Step 2で `data-testid` が必要と判断した場合は、このspanに `data-testid="notification-time"` を追加する。）

- [ ] **Step 5: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/ui/NotificationCard.test.ts`
Expected: PASS（新規テストを含め全件）

- [ ] **Step 6: コミット**

```bash
git add frontend/src/ui/NotificationCard.svelte frontend/src/ui/NotificationCard.test.ts
git commit -m "feat: NotificationCardの通知時刻表示を共有tickで自動更新する"
```

---

### Task 4: 全体検証とPR作成

**Files:** なし（検証のみ）

**Interfaces:**
- Consumes: Task 1〜3の全変更
- Produces: なし（マージ可能な状態にする）

- [ ] **Step 1: フロントエンドの型チェックとlintを通す**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 2: フロントエンドの全テストを通す**

Run: `cd frontend && pnpm test`
Expected: 全件PASS（既存テストの回帰がないことを含む）

- [ ] **Step 3: 実アプリで動作確認する**

`cargo tauri dev` でアプリを起動し、投稿直後のノートの時刻表示（例: "0s"）が、操作せずに数十秒放置するだけで "30s" → "1m" のように自動的に切り替わることを目視確認する。確認後、起動した `cargo tauri dev` プロセスは自分で終了させる。

- [ ] **Step 4: PRを作成する**

```bash
git push -u origin feature/issue-256-relative-time-update
gh pr create --title "feat: ノートの投稿時刻（相対時間）を自動更新する" --body "$(cat <<'EOF'
## 概要
NoteCard/NotificationCardの投稿時刻表示（相対時間）が時間経過に合わせて自動更新されるようにした。

## 変更内容
- AppStoreに共有tick（`app.now`、5秒間隔）を追加
- NoteCard/NotificationCardの時刻表示を`$derived.by`化し、共有tickをトリガーに再計算するよう変更

## 設計
docs/superpowers/specs/2026-09-01-relative-time-update-design.md 参照

Fixes #256

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```
