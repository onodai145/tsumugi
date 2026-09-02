<script lang="ts">
  import type { PaneChild, PaneNode } from "../bindings/tauri.gen";
  import type { TabView } from "../lib/store.svelte";
  import { app } from "../lib/store.svelte";
  import Column from "./Column.svelte";
  import Pane from "./Pane.svelte";

  let {
    node,
    onAddTab,
    onEditTab,
    onEditGroup,
    onSplitDown,
    onSplitRight,
    stretch = false,
  }: {
    node: PaneNode;
    onAddTab: (groupId: string) => void;
    onEditTab: (tab: TabView) => void;
    onEditGroup: (groupId: string) => void;
    onSplitDown: (groupId: string) => void;
    onSplitRight: (groupId: string) => void;
    stretch?: boolean;
  } = $props();

  // Row内のネストしたSplit子の幅(px)ドラッグリサイズ。Leaf子はColumn.svelte自身の
  // ハンドル(group.width)を使うのでここでは扱わない。
  let rowResizing = $state<{ nodeId: string; startX: number; startW: number } | null>(null);

  function onRowSplitResizeDown(e: PointerEvent, child: PaneChild) {
    rowResizing = { nodeId: child.node.id, startX: e.clientX, startW: child.size ?? 300 };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onRowSplitResizeMove(e: PointerEvent, child: PaneChild) {
    if (!rowResizing || rowResizing.nodeId !== child.node.id) return;
    child.size = Math.min(720, Math.max(220, rowResizing.startW + (e.clientX - rowResizing.startX)));
  }
  function onRowSplitResizeUp(child: PaneChild) {
    if (!rowResizing || rowResizing.nodeId !== child.node.id) return;
    rowResizing = null;
    app.resizePane(child.node.id, child.size ?? 300);
  }

  // Column分割の境界(children[i]とchildren[i+1]の間)のペアドラッグリサイズ。
  // 2子の合計ウェイトを保ったまま、ポインタのY移動量をpx→ウェイトに変換して増減させる。
  const MIN_COLUMN_PANE_PX = 60;
  let colResizing = $state<{
    a: PaneChild;
    b: PaneChild;
    startY: number;
    startHeightA: number;
    startHeightB: number;
    startSizeA: number;
    startSizeB: number;
  } | null>(null);

  function onColSplitResizeDown(e: PointerEvent, a: PaneChild, b: PaneChild) {
    const boundary = e.currentTarget as HTMLElement;
    const elA = boundary.previousElementSibling as HTMLElement | null;
    const elB = boundary.nextElementSibling as HTMLElement | null;
    if (!elA || !elB) return;
    colResizing = {
      a,
      b,
      startY: e.clientY,
      startHeightA: elA.getBoundingClientRect().height,
      startHeightB: elB.getBoundingClientRect().height,
      startSizeA: a.size ?? 50,
      startSizeB: b.size ?? 50,
    };
    boundary.setPointerCapture(e.pointerId);
  }
  function onColSplitResizeMove(e: PointerEvent) {
    if (!colResizing) return;
    const { a, b, startY, startHeightA, startHeightB, startSizeA, startSizeB } = colResizing;
    const totalPx = startHeightA + startHeightB;
    const totalWeight = startSizeA + startSizeB;
    if (totalPx <= 0 || totalWeight <= 0) return;
    const deltaY = e.clientY - startY;
    const newHeightA = Math.min(totalPx - MIN_COLUMN_PANE_PX, Math.max(MIN_COLUMN_PANE_PX, startHeightA + deltaY));
    const weightPerPx = totalWeight / totalPx;
    a.size = newHeightA * weightPerPx;
    b.size = totalWeight - a.size;
  }
  function onColSplitResizeUp() {
    if (!colResizing) return;
    const { a, b } = colResizing;
    colResizing = null;
    app.resizePane(a.node.id, a.size ?? 50);
    app.resizePane(b.node.id, b.size ?? 50);
  }
</script>

{#if node.type === "leaf"}
  {@const group = app.groups.find((g) => g.id === node.groupId)}
  {#if group}
    <Column {group} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} {onSplitRight} {stretch} />
  {/if}
{:else if node.direction === "row"}
  <!-- flex-auto(flex:1 1 auto)は親(.columns、flex-direction:row)のflex子として残り幅
       いっぱいに広がるために必須。flex-1(flex:1 1 0)だとコンテンツ幅にshrink-to-fitし、
       内部のauto幅Columnがビューポート幅ではなく縮んだ幅を基準に均等割りしようとして破綻する
       (ウィンドウ幅を変えるたびbroken widthが変わって見えるのはこれが原因)。 -->
  <div class="flex flex-auto min-w-0 h-full overflow-x-auto">
    {#each node.children as child (child.node.id)}
      {#if child.node.type === "leaf"}
        <!-- Leafの幅は今まで通りColumn.svelte側(ColumnGroup.width/auto)が決める。
             child.size/autoは(このSliceでは)Leafの実際の幅とは同期していないため、
             ここでラップして使うと二重管理・食い違いの原因になる。 -->
        <Pane node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} {onSplitRight} />
      {:else}
        <!-- ネストしたSplit(例: 下に分割された塊)にはColumn.svelteに相当する幅指定元が
             無いため、PaneChild.size/autoをそのままflex指定に使う。 -->
        <div
          class="relative flex flex-col h-full min-h-0 min-w-0"
          style={child.auto ? "flex:1 1 0;min-width:220px" : `flex:0 0 ${child.size}px`}
        >
          <Pane node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} {onSplitRight} />
          {#if !child.auto}
            <div
              class="absolute right-[-3px] top-0 h-full w-1.5 cursor-col-resize hover:bg-[color-mix(in_srgb,var(--color-primary)_40%,transparent)]"
              style="z-index:5"
              onpointerdown={(e) => onRowSplitResizeDown(e, child)}
              onpointermove={(e) => onRowSplitResizeMove(e, child)}
              onpointerup={() => onRowSplitResizeUp(child)}
              role="separator"
              aria-label="幅を変更"
            ></div>
          {/if}
        </div>
      {/if}
    {/each}
  </div>
{:else}
  <div class="flex flex-col flex-auto h-full min-h-0">
    {#each node.children as child, i (child.node.id)}
      <div class="relative flex flex-col min-h-0 min-w-0" style={child.auto ? "flex:1 1 0" : `flex:0 0 ${child.size}%`}>
        <Pane node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} {onSplitRight} stretch={true} />
      </div>
      {#if i < node.children.length - 1 && !child.auto && !node.children[i + 1].auto}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="h-1.5 flex-none cursor-row-resize hover:bg-[color-mix(in_srgb,var(--color-primary)_40%,transparent)]"
          onpointerdown={(e) => onColSplitResizeDown(e, child, node.children[i + 1])}
          onpointermove={onColSplitResizeMove}
          onpointerup={onColSplitResizeUp}
          role="separator"
          aria-label="高さを変更"
        ></div>
      {/if}
    {/each}
  </div>
{/if}
