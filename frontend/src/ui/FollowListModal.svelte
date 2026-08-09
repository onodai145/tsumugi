<script lang="ts">
  import { untrack } from "svelte";
  import type { FollowListEntry } from "../bindings/tauri.gen";
  import { app } from "../lib/store.svelte";
  import { acct, displayName } from "../lib/userDisplay";
  import { proxiedEmojiMap } from "../lib/emoji";
  import { openProfile } from "../lib/profileModal.svelte";
  import Modal from "./Modal.svelte";
  import Mfm from "../render/Mfm.svelte";
  import { Button } from "$lib/components/ui/button";

  let {
    kind,
    userId,
    accountId,
    onclose,
  }: { kind: "followers" | "following"; userId: string; accountId: string; onclose: () => void } = $props();

  // ProfileModal/NoteCard と同じパターン: リモートユーザーのカスタム絵文字はaccountの
  // 接続先インスタンス経由のプロキシURLで解決する必要がある。
  const instanceHost = $derived(app.accounts.find((a) => a.id === accountId)?.host);

  let users = $state<FollowListEntry[]>([]);
  let busy = $state(false);
  let done = $state(false);
  let err = $state<string | null>(null);

  // openProfile() は単一の共有 target/accountId を書き換える設計（profileModal.svelte.ts）で、
  // FollowListModal の行クリックからも openProfile() を呼ぶ（このファイル下部）。ProfileModal 側の
  // 同一インスタンスが target 変更で userId を再束縛すると、この FollowListModal も同じインスタンスの
  // まま kind/userId/accountId だけが変わりうる。そのため世代番号でリクエストを追跡し、古い世代の
  // 応答が新しい世代の users を汚染しないようにする。
  let requestGen = 0;

  async function loadMore() {
    if (busy || done) return;
    busy = true;
    err = null;
    const myGen = requestGen;
    try {
      const untilId = users.length > 0 ? users[users.length - 1].cursor : undefined;
      const page =
        kind === "followers"
          ? await app.getUserFollowers(accountId, userId, untilId)
          : await app.getUserFollowing(accountId, userId, untilId);
      if (myGen !== requestGen) return; // 世代遅れの応答は無視する
      if (page.length === 0) done = true;
      users = [...users, ...page];
    } catch (e) {
      if (myGen !== requestGen) return;
      err = String(e);
    } finally {
      if (myGen === requestGen) busy = false;
    }
  }

  // loadMore() は同期部分で users/busy/done を読むため、$effect の本体全体をそのまま
  // 追跡させると、それらの state 変化のたびに再発火し、同じページを重複追加する無限ループに
  // なる（ページネーションを無視するモック実装で each_key_duplicate として顕在化した）。
  // 一方で kind/userId/accountId の変化（同一インスタンスが再利用される場合。上記コメント参照）
  // には反応してリロードする必要があるため、それらの props だけを追跡対象にし、内部の
  // state リセットと loadMore() 呼び出しは untrack でラップする。
  $effect(() => {
    const trackedKind = kind;
    const trackedUserId = userId;
    const trackedAccountId = accountId;
    void trackedKind;
    void trackedUserId;
    void trackedAccountId;
    untrack(() => {
      requestGen++;
      users = [];
      busy = false;
      done = false;
      err = null;
      void loadMore();
    });
  });

  // Column.svelte のタイムライン無限スクロールと同じ閾値(残り300px)で追加取得する。
  // err が立っている間はスクロールのたびに再試行してしまう(失敗した直後は既に閾値内にいるため)
  // のを防ぐため、エラー時は無視する。再試行は下の再試行ボタン経由のみとする。
  function onScroll(e: Event) {
    if (err) return;
    const el = e.currentTarget as HTMLElement;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 300) {
      void loadMore();
    }
  }
</script>

<Modal title={kind === "followers" ? "フォロワー" : "フォロー中"} {onclose}>
  <ul class="-mx-4 mt-1 max-h-[55vh] list-none overflow-y-auto p-0" onscroll={onScroll}>
    {#each users as entry (entry.user.id)}
      <li>
        <button
          class="list-row flex w-full items-center gap-2.5 px-4 py-[9px] text-left text-foreground"
          onclick={() => openProfile({ userId: entry.user.id }, accountId)}
        >
          {#if entry.user.avatarUrl}
            <img class="h-10 w-10 flex-none rounded-lg object-cover" src={entry.user.avatarUrl} alt="" />
          {:else}
            <div class="avatar-ph h-10 w-10 flex-none rounded-lg"></div>
          {/if}
          <span class="flex min-w-0 flex-col gap-0.5">
            <span class="overflow-hidden text-ellipsis whitespace-nowrap text-[0.88rem] font-semibold"
              ><Mfm
                text={displayName(entry.user)}
                emojis={proxiedEmojiMap(entry.user.emojis, instanceHost)}
                simple
              /></span
            >
            <span class="overflow-hidden text-ellipsis whitespace-nowrap text-[0.78rem] text-muted-foreground"
              >{acct(entry.user)}</span
            >
          </span>
        </button>
      </li>
    {/each}
    {#if busy}<li class="p-2.5 text-center text-[0.8rem] text-muted-foreground">読み込み中…</li>{/if}
  </ul>
  {#if err}
    <p class="my-2 text-[0.82rem] text-destructive">{err}</p>
    <Button variant="outline" size="sm" onclick={loadMore} disabled={busy}>再試行</Button>
  {/if}
</Modal>

<style>
  li + li {
    border-top: 1px solid var(--border);
  }
  .list-row:hover {
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
  }
  .avatar-ph {
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  }
</style>
