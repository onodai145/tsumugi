<script lang="ts">
  import { commands, unwrap } from "../lib/ipc";
  import { X, Check } from "@lucide/svelte";
  import type { DriveFile, SourceItem } from "../bindings/tauri.gen";
  import { Button } from "$lib/components/ui/button";

  let {
    accountId,
    onSelect,
    onclose,
  }: { accountId: string; onSelect: (files: DriveFile[]) => void; onclose: () => void } = $props();

  // バックエンド側の1ページ件数(DRIVE_LIST_LIMIT)と一致させる。返却件数がこれ未満なら終端。
  const FILES_LIMIT = 30;

  // ルートからの経路(パンくず)。空配列 = ルート。末尾が現在のフォルダ。
  let path = $state<SourceItem[]>([]);
  const currentFolderId = $derived(path.length > 0 ? path[path.length - 1].id : null);

  let folders = $state<SourceItem[]>([]);
  let files = $state<DriveFile[]>([]);
  let selected = $state<Map<string, DriveFile>>(new Map());
  let revealed = $state<Record<string, boolean>>({});
  let loading = $state(false);
  let loadingMore = $state(false);
  let noMoreFiles = $state(false);
  let err = $state<string | null>(null);
  let requestSeq = 0;

  async function refresh() {
    const seq = ++requestSeq;
    err = null;
    loading = true;
    noMoreFiles = false;
    try {
      const [f, fl] = await Promise.all([
        unwrap(commands.listDriveFolders(accountId, currentFolderId)),
        unwrap(commands.listDriveFiles(accountId, currentFolderId, null)),
      ]);
      if (seq !== requestSeq) return; // 新しいナビゲーションで上書きされた古い応答は破棄
      folders = f;
      files = fl;
      if (fl.length < FILES_LIMIT) noMoreFiles = true;
    } catch (e) {
      if (seq !== requestSeq) return;
      err = String(e);
    } finally {
      if (seq === requestSeq) loading = false;
    }
  }

  async function loadMore() {
    if (files.length === 0 || loadingMore || noMoreFiles) return;
    loadingMore = true;
    err = null;
    try {
      const older = await unwrap(
        commands.listDriveFiles(accountId, currentFolderId, files[files.length - 1].id),
      );
      files = [...files, ...older];
      if (older.length < FILES_LIMIT) noMoreFiles = true;
    } catch (e) {
      err = String(e);
    } finally {
      loadingMore = false;
    }
  }

  function enterFolder(folder: SourceItem) {
    path = [...path, folder];
    void refresh();
  }

  // index === -1 でルートへ。それ以外は path[0..=index] まで残す。
  function goToBreadcrumb(index: number) {
    path = index < 0 ? [] : path.slice(0, index + 1);
    void refresh();
  }

  function toggleSelect(f: DriveFile) {
    const next = new Map(selected);
    if (next.has(f.id)) next.delete(f.id);
    else next.set(f.id, f);
    selected = next;
  }

  function onFileClick(f: DriveFile) {
    if (f.isSensitive && !revealed[f.id]) {
      revealed = { ...revealed, [f.id]: true };
      return;
    }
    toggleSelect(f);
  }

  function confirm() {
    onSelect([...selected.values()]);
    onclose();
  }

  void refresh();
</script>

<div
  class="fixed inset-0 z-[60] grid items-start justify-items-center bg-black/45 pt-[8vh]"
  onclick={onclose}
  onkeydown={(e) => e.key === "Escape" && onclose()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="flex max-h-[78vh] w-[min(520px,92vw)] flex-col rounded-xl border border-border bg-background p-4"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <header class="mb-2.5 flex flex-none items-center justify-between font-semibold">
      <span>ドライブから選択</span>
      <Button type="button" variant="ghost" size="icon-xs" onclick={onclose}><X size={16} /></Button>
    </header>

    <nav class="mb-2.5 flex flex-none flex-wrap items-center gap-1 text-[0.8rem]">
      <button
        type="button"
        class={path.length === 0
          ? "px-1 py-0.5 font-[inherit] font-semibold text-foreground"
          : "px-1 py-0.5 font-[inherit] text-muted-foreground"}
        onclick={() => goToBreadcrumb(-1)}>ドライブ</button
      >
      {#each path as p, i (p.id)}
        <span class="text-muted-foreground">/</span>
        <button
          type="button"
          class={i === path.length - 1
            ? "px-1 py-0.5 font-[inherit] font-semibold text-foreground"
            : "px-1 py-0.5 font-[inherit] text-muted-foreground"}
          onclick={() => goToBreadcrumb(i)}
        >
          {p.name || "(無題)"}
        </button>
      {/each}
    </nav>

    {#if loading}
      <p class="text-[0.8rem] text-muted-foreground">読み込み中…</p>
    {:else}
      {#if folders.length === 0 && files.length === 0}
        <p class="text-[0.8rem] text-muted-foreground">ファイルがありません</p>
      {/if}
      <div class="grid min-h-0 flex-1 auto-rows-[84px] grid-cols-[repeat(auto-fill,minmax(84px,1fr))] gap-2 overflow-y-auto">
        {#each folders as f (f.id)}
          <button
            type="button"
            class="relative flex h-full w-full items-center justify-center overflow-hidden rounded-lg border border-border bg-muted p-1.5 text-[0.72rem] break-all text-foreground"
            onclick={() => enterFolder(f)}
          >
            📁 {f.name || "(無題)"}
          </button>
        {/each}
        {#each files as f (f.id)}
          <button
            type="button"
            class={selected.has(f.id)
              ? "relative flex h-full w-full items-center justify-center overflow-hidden rounded-lg border border-border bg-muted p-0 text-[0.75rem] text-foreground outline outline-2 outline-offset-[-2px] outline-primary"
              : "relative flex h-full w-full items-center justify-center overflow-hidden rounded-lg border border-border bg-muted p-0 text-[0.75rem] text-foreground"}
            onclick={() => onFileClick(f)}
          >
            {#if f.isSensitive && !revealed[f.id]}
              <span class="p-1 text-muted-foreground">閲覧注意（クリックで表示）</span>
            {:else if f.mimeType.startsWith("image/")}
              <img src={f.thumbnailUrl ?? f.url} alt={f.name} loading="lazy" class="h-full w-full object-cover" />
            {:else}
              <span class="text-muted-foreground">{f.mimeType.split("/")[0] || "file"}</span>
            {/if}
            {#if selected.has(f.id)}
              <span
                class="absolute top-1 right-1 flex size-[18px] items-center justify-center rounded-full bg-primary text-white"
                ><Check size={14} /></span
              >
            {/if}
          </button>
        {/each}
      </div>
      {#if !noMoreFiles && files.length > 0}
        <Button
          type="button"
          variant="outline"
          size="sm"
          class="mt-2 self-center"
          disabled={loadingMore}
          onclick={loadMore}
        >
          {loadingMore ? "読み込み中…" : "もっと見る"}
        </Button>
      {/if}
    {/if}

    {#if err}<p class="mt-2 flex-none text-[0.82rem] break-words text-destructive">{err}</p>{/if}

    <div class="mt-3 flex flex-none items-center justify-between">
      <span class="text-[0.8rem] text-muted-foreground">選択中 {selected.size}件</span>
      <Button type="button" variant="default" size="sm" disabled={selected.size === 0} onclick={confirm}
        >添付</Button
      >
    </div>
  </div>
</div>
