# モバイル版タブ/カラム並び替え Implementation Plan (Issue #354)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** モバイル版（`app.useMobileUi()`が`true`）で、タブとカラムの並び替えをタッチ操作（長押しドラッグ、およびカラムはメニューからの移動ボタン）で行えるようにする。

**Architecture:** 「長押し検知→ドラッグ状態遷移」を行うDOM非依存の純粋ロジックを`frontend/src/lib/longPressDrag.ts`に切り出し、タブ・カラムグリップの両方の`pointerdown`/`pointermove`/`pointerup`/`pointercancel`ハンドラから共有する。実際の並び替え反映は、タブは既存の`app.dragOverTab`/`app.dragOverTabBarEnd`/`app.endDragTab`を、カラムは`frontend/src/lib/swipeNav.ts`に追加する純粋関数（`swapAdjacentColumns`/`canMoveAdjacentColumn`）を使った新規`app.moveColumnAdjacent`メソッドを呼ぶ。既存のデスクトップ用HTML5 native DnD実装はそのまま残し、`pointerType === "touch"`かつ`app.useMobileUi()`の場合にのみ新規フローが有効になる。

**Tech Stack:** Svelte 5 (runes)、TypeScript、Vitest、`@lucide/svelte`（アイコン）。

## Global Constraints

- 対象は`app.useMobileUi()`が`true`の場合のみ。デスクトップ版の既存動作（マウスでのnative DnD）は変更しない。
- カラム並び替えの対象は`topLevelLeafGroupIds(app.paneRoot)`（最上位row直下のleafカラムのみ）。ネストした分割配下のカラムは対象外。
- タブバー内でのオートスクロールはv1スコープ外（実装しない）。
- カラムの長押しドラッグは1ジェスチャーにつき隣接1つ分の移動のみ（連続した複数ステップ移動はしない、トグル式）。
- 長押し成立の閾値: 400ms・8px（`longPressDrag.ts`）。カラムの前後ヒント確定の閾値: 40px（`columnDragHint.ts`）。
- 振動フィードバックは既存の`vibrate()`（`frontend/src/lib/ipc.ts`）を使い、呼び出し側で`isMobilePlatform && (app.ui.hapticsEnabled ?? true)`のガードを付ける（既存の他箇所と同じパターン）。
- 浮遊要素の影は`shadow-[0_8px_24px_rgba(0,0,0,0.25)]`（`docs/design/style-guide.md`の標準値）を使う。新しい即値を増やさない。
- Design docは`docs/superpowers/specs/2026-09-14-mobile-tab-column-reorder-design.md`。

---

### Task 1: 長押しドラッグの状態機械 (`longPressDrag.ts`)

**Files:**
- Create: `frontend/src/lib/longPressDrag.ts`
- Test: `frontend/src/lib/longPressDrag.test.ts`

**Interfaces:**
- Consumes: なし（DOM非依存の純粋ロジック）。
- Produces:
  - `LONG_PRESS_MS: number`（400）
  - `CANCEL_THRESHOLD_PX: number`（8）
  - `interface LongPressDragController { readonly armed: boolean; onPointerDown(x: number, y: number): void; onPointerMove(x: number, y: number): void; onPointerUp(): void; onPointerCancel(): void; }`
  - `createLongPressDrag(callbacks: { onArmed: () => void }): LongPressDragController`

- [ ] **Step 1: Write the failing test**

Create `frontend/src/lib/longPressDrag.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CANCEL_THRESHOLD_PX, LONG_PRESS_MS, createLongPressDrag } from "./longPressDrag";

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

describe("createLongPressDrag", () => {
  it("400ms経過するとarmedになりonArmedが呼ばれる", () => {
    const onArmed = vi.fn();
    const ctl = createLongPressDrag({ onArmed });
    ctl.onPointerDown(0, 0);
    expect(ctl.armed).toBe(false);
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(ctl.armed).toBe(true);
    expect(onArmed).toHaveBeenCalledTimes(1);
  });

  it("成立前に閾値を超えて動くと中断し、armedにならない", () => {
    const onArmed = vi.fn();
    const ctl = createLongPressDrag({ onArmed });
    ctl.onPointerDown(0, 0);
    ctl.onPointerMove(CANCEL_THRESHOLD_PX + 1, 0);
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(ctl.armed).toBe(false);
    expect(onArmed).not.toHaveBeenCalled();
  });

  it("閾値以内の動きでは中断しない", () => {
    const ctl = createLongPressDrag({ onArmed: () => {} });
    ctl.onPointerDown(0, 0);
    ctl.onPointerMove(CANCEL_THRESHOLD_PX, 0);
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(ctl.armed).toBe(true);
  });

  it("成立前にpointerupが来ると中断する", () => {
    const onArmed = vi.fn();
    const ctl = createLongPressDrag({ onArmed });
    ctl.onPointerDown(0, 0);
    ctl.onPointerUp();
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(ctl.armed).toBe(false);
    expect(onArmed).not.toHaveBeenCalled();
  });

  it("pointercancelでも中断する", () => {
    const onArmed = vi.fn();
    const ctl = createLongPressDrag({ onArmed });
    ctl.onPointerDown(0, 0);
    ctl.onPointerCancel();
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(ctl.armed).toBe(false);
    expect(onArmed).not.toHaveBeenCalled();
  });

  it("成立後にpointerupするとarmedがfalseに戻る", () => {
    const ctl = createLongPressDrag({ onArmed: () => {} });
    ctl.onPointerDown(0, 0);
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(ctl.armed).toBe(true);
    ctl.onPointerUp();
    expect(ctl.armed).toBe(false);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm vitest run src/lib/longPressDrag.test.ts`
