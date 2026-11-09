<script lang="ts">
  // UserMenu の「リストに追加」サブメニュー(Issue #546)。開いた時に自分のリストを取得し、対象ユーザーが
  // 入っているものにチェックを付ける。押すと追加/解除をトグルする。トグルは楽観更新せず、完了後に反映する。
  import { untrack } from "svelte";
  import { Check, ChevronLeft } from "@lucide/svelte";
  import type { UserListMembership } from "../bindings/tauri.gen";
  import { Button } from "$lib/components/ui/button";

  let {
    load,
    toggle,
    onback,
  }: {
    load: () => Promise<UserListMembership[]>;
    toggle: (listId: string, add: boolean) => Promise<void>;
    onback: () => void;
  } = $props();

  type PanelState =
    | { status: "loading" }
    | { status: "error"; message: string }
    | { status: "ready"; lists: UserListMembership[] };

  let panel = $state<PanelState>({ status: "loading" });
  let busyIds = $state<string[]>([]);
  let toggleErr = $state<string | null>(null);

  async function reload() {
    panel = { status: "loading" };
    try {
      panel = { status: "ready", lists: await load() };
    } catch (e) {
      panel = { status: "error", message: String(e) };
    }
  }

  // 開いた時に1回だけ取得する(再試行は reload を直接呼ぶ)。`load` の中身(ProfileModal では
  // profileState を読む)を依存として追跡すると、ミュートなどでプロフィールが変わるたびに再取得して
  // 「読み込み中…」に戻ってしまうので、untrack で依存を切る。
  $effect(() => {
    untrack(() => {
      void reload();
    });
  });

  async function onToggle(list: UserListMembership) {
    if (busyIds.includes(list.id)) return;
    busyIds = [...busyIds, list.id];
    toggleErr = null;
    const add = !list.hasUser;
    try {
      await toggle(list.id, add);
      if (panel.status === "ready") {
        panel = {
          status: "ready",
          lists: panel.lists.map((l) => (l.id === list.id ? { ...l, hasUser: add } : l)),
        };
      }
    } catch (e) {
      // 他のクライアントで追加済みだった場合。サーバーの状態に合わせて表示を直す(ミュート・ブロックの補正と同じ作法)。
      if (add && String(e).includes("ALREADY_ADDED") && panel.status === "ready") {
        panel = {
          status: "ready",
          lists: panel.lists.map((l) => (l.id === list.id ? { ...l, hasUser: true } : l)),
        };
        toggleErr = "すでにリストに入っています。表示を更新しました";
      } else {
        toggleErr = String(e);
      }
    } finally {
      busyIds = busyIds.filter((id) => id !== list.id);
    }
  }

  const ITEM =
    "box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted disabled:opacity-60";
</script>

<button type="button" class={ITEM} onclick={onback}>
  <ChevronLeft size={16} />
  戻る
</button>
<span class="block px-2 py-1 text-xs text-muted-foreground">リストに追加</span>
{#if panel.status === "loading"}
  <p class="m-0 px-2 py-1.5 text-sm text-muted-foreground">読み込み中…</p>
{:else if panel.status === "error"}
  <p class="m-0 px-2 py-1.5 text-sm text-destructive">{panel.message}</p>
  <div class="px-2 pb-1">
    <Button variant="outline" size="sm" onclick={reload}>再試行</Button>
  </div>
{:else if panel.lists.length === 0}
  <p class="m-0 px-2 py-1.5 text-sm text-muted-foreground">リストがありません</p>
{:else}
  <div class="max-h-[240px] overflow-y-auto">
    {#each panel.lists as list (list.id)}
      <button
        type="button"
        role="checkbox"
        aria-checked={list.hasUser}
        class={ITEM}
        disabled={busyIds.includes(list.id)}
        onclick={() => onToggle(list)}
      >
        <span class="flex h-4 w-4 flex-none items-center justify-center">
          {#if list.hasUser}<Check size={16} />{/if}
        </span>
        <span class="min-w-0 flex-1 truncate">{list.name}</span>
      </button>
    {/each}
  </div>
{/if}
{#if toggleErr}
  <p class="m-0 px-2 py-1.5 text-sm text-destructive">{toggleErr}</p>
{/if}
