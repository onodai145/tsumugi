<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import Modal from "./Modal.svelte";
  import { commands, unwrapAcc } from "../lib/ipc";
  import type { ScheduledNote } from "../bindings/tauri.gen";

  let {
    accountId,
    onrestore,
    onclose,
  }: {
    accountId: string;
    /// 「作成欄に戻す」。呼び出し側(ComposeBar)が内容の読み込みとサーバー側の予約削除を行う。
    onrestore: (s: ScheduledNote) => void;
    onclose: () => void;
  } = $props();

  // list_scheduled_notes の1回の取得件数。これだけ返ったら続きがあるとみなす。
  const PAGE_SIZE = 30;

  let items = $state<ScheduledNote[]>([]);
  let loading = $state(true);
  let hasMore = $state(false);
  let err = $state<string | null>(null);
  // カーソルは表示順(予約日時の昇順)ではなくサーバーが返した順の最後の ID。
  let cursor: string | null = null;

  const sorted = $derived([...items].sort((a, b) => a.scheduledAt - b.scheduledAt));

  async function load() {
    loading = true;
    err = null;
    try {
      const page = await unwrapAcc(accountId, commands.listScheduledNotes(accountId, cursor, PAGE_SIZE));
      items = [...items, ...page];
      hasMore = page.length >= PAGE_SIZE;
      if (page.length > 0) cursor = page[page.length - 1].id;
    } catch (e) {
      err = String(e);
    } finally {
      loading = false;
    }
  }
  void load();

  async function cancel(id: string) {
    err = null;
    try {
      await unwrapAcc(accountId, commands.cancelScheduledNote(accountId, id));
      items = items.filter((n) => n.id !== id);
    } catch (e) {
      err = String(e);
    }
  }

  /// 予約時刻を過ぎても残っている=サーバーが投稿に失敗したもの。
  const isFailed = (n: ScheduledNote) => n.scheduledAt * 1000 <= Date.now();
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
    {#if loading && items.length === 0}
      <div class="py-3 text-sm text-muted-foreground">読み込み中…</div>
    {:else if err && items.length === 0}
      <!-- 取得失敗を「予約なし」と誤解して二重に予約しないよう、空表示は出さず再読み込みだけ出す -->
      <div class="flex justify-center py-2">
        <Button type="button" variant="outline" size="sm" data-testid="scheduled-retry" onclick={load}>再読み込み</Button>
      </div>
    {:else if items.length === 0}
      <div class="py-3 text-sm text-muted-foreground" data-testid="scheduled-empty">予約済みの投稿はありません</div>
    {:else}
      <div class="min-h-0 flex-1 overflow-y-auto">
        {#each sorted as n (n.id)}
          <div class="border-b border-border py-2 last:border-b-0" data-testid={`scheduled-item-${n.id}`}>
            <div class="mb-1 flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
              <span>{formatAt(n)}</span>
              <span>{VISIBILITY_LABEL[n.visibility] ?? n.visibility}</span>
              {#if isFailed(n)}
                <span class="font-semibold text-destructive" data-testid={`scheduled-failed-${n.id}`}>投稿に失敗</span>
              {/if}
            </div>
            <div class="mb-1.5 line-clamp-3 whitespace-pre-wrap break-words text-sm text-foreground">{n.text.trim() || "(本文なし)"}</div>
            <div class="flex justify-end gap-1.5">
              <Button
                type="button"
                variant="outline"
                size="sm"
                data-testid={`scheduled-restore-${n.id}`}
                onclick={() => onrestore(n)}
              >作成欄に戻す</Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                data-testid={`scheduled-cancel-${n.id}`}
                onclick={() => cancel(n.id)}
              >取り消し</Button>
            </div>
          </div>
        {/each}
        {#if hasMore}
          <div class="flex justify-center py-2">
            <Button type="button" variant="ghost" size="sm" disabled={loading} data-testid="scheduled-more" onclick={load}
              >さらに読み込む</Button
            >
          </div>
        {/if}
      </div>
    {/if}
  {/snippet}
</Modal>
