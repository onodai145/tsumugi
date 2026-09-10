# モバイル版スワイプ CSS Scroll Snap化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Issue #296(モバイル版でのタブ/カラムスワイプ移動)を、pointer-drag方式からCSS Scroll Snap方式へ全面的に置き換える。実機検証でpointer-drag方式が機能しないと判明したため(`docs/superpowers/specs/2026-09-10-mobile-swipe-tab-column-scroll-snap-design.md`参照)。

**Architecture:** タブの横スワイプ・カラムの横スワイプの両方を、ブラウザネイティブの`scroll-snap-type`/`scroll-snap-align`で実現する。カラム内には「前/アクティブ/次」の最大3タブをスロットとして横並びに描画する内側のScroll Snapコンテナを新設し、`scrollend`(非対応環境は`scroll`のデバウンス)で着地したスロットを検知して`AppStore`の状態(`activeTabId`/`focusedGroupId`)を更新する。カラム間移動は既存の外側`overflow-x-auto`コンテナに`scroll-snap-type`を足すだけで、同じ着地検知の仕組みを流用する。「タブの端に来たらカラム移動」は、ブラウザ標準のスクロールチェイニング(内側の横スクロール範囲を使い切ると同軸のスクロールが親コンテナへ伝播する)にそのまま乗り、専用のJSロジックは不要。

**Tech Stack:** Svelte 5 (runes), TypeScript, Vitest, Tailwind CSS + CSS `scroll-snap-type`/`scroll-snap-align`/`scroll-snap-stop`。既存の`store.svelte.ts` / `Column.svelte` / `App.svelte`に乗る。新規ライブラリ追加なし。

## Global Constraints

- 対象は`app.useMobileUi()`が`true`の場合のみ。デスクトップUIの挙動・見た目は一切変更しない。
- 「カラムの並び」は最上位rowの直下の子のうちtype: "leaf"のものだけを対象にする(既存の`topLevelLeafGroupIds`のスコープ決定を継続)。
- タブ切替・カラムフォーカス変更は、スクロールが実際に着地(`scrollend`または相当のデバウンス)した時点、またはタブバーのタップ時にのみ行う。スクロール中の中間状態では更新しない。
- 非アクティブ(隣接プレビュー用)スロットは、`tab.notes`の全件ではなく先頭50件だけ描画する。
- 新規E2Eテストは追加しない。実機(Android)での動作確認を本plan完了の条件に含める(前版はここを省略して機能しない実装を見逃した)。

---

## ファイル構成

- **変更(トリム)** `frontend/src/lib/swipeNav.ts` — `resolveSwipeTarget`/`SwipeTarget`/`SwipeDirection`/`SwipeGroup`を削除し、`topLevelLeafGroupIds`のみ残す(カラム間Scroll Snapの着地先groupId解決に引き続き使う)。
- **変更(トリム)** `frontend/src/lib/swipeNav.test.ts` — `resolveSwipeTarget`のテストを削除し、`topLevelLeafGroupIds`のテストのみ残す。
- **削除** `frontend/src/lib/swipeGesture.ts` / `frontend/src/lib/swipeGesture.test.ts` — pointer-drag方式専用の軸判定/確定判定/ラバーバンド計算で、Scroll Snap方式では不要。
- **削除** `frontend/src/lib/store.svelte.swipe.test.ts` — `applySwipe`のテスト。`applySwipe`自体を削除するため。
- **変更** `frontend/src/lib/store.svelte.ts` — `applySwipe`を削除し、`focusColumn(groupId)`を新設。
- **新規** `frontend/src/lib/tabSlots.ts` / `frontend/src/lib/tabSlots.test.ts` — カラム内のタブスロット(前/アクティブ/次)を解決する純粋関数。
- **新規** `frontend/src/lib/scrollSnapIndex.ts` / `frontend/src/lib/scrollSnapIndex.test.ts` — `scrollLeft`から着地スロットのインデックスを解決する純粋関数(タブ用・カラム用で共用)。
- **新規** `frontend/src/lib/store.svelte.focusColumn.test.ts` — `focusColumn`のテスト。
- **変更** `frontend/src/ui/Column.svelte` — pointer-drag実装を削除し、タブ用Scroll Snapスロットコンテナに置き換える。
- **変更** `frontend/src/App.svelte` — 外側カラムコンテナにScroll Snapを追加し、着地検知で`focusColumn`を呼ぶ。

---

### Task 1: pointer-drag実装の削除(クリーンな土台に戻す)

**Files:**
- Delete: `frontend/src/lib/swipeGesture.ts`, `frontend/src/lib/swipeGesture.test.ts`, `frontend/src/lib/store.svelte.swipe.test.ts`
- Modify: `frontend/src/lib/swipeNav.ts`, `frontend/src/lib/swipeNav.test.ts`, `frontend/src/lib/store.svelte.ts`, `frontend/src/ui/Column.svelte`

