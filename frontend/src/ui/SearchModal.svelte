<script lang="ts">
  import flatpickr from "flatpickr";
  import "flatpickr/dist/flatpickr.min.css";
  import { Japanese } from "flatpickr/dist/l10n/ja.js";
  import type { Instance as FlatpickrInstance } from "flatpickr/dist/types/instance";
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

  // flatpickrをSvelteのバインディングなしに素のinputへ被せるアクション。fpインスタンスは
  // クリアボタン(fp.clear())から使えるよう呼び出し元に返す。defaultHour/defaultMinuteは
  // 日付だけクリックして時刻を触らなかった場合の既定値（開始側は0時、終了側はその日の終わり）。
  function datePicker(
    node: HTMLInputElement,
    opts: { defaultHour: number; onChange: (d: Date | null) => void; onCreate: (fp: FlatpickrInstance) => void },
  ) {
    const fp: FlatpickrInstance = flatpickr(node, {
      enableTime: true,
      time_24hr: true,
      dateFormat: "Y-m-d H:i",
      locale: Japanese,
      defaultHour: opts.defaultHour,
      defaultMinute: opts.defaultHour === 0 ? 0 : 59,
      onChange: (dates) => opts.onChange(dates[0] ?? null),
    });
    opts.onCreate(fp);
    return {
      destroy() {
        fp.destroy();
      },
    };
  }

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

        <button
          type="button"
          class="self-start text-xs text-muted-foreground underline"
          onclick={() => (showAdvanced = !showAdvanced)}
        >{showAdvanced ? "詳細条件を隠す" : "詳細条件を指定（ユーザー・インスタンス・日時）"}</button>

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
              placeholder="misskey.example（空欄で全インスタンス対象）"
              bind:value={host}
            />
          </label>
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

<style>
  /* flatpickrはカレンダーpopupをinputの外(通常body直下)に生成するため、Svelteのscoped CSSが
     効かずすべて:globalが必要。app.cssのカラートークンに載せ替えてライト/ダーク両対応にする。 */
  :global(.flatpickr-calendar) {
    background: var(--color-popover);
    color: var(--color-popover-foreground);
    border: 1px solid var(--color-border);
    border-radius: 0.5rem;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    font-family: inherit;
  }
  :global(.flatpickr-calendar.arrowTop:before),
  :global(.flatpickr-calendar.arrowTop:after) {
    display: none;
  }
  :global(.flatpickr-months .flatpickr-month),
  :global(.flatpickr-current-month),
  :global(.flatpickr-weekday) {
    color: var(--color-popover-foreground);
    fill: var(--color-popover-foreground);
  }
  :global(.flatpickr-weekdays) {
    background: transparent;
  }
  :global(.flatpickr-day) {
    color: var(--color-popover-foreground);
  }
  :global(.flatpickr-day.flatpickr-disabled),
  :global(.flatpickr-day.prevMonthDay),
  :global(.flatpickr-day.nextMonthDay) {
    color: var(--color-muted-foreground);
  }
  :global(.flatpickr-day:hover) {
    background: var(--color-accent);
    border-color: var(--color-accent);
  }
  :global(.flatpickr-day.selected) {
    background: var(--color-primary);
    border-color: var(--color-primary);
    color: var(--color-primary-foreground);
  }
  :global(.flatpickr-day.today) {
    border-color: var(--color-primary);
  }
  /* 年/時/分の数値inputと月のselectは、WebKitGTKではネイティブ(GTKテーマ)の
     枠+背景をOSが直接描画し、background-color等のCSSを与えても無視される
     （computed styleの値自体はCSS通りになるが実際の描画には反映されない）。
     -webkit-appearance/appearance を none にしてネイティブウィジェット描画を止めないと
     常にGTKテーマの灰色のままになる。それを止めた上でbackground/borderを載せる。 */
  :global(.numInputWrapper input),
  :global(.flatpickr-time input),
  :global(.flatpickr-current-month .flatpickr-monthDropdown-months) {
    -webkit-appearance: none;
    appearance: none;
    background: var(--color-muted);
    color: var(--color-popover-foreground);
    border: 1px solid var(--color-border);
    border-radius: 0.25rem;
  }
  :global(.flatpickr-current-month .flatpickr-monthDropdown-months .flatpickr-monthDropdown-month) {
    background: var(--color-muted);
    color: var(--color-popover-foreground);
  }
  :global(.flatpickr-time .flatpickr-time-separator),
  :global(.flatpickr-time .flatpickr-am-pm) {
    color: var(--color-popover-foreground);
  }
  :global(.flatpickr-time) {
    border-top: 1px solid var(--color-border);
  }
  :global(.flatpickr-months .flatpickr-prev-month svg),
  :global(.flatpickr-months .flatpickr-next-month svg) {
    fill: var(--color-popover-foreground);
  }
</style>