Expected: FAIL（`longPressDrag.ts`が存在しないため import エラー）

- [ ] **Step 3: Write minimal implementation**

Create `frontend/src/lib/longPressDrag.ts`:

```ts
// タッチでの長押しドラッグ検知(Issue #354)。native drag-and-dropはタッチでは
// dragstartが発火しないため、pointerdown起点で「長押し(400ms)が成立したらドラッグ開始」
// という状態遷移をDOM非依存の純粋なロジックとして提供する。
// 指が閾値(8px)以上動いた状態でタイマーが成立前なら中断し、タップ/横スクロールに委ねる。

export const LONG_PRESS_MS = 400;
export const CANCEL_THRESHOLD_PX = 8;

export interface LongPressDragController {
  /** 長押しが成立してドラッグ中かどうか。 */
  readonly armed: boolean;
  onPointerDown(x: number, y: number): void;
  onPointerMove(x: number, y: number): void;
  onPointerUp(): void;
  onPointerCancel(): void;
}

/// callbacks.onArmedは、長押しタイマーが成立した瞬間に一度だけ呼ばれる
/// (呼び出し側はここでvibrate・視覚効果の付与・並び替え開始処理を行う)。
export function createLongPressDrag(callbacks: { onArmed: () => void }): LongPressDragController {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let startX = 0;
  let startY = 0;
  let armed = false;

  function clearTimer() {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  }

  return {
    get armed() {
      return armed;
    },
    onPointerDown(x, y) {
      clearTimer();
      armed = false;
      startX = x;
      startY = y;
      timer = setTimeout(() => {
        timer = null;
        armed = true;
        callbacks.onArmed();
      }, LONG_PRESS_MS);
    },
    onPointerMove(x, y) {
      if (armed || timer === null) return;
      const dx = x - startX;
      const dy = y - startY;
      if (Math.hypot(dx, dy) > CANCEL_THRESHOLD_PX) clearTimer();
    },
    onPointerUp() {
      clearTimer();
      armed = false;
    },
    onPointerCancel() {
      clearTimer();
      armed = false;
    },
  };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && pnpm vitest run src/lib/longPressDrag.test.ts`
Expected: PASS（全6テスト）

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/longPressDrag.ts frontend/src/lib/longPressDrag.test.ts
git commit -m "feat: 長押しドラッグ検知の状態機械を追加"
```

---

### Task 2: カラムドラッグの前後ヒント判定 (`columnDragHint.ts`)

**Files:**
- Create: `frontend/src/lib/columnDragHint.ts`
- Test: `frontend/src/lib/columnDragHint.test.ts`

**Interfaces:**
- Consumes: なし（DOM非依存の純粋ロジック）。
- Produces:
  - `COLUMN_DRAG_HINT_THRESHOLD_PX: number`（40）
  - `type ColumnDragDirection = "prev" | "next"`
  - `resolveColumnDragHint(deltaX: number, canGoPrev: boolean, canGoNext: boolean): ColumnDragDirection | null`

- [ ] **Step 1: Write the failing test**

Create `frontend/src/lib/columnDragHint.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { COLUMN_DRAG_HINT_THRESHOLD_PX, resolveColumnDragHint } from "./columnDragHint";