**Interfaces:**
- Consumes: なし。
- Produces: `swipeNav.ts`から`topLevelLeafGroupIds(paneRoot: PaneNode): string[]`のみが残る(Task 6が使う)。`Column.svelte`は`{#snippet tabBody(tab: TabView)}`とタブバーはそのまま残し、`{#if activeTab}`以降をpointer-drag無しの素朴な単一ペイン描画に戻す。

- [ ] **Step 1: 不要ファイルを削除する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git rm frontend/src/lib/swipeGesture.ts frontend/src/lib/swipeGesture.test.ts frontend/src/lib/store.svelte.swipe.test.ts
```

- [ ] **Step 2: `swipeNav.ts`をトリムする**

`frontend/src/lib/swipeNav.ts`を以下の内容に置き換える:

```ts
// モバイル版のカラムScroll Snapで「最上位rowの直下leafの並び」を解決する純粋関数(Issue #296)。
import type { PaneNode } from "../bindings/tauri.gen";

/// 最上位rowの直下の子のうちtype:"leaf"のgroupIdだけを、出現順で返す。
/// ネストしたsplit配下のカラムはカラム間Scroll Snapの対象外とする(Issue #296のスコープ決定)。
export function topLevelLeafGroupIds(paneRoot: PaneNode): string[] {
  if (paneRoot.type === "leaf") return [paneRoot.groupId];
  return paneRoot.children.filter((c) => c.node.type === "leaf").map((c) => (c.node as { groupId: string }).groupId);
}
```

- [ ] **Step 3: `swipeNav.test.ts`をトリムする**

`frontend/src/lib/swipeNav.test.ts`を以下の内容に置き換える:

```ts
import { describe, expect, it } from "vitest";
import type { PaneNode } from "../bindings/tauri.gen";
import { topLevelLeafGroupIds } from "./swipeNav";

function leaf(id: string, groupId: string): PaneNode {
  return { type: "leaf", id, groupId };
}

function row(id: string, children: PaneNode[]): PaneNode {
  return { type: "split", id, direction: "row", children: children.map((node) => ({ node, size: null, auto: true })) };
}

describe("topLevelLeafGroupIds", () => {
  it("最上位rowの直下のleafのgroupIdを出現順で返す", () => {
    const root = row("root", [leaf("l1", "g1"), leaf("l2", "g2"), leaf("l3", "g3")]);
    expect(topLevelLeafGroupIds(root)).toEqual(["g1", "g2", "g3"]);
  });

  it("ネストしたsplit配下のleafは含めない", () => {
    const nested = row("nested", [leaf("l2", "g2"), leaf("l3", "g3")]);
    const root = row("root", [leaf("l1", "g1"), nested]);
    expect(topLevelLeafGroupIds(root)).toEqual(["g1"]);
  });

  it("rootがleaf単体の場合はそのgroupIdのみ返す", () => {
    expect(topLevelLeafGroupIds(leaf("l1", "g1"))).toEqual(["g1"]);
  });
});
```

- [ ] **Step 4: `store.svelte.ts`から`applySwipe`を削除する**

`frontend/src/lib/store.svelte.ts`の以下のimport行を削除する:

```ts
import { resolveSwipeTarget, type SwipeDirection, type SwipeTarget } from "./swipeNav";
```

同ファイルの`setActiveTab`メソッドの直後にある以下のメソッド全体を削除する:

```ts
  /// モバイル版の左右スワイプ確定時に呼ぶ(Issue #296)。タブ送り、または
  /// タブの端でのカラム移動を実際に適用する。ドラッグ中の中間状態では呼ばず、
  /// ドラッグ確定時にのみ呼ぶこと。戻り値は実際に適用した内容
  /// (呼び出し側=Column.svelteのアニメーション演出に使う)。
  applySwipe(groupId: string, direction: SwipeDirection): SwipeTarget {
    const target = resolveSwipeTarget(this.groups, this.paneRoot, groupId, direction);
    if (!target) return null;
    if (target.kind === "tab") {
      this.setActiveTab(target.groupId, target.tabId);
    } else {
      this.focusedGroupId = target.groupId;
    }
    return target;
  }
```

- [ ] **Step 5: `Column.svelte`からpointer-drag実装を削除する**

`frontend/src/ui/Column.svelte`の`<script>`内、以下のimportを削除する:

```ts
  import { resolveSwipeTarget, type SwipeTarget } from "../lib/swipeNav";
  import { applyRubberBand, resolveSwipeAxis, shouldCommitSwipe, type SwipeAxis } from "../lib/swipeGesture";
