<script lang="ts">
  import type { Instance as FlatpickrInstance } from "flatpickr/dist/types/instance";
  import { datePicker } from "../lib/flatpickrDatePicker";
  import { app } from "../lib/store.svelte";
  import AccountSelect from "./AccountSelect.svelte";
  import TqlCompletionField from "../input/TqlCompletionField.svelte";
  import NoteCard from "./NoteCard.svelte";
  import Modal from "./Modal.svelte";
  import { Button } from "$lib/components/ui/button";
  import { X } from "@lucide/svelte";
  import type { FilterQuery, Note } from "../bindings/tauri.gen";

  let { onclose }: { onclose: () => void } = $props();

  let uiMode = $state<"guided" | "expert">("guided");
  let accountId = $state(app.defaultAccountId());
  let keyword = $state("");
  // キーワード以外は使う人が少ない想定のオプション項目のため既定では畳んでおく。
  let showAdvanced = $state(false);
  let userAcct = $state("");
  let host = $state("");
  // WebKitGTKはdatetime-local/date/timeいずれもネイティブの日付・時刻ピッカーUIが未成熟
  // （時刻が操作できない・空欄がプレースホルダー色の不一致で埋まって見える等）なので、
  // ネイティブinputではなくflatpickr（自前描画、OSウィジェットに依存しない）を使う。
  let dateFrom = $state<Date | null>(null);
  let dateTo = $state<Date | null>(null);
  let dateFromFp: FlatpickrInstance | undefined;
  let dateToFp: FlatpickrInstance | undefined;
  let tqlText = $state("");
  let tqlErr = $state<string | null>(null);
  // 検索対象。サーバー検索(Issue #430)は Misskey の notes/search に問い合わせる。
  let scope = $state<"cache" | "server">("cache");
  // サーバーが対応する検索機能。取得前・取得失敗は null（日時欄は出さない）。
  let caps = $state<{ dateRange: boolean } | null>(null);
  let capsGen = 0;
  // サーバー検索は簡単モード固定（notes/search はTQLを受け付けない）。
  const showGuided = $derived(scope === "server" || uiMode === "guided");
  const dateVisible = $derived(scope === "cache" || caps?.dateRange === true);

  let notes = $state<Note[]>([]);
  let busy = $state(false);
  let done = $state(false);
  let err = $state<string | null>(null);
  let searched = $state(false);
  let requestGen = 0;
  // 検索ボタンを押した時点の条件。追加読み込み・再試行は入力欄の現在値ではなくこれを使う
  // （欄を書き換えてからスクロールすると、別条件の結果が前の結果に連結されるため）。
  type ActiveSearch =
    | { scope: "server"; params: ReturnType<typeof serverParams> }
    | { scope: "cache"; filter: FilterQuery };
  let active: ActiveSearch | null = null;

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
    if (dateFrom) parts.push(`created_at >= ${Math.floor(dateFrom.getTime() / 1000)}`);
    if (dateTo) parts.push(`created_at <= ${Math.floor(dateTo.getTime() / 1000)}`);
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

  // サーバー検索の条件。日時は秒（Rust側でミリ秒へ変換する）。日時欄が出ていないときは
  // 下の $effect が dateFrom/dateTo を null に戻すので、そのまま使える。
  function serverParams() {
    return {
      query: keyword.trim(),
      acct: userAcct.trim() || undefined,
      host: host.trim() || undefined,
      sinceDate: dateFrom ? Math.floor(dateFrom.getTime() / 1000) : undefined,
      untilDate: dateTo ? Math.floor(dateTo.getTime() / 1000) : undefined,
    };
  }

  async function loadMore() {
    if (busy || done || !active) return;
    const search = active;
    busy = true;
    err = null;
    const myGen = requestGen;
    try {
      const untilId = notes.length > 0 ? notes[notes.length - 1].id : undefined;
      const page =
        search.scope === "server"
          ? await app.searchServerNotes(accountId, search.params, untilId, 20)
          : await app.searchCacheNotes(accountId, search.filter, untilId, 20);
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

  function resetResults() {
    requestGen++;
    active = null;
    notes = [];
    busy = false;
    done = false;
    err = null;
    searched = false;
  }

  // サーバー検索はキーワード必須（notes/search の query は必須）。キャッシュ検索は従来どおり
  // エキスパートモードでTQLエラーがある間だけ不可。
  function canSearch(): boolean {
    if (scope === "server") return keyword.trim() !== "";
    return !(uiMode === "expert" && tqlErr);
  }

  function setScope(next: "cache" | "server") {
    if (scope === next) return;
    scope = next;
    resetResults();
  }

  function runSearch(e: Event) {
    e.preventDefault();
    if (!canSearch()) return;
    resetResults();
    active =
      scope === "server"
        ? { scope: "server", params: serverParams() }
        : { scope: "cache", filter: { kind: "tql", value: currentPredicate() } };
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

  // サーバー検索のときだけ、問い合わせ先アカウントのサーバーが対応する検索機能を取得する。
  // 取得失敗は握りつぶして caps=null のまま（日時欄も注記も出さない）。
  $effect(() => {
    const id = accountId;
    const gen = ++capsGen;
    caps = null;
    if (scope !== "server") return;
    app.getSearchCapabilities(id).then(
      (c) => {
        if (gen === capsGen) caps = c;
      },
      () => {},
    );
  });

  // 日時欄が出ていない間は、残っている日時の値を捨てる（古い値を送らないため）。
  $effect(() => {
    if (!dateVisible) {
      dateFrom = null;
      dateTo = null;
    }
  });

  // サーバー検索は問い合わせ先がアカウントのサーバーなので、アカウントを変えたら結果を作り直す。
  // キャッシュ検索の結果はアカウントに依存しないため従来どおり保持する。
  let prevAccountId: string | undefined;
  $effect(() => {
    const id = accountId;
    if (scope === "server" && prevAccountId !== undefined && id !== prevAccountId) resetResults();
    prevAccountId = id;
  });
</script>

<Modal title="検索" {onclose} width="620px">
  <!-- Modal.svelte のp-4を打ち消してフォーム+結果を1つの高さ制限付きフレックス列にする
       (Settings.svelte と同じ「-mx-4 -mb-4 + max-h + overflow-hidden」パターン)。
       ウィンドウが低くてもモーダル全体が画面外にはみ出さず、結果欄は残り空間いっぱいに
       広がる（固定max-hだったこれまでは常に狭かった）。 -->
  <div class="-mx-4 -mb-4 flex max-h-[calc(84vh-3rem)] flex-col overflow-hidden rounded-b-[11px]">
    <form onsubmit={runSearch} class="flex flex-none flex-col gap-2.5 px-4">
      <div class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground"
          >{scope === "server"
            ? "アカウント（このアカウントのサーバーに検索を問い合わせます）"
            : "アカウント（検索結果の操作に使用。検索条件には影響しません）"}</span
        >
        <AccountSelect bind:value={accountId} accounts={app.accounts} showLabel />
      </div>

      <!-- 検索対象と簡単/エキスパートの切替は、縦に2段積まず1行に並べる（狭い幅では折り返す） -->
      <div class="flex flex-wrap items-center gap-2">
        <div
          class="flex items-center gap-0 overflow-hidden rounded-lg border border-border text-sm"
          role="group"
          aria-label="検索対象"
        >
          <button
            type="button"
            class={scope === "cache"
              ? "border-r border-border bg-primary px-3.5 py-1.5 text-primary-foreground"
              : "border-r border-border bg-muted px-3.5 py-1.5 text-foreground"}
            onclick={() => setScope("cache")}
          >キャッシュ</button>
          <button
            type="button"
            class={scope === "server"
              ? "bg-primary px-3.5 py-1.5 text-primary-foreground"
              : "bg-muted px-3.5 py-1.5 text-foreground"}
            onclick={() => setScope("server")}
          >サーバー</button>
        </div>

        {#if scope === "cache"}
          <div class="flex items-center gap-0 overflow-hidden rounded-lg border border-border text-sm">
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
        {/if}
      </div>

      {#if showGuided}
        <label class="flex flex-col gap-1 text-sm">
          <span class="text-muted-foreground">{scope === "server" ? "キーワード（必須）" : "キーワード"}</span>
          <input
            class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
            placeholder="本文に含まれる語"
            bind:value={keyword}
          />
        </label>

        <button
          type="button"
          class="self-start text-xs text-muted-foreground underline"
          onclick={() => (showAdvanced = !showAdvanced)}
        >{showAdvanced
          ? "詳細条件を隠す"
          : `詳細条件を指定（ユーザー・インスタンス${dateVisible ? "・日時" : ""}）`}</button>

        {#if showAdvanced}
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
              placeholder={scope === "server"
                ? "misskey.example（空欄で全インスタンス。自インスタンスは . かホスト名）"
                : "misskey.example（空欄で全インスタンス対象）"}
              bind:value={host}
            />
          </label>
          {#if dateVisible}
          <div class="flex gap-2.5">
            <label class="flex flex-1 flex-col gap-1 text-sm">
              <span class="text-muted-foreground">日時（開始）</span>
              <div class="flex gap-1.5">
                <input
                  type="text"
                  readonly
                  class="w-0 flex-1 rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
                  placeholder="未指定"
                  use:datePicker={{
                    defaultHour: 0,
                    onChange: (d) => (dateFrom = d),
                    onCreate: (fp) => (dateFromFp = fp),
                  }}
                />
                <Button
                  type="button"
                  variant="outline"
                  size="icon-xs"
                  onclick={() => dateFromFp?.clear()}
                  disabled={!dateFrom}
                  title="クリア"
                ><X size={14} /></Button>
              </div>
            </label>
            <label class="flex flex-1 flex-col gap-1 text-sm">
              <span class="text-muted-foreground">日時（終了）</span>
              <div class="flex gap-1.5">
                <input
                  type="text"
                  readonly
                  class="w-0 flex-1 rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
                  placeholder="未指定"
                  use:datePicker={{
                    defaultHour: 23,
                    onChange: (d) => (dateTo = d),
                    onCreate: (fp) => (dateToFp = fp),
                  }}
                />
                <Button
                  type="button"
                  variant="outline"
                  size="icon-xs"
                  onclick={() => dateToFp?.clear()}
                  disabled={!dateTo}
                  title="クリア"
                ><X size={14} /></Button>
              </div>
            </label>
          </div>
          {/if}
          {#if scope === "server" && caps && !caps.dateRange}
            <p class="mb-0 mt-0 text-xs text-muted-foreground">
              日時範囲の指定は Misskey 2025.7.0 以降のサーバーで利用できます
            </p>
          {/if}
        {/if}
      {:else}
        <label class="flex flex-col gap-1 text-sm">
          <span class="text-muted-foreground">TQL（cacheソースのwhere句。空欄で全件）</span>
          <TqlCompletionField
            mode="predicate"
            bind:value={tqlText}
            placeholder={'例: has_files && user.acct == "@alice@misskey.example"'}
            invalid={!!tqlErr}
            oninput={onTqlInput}
          />
        </label>
        {#if tqlErr}<p class="mb-0 mt-0 text-sm text-destructive break-words">TQLエラー: {tqlErr}</p>{/if}
      {/if}

      <Button type="submit" disabled={busy || !canSearch()} data-testid="search-submit"
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
