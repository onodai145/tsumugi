<script lang="ts">
  import { commands, unwrap } from "../lib/ipc";
  import { X, Check } from "@lucide/svelte";
  import type { DriveFile, SourceItem } from "../bindings/tauri.gen";

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

  async function refresh() {
    err = null;
    loading = true;
    noMoreFiles = false;
    try {
      const [f, fl] = await Promise.all([
        unwrap(commands.listDriveFolders(accountId, currentFolderId)),
        unwrap(commands.listDriveFiles(accountId, currentFolderId, null)),
      ]);
      folders = f;
      files = fl;
      if (fl.length < FILES_LIMIT) noMoreFiles = true;
    } catch (e) {
      err = String(e);
    } finally {
      loading = false;
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

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="overlay" onclick={onclose} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="-1">
    <header class="head">
      <span>ドライブから選択</span>
      <button class="x" onclick={onclose}><X size={16} /></button>
    </header>

    <nav class="breadcrumb">
      <button class="crumb" class:active={path.length === 0} onclick={() => goToBreadcrumb(-1)}>ドライブ</button>
      {#each path as p, i (p.id)}
        <span class="sep">/</span>
        <button class="crumb" class:active={i === path.length - 1} onclick={() => goToBreadcrumb(i)}>
          {p.name || "(無題)"}
        </button>
      {/each}
    </nav>

    {#if loading}
      <p class="hint">読み込み中…</p>
    {:else}
      {#if folders.length === 0 && files.length === 0}
        <p class="hint">ファイルがありません</p>
      {/if}
      <div class="grid">
        {#each folders as f (f.id)}
          <button class="cell folder" onclick={() => enterFolder(f)}>📁 {f.name || "(無題)"}</button>
        {/each}
        {#each files as f (f.id)}
          <button class="cell file" class:selected={selected.has(f.id)} onclick={() => onFileClick(f)}>
            {#if f.isSensitive && !revealed[f.id]}
              <span class="sensitive-cover">閲覧注意（クリックで表示）</span>
            {:else if f.mimeType.startsWith("image/")}
              <img src={f.thumbnailUrl ?? f.url} alt={f.name} loading="lazy" />
            {:else}
              <span class="badge">{f.mimeType.split("/")[0] || "file"}</span>
            {/if}
            {#if selected.has(f.id)}
              <span class="check"><Check size={14} /></span>
            {/if}
          </button>
        {/each}
      </div>
      {#if !noMoreFiles && files.length > 0}
        <button class="mini more" disabled={loadingMore} onclick={loadMore}>
          {loadingMore ? "読み込み中…" : "もっと見る"}
        </button>
      {/if}
    {/if}

    {#if err}<p class="err">{err}</p>{/if}

    <div class="actions">
      <span class="count">選択中 {selected.size}件</span>
      <button class="submit" disabled={selected.size === 0} onclick={confirm}>添付</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: grid;
    place-items: start center;
    padding-top: 8vh;
    z-index: 60;
  }
  .modal {
    width: min(520px, 92vw);
    max-height: 78vh;
    display: flex;
    flex-direction: column;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 16px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 600;
    margin-bottom: 10px;
    flex: none;
  }
  .x {
    display: inline-flex;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
  .breadcrumb {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
    font-size: 0.8rem;
    margin-bottom: 10px;
    flex: none;
  }
  .crumb {
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    padding: 2px 4px;
    font: inherit;
  }
  .crumb.active {
    color: var(--text);
    font-weight: 600;
  }
  .sep {
    color: var(--text-dim);
  }
  .hint {
    font-size: 0.8rem;
    color: var(--text-dim);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(84px, 1fr));
    gap: 8px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }
  .cell {
    position: relative;
    aspect-ratio: 1;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    overflow: hidden;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.75rem;
  }
  .cell.folder {
    font-size: 0.72rem;
    padding: 6px;
    word-break: break-all;
  }
  .cell.file img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .cell.file.selected {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .sensitive-cover {
    padding: 4px;
    color: var(--text-dim);
  }
  .badge {
    color: var(--text-dim);
  }
  .check {
    position: absolute;
    top: 4px;
    right: 4px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--accent);
    color: #fff;
  }
  .mini.more {
    margin-top: 8px;
    align-self: center;
    padding: 5px 14px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    font-size: 0.8rem;
    flex: none;
  }
  .mini.more:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 12px;
    flex: none;
  }
  .count {
    font-size: 0.8rem;
    color: var(--text-dim);
  }
  .submit {
    border: none;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    border-radius: 8px;
    padding: 7px 20px;
    cursor: pointer;
  }
  .submit:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .err {
    color: #ef4444;
    font-size: 0.82rem;
    margin: 8px 0 0;
    word-break: break-word;
    flex: none;
  }
</style>