```

`onScroll`関数の直後から`onSwipeCancel`関数の終わりまで(`// モバイル版: カラム本体の左右スワイプで...`のコメントで始まり、`function onSwipeCancel(e: PointerEvent) { ... }`で終わるブロック全体。`SETTLE_MS`定数、`SwipeDrag`型、`drag`/`settling`/`contentEl`/`activePaneEl`の`$state`、`peekTab`/`onSwipeDown`/`onSwipeMove`/`settle`/`onSwipeUp`/`onSwipeCancel`関数を含む)を丸ごと削除する。

テンプレート内、`{#if activeTab}`ブロック(`{@const peek = ...}`から始まり、`transform`つきのラッパーdiv構造を持つブロック)を、以下に置き換える(`{#snippet tabBody}`はそのまま残す):

```svelte
  {#if activeTab}
    <div class="flex-1 overflow-y-auto" onscroll={onScroll}>
      {@render tabBody(activeTab)}
    </div>
  {/if}
```

`data-group-id={group.id}`属性(`<section class="column-root ...">`上)はそのまま残す(Task 6でカラム間Scroll Snapの着地先解決に使う)。

- [ ] **Step 6: 型チェックと既存テストの回帰確認**

Run: `cd frontend && pnpm check && pnpm test`
Expected: 両方PASS。テスト件数は削除したファイル分(`swipeGesture.test.ts`の9件、`swipeNav.test.ts`の`resolveSwipeTarget`分8件、`store.svelte.swipe.test.ts`の3件)減っているはず。

- [ ] **Step 7: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add -A
git commit -m "refactor: pointer-drag方式のスワイプ実装を削除(Scroll Snap方式への移行準備)"
```

---

### Task 2: タブスロット解決の純粋関数

**Files:**
- Create: `frontend/src/lib/tabSlots.ts`
- Test: `frontend/src/lib/tabSlots.test.ts`

**Interfaces:**
- Consumes: なし(独立モジュール)。
- Produces:
  - `export interface TabSlot<T extends { id: string }> { tab: T; role: "prev" | "active" | "next"; }`
  - `export function computeTabSlots<T extends { id: string }>(tabs: T[], activeTabId: string): TabSlot<T>[]`
  - `export function activeSlotIndex<T extends { id: string }>(slots: TabSlot<T>[]): number`
  - `export function notesForSlot<N>(notes: N[], role: "prev" | "active" | "next"): N[]`
  - Task 5がこの3関数をそのまま使う。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/tabSlots.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { activeSlotIndex, computeTabSlots, notesForSlot } from "./tabSlots";

function tab(id: string) {
  return { id };
}

describe("computeTabSlots", () => {
  it("中間のタブは前/アクティブ/次の3スロットになる", () => {
    const tabs = [tab("a"), tab("b"), tab("c")];
    expect(computeTabSlots(tabs, "b")).toEqual([
      { tab: tab("a"), role: "prev" },
      { tab: tab("b"), role: "active" },
      { tab: tab("c"), role: "next" },
    ]);
  });

  it("先頭タブはprevスロットが無い(2スロット)", () => {
    const tabs = [tab("a"), tab("b"), tab("c")];
    expect(computeTabSlots(tabs, "a")).toEqual([
      { tab: tab("a"), role: "active" },
      { tab: tab("b"), role: "next" },
    ]);
  });

  it("末尾タブはnextスロットが無い(2スロット)", () => {
    const tabs = [tab("a"), tab("b"), tab("c")];
    expect(computeTabSlots(tabs, "c")).toEqual([
      { tab: tab("b"), role: "prev" },
      { tab: tab("c"), role: "active" },
    ]);
  });

  it("タブが1つしかない場合は1スロットのみ", () => {
    const tabs = [tab("a")];
    expect(computeTabSlots(tabs, "a")).toEqual([{ tab: tab("a"), role: "active" }]);
  });

  it("activeTabIdが見つからない場合は空配列", () => {
    const tabs = [tab("a"), tab("b")];
    expect(computeTabSlots(tabs, "missing")).toEqual([]);
  });
});

describe("activeSlotIndex", () => {
  it("3スロット中のactiveの位置(中央=1)を返す", () => {
    const slots = computeTabSlots([tab("a"), tab("b"), tab("c")], "b");
    expect(activeSlotIndex(slots)).toBe(1);
  });

  it("先頭タブ(2スロット)ではactiveは0番目", () => {
    const slots = computeTabSlots([tab("a"), tab("b")], "a");
    expect(activeSlotIndex(slots)).toBe(0);
  });
});

describe("notesForSlot", () => {
  const notes = Array.from({ length: 80 }, (_, i) => ({ id: `n${i}` }));

  it("activeスロットは全件そのまま返す", () => {
    expect(notesForSlot(notes, "active")).toHaveLength(80);
  });

  it("prev/nextスロットは先頭50件に制限する", () => {
    expect(notesForSlot(notes, "prev")).toHaveLength(50);
    expect(notesForSlot(notes, "next")).toHaveLength(50);
    expect(notesForSlot(notes, "prev")[0]).toEqual({ id: "n0" });
  });

  it("件数が制限未満ならそのまま返す", () => {
    const few = notes.slice(0, 10);
    expect(notesForSlot(few, "next")).toHaveLength(10);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm exec vitest run src/lib/tabSlots.test.ts`
