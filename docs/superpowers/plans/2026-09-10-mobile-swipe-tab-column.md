# モバイル版スワイプでのタブ/カラム移動 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** モバイル版で、カラム本体を左右スワイプするとタブを送り、タブの端に達したらカラムを移動できるようにする(Issue #296)。

**Architecture:** 「次/前のタブ・カラムを決める」ロジックと「スワイプ角度判定・確定判定」の数値計算をそれぞれ独立した純粋関数モジュールに切り出し、Vitestで単体テストする。`AppStore`(`store.svelte.ts`)はその純粋関数を使って実際に状態(`activeTabId`/`focusedGroupId`)を更新する薄いメソッドを1つ持つ。`Column.svelte`はpointerイベントでドラッグ量を追跡し、上記の純粋関数で確定判定・移動先解決を行った上で、ドラッグ追従とスプリングバック/確定アニメーションのUIを実装する。

**Tech Stack:** Svelte 5 (runes), TypeScript, Vitest, Tailwind CSS。既存の`store.svelte.ts` / `Column.svelte` / `App.svelte`に乗る。新規ライブラリ追加なし。

## Global Constraints

- 対象は`app.useMobileUi()`が`true`の場合のみ。デスクトップUIの挙動・見た目は一切変更しない。
- 「カラムの並び」は**最上位rowの直下の子のうちtype: "leaf"のものだけ**を対象にする。ネストしたsplit配下のカラムはカラム間移動の対象外。
- タブ切替・カラム移動は、ドラッグ確定(pointerup後のアニメーション完了)時にのみ状態を更新する。ドラッグ中の中間状態では`activeTabId`/`focusedGroupId`を変更しない。
- 移動先が存在しない端(先頭カラムの先頭タブでさらに前、末尾カラムの末尾タブでさらに次)ではラバーバンド(抵抗)を出し、確定させない。
- 新規E2Eテストは追加しない(既存`e2e/`はデスクトップ想定のtauri-driver構成のため)。Android実機での手動確認は本plan完了後にユーザー側で行う。

---

## ファイル構成

- **新規** `frontend/src/lib/swipeNav.ts` — 次/前のタブ・カラムを解決する純粋関数。
- **新規** `frontend/src/lib/swipeNav.test.ts` — 上記のユニットテスト。
- **新規** `frontend/src/lib/swipeGesture.ts` — スワイプの軸判定・確定判定・ラバーバンド量を計算する純粋関数。
- **新規** `frontend/src/lib/swipeGesture.test.ts` — 上記のユニットテスト。
- **変更** `frontend/src/lib/store.svelte.ts` — `applySwipe(groupId, direction)`メソッドを追加。
- **新規** `frontend/src/lib/store.svelte.swipe.test.ts` — `applySwipe`のユニットテスト(既存`store.svelte.haptics.test.ts`と同じ、ファイルスコープ`vi.mock`構成に倣う)。
- **変更** `frontend/src/ui/Column.svelte` — pointerイベントでのジェスチャー検知・ドラッグ追従・確定/スプリングバックアニメーション・2ペイン描画を追加。

---

### Task 1: 次/前のタブ・カラムを解決する純粋関数

**Files:**
- Create: `frontend/src/lib/swipeNav.ts`
- Test: `frontend/src/lib/swipeNav.test.ts`

**Interfaces:**
- Consumes: なし(このタスクは他タスクに依存しない独立モジュール)。`PaneNode`/`PaneChild`型は`frontend/src/bindings/tauri.gen.ts`からimportする(生成済み、変更不要)。
- Produces:
  - `export type SwipeDirection = "prev" | "next";`
  - `export interface SwipeGroup { id: string; tabs: { id: string }[]; activeTabId: string; }`
  - `export type SwipeTarget = { kind: "tab"; groupId: string; tabId: string } | { kind: "group"; groupId: string } | null;`
  - `export function topLevelLeafGroupIds(paneRoot: PaneNode): string[]`
  - `export function resolveSwipeTarget(groups: SwipeGroup[], paneRoot: PaneNode, groupId: string, direction: SwipeDirection): SwipeTarget`
  - Task 3は`SwipeGroup`/`SwipeDirection`/`SwipeTarget`/`resolveSwipeTarget`を、Task 4は`SwipeTarget`/`resolveSwipeTarget`を使う。`SwipeGroup`は`store.svelte.ts`の`GroupView`との循環import回避のためのミニマルな型(`GroupView`は構造的にこれを満たすのでそのまま渡せる)。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/swipeNav.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { PaneNode } from "../bindings/tauri.gen";
