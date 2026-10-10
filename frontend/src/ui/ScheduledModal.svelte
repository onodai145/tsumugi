<script lang="ts">
  import { untrack } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import Modal from "./Modal.svelte";
  import { commands, unwrapAcc } from "../lib/ipc";
  import type { LocalScheduledNote, ScheduledNote } from "../bindings/tauri.gen";
  import { LOCAL_SCHEDULE_NOTICE, localStatusLabel, type ScheduledListItem } from "../lib/scheduledList";

  let {
    accountId,
    serverSide,
    reloadToken,
    onrestore,
    onclose,
  }: {
    accountId: string;
    /// サーバー予約(notes/drafts)も一覧に含めるか。含めないアカウントでも、ローカル予約は常に出す。
    serverSide: boolean;
    /// 値が変わったらローカル予約を読み直す(予約の投稿・失敗・期限切れのイベントで増える)。
    reloadToken: number;
    /// 「作成欄に戻す」。呼び出し側(ComposeBar)が内容の読み込みと予約の削除を行う。
    onrestore: (item: ScheduledListItem) => void;
    onclose: () => void;
  } = $props();

  // list_scheduled_notes の1回の取得件数。これだけ返ったら続きがあるとみなす。
  const PAGE_SIZE = 30;

  let serverItems = $state<ScheduledNote[]>([]);
  let localItems = $state<LocalScheduledNote[]>([]);
  let serverLoading = $state(false);
  let localLoading = $state(true);
  let hasMore = $state(false);
  let serverErr = $state<string | null>(null);
  let localErr = $state<string | null>(null);
  let actionErr = $state<string | null>(null);
  // カーソルは表示順(予約日時の昇順)ではなくサーバーが返した順の最後の ID。
  let cursor: string | null = null;
  let seenToken = untrack(() => reloadToken);

  const rows = $derived<ScheduledListItem[]>(
    [
      ...serverItems.map((note): ScheduledListItem => ({ origin: "server", note })),
      ...localItems.map(
        (l): ScheduledListItem => ({ origin: "local", note: l.note, status: l.status, error: l.error }),
      ),
    ].sort((a, b) => a.note.scheduledAt - b.note.scheduledAt),
  );
  const err = $derived(serverErr ?? localErr ?? actionErr);
  const loadFailed = $derived(serverErr !== null || localErr !== null);

  async function loadServer() {
    if (!serverSide) return;
    serverLoading = true;
    serverErr = null;
    try {
      const page = await unwrapAcc(accountId, commands.listScheduledNotes(accountId, cursor, PAGE_SIZE));
      serverItems = [...serverItems, ...(page ?? [])];
      hasMore = (page?.length ?? 0) >= PAGE_SIZE;
      if (page && page.length > 0) cursor = page[page.length - 1].id;
    } catch (e) {
      serverErr = String(e);
    } finally {
      serverLoading = false;
    }
  }

  async function loadLocal() {
    localLoading = true;
    localErr = null;
    try {
      localItems = (await unwrapAcc(accountId, commands.listLocalScheduledNotes(accountId))) ?? [];
    } catch (e) {
      localErr = String(e);
    } finally {
      localLoading = false;
    }
  }

  function reloadAll() {
    serverItems = [];
    cursor = null;
    void loadServer();
    void loadLocal();
  }

  void loadServer();
  void loadLocal();

  // 予約の投稿・失敗・期限切れ(イベント)で、ローカル予約だけを読み直す。
  $effect(() => {
    const t = reloadToken;
    if (t === seenToken) return;
    seenToken = t;
    void loadLocal();
  });

  async function cancel(item: ScheduledListItem) {
    actionErr = null;
    try {
      if (item.origin === "server") {
        await unwrapAcc(accountId, commands.cancelScheduledNote(accountId, item.note.id));
        serverItems = serverItems.filter((n) => n.id !== item.note.id);
      } else {
        await unwrapAcc(accountId, commands.cancelLocalScheduledNote(accountId, item.note.id));
        localItems = localItems.filter((l) => l.note.id !== item.note.id);
      }
    } catch (e) {
      actionErr = String(e);
    }
  }

  async function runNow(item: ScheduledListItem) {
    actionErr = null;
    try {
      await unwrapAcc(accountId, commands.postLocalScheduledNow(accountId, item.note.id));
    } catch (e) {
      actionErr = String(e);
    } finally {
      await loadLocal();
    }
  }

  /// サーバー予約で、予約時刻を過ぎても残っている=サーバーが投稿に失敗したもの。
  const isServerFailed = (n: ScheduledNote) => n.scheduledAt * 1000 <= Date.now();
  const formatAt = (n: ScheduledNote) => new Date(n.scheduledAt * 1000).toLocaleString();
  const VISIBILITY_LABEL: Record<string, string> = {
    public: "公開",
    home: "ホーム",
    followers: "フォロワー",
    specified: "ダイレクト",
  };
