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
  // 検索対象。サーバー検索(Issue #430)は Misskey の notes/search に問い合わせる。
  let scope = $state<"cache" | "server">("cache");
  // サーバーが対応する検索機能。取得前・取得失敗は null（日時欄は出さない）。
  let caps = $state<{ dateRange: boolean } | null>(null);
  let capsGen = 0;
  // サーバー検索は簡単モード固定（notes/search はTQLを受け付けない）。
  const showGuided = $derived(scope === "server" || uiMode === "guided");
  const dateVisible = $derived(scope === "cache" || caps?.dateRange === true);

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
      // flatpickr本体はヘッダを常に「月セレクト→年input」の順でDOM生成し、これを入れ替える
      // 設定は無い。buildMonths()自体は初期化時に1度しか呼ばれない(月送りは値の更新のみ)
      // ので、onReadyで年側のwrapperを月セレクトの前に差し替えれば以後も維持される。
      onReady: (_selectedDates, _dateStr, instance) => {
        const yearWrapper = instance.currentYearElement.closest(".numInputWrapper");
        const monthSelect = instance.monthsDropdownContainer;
        if (yearWrapper && monthSelect?.parentElement) {
          monthSelect.parentElement.insertBefore(yearWrapper, monthSelect);
        }
      },
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

      <div
        class="flex items-center gap-0 self-start overflow-hidden rounded-lg border border-border text-sm"
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
      {/if}

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
  :global(.flatpickr-current-month) {
    color: var(--color-popover-foreground);
    fill: var(--color-popover-foreground);
  }
  /* flatpickr本体が `span.flatpickr-weekday { color: rgba(0,0,0,0.54); }` を要素+クラスの
     セレクタで持っており、こちらをクラス単体(.flatpickr-weekday)で上書きしても詳細度で
     負けて常に元の色が勝つ（読み込み順に関係なく曜日だけ暗いまま沈んで見えた原因）。
     詳細度を合わせるため要素セレクタを揃える。 */
  :global(span.flatpickr-weekday) {
    color: var(--color-popover-foreground);
  }
  /* 日本の慣習に合わせ土曜=青・日曜=赤にする。firstDayOfWeekが既定の0(日曜始まり)なので、
     曜日ヘッダ・日付セルとも7列グリッドの1列目=日曜・7列目=土曜になる。 */
  :global(.flatpickr-weekdaycontainer span.flatpickr-weekday:nth-child(1)) {
    color: var(--danger);
  }
  :global(.flatpickr-weekdaycontainer span.flatpickr-weekday:nth-child(7)) {
    color: var(--info);
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
  /* :not()で選択中/前後月/無効セルを除外し、cascadeの並び順に依存せず正しく上書きされる
     ようにする（土日色 vs 選択中の白文字 vs 前後月の淡色、が競合しないように）。 */
  :global(
    .flatpickr-day:nth-child(7n + 1):not(.selected):not(.prevMonthDay):not(.nextMonthDay):not(.flatpickr-disabled)
  ) {
    color: var(--danger);
  }
  :global(
    .flatpickr-day:nth-child(7n):not(.selected):not(.prevMonthDay):not(.nextMonthDay):not(.flatpickr-disabled)
  ) {
    color: var(--info);
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
  /* flatpickr本体の `.flatpickr-time input:hover, .flatpickr-time input:focus { background: #eee; }`
     は疑似クラスの分だけ上のbaseルールより詳細度が高く、フォーカス時(＝実際に時刻を
     入力しようとした瞬間)だけ強制的に#eee(明るいグレー)に戻っていた。同じ詳細度で
     上書きする。 */
  :global(.flatpickr-time input:hover),
  :global(.flatpickr-time input:focus) {
    background: var(--color-accent);
  }
  :global(.flatpickr-time) {
    border-top: 1px solid var(--color-border);
  }
  :global(.flatpickr-months .flatpickr-prev-month svg),
  :global(.flatpickr-months .flatpickr-next-month svg) {
    fill: var(--color-popover-foreground);
  }
</style>