Expected: FAIL(`./tabSlots`が存在せずimportエラー)

- [ ] **Step 3: 最小実装を書く**

`frontend/src/lib/tabSlots.ts`:

```ts
// モバイル版のタブ横スワイプ(Scroll Snap)で「どのタブをスロットとして描画するか」を
// 解決する純粋関数(Issue #296)。前/アクティブ/次の最大3スロットを組み立てる。

export interface TabSlot<T extends { id: string }> {
  tab: T;
  role: "prev" | "active" | "next";
}

const PREVIEW_NOTE_LIMIT = 50;

/// タブ配列とアクティブなタブIDから、描画すべきスロット(最大3つ、出現順)を返す。
/// アクティブタブが見つからない場合は空配列を返す。
export function computeTabSlots<T extends { id: string }>(tabs: T[], activeTabId: string): TabSlot<T>[] {
  const index = tabs.findIndex((t) => t.id === activeTabId);
  if (index < 0) return [];
  const slots: TabSlot<T>[] = [];
  if (index > 0) slots.push({ tab: tabs[index - 1], role: "prev" });
  slots.push({ tab: tabs[index], role: "active" });
  if (index < tabs.length - 1) slots.push({ tab: tabs[index + 1], role: "next" });
  return slots;
}

/// computeTabSlotsの結果からアクティブスロットのインデックス(0始まり)を返す。
export function activeSlotIndex<T extends { id: string }>(slots: TabSlot<T>[]): number {
  return slots.findIndex((s) => s.role === "active");
}

/// 非アクティブ(prev/next)スロットは、フルマウントのコストを抑えるため先頭
/// PREVIEW_NOTE_LIMIT件だけ描画する。アクティブスロットは全件そのまま返す。
export function notesForSlot<N>(notes: N[], role: "prev" | "active" | "next"): N[] {
  if (role === "active") return notes;
  return notes.slice(0, PREVIEW_NOTE_LIMIT);
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && pnpm exec vitest run src/lib/tabSlots.test.ts`
Expected: PASS(全ケース)

- [ ] **Step 5: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/tabSlots.ts frontend/src/lib/tabSlots.test.ts
git commit -m "feat: タブScroll Snap用のスロット解決ロジックを追加"
```

---

### Task 3: scrollLeftから着地スロットを解決する純粋関数

**Files:**
- Create: `frontend/src/lib/scrollSnapIndex.ts`
- Test: `frontend/src/lib/scrollSnapIndex.test.ts`

**Interfaces:**
- Consumes: なし(独立モジュール)。
- Produces: `export function resolveSettledIndex(scrollLeft: number, containerWidth: number, slotCount: number): number` — Task 5(タブ用)・Task 6(カラム用)の両方がそのまま使う。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/scrollSnapIndex.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { resolveSettledIndex } from "./scrollSnapIndex";

describe("resolveSettledIndex", () => {
  it("scrollLeftがちょうどコンテナ幅の倍数ならそのインデックス", () => {
    expect(resolveSettledIndex(0, 300, 3)).toBe(0);
    expect(resolveSettledIndex(300, 300, 3)).toBe(1);
    expect(resolveSettledIndex(600, 300, 3)).toBe(2);
  });

  it("端数は最も近いインデックスに丸める", () => {
    expect(resolveSettledIndex(280, 300, 3)).toBe(1);
    expect(resolveSettledIndex(320, 300, 3)).toBe(1);
  });

  it("オーバースクロールで負の値になっても0にクランプする", () => {
    expect(resolveSettledIndex(-40, 300, 3)).toBe(0);
  });

  it("最後を超えるスクロールでも最大インデックスにクランプする", () => {
    expect(resolveSettledIndex(1000, 300, 3)).toBe(2);
  });

  it("containerWidthが0以下なら0を返す", () => {
    expect(resolveSettledIndex(300, 0, 3)).toBe(0);
  });

  it("slotCountが0以下なら0を返す", () => {
    expect(resolveSettledIndex(300, 300, 0)).toBe(0);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm exec vitest run src/lib/scrollSnapIndex.test.ts`
Expected: FAIL(`./scrollSnapIndex`が存在せずimportエラー)