</script>

<Modal title="予約済みの投稿" {onclose} width="520px" maxHeight="80vh">
  {#snippet children()}
    {#if err}
      <p class="mb-2 mt-0 whitespace-pre-wrap break-words text-sm text-destructive">{err}</p>
    {/if}
    {#if (serverLoading || localLoading) && rows.length === 0}
      <div class="py-3 text-sm text-muted-foreground">読み込み中…</div>
    {:else if loadFailed && rows.length === 0}
      <!-- 取得失敗を「予約なし」と誤解して二重に予約しないよう、空表示は出さず再読み込みだけ出す -->
      <div class="flex justify-center py-2">
        <Button type="button" variant="outline" size="sm" data-testid="scheduled-retry" onclick={reloadAll}>再読み込み</Button>
      </div>
    {:else if rows.length === 0}
      <div class="py-3 text-sm text-muted-foreground" data-testid="scheduled-empty">予約済みの投稿はありません</div>
    {:else}
      <div class="min-h-0 flex-1 overflow-y-auto">
        {#each rows as item (item.origin + ":" + item.note.id)}
          {@const n = item.note}
          {@const statusLabel = item.origin === "local" ? localStatusLabel(item.status) : null}
          <div class="border-b border-border py-2 last:border-b-0" data-testid={`scheduled-item-${n.id}`}>
            <div class="mb-1 flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
              <span>{formatAt(n)}</span>
              <span>{VISIBILITY_LABEL[n.visibility] ?? n.visibility}</span>
              {#if item.origin === "server" && isServerFailed(n)}
                <span class="font-semibold text-destructive" data-testid={`scheduled-failed-${n.id}`}>投稿に失敗</span>
              {/if}
              {#if statusLabel}
                <span class="font-semibold text-destructive" data-testid={`scheduled-local-status-${n.id}`}>{statusLabel}</span>
              {/if}
            </div>
            <div class="mb-1.5 line-clamp-3 whitespace-pre-wrap break-words text-sm text-foreground">{n.text.trim() || "(本文なし)"}</div>
            {#if item.origin === "local" && item.status === "pending"}
              <div class="mb-1.5 text-xs text-muted-foreground" data-testid={`scheduled-local-notice-${n.id}`}>{LOCAL_SCHEDULE_NOTICE}</div>
            {/if}
            {#if item.origin === "local" && item.error}
              <div class="mb-1.5 whitespace-pre-wrap break-words text-xs text-destructive">{item.error}</div>
            {/if}
            {#if !(item.origin === "local" && item.status === "posting")}
              <div class="flex justify-end gap-1.5">
                {#if item.origin === "local" && (item.status === "expired" || item.status === "failed")}
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    data-testid={`scheduled-run-now-${n.id}`}
                    onclick={() => runNow(item)}
                  >今すぐ投稿</Button>
                {/if}
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  data-testid={`scheduled-restore-${n.id}`}
                  onclick={() => onrestore(item)}
                >作成欄に戻す</Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  data-testid={`scheduled-cancel-${n.id}`}
                  onclick={() => cancel(item)}
                >取り消し</Button>
              </div>
            {/if}
          </div>
        {/each}
        {#if hasMore}
          <div class="flex justify-center py-2">
            <Button type="button" variant="ghost" size="sm" disabled={serverLoading} data-testid="scheduled-more" onclick={loadServer}
              >さらに読み込む</Button
            >
          </div>
        {/if}
      </div>
    {/if}
  {/snippet}
</Modal>
