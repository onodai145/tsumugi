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
  <div class="row">
    {#each node.children as child (child.node.id)}
      {#if child.node.type === "leaf"}
        <!-- Leafの幅は今まで通りColumn.svelte側(ColumnGroup.width/auto)が決める。
             child.size/autoは(このSliceでは)Leafの実際の幅とは同期していないため、
             ここでラップして使うと二重管理・食い違いの原因になる。 -->
        <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} />
      {:else}
        <!-- ネストしたSplit(例: 下に分割された塊)にはColumn.svelteに相当する幅指定元が
             無いため、PaneChild.size/autoをそのままflex指定に使う。 -->
        <div class="row-item" style={child.auto ? "flex:1 1 0;min-width:220px" : `flex:0 0 ${child.size}px`}>
          <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} />
        </div>
      {/if}
    {/each}
  </div>
{:else}
  <div class="col">
    {#each node.children as child (child.node.id)}
      <div class="col-item" style={child.auto ? "flex:1 1 0" : `flex:0 0 ${child.size}%`}>
        <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} stretch={true} />
      </div>
    {/each}
  </div>
{/if}

<style>
  .row {
    display: flex;
    /* このdivは.columns(親、flex-direction:row)の中のflex子になりうる。flex指定が
       無いと既定(flex:0 1 auto)でコンテンツ幅にshrink-to-fitしてしまい、内部のauto
       (flex:1 1 0)なColumnがビューポート幅ではなくこの縮んだ幅を基準に均等割りしようと
       して破綻する(ウィンドウ幅を変えるたびbroken widthが変わって見えるのはこれが原因)。
       flex:1でこのdivが常に親の残り幅いっぱいに広がるようにする。 */
    flex: 1 1 auto;
    min-width: 0;
    height: 100%;
    overflow-x: auto;
  }
  .col {
    display: flex;
    flex-direction: column;
    /* 上と同じ理由。.col-item(親、flex-direction:column)の中のflex子になりうるため。 */
    flex: 1 1 auto;
    height: 100%;
    min-height: 0;
  }
  .col-item {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }
  .row-item {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    min-width: 0;
  }
</style>