- [ ] **Step 3: 最小実装を書く**

`frontend/src/lib/scrollSnapIndex.ts`:

```ts
// Scroll Snapコンテナの現在のscrollLeftから、実際に着地したスロットのインデックスを
// 解決する純粋関数(Issue #296)。タブ用・カラム用のscrollend(または相当のデバウンス)
// ハンドラの両方から使う。

/// scrollLeftをcontainerWidthで割って最も近いスロットインデックスに丸め、
/// 0..slotCount-1の範囲にクランプする。オーバースクロール(負の値/範囲外)にも耐える。
export function resolveSettledIndex(scrollLeft: number, containerWidth: number, slotCount: number): number {
  if (containerWidth <= 0 || slotCount <= 0) return 0;
  const raw = Math.round(scrollLeft / containerWidth);
  return Math.max(0, Math.min(slotCount - 1, raw));
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && pnpm exec vitest run src/lib/scrollSnapIndex.test.ts`
Expected: PASS(全ケース)

- [ ] **Step 5: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/scrollSnapIndex.ts frontend/src/lib/scrollSnapIndex.test.ts
git commit -m "feat: Scroll Snapの着地スロット解決ロジックを追加"
```

---

### Task 4: AppStoreへのfocusColumnメソッド追加

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`
- Test: `frontend/src/lib/store.svelte.focusColumn.test.ts`

**Interfaces:**
- Consumes: なし(既存の`this.groups`/`this.focusedGroupId`のみ)。
- Produces: `AppStore.focusColumn(groupId: string): void` — Task 6がカラムScroll Snap確定時に呼ぶ。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/store.svelte.focusColumn.test.ts`(既存`store.svelte.haptics.test.ts`と同じファイルスコープ`vi.mock`構成に倣う):

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

beforeEach(() => {
  invokeMock.mockClear();
  app.groups = [];
  app.focusedGroupId = null;
});

describe("focusColumn(Issue #296)", () => {
  it("存在するグループへフォーカスを移す", () => {
    app.groups = [makeGroup("g1"), makeGroup("g2")];
    app.focusColumn("g2");
    expect(app.focusedGroupId).toBe("g2");
  });

  it("存在しないグループIDでは何も変更しない", () => {
    app.groups = [makeGroup("g1")];
    app.focusedGroupId = "g1";
    app.focusColumn("missing");
    expect(app.focusedGroupId).toBe("g1");
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm exec vitest run src/lib/store.svelte.focusColumn.test.ts`
Expected: FAIL(`app.focusColumn is not a function`)

- [ ] **Step 3: 最小実装を書く**

`frontend/src/lib/store.svelte.ts`の`setActiveTab`メソッドの直後に追加(Task 1で`applySwipe`を削除した跡地):

```ts
  /// モバイル版のカラム間Scroll Snap確定時に呼ぶ(Issue #296)。タブは変えず、
  /// フォーカスのみ対象カラムへ移す。存在しないgroupIdなら何もしない。
  focusColumn(groupId: string) {
    if (!this.groups.some((g) => g.id === groupId)) return;
    this.focusedGroupId = groupId;
  }
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && pnpm exec vitest run src/lib/store.svelte.focusColumn.test.ts`
Expected: PASS(全ケース)

- [ ] **Step 5: 型チェックと既存テストの回帰確認**

Run: `cd frontend && pnpm check && pnpm test`
Expected: 両方PASS

- [ ] **Step 6: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.focusColumn.test.ts
git commit -m "feat: AppStoreにfocusColumnメソッドを追加"
```

---

### Task 5: Column.svelteへのタブScroll Snap統合

**Files:**
- Modify: `frontend/src/ui/Column.svelte`

**Interfaces:**
- Consumes: Task 2の`computeTabSlots`/`activeSlotIndex`/`notesForSlot`(`../lib/tabSlots`)、Task 3の`resolveSettledIndex`(`../lib/scrollSnapIndex`)、既存の`app.setActiveTab`(変更なし)。
- Produces: なし(末端のUI統合)。

このタスクはネイティブスクロール+CSSの統合が中心で、jsdom上のユニットテストでは実際の挙動(タッチでのスクロール、スナップ、`scrollend`のタイミング)を検証できない。純粋ロジックはTask 2/3で既にテスト済みという前提で、ここでは型チェックと既存テストの回帰確認のみ行う。実機(Android)でのスワイプ操作の手動確認は、この一連のタスク完了後にユーザー側で行う。

- [ ] **Step 1: importと状態を追加する**

`frontend/src/ui/Column.svelte`の`<script>`内、既存のimportに追記:

```ts
  import { activeSlotIndex, computeTabSlots, notesForSlot } from "../lib/tabSlots";
  import { resolveSettledIndex } from "../lib/scrollSnapIndex";
