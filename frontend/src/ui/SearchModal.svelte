<script lang="ts">
  import { app } from "../lib/store.svelte";
  import AccountSelect from "./AccountSelect.svelte";
  import TqlCompletionField from "../input/TqlCompletionField.svelte";
  import NoteCard from "./NoteCard.svelte";
  import Modal from "./Modal.svelte";
  import { Button } from "$lib/components/ui/button";
  import type { FilterQuery, Note } from "../bindings/tauri.gen";

  let { onclose }: { onclose: () => void } = $props();

  let uiMode = $state<"guided" | "expert">("guided");
  let accountId = $state(app.defaultAccountId());
  let keyword = $state("");
  let userAcct = $state("");
  let host = $state("");
  // WebKitGTKはdatetime-localの組み合わせピッカーUIがdate/time単体より未成熟（日付しか
  // 操作できず入力しづらい）ため、日付欄と時刻欄を分けて素直なネイティブウィジェットにする。
  let dateFrom = $state("");
  let dateFromTime = $state("");
  let dateTo = $state("");
  let dateToTime = $state("");
  let tqlText = $state("");
  let tqlErr = $state<string | null>(null);

  let notes = $state<Note[]>([]);
  let busy = $state(false);
  let done = $state(false);
  let err = $state<string | null>(null);
  let searched = $state(false);
  let requestGen = 0;

  // AddColumnModal.svelte の tqlStr() と同じエスケープ規則（本家パーサの読み方に合わせる）
  function tqlStr(s: string): string {
    return `"${s.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
  }

  // ガイドモードの固定フィールドから TQL の where 句を組み立てる。空欄の項目は述語を出さない。
  function guidedPredicate(): string {
    const parts: string[] = [];
    if (keyword.trim()) parts.push(`text -> ${tqlStr(keyword.trim())}`);
    if (userAcct.trim()) parts.push(`user.acct == ${tqlStr(userAcct.trim())}`);
    if (host.trim()) parts.push(`host == ${tqlStr(host.trim())}`);
    // 時刻欄が空なら開始は0時、終了はその日の終わり(23:59)を補って範囲指定を直感的にする。
    if (dateFrom) {
      const epoch = Math.floor(new Date(`${dateFrom}T${dateFromTime || "00:00"}`).getTime() / 1000);
      parts.push(`created_at >= ${epoch}`);
    }
    if (dateTo) {
      const epoch = Math.floor(new Date(`${dateTo}T${dateToTime || "23:59"}`).getTime() / 1000);
      parts.push(`created_at <= ${epoch}`);
    }
    return parts.join(" && ");
  }

  function currentPredicate(): string {
    return uiMode === "expert" ? tqlText.trim() : guidedPredicate();
  }

  // 簡単→エキスパートへ切替た時、まだ何も書いていなければ今の選択内容を反映する
  // (AddColumnModal.svelte の switchToExpert() と同じパターン)。
  function switchToExpert() {
    if (!tqlText.trim()) tqlText = guidedPredicate();
    uiMode = "expert";
  }

  async function onTqlInput() {
    if (!tqlText.trim()) {
      tqlErr = null;
      return;
    }
    tqlErr = await app.validateFilter({ kind: "tql", value: tqlText });
  }

  async function loadMore() {
    if (busy || done) return;
    busy = true;
    err = null;
    const myGen = requestGen;
    try {
      const untilId = notes.length > 0 ? notes[notes.length - 1].id : undefined;
      const filter: FilterQuery = { kind: "tql", value: currentPredicate() };
      const page = await app.searchCacheNotes(accountId, filter, untilId, 20);
      if (myGen !== requestGen) return;
      if (page.length === 0) done = true;
      const seen = new Set(notes.map((n) => n.id));
      const deduped = page.filter((n) => !seen.has(n.id));
      notes = [...notes, ...deduped];
    } catch (e) {
      if (myGen !== requestGen) return;
      err = String(e);
    } finally {
      if (myGen === requestGen) busy = false;
    }
  }

  function runSearch(e: Event) {
    e.preventDefault();
    if (uiMode === "expert" && tqlErr) return;
    requestGen++;
    notes = [];
    busy = false;
    done = false;
    err = null;
    searched = true;
    void loadMore();
  }

  // FollowListModal.svelte の onScroll() と同じ「残り300px」判定。
  function onScroll(e: Event) {
    if (err) return;
    const el = e.currentTarget as HTMLElement;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 300) {
      void loadMore();
    }
  }
</script>

<Modal title="検索" {onclose} width="620px">
  <!-- Modal.svelte のp-4を打ち消してフォーム+結果を1つの高さ制限付きフレックス列にする
       (Settings.svelte と同じ「-mx-4 -mb-4 + max-h + overflow-hidden」パターン)。
       ウィンドウが低くてもモーダル全体が画面外にはみ出さず、結果欄は残り空間いっぱいに
       広がる（固定max-hだったこれまでは常に狭かった）。 -->
  <div class="-mx-4 -mb-4 flex max-h-[calc(84vh-3rem)] flex-col overflow-hidden rounded-b-[11px]">
    <form onsubmit={runSearch} class="flex flex-none flex-col gap-2.5 px-4">
      <div class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">アカウント（検索結果の操作に使用。検索条件には影響しません）</span>
        <AccountSelect bind:value={accountId} accounts={app.accounts} showLabel />
      </div>

      <div class="flex items-center gap-0 self-start overflow-hidden rounded-lg border border-border text-sm">
        <button
          type="button"
          class={uiMode === "guided"
            ? "border-r border-border bg-primary px-3.5 py-1.5 text-primary-foreground"
            : "border-r border-border bg-muted px-3.5 py-1.5 text-foreground"}
          onclick={() => (uiMode = "guided")}
        >簡単</button>
        <button
          type="button"
          class={uiMode === "expert"
            ? "bg-primary px-3.5 py-1.5 text-primary-foreground"
            : "bg-muted px-3.5 py-1.5 text-foreground"}
          onclick={switchToExpert}
        >エキスパート(TQL)</button>
      </div>

      {#if uiMode === "guided"}
        <label class="flex flex-col gap-1 text-sm">
          <span class="text-muted-foreground">キーワード</span>
          <input
            class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
            placeholder="本文に含まれる語"
            bind:value={keyword}
          />
        </label>
        <label class="flex flex-col gap-1 text-sm">
          <span class="text-muted-foreground">ユーザー</span>
          <input
            class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
            placeholder="@user@host（自インスタンスのユーザーは @user）"
            bind:value={userAcct}
          />
        </label>
        <label class="flex flex-col gap-1 text-sm">
          <span class="text-muted-foreground">インスタンス</span>
          <input
            class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
            placeholder="example.com（空欄で全インスタンス対象）"
            bind:value={host}
          />
        </label>
        <div class="flex gap-2.5">
          <label class="flex flex-1 flex-col gap-1 text-sm">
            <span class="text-muted-foreground">日時（開始・空欄でその日の0時）</span>
            <div class="flex gap-1.5">
              <input
                type="date"
                class="w-0 flex-1 rounded-lg border border-border bg-muted px-2 py-2 font-[inherit] text-foreground"
                bind:value={dateFrom}
              />
              <input
                type="time"
                class="w-0 flex-1 rounded-lg border border-border bg-muted px-2 py-2 font-[inherit] text-foreground"
                bind:value={dateFromTime}
                disabled={!dateFrom}
              />
            </div>
          </label>
          <label class="flex flex-1 flex-col gap-1 text-sm">
            <span class="text-muted-foreground">日時（終了・空欄でその日の23:59）</span>
            <div class="flex gap-1.5">
              <input
                type="date"
                class="w-0 flex-1 rounded-lg border border-border bg-muted px-2 py-2 font-[inherit] text-foreground"
                bind:value={dateTo}
              />
              <input
                type="time"
                class="w-0 flex-1 rounded-lg border border-border bg-muted px-2 py-2 font-[inherit] text-foreground"
                bind:value={dateToTime}
                disabled={!dateTo}
              />
            </div>
          </label>
        </div>
      {:else}
        <label class="flex flex-col gap-1 text-sm">
          <span class="text-muted-foreground">TQL（cacheソースのwhere句。空欄で全件）</span>
          <TqlCompletionField
            mode="predicate"
            bind:value={tqlText}
            placeholder={'例: has_files && user.acct == "@alice@example.com"'}
            invalid={!!tqlErr}
            oninput={onTqlInput}
          />
        </label>
        {#if tqlErr}<p class="mb-0 mt-0 text-sm text-destructive break-words">TQLエラー: {tqlErr}</p>{/if}
      {/if}

      <Button type="submit" disabled={busy || (uiMode === "expert" && !!tqlErr)} data-testid="search-submit"
        >検索</Button
      >
    </form>

    <div
      class="mt-3 min-h-0 flex-1 overflow-y-auto border-t border-border px-4 py-2"
      data-testid="search-results-scroll"
      onscroll={onScroll}
    >
      {#each notes as note (note.id)}
        <NoteCard {note} {accountId} />
      {/each}
      {#if busy}<p class="px-1 py-2.5 text-center text-sm text-muted-foreground">読み込み中…</p>{/if}
      {#if searched && !busy && notes.length === 0 && !err}
        <p class="px-1 py-2.5 text-center text-sm text-muted-foreground">該当するノートが見つかりませんでした</p>
      {/if}
    </div>
    {#if err}
      <div class="flex-none px-4 pt-2 pb-4">
        <p class="mt-0 mb-2 text-sm text-destructive">{err}</p>
        <Button variant="outline" size="sm" onclick={loadMore} disabled={busy}>再試行</Button>
      </div>
    {/if}
  </div>
</Modal>