import { resolveSwipeTarget, topLevelLeafGroupIds, type SwipeGroup } from "./swipeNav";

function leaf(id: string, groupId: string): PaneNode {
  return { type: "leaf", id, groupId };
}

function row(id: string, children: PaneNode[]): PaneNode {
  return { type: "split", id, direction: "row", children: children.map((node) => ({ node, size: null, auto: true })) };
}

function group(id: string, tabIds: string[], activeTabId: string): SwipeGroup {
  return { id, tabs: tabIds.map((tid) => ({ id: tid })), activeTabId };
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

describe("resolveSwipeTarget", () => {
  it("次のタブがあればタブ送りを返す", () => {
    const groups = [group("g1", ["t1", "t2"], "t1")];
    const root = leaf("l1", "g1");
    expect(resolveSwipeTarget(groups, root, "g1", "next")).toEqual({ kind: "tab", groupId: "g1", tabId: "t2" });
  });

  it("前のタブがあればタブ戻りを返す", () => {
    const groups = [group("g1", ["t1", "t2"], "t2")];
    const root = leaf("l1", "g1");
    expect(resolveSwipeTarget(groups, root, "g1", "prev")).toEqual({ kind: "tab", groupId: "g1", tabId: "t1" });
  });

  it("末尾タブでさらに次へスワイプすると次のカラムを返す", () => {
    const groups = [group("g1", ["t1", "t2"], "t2"), group("g2", ["t3"], "t3")];
    const root = row("root", [leaf("l1", "g1"), leaf("l2", "g2")]);
    expect(resolveSwipeTarget(groups, root, "g1", "next")).toEqual({ kind: "group", groupId: "g2" });
  });

  it("先頭タブでさらに前へスワイプすると前のカラムを返す", () => {
    const groups = [group("g1", ["t1"], "t1"), group("g2", ["t2"], "t2")];
    const root = row("root", [leaf("l1", "g1"), leaf("l2", "g2")]);
    expect(resolveSwipeTarget(groups, root, "g2", "prev")).toEqual({ kind: "group", groupId: "g1" });
  });

  it("タブが1つしかないカラムはスワイプが直接カラム移動になる", () => {
    const groups = [group("g1", ["t1"], "t1"), group("g2", ["t2"], "t2")];
    const root = row("root", [leaf("l1", "g1"), leaf("l2", "g2")]);
    expect(resolveSwipeTarget(groups, root, "g1", "next")).toEqual({ kind: "group", groupId: "g2" });
  });

  it("末尾カラムの末尾タブでさらに次へは移動先なし(null)", () => {
    const groups = [group("g1", ["t1"], "t1")];
    const root = leaf("l1", "g1");
    expect(resolveSwipeTarget(groups, root, "g1", "next")).toBeNull();
  });

  it("ネストしたsplit配下のカラムはカラム間移動の対象外(見つからずnull)", () => {
    const groups = [group("g1", ["t1"], "t1"), group("g2", ["t2"], "t2")];
    const nested = row("nested", [leaf("l2", "g2")]);
    const root = row("root", [leaf("l1", "g1"), nested]);
    // g2はtopLevelLeafGroupIdsに含まれないため、g1からの次カラム移動先は無い
    expect(resolveSwipeTarget(groups, root, "g1", "next")).toBeNull();
  });

  it("groupが空(タブなし)ならnull", () => {
    const groups = [group("g1", [], "")];
    const root = leaf("l1", "g1");
    expect(resolveSwipeTarget(groups, root, "g1", "next")).toBeNull();
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm exec vitest run src/lib/swipeNav.test.ts`
Expected: FAIL(`./swipeNav`が存在せずimportエラー)

- [ ] **Step 3: 最小実装を書く**

`frontend/src/lib/swipeNav.ts`:

```ts
// モバイル版の左右スワイプで「次/前のタブ・カラム」を解決する純粋関数(Issue #296)。
// store.svelte.tsとの循環importを避けるため、GroupViewそのものではなく最小限の
// 構造(SwipeGroup)を受け取る。GroupViewは構造的にこれを満たすのでそのまま渡せる。
import type { PaneNode } from "../bindings/tauri.gen";

export type SwipeDirection = "prev" | "next";

export interface SwipeGroup {
  id: string;
  tabs: { id: string }[];
  activeTabId: string;
}

export type SwipeTarget = { kind: "tab"; groupId: string; tabId: string } | { kind: "group"; groupId: string } | null;

/// 最上位rowの直下の子のうちtype:"leaf"のgroupIdだけを、出現順で返す。
/// ネストしたsplit配下のカラムはカラム間スワイプの対象外とする(Issue #296のスコープ決定)。
export function topLevelLeafGroupIds(paneRoot: PaneNode): string[] {
  if (paneRoot.type === "leaf") return [paneRoot.groupId];
  return paneRoot.children.filter((c) => c.node.type === "leaf").map((c) => (c.node as { groupId: string }).groupId);
}

/// group内でのタブ送りを試み、タブが1つしかない/端に達している場合は
/// 最上位rowの直下leafの並びから次/前のカラムを解決する。移動先が無ければnull。
export function resolveSwipeTarget(
  groups: SwipeGroup[],
  paneRoot: PaneNode,
  groupId: string,
  direction: SwipeDirection,
): SwipeTarget {
  const group = groups.find((g) => g.id === groupId);
  if (!group || group.tabs.length === 0) return null;

  const currentIndex = Math.max(0, group.tabs.findIndex((t) => t.id === group.activeTabId));

  if (direction === "next" && currentIndex < group.tabs.length - 1) {
    return { kind: "tab", groupId, tabId: group.tabs[currentIndex + 1].id };
  }
  if (direction === "prev" && currentIndex > 0) {
    return { kind: "tab", groupId, tabId: group.tabs[currentIndex - 1].id };
  }

  const order = topLevelLeafGroupIds(paneRoot);
  const groupIndex = order.indexOf(groupId);
  if (groupIndex < 0) return null;
  const targetIndex = direction === "next" ? groupIndex + 1 : groupIndex - 1;
  if (targetIndex < 0 || targetIndex >= order.length) return null;
  return { kind: "group", groupId: order[targetIndex] };
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && pnpm exec vitest run src/lib/swipeNav.test.ts`
Expected: PASS(全ケース)

- [ ] **Step 5: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/swipeNav.ts frontend/src/lib/swipeNav.test.ts
git commit -m "feat: スワイプでの次/前タブ・カラム解決ロジックを追加"
```

---

### Task 2: スワイプの軸判定・確定判定・ラバーバンドの純粋関数

**Files:**
- Create: `frontend/src/lib/swipeGesture.ts`
- Test: `frontend/src/lib/swipeGesture.test.ts`

**Interfaces:**
- Consumes: なし(独立モジュール)。
- Produces:
  - `export type SwipeAxis = "horizontal" | "vertical";`
  - `export function resolveSwipeAxis(dx: number, dy: number, thresholdPx?: number): SwipeAxis | null`
  - `export function shouldCommitSwipe(dx: number, containerWidth: number, velocityPxPerMs: number, distanceRatioThreshold?: number, velocityThreshold?: number): boolean`
  - `export function applyRubberBand(dx: number, resistance?: number): number`
  - Task 4はこの3関数をそのまま使う。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/swipeGesture.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { applyRubberBand, resolveSwipeAxis, shouldCommitSwipe } from "./swipeGesture";

describe("resolveSwipeAxis", () => {
  it("移動量が閾値未満ならnull(判定保留)", () => {
    expect(resolveSwipeAxis(3, 2)).toBeNull();
  });

  it("横移動が縦移動より大きければhorizontal", () => {
    expect(resolveSwipeAxis(20, 5)).toBe("horizontal");
  });

  it("縦移動が横移動より大きければvertical", () => {
    expect(resolveSwipeAxis(5, 20)).toBe("vertical");
  });
});

describe("shouldCommitSwipe", () => {
  it("移動量がコンテナ幅の35%以上なら確定", () => {
    expect(shouldCommitSwipe(-140, 300, 0)).toBe(true);
  });

  it("移動量が小さくても速度が閾値以上なら確定", () => {
    expect(shouldCommitSwipe(-20, 300, 0.8)).toBe(true);
  });

  it("移動量も速度も足りなければ確定しない", () => {
    expect(shouldCommitSwipe(-20, 300, 0.1)).toBe(false);
  });

  it("コンテナ幅が0以下なら確定しない", () => {
    expect(shouldCommitSwipe(-140, 0, 0)).toBe(false);
  });
});

describe("applyRubberBand", () => {
  it("移動量を抵抗係数分だけ減衰させる", () => {
    expect(applyRubberBand(100, 0.35)).toBeCloseTo(35);
    expect(applyRubberBand(-100, 0.35)).toBeCloseTo(-35);
  });

  it("デフォルトの抵抗係数(0.35)を使う", () => {
    expect(applyRubberBand(100)).toBeCloseTo(35);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm exec vitest run src/lib/swipeGesture.test.ts`
Expected: FAIL(`./swipeGesture`が存在せずimportエラー)

- [ ] **Step 3: 最小実装を書く**

`frontend/src/lib/swipeGesture.ts`:

```ts
// モバイル版の左右スワイプのジェスチャー判定に使う純粋関数(Issue #296)。
// ノート一覧の縦スクロールと衝突しないよう、横/縦の軸判定と、ドラッグ終了時に
// 確定させるかどうかの判定、移動先が無い端でのラバーバンド量の計算を切り出す。
export type SwipeAxis = "horizontal" | "vertical";

const AXIS_LOCK_THRESHOLD_PX = 10;
const COMMIT_DISTANCE_RATIO = 0.35;
const COMMIT_VELOCITY_PX_MS = 0.5;
const RUBBER_BAND_RESISTANCE = 0.35;

/// ポインタの累積移動量から、ジェスチャーの軸を判定する。
/// dx/dyの絶対値がどちらも閾値未満なら判定を保留してnullを返す。
export function resolveSwipeAxis(dx: number, dy: number, thresholdPx = AXIS_LOCK_THRESHOLD_PX): SwipeAxis | null {
  if (Math.abs(dx) < thresholdPx && Math.abs(dy) < thresholdPx) return null;
  return Math.abs(dx) > Math.abs(dy) ? "horizontal" : "vertical";
}

/// ドラッグ終了時、残りをアニメーションで確定させるかを判定する。
/// コンテナ幅に対する移動量の割合、またはリリース時の速度(px/ms)のいずれかが
/// 閾値以上なら確定とする。
export function shouldCommitSwipe(
  dx: number,
  containerWidth: number,
  velocityPxPerMs: number,
  distanceRatioThreshold = COMMIT_DISTANCE_RATIO,
  velocityThreshold = COMMIT_VELOCITY_PX_MS,
): boolean {
  if (containerWidth <= 0) return false;
  const ratio = Math.abs(dx) / containerWidth;
  return ratio >= distanceRatioThreshold || Math.abs(velocityPxPerMs) >= velocityThreshold;
}

/// 移動先が無い端でのドラッグ量を減衰させ、ゴムのような抵抗感(rubber-band)を出す。
export function applyRubberBand(dx: number, resistance = RUBBER_BAND_RESISTANCE): number {
  return dx * resistance;
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && pnpm exec vitest run src/lib/swipeGesture.test.ts`
Expected: PASS(全ケース)

- [ ] **Step 5: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/swipeGesture.ts frontend/src/lib/swipeGesture.test.ts
git commit -m "feat: スワイプの軸判定・確定判定・ラバーバンド計算を追加"
```

---

### Task 3: AppStoreへのスワイプ確定メソッド追加

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`
- Test: `frontend/src/lib/store.svelte.swipe.test.ts`

**Interfaces:**
- Consumes: Task 1の`resolveSwipeTarget`、`SwipeDirection`、`SwipeTarget`(`./swipeNav`からimport)。
- Produces: `AppStore.applySwipe(groupId: string, direction: SwipeDirection): SwipeTarget` — Task 4がドラッグ確定時に呼ぶ。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/store.svelte.swipe.test.ts`(既存`store.svelte.haptics.test.ts`と同じモック構成に倣う):

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

function makeGroup(id: string, tabIds: string[], activeTabId: string) {
  return {
    id,
    width: 320,
    auto: false,
    tabs: tabIds.map((tid) => ({ id: tid }) as never),
    activeTabId,
  };
}

beforeEach(() => {
  invokeMock.mockClear();
  app.groups = [];
  app.paneRoot = { type: "split", id: "root", direction: "row", children: [] };
  app.focusedGroupId = null;
});

describe("applySwipe(Issue #296)", () => {
  it("次のタブがあればactiveTabIdをタブ送りする", () => {
    app.groups = [makeGroup("g1", ["t1", "t2"], "t1")];
    app.paneRoot = { type: "leaf", id: "l1", groupId: "g1" };

    const target = app.applySwipe("g1", "next");

    expect(target).toEqual({ kind: "tab", groupId: "g1", tabId: "t2" });
    expect(app.groups[0].activeTabId).toBe("t2");
  });

  it("末尾タブでさらに次へスワイプすると次のカラムへfocusedGroupIdを移す", () => {
    app.groups = [makeGroup("g1", ["t1"], "t1"), makeGroup("g2", ["t2"], "t2")];
    app.paneRoot = {
      type: "split",
      id: "root",
      direction: "row",
      children: [
        { node: { type: "leaf", id: "l1", groupId: "g1" }, size: null, auto: true },
        { node: { type: "leaf", id: "l2", groupId: "g2" }, size: null, auto: true },
      ],
    };

    const target = app.applySwipe("g1", "next");

    expect(target).toEqual({ kind: "group", groupId: "g2" });
    expect(app.focusedGroupId).toBe("g2");
    // カラム移動ではタブは変えない
    expect(app.groups[1].activeTabId).toBe("t2");
  });

  it("移動先が無ければ何も変更せずnullを返す", () => {
    app.groups = [makeGroup("g1", ["t1"], "t1")];
    app.paneRoot = { type: "leaf", id: "l1", groupId: "g1" };
    app.focusedGroupId = null;

    const target = app.applySwipe("g1", "next");

    expect(target).toBeNull();
    expect(app.groups[0].activeTabId).toBe("t1");
    expect(app.focusedGroupId).toBeNull();
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm exec vitest run src/lib/store.svelte.swipe.test.ts`
Expected: FAIL(`app.applySwipe is not a function`)

- [ ] **Step 3: 最小実装を書く**

`frontend/src/lib/store.svelte.ts`の先頭付近、既存の`import { isMobilePlatform } from "./platform";`の直後に追加:

```ts
import { resolveSwipeTarget, type SwipeDirection, type SwipeTarget } from "./swipeNav";
```

`AppStore`クラス内、`setActiveTab`メソッド(`frontend/src/lib/store.svelte.ts:456`付近)の直後に追加:

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

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && pnpm exec vitest run src/lib/store.svelte.swipe.test.ts`
Expected: PASS(全ケース)

- [ ] **Step 5: 型チェックと既存テストの回帰確認**

Run: `cd frontend && pnpm check && pnpm test`
Expected: 両方PASS(既存テストに影響なし)

- [ ] **Step 6: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.swipe.test.ts
git commit -m "feat: AppStoreにスワイプ確定メソッドapplySwipeを追加"
```

---

### Task 4: Column.svelteへのジェスチャー・アニメーション統合

**Files:**
- Modify: `frontend/src/ui/Column.svelte`

**Interfaces:**
- Consumes: Task 1の`resolveSwipeTarget`/`SwipeTarget`(`../lib/swipeNav`)、Task 2の`resolveSwipeAxis`/`shouldCommitSwipe`/`applyRubberBand`/`SwipeAxis`(`../lib/swipeGesture`)、Task 3の`app.applySwipe`。ジェスチャー無効化の判定に既存の`app.showComposeModal`/`app.errorModal`/`app.reactPicker`/`app.draggingTabId`/`app.draggingGroupId`(`store.svelte.ts`)とColumn.svelte内のローカル状態`menuOpen`も読む。
- Produces: なし(末端のUI統合。他タスクはこれに依存しない)。

このタスクはポインタイベント統合とCSSアニメーションが中心で、jsdom上のユニットテストでは実際の挙動(指の追従・慣性・実機でのタッチ挙動)を検証できない。そのため純粋ロジックはTask 1/2で既にテスト済みという前提で、ここでは型チェックと既存テストの回帰確認のみ行う。実機(Android)でのスワイプ操作の手動確認は、この一連のタスク完了後にユーザー側で行う。

- [ ] **Step 1: importとスワイプ用の状態を追加する**

`frontend/src/ui/Column.svelte`の`<script>`冒頭、既存のimportに追記(`import { edgeFromPointer } from "../lib/paneEdge";`の直後):

```ts
  import { resolveSwipeTarget, type SwipeTarget } from "../lib/swipeNav";
  import { applyRubberBand, resolveSwipeAxis, shouldCommitSwipe, type SwipeAxis } from "../lib/swipeGesture";
```

`onScroll`関数の直後に追加:

```ts
  // モバイル版: カラム本体の左右スワイプでタブ/カラムを移動する(Issue #296)。
  const SETTLE_MS = 180;

  type SwipeDrag = {
    pointerId: number;
    axis: SwipeAxis | null;
    startX: number;
    startY: number;
    startTime: number;
    dx: number;
    target: SwipeTarget;
  };
  let drag = $state<SwipeDrag | null>(null);
  let settling = $state(false);
  let contentEl = $state<HTMLElement | null>(null);

  /// スワイプ移動先(target)が表示すべきタブを返す。タブ送りならそのタブ、
  /// カラム移動なら移動先カラムの現在のアクティブタブ。
  function peekTab(target: SwipeTarget): TabView | null {
    if (!target) return null;
    const g = app.groups.find((x) => x.id === target.groupId);
    if (!g) return null;
    if (target.kind === "tab") return g.tabs.find((t) => t.id === target.tabId) ?? null;
    return g.tabs.find((t) => t.id === g.activeTabId) ?? g.tabs[0] ?? null;
  }

  function onSwipeDown(e: PointerEvent) {
    // 他のオーバーレイ(カラムメニュー/投稿モーダル/エラーモーダル/リアクションピッカー)や
    // 既存のタブ・カラムのドラッグ&ドロップ操作中はスワイプジェスチャーを無効化する。
    if (
      !app.useMobileUi() ||
      e.pointerType !== "touch" ||
      drag ||
      settling ||
      menuOpen ||
      app.showComposeModal ||
      app.errorModal ||
      app.reactPicker ||
      app.draggingTabId ||
      app.draggingGroupId
    )
      return;
    drag = { pointerId: e.pointerId, axis: null, startX: e.clientX, startY: e.clientY, startTime: e.timeStamp, dx: 0, target: null };
  }

  function onSwipeMove(e: PointerEvent) {
    if (!drag || e.pointerId !== drag.pointerId) return;
    const dx = e.clientX - drag.startX;
    const dy = e.clientY - drag.startY;
    if (drag.axis === null) {
      drag.axis = resolveSwipeAxis(dx, dy);
      if (drag.axis === null) return;
    }
    if (drag.axis === "vertical") return; // ネイティブの縦スクロールに任せる
    e.preventDefault();
    const direction = dx < 0 ? "next" : "prev";
    drag.target = resolveSwipeTarget(app.groups, app.paneRoot, group.id, direction);
    drag.dx = drag.target ? dx : applyRubberBand(dx);
  }

  function settle(commitDirection: "next" | "prev" | null) {
    if (!drag) return;
    settling = true;
    drag.dx = commitDirection === null ? 0 : commitDirection === "next" ? -(contentEl?.clientWidth ?? drag.dx) : (contentEl?.clientWidth ?? -drag.dx);
    setTimeout(() => {
      if (commitDirection) app.applySwipe(group.id, commitDirection);
      drag = null;
      settling = false;
    }, SETTLE_MS);
  }

  function onSwipeUp(e: PointerEvent) {
    if (!drag || e.pointerId !== drag.pointerId) return;
    if (drag.axis !== "horizontal") {
      drag = null;
      return;
    }
    const elapsed = Math.max(1, e.timeStamp - drag.startTime);
    const velocity = drag.dx / elapsed;
    const width = contentEl?.clientWidth ?? 0;
    const willCommit = drag.target !== null && shouldCommitSwipe(drag.dx, width, velocity);
    settle(willCommit ? (drag.dx < 0 ? "next" : "prev") : null);
  }

  function onSwipeCancel(e: PointerEvent) {
    if (!drag || e.pointerId !== drag.pointerId) return;
    settle(null);
  }
```

- [ ] **Step 2: `TabView`のimportを確認する**

ファイル冒頭のimportは既に`import type { GroupView, TabView } from "../lib/store.svelte";`になっている(`Column.svelte:2`)ので変更不要。`peekTab`の戻り値型`TabView | null`はこのimportで解決できる。

- [ ] **Step 3: テンプレートを2ペイン描画+ジェスチャーハンドラ付きに変更する**

既存の(`Column.svelte:238-276`付近):

```svelte
  {#if activeTab}
    <div class="flex-1 overflow-y-auto" onscroll={onScroll}>
      {#if isNotif}
        {#each activeTab.notifications as n (n.id)}
          <NotificationCard notification={n} accountId={activeTab.accountId} />
        {/each}
        {#if activeTab.notifications.length === 0 && !activeTab.loadingMore}
          <div class="p-3.5 text-center text-sm text-muted-foreground">まだ通知がありません</div>
        {/if}
      {:else}
        {#each activeTab.notes as note (note.id)}
          <NoteCard
            {note}
            accountId={activeTab.accountId}
            tabId={activeTab.id}
            selected={note.id === activeTab.selectedNoteId}
          />
          {#if activeTab.gapMarker && note.id === activeTab.gapMarker.boundaryId}
            <div class="flex items-center gap-2 border-y border-border bg-muted/40 px-3.5 py-2 text-sm text-muted-foreground">
              <span class="flex-1">この間の投稿は省略されています</span>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={activeTab.fillingGap}
                onclick={() => app.fillRemainingGap(activeTab.id)}
              >
                {activeTab.fillingGap ? "取得中…" : "省略された投稿を表示"}
              </Button>
            </div>
          {/if}
        {/each}
        {#if activeTab.notes.length === 0 && !activeTab.loadingMore}
          <div class="p-3.5 text-center text-sm text-muted-foreground">まだノートがありません</div>
        {/if}
      {/if}
      {#if activeTab.loadingMore}<div class="p-3.5 text-center text-sm text-muted-foreground">読み込み中…</div>{/if}
    </div>
  {/if}
```

を、以下に置き換える(タブ本文の描画を`{#snippet tabBody}`に切り出し、現在のタブとスワイプ先(peek)のタブを横並びで`translateX`する外側コンテナを追加):

```svelte
  {#snippet tabBody(tab: TabView)}
    {@const notif = tab.kind.type === "notifications"}
    {#if notif}
      {#each tab.notifications as n (n.id)}
        <NotificationCard notification={n} accountId={tab.accountId} />
      {/each}
      {#if tab.notifications.length === 0 && !tab.loadingMore}
        <div class="p-3.5 text-center text-sm text-muted-foreground">まだ通知がありません</div>
      {/if}
    {:else}
      {#each tab.notes as note (note.id)}
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

  {#if activeTab}
    {@const peek = drag ? peekTab(drag.target) : null}
    <div class="relative flex-1 overflow-hidden" bind:this={contentEl}>
      <div
        class="flex h-full"
        style={[
          `transform:translateX(${drag?.dx ?? 0}px)`,
          settling ? `transition:transform ${SETTLE_MS}ms ease-out` : "",
        ].join(";")}
        onpointerdown={onSwipeDown}
        onpointermove={onSwipeMove}
        onpointerup={onSwipeUp}
        onpointercancel={onSwipeCancel}
        style:touch-action="pan-y"
      >
        <div class="h-full w-full flex-none overflow-y-auto" onscroll={onScroll}>
          {@render tabBody(activeTab)}
        </div>
        {#if peek}
          <div class="h-full w-full flex-none overflow-y-auto">
            {@render tabBody(peek)}
          </div>
        {/if}
      </div>
    </div>
  {/if}
```

`style:touch-action`はSvelteのディレクティブ構文と衝突しない位置(通常の`style`属性の後)に置く。既存の`class="flex-1 overflow-y-auto"`は外側の`relative overflow-hidden`ラッパーに置き換わるため、`onscroll`は内側の実際にスクロールするdivに移した。

- [ ] **Step 4: `prev`方向(dx > 0)のpeek配置を確認する**

`prev`方向にスワイプする場合、peek(前のタブ/カラム)は現在のコンテンツより左側に来る必要がある。上記のテンプレートは`peek`を常に`activeTab`の後(右)に描画しているため、`prev`方向のときはコンテナ自体の並び順を反転させる必要がある。`{@const peek = ...}`の下、コンテナのdivを以下のように順序を`drag`の方向で切り替える:

既存Step 3の`<div class="flex h-full" ...>`内側を、`peek`を先に置くかどうかで分岐させる:

```svelte
        {#if drag && drag.dx > 0 && peek}
          <div class="h-full w-full flex-none overflow-y-auto">
            {@render tabBody(peek)}
          </div>
        {/if}
        <div class="h-full w-full flex-none overflow-y-auto" onscroll={onScroll}>
          {@render tabBody(activeTab)}
        </div>
        {#if peek && !(drag && drag.dx > 0)}
          <div class="h-full w-full flex-none overflow-y-auto">
            {@render tabBody(peek)}
          </div>
        {/if}
```

このとき、`prev`方向でpeekを先頭に描画すると`translateX(dx)`(dx > 0)でコンテナ全体が右にずれ、先頭のpeekペインが見えてくる形になる。`next`方向(dx < 0)は元の並び(activeTab→peek)のままでよい。`settle`関数の`commitDirection === "prev"`のケースでは、`drag.dx`(既にpeekがある状態の並び)を`contentEl.clientWidth`まで動かせば、peekペインがちょうど画面いっぱいに表示された状態でアニメーションが止まる。

- [ ] **Step 5: 型チェックと既存テストの回帰確認**

Run: `cd frontend && pnpm check`
Expected: PASS(型エラーなし)

Run: `cd frontend && pnpm test`
Expected: PASS(既存のColumn関連テスト・全体スイートに影響なし)

- [ ] **Step 6: デスクトップUIに影響が無いことを確認する**

`onSwipeDown`は`app.useMobileUi()`が`false`のとき何もしない(`drag`が作られないので`onSwipeMove`/`onSwipeUp`も早期return)。`cd frontend && pnpm exec vitest run`を実行し、デスクトップ向け既存テスト(`Column.svelte`を直接テストするものがあれば)に差分が出ていないことをdiffで目視確認する。

Run: `grep -rl "Column.svelte" frontend/src/ui/*.test.ts 2>/dev/null || true`
Expected: 該当ファイルが無ければ何も出力されない(=既存の直接テストなし。回帰は`pnpm test`全体スイートと`pnpm check`でカバーする)。

- [ ] **Step 7: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/Column.svelte
git commit -m "feat: モバイル版でカラム本体の左右スワイプによるタブ/カラム移動を実装"
```

---

## 完了後の確認事項(ユーザー側)

- Android実機(または`cargo tauri android build --debug --target aarch64`でビルドしたAPK)で、実際に指でスワイプしてタブ送り・カラム移動・端でのラバーバンドの感触を確認する。
- 特に、通知タブとノートタブが混在するカラムでのpeek表示、リアクションピッカー表示中のスワイプ無効化(既存の`app.draggingTabId`/`app.draggingGroupId`とは独立した状態のため、他のオーバーレイ操作中に誤発火しないか)を重点確認する。
