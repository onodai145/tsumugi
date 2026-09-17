<script lang="ts">
  import type { GroupView, TabView } from "../lib/store.svelte";
  import { app, tabName } from "../lib/store.svelte";
  import NoteCard from "./NoteCard.svelte";
  import NotificationCard from "./NotificationCard.svelte";
  import { X, GripVertical, MoreHorizontal, Plus, SquareSplitHorizontal, SquareSplitVertical, Settings, ChevronLeft, ChevronRight } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import { portal } from "../lib/portal";
  import { edgeFromPointer } from "../lib/paneEdge";
  import { activeSlotIndex, computeTabSlots, notesForSlot } from "../lib/tabSlots";
  import { resolveSettledIndex } from "../lib/scrollSnapIndex";
  import { createLongPressDrag } from "../lib/longPressDrag";
  import { vibrate } from "../lib/ipc";
  import { isMobilePlatform } from "../lib/platform";
  import { resolveColumnDragHint, type ColumnDragDirection } from "../lib/columnDragHint";

  let {
    group,
    onAddTab,
    onEditTab,
    onEditGroup,
    onSplitDown,
    onSplitRight,
    stretch = false,
  }: {
    group: GroupView;
    onAddTab: (groupId: string) => void;
    onEditTab: (tab: TabView) => void;
    onEditGroup: (groupId: string) => void;
    onSplitDown: (groupId: string) => void;
    onSplitRight: (groupId: string) => void;
    stretch?: boolean;
  } = $props();

  const activeTab = $derived(
    group.tabs.find((t) => t.id === group.activeTabId) ?? group.tabs[0],
  );
  function onScroll(e: Event) {
    const el = e.currentTarget as HTMLElement;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 300 && activeTab) {
      app.loadMore(activeTab.id);
    }
  }

  // モバイル版: カラム内タブの横スワイプをCSS Scroll Snapで実現する(Issue #296)。
  // 前/アクティブ/次の最大3スロットを横並びに描画し、scrollend(またはscrollの
  // デバウンス)で着地したスロットを検知してsetActiveTabを呼ぶ。
  // デスクトップUIでは`computeTabSlots`を使わず常に1要素([activeTab])に固定し、
  // 既存の見た目・挙動を完全に変えない。
  const slots = $derived(
    app.useMobileUi()
      ? computeTabSlots(group.tabs, activeTab?.id ?? "")
      : activeTab
        ? [{ tab: activeTab, role: "active" as const }]
        : [],
  );
  let tabsEl = $state<HTMLElement | null>(null);
  let settleTimer: ReturnType<typeof setTimeout> | null = null;
  const supportsScrollEnd = typeof window !== "undefined" && "onscrollend" in window;

  // スロット構成(=activeTabId)が変わるたびに、スクロール位置をアニメーション無しで
  // 対応するスロットへ即座に合わせ直す。タブバーのタップによる切替でも、スワイプ確定に
  // よる切替でも、この一箇所で辻褄を合わせる(前/次の中身が入れ替わることでスロット配列の
  // 要素数・並びが変わりうるため、常にscrollLeftを引き直す必要がある)。
  // 依存はactiveIndex(数値)ではなくslots(配列参照)そのものにする。例えば4タブ中
  // 中間のタブ間の切替(B→C等)ではactiveSlotIndexの値が1のまま変わらないことがあり、
  // $derivedが同じプリミティブ値に収束すると依存側は再実行されないため、値ではなく
  // 配列参照の変化を捕まえる必要がある。
  $effect(() => {
    const s = slots;
    if (!tabsEl) return;
    const width = tabsEl.clientWidth;
    if (width <= 0) return;
    tabsEl.scrollLeft = Math.max(0, activeSlotIndex(s)) * width;
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

  // アンマウント後に保留中のデバウンスタイマーが発火してsetActiveTabを呼ばないよう、
  // コンポーネント破棄時にタイマーを片付ける(scrollend未対応環境のフォールバック経路向け)。
  $effect(() => {
    return () => {
      if (settleTimer) clearTimeout(settleTimer);
    };
  });

  // 幅リサイズ
  let resizing = false;
  let startX = 0;
  let startW = 0;
  function onResizeDown(e: PointerEvent) {
    resizing = true;
    startX = e.clientX;
    startW = group.width;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onResizeMove(e: PointerEvent) {
    if (!resizing) return;
    const w = Math.min(720, Math.max(220, startW + (e.clientX - startX)));
    app.setGroupWidthLocal(group.id, w);
  }
  function onResizeUp() {
    if (!resizing) return;
    resizing = false;
    app.persistGroupWidth(group.id, group.width);
  }

  // カラムヘッダーメニュー（タブ追加／下に分割／カラム設定を1つの「⋯」に集約）
  let menuOpen = $state(false);
  let menuTrigger = $state<HTMLElement | null>(null);
  let menuPos = $state<{ left: number; top: number } | null>(null);

  function toggleMenu() {
    if (menuOpen) {
      menuOpen = false;
      return;
    }
    const r = menuTrigger?.getBoundingClientRect();
    const MENU_WIDTH = 160;
    const MENU_MARGIN = 8;
    if (r)
      menuPos = {
        left: Math.max(0, Math.min(r.left, window.innerWidth - MENU_WIDTH - MENU_MARGIN)),
        top: r.bottom + 4,
      };
    menuOpen = true;
  }

  function pickMenuItem(action: () => void) {
    menuOpen = false;
    action();
  }

  // 長押しドラッグ中(タブ/カラムグリップ共通)、指がタブバーの外(ノート本文など、
  // `select-text`で意図的に選択可能にしているテキスト)へ僅かにずれただけで、OS標準の
  // 「長押しでテキスト選択」がドラッグと同時に発火してしまうことが実機で確認された
  // (Issue #354)。ドラッグ確定(armed)からドラッグ終了までの間はページ全体を選択不可にし、
  // 終了時に元の状態へ戻す(sortablejs等のドラッグ実装で広く使われる標準的な回避策)。
  let restoreUserSelect: (() => void) | null = null;
  function suppressTextSelectionDuringDrag() {
    if (restoreUserSelect) return; // 既に抑制中なら多重に上書きしない
    const html = document.documentElement;
    const prevUserSelect = html.style.userSelect;
    const prevWebkitUserSelect = html.style.getPropertyValue("-webkit-user-select");
    html.style.userSelect = "none";
    html.style.setProperty("-webkit-user-select", "none");
    restoreUserSelect = () => {
      html.style.userSelect = prevUserSelect;
      html.style.setProperty("-webkit-user-select", prevWebkitUserSelect || "");
      restoreUserSelect = null;
    };
  }
  function restoreTextSelection() {
    restoreUserSelect?.();
  }

  // タッチ長押しでのタブ並び替え(Issue #354)。native drag-and-dropはタッチでは
  // dragstartが発火しないため、長押し(400ms)が成立したら同じapp.startDragTab等を
  // 呼び出す形でモバイル版に対応する。マウス操作(pointerType!=="touch")では何もせず、
  // 既存のdraggable属性によるnative DnDに委ねる。
  let touchDraggingTabId = $state<string | null>(null);
  let touchDragTabPendingId: string | null = null;
  let touchDragStartX = 0;
  let touchDragDeltaX = $state(0);
  let touchDragPendingEl: HTMLElement | null = null;
  let touchDragPendingPointerId: number | null = null;

  const tabDrag = createLongPressDrag({
    onArmed: () => {
      const tabId = touchDragTabPendingId;
      if (!tabId) return;
      touchDraggingTabId = tabId;
      touchDragDeltaX = 0;
      // ポインターキャプチャはここ(長押し成立後)で初めて行う。pointerdown時点で
      // 即座にキャプチャすると、キャプチャ要素へのclickリターゲティング(Pointer Events
      // のcompatibility mapping仕様)により、キャプチャ対象の外側divより内側にある
      // タブ切替ボタンのonclickが、長押しに至らない通常タップでも届かなくなる恐れがある。
      if (touchDragPendingEl && touchDragPendingPointerId !== null) {
        touchDragPendingEl.setPointerCapture(touchDragPendingPointerId);
      }
      if (isMobilePlatform && (app.ui.hapticsEnabled ?? true)) vibrate("light");
      suppressTextSelectionDuringDrag();
      app.startDragTab(tabId);
    },
  });

  function onTabPointerDown(e: PointerEvent, tabId: string) {
    if (e.pointerType !== "touch" || !app.useMobileUi()) return;
    touchDragTabPendingId = tabId;
    touchDragStartX = e.clientX;
    // ポインターキャプチャの対象は、ドラッグ中のタブ自身(e.currentTarget)ではなく、
    // 常に位置が変わらないタブバーコンテナ(data-tabbar-group-id)にする。ドラッグ中に
    // app.dragOverTab/dragOverTabBarEndでgroup.tabsが並び替わると、Svelteのkeyed each
    // ブロックがドラッグ中タブのDOM要素自体をinsertBeforeで物理的に移動させる。
    // アクティブなポインターキャプチャを持つ要素がこうしてDOM内で移動すると、一部の
    // Android WebViewではその後pointerup/pointercancelが一切届かなくなり、ドラッグが
    // 完了しないまま固着することが実機で確認された(Issue #354)。タブバーコンテナ自身は
    // 子要素の並びが変わっても自身の位置は動かないため、キャプチャ先をこちらにすることで
    // イベント配信を安定させる。
    touchDragPendingEl = (e.currentTarget as HTMLElement).closest<HTMLElement>("[data-tabbar-group-id]");
    touchDragPendingPointerId = e.pointerId;
    tabDrag.onPointerDown(e.clientX, e.clientY);
  }

  /// ドラッグ中の指の位置から並び替え対象を解決する。タブの上ならそのタブ、タブの無い
  /// 「タブバーの空き部分」なら末尾送り(tabId:null)。カラム内でもタブバーの外
  /// (ノート一覧など)はドロップ対象外なのでnullを返す。data-group-idはカラムの
  /// <section>全体に付いているため、空き部分の判定にはタブバー自身の
  /// data-tabbar-group-id を使う(デスクトップのnative DnDと同じ範囲に揃える)。
  function resolveTabHit(clientX: number, clientY: number): { groupId: string; tabId: string | null } | null {
    const el = document.elementFromPoint(clientX, clientY) as HTMLElement | null;
    if (!el) return null;
    const tabEl = el.closest<HTMLElement>("[data-tab-id]");
    if (tabEl) {
      const groupEl = tabEl.closest<HTMLElement>("[data-group-id]");
      if (!groupEl) return null;
      return { groupId: groupEl.dataset.groupId!, tabId: tabEl.dataset.tabId! };
    }
    const tabBarEl = el.closest<HTMLElement>("[data-tabbar-group-id]");
    if (!tabBarEl) return null;
    return { groupId: tabBarEl.dataset.tabbarGroupId!, tabId: null };
  }

  /// 現在のタブ並び(全グループ分)のスナップショット。dragOverTab/dragOverTabBarEndが
  /// 実際に並びを変えたかどうかを判定するために使う(どちらも「自分自身の上」「既に末尾」
  /// といった条件で何もせず返るため、解決先のIDの変化だけでは入れ替え有無を判定できない)。
  function tabOrderSnapshot(): string {
    return app.groups.map((g) => `${g.id}:${g.tabs.map((t) => t.id).join(",")}`).join("|");
  }

  function onTabPointerMove(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    tabDrag.onPointerMove(e.clientX, e.clientY);
    if (!tabDrag.armed) return;
    e.preventDefault();
    touchDragDeltaX = e.clientX - touchDragStartX;
    const hit = resolveTabHit(e.clientX, e.clientY);
    if (!hit) return;
    const before = tabOrderSnapshot();
    if (hit.tabId) app.dragOverTab(hit.groupId, hit.tabId);
    else app.dragOverTabBarEnd(hit.groupId);
    // 入れ替えが実際に起きた場合、ドラッグ中タブのレイアウト上の位置自体が動くため、
    // translateXの基準を今の指の位置へ取り直す。取り直さないと、元のpointerdown位置から
    // の差分が新しいレイアウト位置に上乗せされ、入れ替えのたびに指より1タブ分ずつ
    // 先走って見える。
    if (tabOrderSnapshot() !== before) {
      touchDragStartX = e.clientX;
      touchDragDeltaX = 0;
    }
  }

  function endTabTouchDrag(wasArmed: boolean) {
    touchDraggingTabId = null;
    touchDragTabPendingId = null;
    touchDragDeltaX = 0;
    touchDragPendingEl = null;
    touchDragPendingPointerId = null;
    restoreTextSelection();
    if (wasArmed) void app.endDragTab();
  }

  function onTabPointerUp(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    const wasArmed = tabDrag.armed;
    tabDrag.onPointerUp();
    endTabTouchDrag(wasArmed);
  }

  function onTabPointerCancel(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    const wasArmed = tabDrag.armed;
    tabDrag.onPointerCancel();
    endTabTouchDrag(wasArmed);
  }

  // タッチ長押しでのカラム並び替え(Issue #354)。モバイル版は1カラムが画面全幅表示のため、
  // 隣のカラムが画面外にあり位置に追従する自由なドラッグは分かりにくい。そのため
  // 「前へ/次へ」の1ステップ移動として実装する(resolveColumnDragHintのトグル式判定)。
  let columnDragHint = $state<ColumnDragDirection | null>(null);
  // 長押しが成立した時点でtrue。「前へ/次へ」のヒントは、まだどちらへも動かしていない
  // (columnDragHint === null)段階から両方を非活性表示で出しておくため、オーバーレイの
  // 表示可否はcolumnDragHintではなくこちらで判定する。
  let columnDragArmed = $state(false);
  let columnDragStartX = 0;
  let gripPendingEl: HTMLElement | null = null;
  let gripPendingPointerId: number | null = null;

  const columnDrag = createLongPressDrag({
    onArmed: () => {
      columnDragHint = null;
      columnDragArmed = true;
      // タブ側と同じく、ポインターキャプチャは長押し成立後に行う(理由はonTabPointerDown
      // 付近のコメント参照)。
      if (gripPendingEl && gripPendingPointerId !== null) {
        gripPendingEl.setPointerCapture(gripPendingPointerId);
      }
      if (isMobilePlatform && (app.ui.hapticsEnabled ?? true)) vibrate("light");
      suppressTextSelectionDuringDrag();
    },
  });

  function onGripPointerDown(e: PointerEvent) {
    if (e.pointerType !== "touch" || !app.useMobileUi()) return;
    columnDragStartX = e.clientX;
    gripPendingEl = e.currentTarget as HTMLElement;
    gripPendingPointerId = e.pointerId;
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

  function endGripTouchDrag(wasArmed: boolean) {
    const hint = columnDragHint;
    columnDragHint = null;
    columnDragArmed = false;
    gripPendingEl = null;
    gripPendingPointerId = null;
    restoreTextSelection();
    if (wasArmed && hint) void app.moveColumnAdjacent(group.id, hint);
  }

  function onGripPointerUp(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    const wasArmed = columnDrag.armed;
    columnDrag.onPointerUp();
    endGripTouchDrag(wasArmed);
  }

  function onGripPointerCancel(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    columnDrag.onPointerCancel();
    endGripTouchDrag(false);
  }
</script>

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
    if (!app.draggingGroupId) return;
    e.preventDefault();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const edge = edgeFromPointer(e.clientX - rect.left, e.clientY - rect.top, rect.width, rect.height);
    app.dragOverPaneEdge(group.id, edge);
  }}
  ondragleave={(e) => {
    if (app.dragOverEdgeTarget?.groupId === group.id) app.dragOverPaneEdge(group.id, null);
  }}
  role="group"
>
  <!-- メニューボタンはタブ数に関係なく常にカラム右端に固定表示したいため、グリップ＋タブの
       横スクロール領域(内側のoverflow-x-auto)と分離し、外側のflex行にflex-noneで置く。 -->
  <div class="tabbar-bg flex min-h-[26px] items-stretch border-b border-border border-t-2">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="flex min-w-0 flex-1 items-stretch gap-px overflow-x-auto"
      data-tabbar-group-id={group.id}
      ondragover={(e) => {
        if (app.draggingTabId) {
          e.preventDefault();
          app.dragOverTabBarEnd(group.id);
        }
      }}
      onpointermove={onTabPointerMove}
      onpointerup={onTabPointerUp}
      onpointercancel={onTabPointerCancel}
    >
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <span
        class="flex w-[26px] flex-none cursor-grab select-none items-center justify-center text-muted-foreground active:cursor-grabbing [touch-action:none] [-webkit-touch-callout:none]"
        draggable={!app.useMobileUi()}
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

      {#each group.tabs as t (t.id)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class={[
            "flex cursor-grab items-center active:cursor-grabbing select-none [touch-action:none] [-webkit-touch-callout:none]",
            {
              "shadow-[inset_0_-2px_0_var(--color-primary)]": t.id === group.activeTabId,
              "relative z-20 scale-105 shadow-[0_8px_24px_rgba(0,0,0,0.25)] pointer-events-none": touchDraggingTabId === t.id,
            },
            app.draggingTabId === t.id ? "opacity-40" : t.id !== group.activeTabId ? "opacity-65" : "",
          ]}
          style:transform={touchDraggingTabId === t.id ? `translateX(${touchDragDeltaX}px)` : undefined}
          data-tab-id={t.id}
          draggable={!app.useMobileUi()}
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
            data-testid="column-tab-name"
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
    </div>

    <Button
      variant="ghost"
      size="icon-xs"
      class="flex-none text-muted-foreground"
      title="メニュー"
      onclick={toggleMenu}
      bind:ref={menuTrigger}
    ><MoreHorizontal size={16} /></Button>
  </div>

  {#if menuOpen && menuPos}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (menuOpen = false)} role="presentation">
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="fixed w-[160px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
        style={`left:${menuPos.left}px;top:${menuPos.top}px`}
        onclick={(e) => e.stopPropagation()}
        role="menu"
        tabindex="-1"
      >
        <button
          type="button"
          role="menuitem"
          class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted"
          onclick={() => pickMenuItem(() => onAddTab(group.id))}
        >
          <Plus size={16} /> タブを追加
        </button>
        <button
          type="button"
          role="menuitem"
          class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted"
          onclick={() => pickMenuItem(() => onSplitRight(group.id))}
        >
          <SquareSplitHorizontal size={16} /> 右に分割
        </button>
        <button
          type="button"
          role="menuitem"
          class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted"
          onclick={() => pickMenuItem(() => onSplitDown(group.id))}
        >
          <SquareSplitVertical size={16} /> 下に分割
        </button>
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
        <button
          type="button"
          role="menuitem"
          class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted"
          onclick={() => pickMenuItem(() => onEditGroup(group.id))}
        >
          <Settings size={16} /> カラム設定
        </button>
      </div>
    </div>
  {/if}

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
        <NoteCard {note} accountId={tab.accountId} tabId={tab.id} selected={role === "active" && note.id === tab.selectedNoteId} />
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
    <div
      class="flex min-h-0 flex-1 [overflow-x:auto] [overscroll-behavior-x:auto]"
      class:mobile-scroll-snap={app.useMobileUi()}
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

  {#if !stretch && !group.auto}
    <div
      class="absolute right-[-3px] top-0 h-full w-1.5 cursor-col-resize hover:bg-[color-mix(in_srgb,var(--color-primary)_40%,transparent)]"
      style="z-index:5"
      onpointerdown={onResizeDown}
      onpointermove={onResizeMove}
      onpointerup={onResizeUp}
      role="separator"
      aria-label="幅を変更"
    ></div>
  {/if}

  {#if app.draggingGroupId && app.draggingGroupId !== group.id && app.dragOverEdgeTarget?.groupId === group.id}
    {@const edge = app.dragOverEdgeTarget.edge}
    <div
      class="pointer-events-none absolute bg-[color-mix(in_srgb,var(--color-primary)_35%,transparent)]"
      style:left={edge === "right" ? "auto" : "0"}
      style:right={edge === "left" ? "auto" : "0"}
      style:top={edge === "bottom" ? "auto" : "0"}
      style:bottom={edge === "top" ? "auto" : "0"}
      style:width={edge === "left" || edge === "right" ? "35%" : "auto"}
      style:height={edge === "top" || edge === "bottom" ? "35%" : "auto"}
      style="z-index:6"
    ></div>
  {/if}

  {#if columnDragArmed}
    <div class="pointer-events-none fixed inset-x-0 top-[max(4px,env(safe-area-inset-top))] z-30 flex justify-center" use:portal>
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
</section>

<style>
  /* 背景画像設定時にカラムを透けさせるための不透明度(--column-opacity)。Tailwindに
     color-mix()のユーティリティが無いことに加えて、Svelteのコンポーネントスコープ
     CSSはunlayeredで注入される(Tailwindのutilitiesレイヤーより優先度が高い)ため、
     この2つの背景は<style>に残す必要がある。逆に言うと、.col-bg/.tabbar-bgが付いた
     要素に後からbg-*系のTailwindクラスを足しても無効化される点に注意。 */
  .col-bg {
    background: color-mix(in srgb, var(--surface-1) var(--column-opacity, 100%), transparent);
  }
  .tabbar-bg {
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
    border-top-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  /* キーボードフォーカス中のカラムは上端をはっきり表示。.column-root/.focusedは同一
     コンポーネントテンプレート内の要素なのでSvelteのスコープ付きCSSがそのまま効く
     (:global()不要)。*/
  .column-root.focused .tabbar-bg {
    border-top-color: var(--accent);
  }
</style>