```

`onScroll`関数の直後に追加:

```ts
  // モバイル版: カラム内タブの横スワイプをCSS Scroll Snapで実現する(Issue #296)。
  // 前/アクティブ/次の最大3スロットを横並びに描画し、scrollend(またはscrollの
  // デバウンス)で着地したスロットを検知してsetActiveTabを呼ぶ。
  const slots = $derived(computeTabSlots(group.tabs, group.activeTabId));
  const activeIndex = $derived(activeSlotIndex(slots));

  let tabsEl = $state<HTMLElement | null>(null);
  let settleTimer: ReturnType<typeof setTimeout> | null = null;
  const supportsScrollEnd = typeof window !== "undefined" && "onscrollend" in window;

  // スロット構成(=activeTabId)が変わるたびに、スクロール位置をアニメーション無しで
  // 対応するスロットへ即座に合わせ直す。タブバーのタップによる切替でも、スワイプ確定に
  // よる切替でも、この一箇所で辻褄を合わせる(前/次の中身が入れ替わることでスロット配列の
  // 要素数・並びが変わりうるため、常にscrollLeftを引き直す必要がある)。
  $effect(() => {
    const idx = activeIndex;
    if (!tabsEl) return;
    const width = tabsEl.clientWidth;
    if (width <= 0) return;
    tabsEl.scrollLeft = idx * width;
  });

  function onTabsSettled() {
    if (!tabsEl || tabsEl.clientWidth <= 0) return;
    const idx = resolveSettledIndex(tabsEl.scrollLeft, tabsEl.clientWidth, slots.length);
    const settled = slots[idx]?.tab;
    if (settled && settled.id !== group.activeTabId) {
      app.setActiveTab(group.id, settled.id);
    }
  }

  function onTabsScroll() {
    // scrollend対応環境ではそちらに任せる(二重発火を避けるため、ここでは何もしない)。
    if (supportsScrollEnd) return;
    if (settleTimer) clearTimeout(settleTimer);
    settleTimer = setTimeout(onTabsSettled, 120);
  }
