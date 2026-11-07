<script lang="ts">
  // ProfileModal の「…」メニュー(Issue #545)。見た目と項目は NoteMenu に揃える。
  // 非同期処理とエラー表示は呼び出し側(ProfileModal)が持つ。ここは表示と選択だけ。
  import { ChevronLeft, ChevronRight, Eye, EyeOff } from "@lucide/svelte";
  import type { MutePeriod } from "../bindings/tauri.gen";

  let {
    muted,
    onmute,
    onunmute,
  }: {
    muted: boolean;
    onmute: (period: MutePeriod) => void;
    onunmute: () => void;
  } = $props();

  // Misskey Web の「ミュートする期限」と同じ選択肢(既定の無期限が先頭)。
  const PERIODS: { value: MutePeriod; label: string }[] = [
    { value: "indefinitely", label: "無期限" },
    { value: "tenMinutes", label: "10分" },
    { value: "oneHour", label: "1時間" },
    { value: "oneDay", label: "1日" },
    { value: "oneWeek", label: "1週間" },
  ];

  // "root": 項目の一覧 / "period": 期間の選択。同じパネルの中で切り替える。
  let view = $state<"root" | "period">("root");

  const ITEM =
    "box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted";
</script>

<div
  class="w-[200px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
  data-testid="user-menu"
>
  {#if view === "root"}
    {#if muted}
      <button type="button" class={ITEM} onclick={onunmute}>
        <Eye size={16} />
        ミュート解除
      </button>
    {:else}
      <button type="button" class={ITEM} onclick={() => (view = "period")}>
        <EyeOff size={16} />
        ミュート
        <ChevronRight size={16} class="ml-auto" />
      </button>
    {/if}
  {:else}
    <button type="button" class={ITEM} onclick={() => (view = "root")}>
      <ChevronLeft size={16} />
      戻る
    </button>
    <span class="block px-2 py-1 text-xs text-muted-foreground">ミュートする期限</span>
    {#each PERIODS as period (period.value)}
      <button type="button" class={ITEM} onclick={() => onmute(period.value)}>{period.label}</button>
    {/each}
  {/if}
</div>