describe("resolveColumnDragHint", () => {
  it("閾値未満ではnullを返す", () => {
    expect(resolveColumnDragHint(COLUMN_DRAG_HINT_THRESHOLD_PX - 1, true, true)).toBeNull();
    expect(resolveColumnDragHint(-(COLUMN_DRAG_HINT_THRESHOLD_PX - 1), true, true)).toBeNull();
  });

  it("右方向へ閾値を超えるとnextを返す", () => {
    expect(resolveColumnDragHint(COLUMN_DRAG_HINT_THRESHOLD_PX, true, true)).toBe("next");
  });

  it("左方向へ閾値を超えるとprevを返す", () => {
    expect(resolveColumnDragHint(-COLUMN_DRAG_HINT_THRESHOLD_PX, true, true)).toBe("prev");
  });

  it("末尾カラムではnextを許可しない", () => {
    expect(resolveColumnDragHint(COLUMN_DRAG_HINT_THRESHOLD_PX, true, false)).toBeNull();
  });

  it("先頭カラムではprevを許可しない", () => {
    expect(resolveColumnDragHint(-COLUMN_DRAG_HINT_THRESHOLD_PX, false, true)).toBeNull();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm vitest run src/lib/columnDragHint.test.ts`
Expected: FAIL（`columnDragHint.ts`が存在しないため import エラー）

- [ ] **Step 3: Write minimal implementation**

Create `frontend/src/lib/columnDragHint.ts`:

```ts
// モバイル版のカラム長押しドラッグで、指のX移動量から「前へ/次へ」どちらのヒントを
// ハイライトするかを解決する純粋関数(Issue #354)。閾値を超えるまではnull(トグル式、
// 連続した複数ステップ移動はしない)。
export const COLUMN_DRAG_HINT_THRESHOLD_PX = 40;

export type ColumnDragDirection = "prev" | "next";

export function resolveColumnDragHint(
  deltaX: number,
  canGoPrev: boolean,
  canGoNext: boolean,
): ColumnDragDirection | null {
  if (deltaX <= -COLUMN_DRAG_HINT_THRESHOLD_PX) return canGoPrev ? "prev" : null;
  if (deltaX >= COLUMN_DRAG_HINT_THRESHOLD_PX) return canGoNext ? "next" : null;
  return null;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && pnpm vitest run src/lib/columnDragHint.test.ts`
Expected: PASS（全5テスト）

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/columnDragHint.ts frontend/src/lib/columnDragHint.test.ts
git commit -m "feat: カラムドラッグの前後ヒント判定を追加"
```

---

### Task 3: カラム隣接swapの純粋関数 (`swipeNav.ts`)

**Files:**
- Modify: `frontend/src/lib/swipeNav.ts`
- Test: `frontend/src/lib/swipeNav.test.ts`（既存ファイルに追記）

**Interfaces:**
- Consumes: 既存の`topLevelLeafGroupIds(paneRoot: PaneNode): string[]`（同ファイル内、変更なし）。
- Produces:
  - `type AdjacentDirection = "prev" | "next"`
  - `canMoveAdjacentColumn(paneRoot: PaneNode, groupId: string, direction: AdjacentDirection): boolean`
  - `swapAdjacentColumns<T extends { id: string }>(groups: T[], paneRoot: PaneNode, groupId: string, direction: AdjacentDirection): T[] | null`

- [ ] **Step 1: Write the failing test**

現在の`frontend/src/lib/swipeNav.test.ts`は以下（先頭部分は変更しない）:

```ts
import { describe, expect, it } from "vitest";
import type { PaneNode } from "../bindings/tauri.gen";
import { canMoveAdjacentColumn, pageOrder, swapAdjacentColumns, topLevelLeafGroupIds } from "./swipeNav";

function leaf(id: string, groupId: string): PaneNode {
  return { type: "leaf", id, groupId };
}

function row(id: string, children: PaneNode[]): PaneNode {
  return { type: "split", id, direction: "row", children: children.map((node) => ({ node, size: null, auto: true })) };
}
```

（`import`行を上記のように書き換え、`canMoveAdjacentColumn`/`swapAdjacentColumns`を追加する）

ファイル末尾に以下を追記する:

```ts

describe("canMoveAdjacentColumn", () => {
  const root = row("root", [leaf("l1", "g1"), leaf("l2", "g2"), leaf("l3", "g3")]);

  it("先頭カラムはprevへ移動できない", () => {
    expect(canMoveAdjacentColumn(root, "g1", "prev")).toBe(false);
  });
  it("末尾カラムはnextへ移動できない", () => {
    expect(canMoveAdjacentColumn(root, "g3", "next")).toBe(false);
  });
  it("中間カラムはどちらへも移動できる", () => {
    expect(canMoveAdjacentColumn(root, "g2", "prev")).toBe(true);
    expect(canMoveAdjacentColumn(root, "g2", "next")).toBe(true);
  });
  it("対象外のgroupIdはfalseを返す", () => {
    expect(canMoveAdjacentColumn(root, "missing", "next")).toBe(false);
  });
});

describe("swapAdjacentColumns", () => {
  const root = row("root", [leaf("l1", "g1"), leaf("l2", "g2"), leaf("l3", "g3")]);
  const groups = [{ id: "g1" }, { id: "g2" }, { id: "g3" }];

  it("nextで隣のカラムと入れ替わる", () => {
    const result = swapAdjacentColumns(groups, root, "g1", "next");
    expect(result?.map((g) => g.id)).toEqual(["g2", "g1", "g3"]);
  });

  it("prevで隣のカラムと入れ替わる", () => {
    const result = swapAdjacentColumns(groups, root, "g3", "prev");
    expect(result?.map((g) => g.id)).toEqual(["g1", "g3", "g2"]);
  });

  it("先頭カラムでprevを指定するとnullを返す", () => {
    expect(swapAdjacentColumns(groups, root, "g1", "prev")).toBeNull();
  });

  it("末尾カラムでnextを指定するとnullを返す", () => {
    expect(swapAdjacentColumns(groups, root, "g3", "next")).toBeNull();
  });

  it("ネストしたsplit配下のカラムはnullを返す", () => {
    const nested = row("nested", [leaf("l4", "g4"), leaf("l5", "g5")]);
    const rootWithNested = row("root", [leaf("l1", "g1"), nested]);
    const groupsWithNested = [{ id: "g1" }, { id: "g4" }, { id: "g5" }];
    expect(swapAdjacentColumns(groupsWithNested, rootWithNested, "g4", "next")).toBeNull();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm vitest run src/lib/swipeNav.test.ts`
Expected: FAIL（`canMoveAdjacentColumn`/`swapAdjacentColumns`が`swipeNav.ts`に存在しないため import エラー）

- [ ] **Step 3: Write minimal implementation**

`frontend/src/lib/swipeNav.ts`の末尾に追記する:

```ts

export type AdjacentDirection = "prev" | "next";

/// groupIdが最上位rowのleafの並びの中で、前/次に隣接カラムへ移動できるかどうか。
export function canMoveAdjacentColumn(paneRoot: PaneNode, groupId: string, direction: AdjacentDirection): boolean {
  const order = topLevelLeafGroupIds(paneRoot);
  const i = order.indexOf(groupId);
  if (i < 0) return false;
  const j = direction === "prev" ? i - 1 : i + 1;
  return j >= 0 && j < order.length;
}

/// groupIdと隣接するtop-level leafカラムの位置を1つ入れ替えた新しい全体順序(groups配列と
/// 同じ要素を持つ配列)を返す。移動できない場合(端、対象外のgroupId、ネスト配下のカラム)は
/// nullを返す。ネストしたsplit配下のカラムの相対順序には触れない。
export function swapAdjacentColumns<T extends { id: string }>(
  groups: T[],
  paneRoot: PaneNode,
  groupId: string,
  direction: AdjacentDirection,
): T[] | null {
  const order = topLevelLeafGroupIds(paneRoot);
  const i = order.indexOf(groupId);
  if (i < 0) return null;
  const j = direction === "prev" ? i - 1 : i + 1;
  if (j < 0 || j >= order.length) return null;
  const otherId = order[j];
  const gi = groups.findIndex((g) => g.id === groupId);
  const gj = groups.findIndex((g) => g.id === otherId);
  if (gi < 0 || gj < 0) return null;
  const next = [...groups];
  [next[gi], next[gj]] = [next[gj], next[gi]];
  return next;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && pnpm vitest run src/lib/swipeNav.test.ts`
Expected: PASS（既存3 describeブロック + 新規2 describeブロック、全15テスト）

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/swipeNav.ts frontend/src/lib/swipeNav.test.ts
git commit -m "feat: カラム隣接swapの純粋関数を追加"
```

---

### Task 4: `AppStore.moveColumnAdjacent`メソッド

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`
- Test: Create `frontend/src/lib/store.svelte.moveColumnAdjacent.test.ts`

**Interfaces:**
- Consumes: Task 3の`swapAdjacentColumns`/`canMoveAdjacentColumn`/`AdjacentDirection`（`./swipeNav`）。既存の`this.groups: GroupView[]`、`this.paneRoot: PaneNode`、`this.#queuePaneWrite<T>(fn: () => Promise<T>): Promise<T>`、`this.#logFailure(e: unknown)`、`commands.reorderGroups(orderedIds: string[])`。
- Produces:
  - `moveColumnAdjacent(groupId: string, direction: AdjacentDirection): Promise<void>`（メニュー・長押しドラッグの両方から呼ばれる）
  - `canMoveColumnAdjacent(groupId: string, direction: AdjacentDirection): boolean`（メニュー項目の表示可否に使う。内部で`canMoveAdjacentColumn(this.paneRoot, ...)`を呼ぶだけの薄いラッパー）

- [ ] **Step 1: Write the failing test**

Create `frontend/src/lib/store.svelte.moveColumnAdjacent.test.ts`（`store.svelte.focusColumn.test.ts`と同じモック構成に倣う）:

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
const invokeMock = vi.fn().mockResolvedValue({ status: "ok", data: null });
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { app } = await import("./store.svelte");

function makeGroup(id: string) {
  return { id, width: 320, auto: false, tabs: [{ id: `${id}-t1` }] as never, activeTabId: `${id}-t1` };
}

function leafRoot(groupIds: string[]) {
  return {
    type: "split" as const,
    id: "root",
    direction: "row" as const,
    children: groupIds.map((groupId) => ({
      node: { type: "leaf" as const, id: `leaf-${groupId}`, groupId },
      size: null,
      auto: true,
    })),
  };
}

beforeEach(() => {
  invokeMock.mockClear();
  app.groups = [makeGroup("g1"), makeGroup("g2"), makeGroup("g3")];
  app.paneRoot = leafRoot(["g1", "g2", "g3"]);
});

describe("moveColumnAdjacent(Issue #354)", () => {
  it("nextで隣のカラムと入れ替え、reorderGroupsを呼ぶ", async () => {
    await app.moveColumnAdjacent("g1", "next");
    expect(app.groups.map((g) => g.id)).toEqual(["g2", "g1", "g3"]);
    expect(invokeMock).toHaveBeenCalledWith("reorder_groups", { orderedIds: ["g2", "g1", "g3"] });
  });

  it("先頭カラムでprevを指定しても何もしない", async () => {
    await app.moveColumnAdjacent("g1", "prev");
    expect(app.groups.map((g) => g.id)).toEqual(["g1", "g2", "g3"]);
    expect(invokeMock).not.toHaveBeenCalledWith("reorder_groups", expect.anything());
  });
});

describe("canMoveColumnAdjacent(Issue #354)", () => {
  it("先頭カラムはprevへ移動できない", () => {
    expect(app.canMoveColumnAdjacent("g1", "prev")).toBe(false);
  });
  it("末尾カラムはnextへ移動できない", () => {
    expect(app.canMoveColumnAdjacent("g3", "next")).toBe(false);
  });
  it("中間カラムはどちらへも移動できる", () => {
    expect(app.canMoveColumnAdjacent("g2", "prev")).toBe(true);
    expect(app.canMoveColumnAdjacent("g2", "next")).toBe(true);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm vitest run src/lib/store.svelte.moveColumnAdjacent.test.ts`
Expected: FAIL（`app.moveColumnAdjacent`/`app.canMoveColumnAdjacent`が存在しない）

- [ ] **Step 3: Write minimal implementation**

`frontend/src/lib/store.svelte.ts`の43行目（`import { isMobilePlatform } from "./platform";`）の直後に追記する:

```ts
import { swapAdjacentColumns, canMoveAdjacentColumn, type AdjacentDirection } from "./swipeNav";
```

同ファイルの`focusColumn`メソッド（467行目付近、`focusColumn(groupId: string) { ... }`の直後）に以下を追加する:

```ts

  /// タッチ長押しドラッグ確定時、および「…」メニューの「左/右に移動」から呼ぶ(Issue #354)。
  /// 対象はtopLevelLeafGroupIds上でgroupIdと隣接するtop-level leafカラムのみ(ネストした
  /// 分割配下のカラムは対象外)。移動できない場合(端、対象外のgroupId)は何もしない。
  canMoveColumnAdjacent(groupId: string, direction: AdjacentDirection): boolean {
    return canMoveAdjacentColumn(this.paneRoot, groupId, direction);
  }

  async moveColumnAdjacent(groupId: string, direction: AdjacentDirection) {
    const next = swapAdjacentColumns(this.groups, this.paneRoot, groupId, direction);
    if (!next) return;
    this.groups = next;
    await this.#queuePaneWrite(async () => {
      try {
        await unwrap(commands.reorderGroups(this.groups.map((g) => g.id)));
      } catch (e) {
        this.#logFailure(e);
      }
    });
  }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && pnpm vitest run src/lib/store.svelte.moveColumnAdjacent.test.ts`
Expected: PASS（全5テスト）

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.moveColumnAdjacent.test.ts
git commit -m "feat: AppStoreにmoveColumnAdjacent/canMoveColumnAdjacentを追加"
```

---

### Task 5: タブの長押しドラッグ (`Column.svelte`)

**Files:**
- Modify: `frontend/src/ui/Column.svelte`

**Interfaces:**
- Consumes: Task 1の`createLongPressDrag`（`../lib/longPressDrag`）。既存の`app.startDragTab(tabId: string)`、`app.dragOverTab(overGroupId: string, overTabId: string)`、`app.dragOverTabBarEnd(groupId: string)`、`app.endDragTab(): Promise<void>`、`app.draggingTabId: string | null`（すべて`store.svelte.ts`に既存、変更なし）。既存の`vibrate`（`../lib/ipc`）、`isMobilePlatform`（`../lib/platform`）。
- Produces: なし（UI wiring のみ、他タスクから参照されない）。

このタスクはUIワイヤリングのため、ユニットテストではなく型チェック（`pnpm check`）と実機確認で検証する。

- [ ] **Step 1: importを追加する**

`frontend/src/ui/Column.svelte`の1〜11行目のimportブロックに以下を追加する:

```ts
  import { createLongPressDrag } from "../lib/longPressDrag";
  import { vibrate } from "../lib/ipc";
  import { isMobilePlatform } from "../lib/platform";
```

- [ ] **Step 2: タブ用の長押し状態を`<script>`に追加する**

`toggleMenu`/`pickMenuItem`関数の直後（143行目の`</script>`の直前）に以下を追加する:

```ts
  // タッチ長押しでのタブ並び替え(Issue #354)。native drag-and-dropはタッチでは
  // dragstartが発火しないため、長押し(400ms)が成立したら同じapp.startDragTab等を
  // 呼び出す形でモバイル版に対応する。マウス操作(pointerType!=="touch")では何もせず、
  // 既存のdraggable属性によるnative DnDに委ねる。
  let touchDraggingTabId = $state<string | null>(null);
  let touchDragTabPendingId: string | null = null;
  let touchDragStartX = 0;
  let touchDragDeltaX = $state(0);

  const tabDrag = createLongPressDrag({
    onArmed: () => {
      const tabId = touchDragTabPendingId;
      if (!tabId) return;
      touchDraggingTabId = tabId;
      touchDragDeltaX = 0;
      if (isMobilePlatform && (app.ui.hapticsEnabled ?? true)) vibrate("light");
      app.startDragTab(tabId);
    },
  });

  function onTabPointerDown(e: PointerEvent, tabId: string) {
    if (e.pointerType !== "touch" || !app.useMobileUi()) return;
    touchDragTabPendingId = tabId;
    touchDragStartX = e.clientX;
    tabDrag.onPointerDown(e.clientX, e.clientY);
  }

  function resolveTabHit(clientX: number, clientY: number): { groupId: string; tabId: string | null } | null {
    const el = document.elementFromPoint(clientX, clientY) as HTMLElement | null;
    if (!el) return null;
    const groupEl = el.closest<HTMLElement>("[data-group-id]");
    if (!groupEl) return null;
    const tabEl = el.closest<HTMLElement>("[data-tab-id]");
    return { groupId: groupEl.dataset.groupId!, tabId: tabEl?.dataset.tabId ?? null };
  }

  function onTabPointerMove(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    tabDrag.onPointerMove(e.clientX, e.clientY);
    if (!tabDrag.armed) return;
    e.preventDefault();
    touchDragDeltaX = e.clientX - touchDragStartX;
    const hit = resolveTabHit(e.clientX, e.clientY);
    if (!hit) return;
    if (hit.tabId) app.dragOverTab(hit.groupId, hit.tabId);
    else app.dragOverTabBarEnd(hit.groupId);
  }

  function endTabTouchDrag() {
    const wasArmed = tabDrag.armed;
    touchDraggingTabId = null;
    touchDragTabPendingId = null;
    touchDragDeltaX = 0;
    if (wasArmed) void app.endDragTab();
  }

  function onTabPointerUp(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    tabDrag.onPointerUp();
    endTabTouchDrag();
  }

  function onTabPointerCancel(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    tabDrag.onPointerCancel();
    endTabTouchDrag();
  }
```

- [ ] **Step 3: 各タブ要素にpointerハンドラとdata属性を追加する**

`{#each group.tabs as t (t.id)}`の中の`<div>`（190〜214行目）を以下に置き換える:

```svelte
      {#each group.tabs as t (t.id)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class={[
            "flex cursor-grab items-center active:cursor-grabbing",
            {
              "shadow-[inset_0_-2px_0_var(--color-primary)]": t.id === group.activeTabId,
              "relative z-20 scale-105 shadow-[0_8px_24px_rgba(0,0,0,0.25)] pointer-events-none": touchDraggingTabId === t.id,
            },
            app.draggingTabId === t.id ? "opacity-40" : t.id !== group.activeTabId ? "opacity-65" : "",
          ]}
          style:transform={touchDraggingTabId === t.id ? `translateX(${touchDragDeltaX}px)` : undefined}
          data-tab-id={t.id}
          draggable="true"
          ondragstart={(e) => {
            e.dataTransfer?.setData("text/plain", t.id);
            e.stopPropagation();
            app.startDragTab(t.id);
          }}
          ondragend={() => app.endDragTab()}
          ondragover={(e) => {
            if (app.draggingTabId) {
              e.preventDefault();
              e.stopPropagation();
              app.dragOverTab(group.id, t.id);
            }
          }}
          onpointerdown={(e) => onTabPointerDown(e, t.id)}
          onpointermove={onTabPointerMove}
          onpointerup={onTabPointerUp}
          onpointercancel={onTabPointerCancel}
        >
          <button
            class="flex items-center gap-1 whitespace-nowrap border-none bg-transparent px-1.5 py-0.5 text-xs text-foreground"
            onclick={() => app.setActiveTab(group.id, t.id)}
            ondblclick={() => onEditTab(t)}
            title={`${tabName(t)}（ダブルクリックで編集）`}
          >
            <span
              class="h-1.5 w-1.5 flex-none rounded-full bg-muted-foreground data-[state=connected]:bg-[var(--success)] data-[state=connecting]:bg-[var(--warning)] data-[state=reconnecting]:bg-[var(--warning)] data-[state=error]:bg-destructive"
              data-state={t.state}
            ></span>{tabName(t)}
          </button>
          <button
            class={[
              t.id === group.activeTabId ? "inline-flex" : "hidden",
              "border-none bg-transparent py-0 pr-1 text-muted-foreground",
            ]}
            title="タブを閉じる"
            onclick={() => app.closeTab(t.id)}
          ><X size={12} /></button>
        </div>
      {/each}
```

（`pointer-events-none`は`touchDraggingTabId === t.id`の間だけ効くため、`document.elementFromPoint`が自分自身ではなく下に重なっている別のタブを返すようになる点に注意。）

- [ ] **Step 4: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし（`app.ui.hapticsEnabled`の型、`vibrate`/`isMobilePlatform`のimportパスに注意）

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/Column.svelte
git commit -m "feat: モバイル版でタブの長押しドラッグ並び替えに対応"
```

---

### Task 6: カラムの長押しドラッグ + 前後ヒント (`Column.svelte`)

**Files:**
- Modify: `frontend/src/ui/Column.svelte`

**Interfaces:**
- Consumes: Task 1の`createLongPressDrag`。Task 2の`resolveColumnDragHint`/`ColumnDragDirection`（`../lib/columnDragHint`）。Task 4の`app.moveColumnAdjacent(groupId, direction)`/`app.canMoveColumnAdjacent(groupId, direction)`。
- Produces: なし。

- [ ] **Step 1: importを追加する**

Task 5で追加したimportブロックの直後に追加する:

```ts
  import { resolveColumnDragHint, type ColumnDragDirection } from "../lib/columnDragHint";
  import { ChevronLeft, ChevronRight } from "@lucide/svelte";
```

（`ChevronLeft`/`ChevronRight`は既存の`import { X, GripVertical, ... } from "@lucide/svelte";`の1行に統合してもよい。）

- [ ] **Step 2: カラムグリップ用の長押し状態を`<script>`に追加する**

Task 5で追加したコードブロックの直後に追加する:

```ts
  // タッチ長押しでのカラム並び替え(Issue #354)。モバイル版は1カラムが画面全幅表示のため、
  // 隣のカラムが画面外にあり位置に追従する自由なドラッグは分かりにくい。そのため
  // 「前へ/次へ」の1ステップ移動として実装する(resolveColumnDragHintのトグル式判定)。
  let columnDragHint = $state<ColumnDragDirection | null>(null);
  let columnDragStartX = 0;

  const columnDrag = createLongPressDrag({
    onArmed: () => {
      columnDragHint = null;
      if (isMobilePlatform && (app.ui.hapticsEnabled ?? true)) vibrate("light");
    },
  });

  function onGripPointerDown(e: PointerEvent) {
    if (e.pointerType !== "touch" || !app.useMobileUi()) return;
    columnDragStartX = e.clientX;
    columnDrag.onPointerDown(e.clientX, e.clientY);
  }

  function onGripPointerMove(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    columnDrag.onPointerMove(e.clientX, e.clientY);
    if (!columnDrag.armed) return;
    e.preventDefault();
    const deltaX = e.clientX - columnDragStartX;
    columnDragHint = resolveColumnDragHint(
      deltaX,
      app.canMoveColumnAdjacent(group.id, "prev"),
      app.canMoveColumnAdjacent(group.id, "next"),
    );
  }

  function endGripTouchDrag() {
    const hint = columnDragHint;
    const wasArmed = columnDrag.armed;
    columnDragHint = null;
    if (wasArmed && hint) void app.moveColumnAdjacent(group.id, hint);
  }

  function onGripPointerUp(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    columnDrag.onPointerUp();
    endGripTouchDrag();
  }

  function onGripPointerCancel(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    columnDrag.onPointerCancel();
    columnDragHint = null;
  }
```

- [ ] **Step 3: グリップアイコンにpointerハンドラを追加する**

`<span class="flex w-[26px] flex-none ...">`（179〜188行目）を以下に置き換える:

```svelte
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <span
        class="flex w-[26px] flex-none cursor-grab select-none items-center justify-center text-muted-foreground active:cursor-grabbing"
        draggable="true"
        ondragstart={(e) => {
          e.dataTransfer?.setData("text/plain", group.id);
          app.startDragGroup(group.id);
        }}
        ondragend={() => app.endDragGroup()}
        onpointerdown={onGripPointerDown}
        onpointermove={onGripPointerMove}
        onpointerup={onGripPointerUp}
        onpointercancel={onGripPointerCancel}
        title="ドラッグでカラムを並べ替え"
      ><GripVertical size={16} /></span>
```

- [ ] **Step 4: 前後ヒントのオーバーレイを追加する**

`</section>`の直前（372行目、既存の`{#if app.draggingGroupId && ...}`ブロックの直後）に追加する:

```svelte
  {#if columnDragHint !== null}
    <div class="pointer-events-none absolute inset-x-0 top-1 z-30 flex justify-center" use:portal>
      <div class="flex items-center gap-3 rounded-lg bg-background px-3 py-1.5 text-sm shadow-[0_8px_24px_rgba(0,0,0,0.25)]">
        <span class:text-foreground={columnDragHint === "prev"} class:text-muted-foreground={columnDragHint !== "prev"}>
          <ChevronLeft size={16} class="inline" /> 前へ
        </span>
        <span class:text-foreground={columnDragHint === "next"} class:text-muted-foreground={columnDragHint !== "next"}>
          次へ <ChevronRight size={16} class="inline" />
        </span>
      </div>
    </div>
  {/if}
```

- [ ] **Step 5: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 6: Commit**

```bash
git add frontend/src/ui/Column.svelte
git commit -m "feat: モバイル版でカラムの長押しドラッグ並び替えに対応"
```

---

### Task 7: 「…」メニューへの「左に移動」「右に移動」

**Files:**
- Modify: `frontend/src/ui/Column.svelte`

**Interfaces:**
- Consumes: Task 4の`app.canMoveColumnAdjacent`/`app.moveColumnAdjacent`。既存の`pickMenuItem`関数。
- Produces: なし。

- [ ] **Step 1: メニュー項目を追加する**

`menuOpen`ドロップダウン内、「カラム設定」ボタン（285〜292行目）の直前に追加する:

```svelte
        {#if app.canMoveColumnAdjacent(group.id, "prev")}
          <button
            type="button"
            role="menuitem"
            class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted"
            onclick={() => pickMenuItem(() => app.moveColumnAdjacent(group.id, "prev"))}
          >
            <ChevronLeft size={16} /> 左に移動
          </button>
        {/if}
        {#if app.canMoveColumnAdjacent(group.id, "next")}
          <button
            type="button"
            role="menuitem"
            class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted"
            onclick={() => pickMenuItem(() => app.moveColumnAdjacent(group.id, "next"))}
          >
            <ChevronRight size={16} /> 右に移動
          </button>
        {/if}
```

（`pickMenuItem`は`action: () => void`を受け取る想定だが、既存呼び出し（`onAddTab`等）も戻り値を無視しているだけなので、`() => app.moveColumnAdjacent(...)`（Promiseを返す関数）をそのまま渡してよい。）

- [ ] **Step 2: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 3: Commit**

```bash
git add frontend/src/ui/Column.svelte
git commit -m "feat: カラムメニューに左右移動ボタンを追加"
```

---

### Task 8: ユーザーガイド更新 + 最終確認

**Files:**
- Modify: `docs/guide/user-guide.md`

**Interfaces:**
- Consumes: なし。
- Produces: なし（ドキュメントのみ）。

- [ ] **Step 1: ドキュメントを追記する**

`docs/guide/user-guide.md`の「タブ・カラムの並び替え/幅調整」セクション、既存の「**モバイル版でのスワイプ操作**」の行（84行目付近）の直後に追加する:

```markdown
- **モバイル版でのタブ/カラム並び替え**: タブ・カラムグリップのどちらも指で長押しするとドラッグモードに入ります。タブは指の動きに追従して並び替えられます。カラムは画面上部に出る「◀ 前へ / 次へ ▶」のヒントで隣のカラムと入れ替えます（1回のドラッグで1つ隣まで）。「⋯」メニューの「左に移動」「右に移動」からも同じ操作ができます。
```

- [ ] **Step 2: 全体テストを実行する**

Run: `cd frontend && pnpm vitest run`
Expected: 全テストPASS（既存テスト + Task 1〜4で追加したテスト）

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 3: Commit**

```bash
git add docs/guide/user-guide.md
git commit -m "docs: モバイル版タブ/カラム並び替えの操作方法を追記"
```

- [ ] **Step 4: Android実機での動作確認（完了条件）**

`cargo tauri android build --debug --target aarch64`でビルドし、実機（または`docs/superpowers/specs/2026-09-14-mobile-tab-column-reorder-design.md`のテスト方針に記載の`chrome://inspect`でのpointerイベントトレース）で以下を確認する:

- タブバーの横スクロール中に長押し判定が誤発火しないこと。
- タブを長押し→ドラッグ→指を離すと、意図した位置に並び替わること。
- カラムグリップを長押し→左右にドラッグ→ヒントが正しくハイライトされ、指を離すと隣のカラムと入れ替わること。
- 先頭/末尾のカラムでは、該当方向のヒントが出ない（または常に非活性）こと。
- 「⋯」メニューの「左に移動」「右に移動」で同じ結果が得られること。

この実機確認が完了するまで、本Issueの完了とはしない（`touch-action-pan-y-pointercancel-native-scroll-conflict`の教訓）。

---

## Self-Review

- **Spec coverage**: design docの「タブの並び替え」→Task 5、「カラムの並び替え(長押しドラッグ+前後ヒント)」→Task 6、「…メニューへの左右移動」→Task 7、「共通関数moveColumnAdjacent」→Task 4、「longPressDrag.tsの状態機械テスト」→Task 1、「moveColumnAdjacentのユニットテスト」→Task 4、「実機確認を完了条件に含める」→Task 8 Step 4。オートスクロール非対応・連続複数ステップ非対応・ネスト配下非対応は各タスクのコード内コメント/Global Constraintsに明記済み。
- **Placeholder scan**: 各コードブロックは実際に貼り付け可能な完全なコードであり、TBD/TODOの類はない。
- **Type consistency**: `AdjacentDirection`/`ColumnDragDirection`（同じ`"prev"|"next"`の意味だが別モジュールの型としてTask 2/3で個別定義——`columnDragHint.ts`はDOM非依存の汎用モジュールとして`swipeNav.ts`に依存させたくないため意図的に分離。Column.svelteでの呼び出し側では両方とも`"prev"|"next"`の文字列リテラルなので相互に代入可能）。`app.moveColumnAdjacent`/`app.canMoveColumnAdjacent`のシグネチャはTask 4で定義した通りTask 6/7で使用。`resolveTabHit`/`resolveColumnDragHint`等の関数名はTask定義後に変更していない。
