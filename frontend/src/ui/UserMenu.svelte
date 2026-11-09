<script lang="ts">
  // ProfileModal の「…」メニュー(Issue #545 / #546)。見た目と項目は NoteMenu に揃える。
  // 非同期処理とエラー表示は呼び出し側(ProfileModal)が持つ。ここは表示と選択だけ
  // (リストの取得とトグルの状態だけは、サブメニューの UserListsPanel が持つ)。
  import {
    Ban,
    ChevronLeft,
    ChevronRight,
    Eye,
    EyeOff,
    ExternalLink,
    Flag,
    ListPlus,
    Repeat2,
  } from "@lucide/svelte";
  import type { MutePeriod, UserListMembership } from "../bindings/tauri.gen";
  import UserListsPanel from "./UserListsPanel.svelte";

  let {
    muted,
    onmute,
    onunmute,
    renoteMuted = null,
    onrenotemute = () => {},
    onunrenotemute = () => {},
    blocking = null,
    onblock = () => {},
    onunblock = () => {},
    loadLists,
    togglelist,
    profileUrl = null,
    onopen = () => {},
    onreport,
  }: {
    muted: boolean;
    onmute: (period: MutePeriod) => void;
    onunmute: () => void;
    /// null は「サーバーが返さなかった」。その項目は出さない。
    renoteMuted?: boolean | null;
    onrenotemute?: () => void;
    onunrenotemute?: () => void;
    blocking?: boolean | null;
    /// 「ブロック」を押したときに呼ぶ。確認ダイアログは呼び出し側が出す。
    onblock?: () => void;
    onunblock?: () => void;
    /// 両方あるときだけ「リストに追加」を出す。
    loadLists?: () => Promise<UserListMembership[]>;
    togglelist?: (listId: string, add: boolean) => Promise<void>;
    /// リモートユーザーの元ページ。null なら「ブラウザで開く」を出さない。
    profileUrl?: string | null;
    onopen?: () => void;
    /// あるときだけ「通報」を出す。ダイアログは呼び出し側が出す。
    onreport?: () => void;
  } = $props();

  // Misskey Web の「ミュートする期限」と同じ選択肢(既定の無期限が先頭)。
  const PERIODS: { value: MutePeriod; label: string }[] = [
    { value: "indefinitely", label: "無期限" },
    { value: "tenMinutes", label: "10分" },
    { value: "oneHour", label: "1時間" },
    { value: "oneDay", label: "1日" },
    { value: "oneWeek", label: "1週間" },
  ];

  // "root": 項目の一覧 / "period": 期間の選択 / "lists": リストへの追加。同じパネルの中で切り替える。
  let view = $state<"root" | "period" | "lists">("root");

  const ITEM =
    "box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted";
  const DANGER_ITEM =
    "box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-destructive hover:bg-muted";
</script>

<div
  class="w-[220px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
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
    {#if renoteMuted === true}
      <button type="button" class={ITEM} onclick={onunrenotemute}>
        <Repeat2 size={16} />
        リノートのミュートを解除
      </button>
    {:else if renoteMuted === false}
      <button type="button" class={ITEM} onclick={onrenotemute}>
        <Repeat2 size={16} />
        リノートをミュート
      </button>
    {/if}
    {#if blocking === true}
      <button type="button" class={ITEM} onclick={onunblock}>
        <Ban size={16} />
        ブロック解除
      </button>
    {:else if blocking === false}
      <button type="button" class={ITEM} onclick={onblock}>
        <Ban size={16} />
        ブロック
      </button>
    {/if}
    {#if loadLists && togglelist}
      <button type="button" class={ITEM} onclick={() => (view = "lists")}>
        <ListPlus size={16} />
        リストに追加
        <ChevronRight size={16} class="ml-auto" />
      </button>
    {/if}
    {#if profileUrl}
      <button type="button" class={ITEM} onclick={onopen}>
        <ExternalLink size={16} />
        ブラウザで開く
      </button>
    {/if}
    {#if onreport}
      <div class="my-1 h-px bg-border" role="separator"></div>
      <button type="button" class={DANGER_ITEM} onclick={onreport}>
        <Flag size={16} />
        通報
      </button>
    {/if}
  {:else if view === "period"}
    <button type="button" class={ITEM} onclick={() => (view = "root")}>
      <ChevronLeft size={16} />
      戻る
    </button>
    <span class="block px-2 py-1 text-xs text-muted-foreground">ミュートする期限</span>
    {#each PERIODS as period (period.value)}
      <button type="button" class={ITEM} onclick={() => onmute(period.value)}>{period.label}</button>
    {/each}
  {:else if loadLists && togglelist}
    <UserListsPanel load={loadLists} toggle={togglelist} onback={() => (view = "root")} />
  {/if}
</div>
