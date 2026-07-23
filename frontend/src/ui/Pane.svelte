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
  }: {
    node: PaneNode;
    onAddTab: (groupId: string) => void;
    onEditTab: (tab: TabView) => void;
    onEditGroup: (groupId: string) => void;
    onSplitDown: (groupId: string) => void;
  } = $props();
</script>

{#if node.type === "leaf"}
  {@const group = app.groups.find((g) => g.id === node.group_id)}
  {#if group}
    <Column {group} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} />
  {/if}
{:else if node.direction === "row"}
  <div class="row">
    {#each node.children as child (child.node.id)}
      <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} />
    {/each}
  </div>
{:else}
  <div class="col">
    {#each node.children as child (child.node.id)}
      <div class="col-item" style={`flex:${child.size} 1 0`}>
        <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} />
      </div>
    {/each}
  </div>
{/if}

<style>
  .row {
    display: flex;
    height: 100%;
    overflow-x: auto;
  }
  .col {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .col-item {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }
</style>
