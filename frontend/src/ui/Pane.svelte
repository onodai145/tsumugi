<script lang="ts">
  import type { PaneNode } from "../bindings/tauri.gen";
  import type { TabView } from "../lib/store.svelte";
  import { app } from "../lib/store.svelte";
  import Column from "./Column.svelte";

  let {
    node,
    onAddTab,
    onEditTab,
    onEditGroup,
    onSplitDown,
    stretch = false,
  }: {
    node: PaneNode;
    onAddTab: (groupId: string) => void;
    onEditTab: (tab: TabView) => void;
    onEditGroup: (groupId: string) => void;
    onSplitDown: (groupId: string) => void;
    stretch?: boolean;
  } = $props();
</script>

{#if node.type === "leaf"}
  {@const group = app.groups.find((g) => g.id === node.groupId)}
  {#if group}
    <Column {group} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} {stretch} />
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
        <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} />
      {:else}
        <!-- ネストしたSplit(例: 下に分割された塊)にはColumn.svelteに相当する幅指定元が
             無いため、PaneChild.size/autoをそのままflex指定に使う。 -->
        <div
          class="flex flex-col h-full min-h-0 min-w-0"
          style={child.auto ? "flex:1 1 0;min-width:220px" : `flex:0 0 ${child.size}px`}
        >
          <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} />
        </div>
      {/if}
    {/each}
  </div>
{:else}
  <div class="flex flex-col flex-auto h-full min-h-0">
    {#each node.children as child (child.node.id)}
      <div class="flex flex-col min-h-0 min-w-0" style={child.auto ? "flex:1 1 0" : `flex:0 0 ${child.size}%`}>
        <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} stretch={true} />
      </div>
    {/each}
  </div>
{/if}