```

- [ ] **Step 2: テンプレートをタブScroll Snapコンテナに置き換える**

Task 1で以下の形に戻した`{#if activeTab}`ブロックを:

```svelte
  {#if activeTab}
    <div class="flex-1 overflow-y-auto" onscroll={onScroll}>
      {@render tabBody(activeTab)}
    </div>
  {/if}
```

以下に置き換える(`{#snippet tabBody}`はそのまま、この直後に配置する):

```svelte
  {#if activeTab}
    <div
      class="flex flex-1 [overflow-x:auto] [overscroll-behavior-x:auto]"
      style:scroll-snap-type={app.useMobileUi() ? "x mandatory" : undefined}
      bind:this={tabsEl}
      onscroll={onTabsScroll}
      onscrollend={onTabsSettled}
    >
      {#each slots as slot (slot.tab.id)}
        <div class="h-full w-full flex-none [scroll-snap-align:start] [scroll-snap-stop:always] overflow-y-auto" onscroll={slot.role === "active" ? onScroll : undefined}>
          {@render tabBody(slot.tab, slot.role)}
        </div>
      {/each}
    </div>
  {/if}
```

`{#each ... (slot.tab.id)}`のキーにより、同じタブが前後どちらのロールでも同じDOMノードを
再利用する(縦スクロール位置が保持される)。`overscroll-behavior-x: auto`(デフォルト値を明示)は、
このコンテナが横方向のスクロール範囲を使い切ったとき、同軸のスクロールが親(カラム間コンテナ、
Task 6で追加)へ伝播する「スクロールチェイニング」を妨げないようにするため明示している。

- [ ] **Step 3: `tabBody`スニペットにroleパラメータを追加する**

`{#snippet tabBody(tab: TabView)}`を`{#snippet tabBody(tab: TabView, role: "prev" | "active" | "next" = "active")}`に変更し、スニペット内の`tab.notifications`/`tab.notes`を描画する`{#each}`の対象を、Task 2の`notesForSlot`で絞り込む:

```svelte
  {#snippet tabBody(tab: TabView, role: "prev" | "active" | "next" = "active")}
    {@const notif = tab.kind.type === "notifications"}
    {#if notif}
      {#each notesForSlot(tab.notifications, role) as n (n.id)}
        <NotificationCard notification={n} accountId={tab.accountId} />
      {/each}
      {#if tab.notifications.length === 0 && !tab.loadingMore}
        <div class="p-3.5 text-center text-sm text-muted-foreground">まだ通知がありません</div>
      {/if}
    {:else}
      {#each notesForSlot(tab.notes, role) as note (note.id)}
        <NoteCard {note} accountId={tab.accountId} tabId={tab.id} selected={note.id === tab.selectedNoteId} />
        {#if tab.gapMarker && note.id === tab.gapMarker.boundaryId}
          <div class="flex items-center gap-2 border-y border-border bg-muted/40 px-3.5 py-2 text-sm text-muted-foreground">
            <span class="flex-1">この間の投稿は省略されています</span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={tab.fillingGap}
              onclick={() => app.fillRemainingGap(tab.id)}
            >
              {tab.fillingGap ? "取得中…" : "省略された投稿を表示"}
            </Button>
          </div>
        {/if}
      {/each}
      {#if tab.notes.length === 0 && !tab.loadingMore}
        <div class="p-3.5 text-center text-sm text-muted-foreground">まだノートがありません</div>
      {/if}
    {/if}
    {#if tab.loadingMore}<div class="p-3.5 text-center text-sm text-muted-foreground">読み込み中…</div>{/if}
  {/snippet}
```

デスクトップUI(`useMobileUi()`が`false`)では常に`role="active"`(デフォルト値)で呼ばれるため、
`notesForSlot`は常に全件をそのまま返し、既存の見た目・挙動は変わらない。

デスクトップUIでは`slots`は依然として計算されるが(`computeTabSlots`はモバイル/デスクトップを
区別しない)、`scroll-snap-type`が`undefined`(未設定)のため、複数スロットが横に並ぶDOM自体は
存在してもCSS上はただの横に並んだブロックになり、`.flex`コンテナの`[overflow-x:auto]`により
ユーザーが手動で横スクロールすれば見えてしまう。デスクトップでは`useMobileUi()`が`false`のときのみ
`slots`を`[{ tab: activeTab, role: "active" }]`相当の1要素配列にフォールバックさせ、デスクトップの
見た目を完全に変えないようにする。`slots`の`$derived`定義を以下に差し替える:

```ts
  const slots = $derived(
    app.useMobileUi() ? computeTabSlots(group.tabs, group.activeTabId) : activeTab ? [{ tab: activeTab, role: "active" as const }] : [],
  );
```

- [ ] **Step 4: 型チェックと既存テストの回帰確認**

Run: `cd frontend && pnpm check`
Expected: PASS(型エラーなし)

Run: `cd frontend && pnpm test`
Expected: PASS(既存テストに影響なし)

- [ ] **Step 5: デスクトップUIへの影響が無いことを確認する**

`grep -rl "Column.svelte" frontend/src/ui/*.test.ts`で直接テストが無いことを確認済み(前版のTask 4で確認済み)。`app.useMobileUi()`が`false`のとき`slots`が1要素固定になること、`style:scroll-snap-type`が`undefined`になることをコードレビューで確認する。

- [ ] **Step 6: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/Column.svelte
git commit -m "feat: モバイル版のタブ横スワイプをCSS Scroll Snapで実装"
```

---

### Task 6: App.svelte/Column.svelteへのカラムScroll Snap統合

**Files:**
- Modify: `frontend/src/App.svelte`, `frontend/src/ui/Column.svelte`

**Interfaces:**
- Consumes: Task 1で残した`topLevelLeafGroupIds`(`../lib/swipeNav`)、Task 3の`resolveSettledIndex`(`../lib/scrollSnapIndex`)、Task 4の`app.focusColumn`。
- Produces: なし(末端のUI統合)。

Task 5と同様、ネイティブスクロールの実際の挙動はjsdomで検証できないため、型チェックと既存テストの回帰確認のみ行い、実機確認は後続でユーザーが行う。

- [ ] **Step 1: カラムをモバイルでは画面幅いっぱいにする**

`frontend/src/ui/Column.svelte`の`<section class="column-root ...">`の`style`属性を、モバイル時は
常に幅100%(1カラム=1ページ)になるよう分岐を追加する:

現在:

```svelte
  style={stretch ? "flex:1 1 0;min-width:0" : group.auto ? "flex:1 1 0;min-width:220px" : `width:${group.width}px`}
```

変更後:

```svelte
  style={app.useMobileUi() ? "flex:0 0 100%;width:100%;min-width:0" : stretch ? "flex:1 1 0;min-width:0" : group.auto ? "flex:1 1 0;min-width:220px" : `width:${group.width}px`}
```

これにより、モバイルではカラムの分割設定(`group.auto`/`group.width`)に関わらず、常に外側スクロール
コンテナの幅いっぱいの1ページとして表示される(スクロールチェイニング・スナップともに、各カラムが
均等な幅を持つ前提で成立する設計のため)。

`<section>`にモバイル時のみ`scroll-snap-align: start`を追加する(`data-group-id={group.id}`の直後):

```svelte
  style:scroll-snap-align={app.useMobileUi() ? "start" : undefined}
  style:scroll-snap-stop={app.useMobileUi() ? "always" : undefined}
```

- [ ] **Step 2: `App.svelte`の外側コンテナにScroll Snapを追加する**

`frontend/src/App.svelte`の`import`に以下を追加:

```ts
  import { topLevelLeafGroupIds } from "./lib/swipeNav";
  import { resolveSettledIndex } from "./lib/scrollSnapIndex";
```

`useMobileUi`の`$derived`定義の直後あたりに、以下の状態とハンドラを追加:

```ts
  // モバイル版: カラム間の横スワイプをCSS Scroll Snapで実現する(Issue #296)。
  let columnsScrollEl = $state<HTMLElement | null>(null);
  let columnSettleTimer: ReturnType<typeof setTimeout> | null = null;
  const supportsScrollEnd = typeof window !== "undefined" && "onscrollend" in window;

  function onColumnsSettled() {
    if (!useMobileUi || !columnsScrollEl || columnsScrollEl.clientWidth <= 0) return;
    const order = topLevelLeafGroupIds(app.paneRoot);
    if (order.length === 0) return;
    const idx = resolveSettledIndex(columnsScrollEl.scrollLeft, columnsScrollEl.clientWidth, order.length);
    const groupId = order[idx];
    if (groupId && groupId !== app.focusedGroupId) app.focusColumn(groupId);
  }

  function onColumnsScroll() {
    if (supportsScrollEnd) return;
    if (columnSettleTimer) clearTimeout(columnSettleTimer);
    columnSettleTimer = setTimeout(onColumnsSettled, 120);
  }
```

`<div class="flex h-full overflow-x-auto" data-columns-scroll>`を以下に置き換える:

```svelte
      <div
        class="flex h-full overflow-x-auto"
        data-columns-scroll
        style:scroll-snap-type={useMobileUi ? "x mandatory" : undefined}
        bind:this={columnsScrollEl}
        onscroll={onColumnsScroll}
        onscrollend={onColumnsSettled}
      >
        <Pane node={app.paneRoot} onAddTab={openAddTab} onEditTab={openEditTab} onEditGroup={openColumnSettings} onSplitDown={splitDown} onSplitRight={splitRight} />
      </div>
```

- [ ] **Step 3: フォーカス変更時にカラム位置を合わせ直す(タップ相当の操作向け)**

キーボードショートカット等で`focusedGroupId`が変わった場合にも、モバイルではカラム表示位置を
追随させる必要がある。`App.svelte`に以下の`$effect`を追加する(`columnsScrollEl`の定義の後):

```ts
  $effect(() => {
    const groupId = app.focusedGroupId;
    if (!useMobileUi || !columnsScrollEl || !groupId) return;
    const order = topLevelLeafGroupIds(app.paneRoot);
    const idx = order.indexOf(groupId);
    if (idx < 0) return;
    const width = columnsScrollEl.clientWidth;
    if (width <= 0) return;
    columnsScrollEl.scrollLeft = idx * width;
  });
```

Column.svelteのタブScroll Snap(Task 5)と同様、この効果は「フォーカス対象カラムが変わるたび、
アニメーション無しでスクロール位置を合わせ直す」役割を持つ。カラム自体のScroll Snapによる
着地検知(`onColumnsSettled`)が`app.focusColumn`を呼んだ結果としてこの`$effect`が再度走っても、
既に正しい位置にいるため`scrollLeft`の代入は実質的に無変化(冪等)であり、問題ない。

- [ ] **Step 4: 型チェックと既存テストの回帰確認**

Run: `cd frontend && pnpm check`
Expected: PASS(型エラーなし)

Run: `cd frontend && pnpm test`
Expected: PASS(既存テストに影響なし)

- [ ] **Step 5: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/App.svelte frontend/src/ui/Column.svelte
git commit -m "feat: モバイル版のカラム横スワイプをCSS Scroll Snapで実装"
```

---

## 完了後の確認事項(ユーザー側・必須)

前版はこの確認を省略して機能しない実装を見逃した。今回は完了条件に含める。

- Android実機で以下を確認する:
  - カラム内にタブが複数あるとき、ノート一覧を横にスワイプするとタブが切り替わり、スナップして
    静止すること(縦スクロールとの誤反応が無いこと)。
  - 先頭/末尾タブでさらに同方向にスワイプを続けると、途切れず連続した動きで次/前のカラムへ
    移動すること(スクロールチェイニング)。
  - タブバーをタップした場合も、アニメーション無しで即座に該当タブへ切り替わること。
  - 通知タブとノートタブが混在するカラムでの表示、スワイプ開始直後のフレーム落ちの有無。
  - デスクトップ版(`cargo tauri dev`)で、タブ切替・カラム分割・幅リサイズなど既存の挙動に
    変化が無いこと。
