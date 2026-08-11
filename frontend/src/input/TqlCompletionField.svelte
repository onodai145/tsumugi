<script lang="ts">
  import { tick } from "svelte";
  import { app } from "../lib/store.svelte";
  import CompletionPopover from "../ui/CompletionPopover.svelte";
  import { getCaretCoordinates } from "../lib/caretPosition";
  import {
    applyTqlCompletion,
    charOffset,
    currentWordTrigger,
    detectIdArgTrigger,
    idCandidates,
    syntaxCandidates,
    type TqlTrigger,
  } from "../lib/tqlCompletion";
  import type { SourceItem, TqlCompletionItem, TqlEditMode, UserList } from "../bindings/tauri.gen";
  import type { CompletionItem } from "../lib/mfmCompletion";

  let {
    mode,
    value = $bindable(),
    placeholder = "",
    rows,
    invalid = false,
    oninput,
    lists = [],
    antennas = [],
    channels = [],
  }: {
    mode: TqlEditMode;
    value: string;
    placeholder?: string;
    rows?: number;
    invalid?: boolean;
    oninput?: () => void;
    lists?: UserList[];
    antennas?: SourceItem[];
    channels?: SourceItem[];
  } = $props();

  let el = $state<HTMLTextAreaElement | HTMLInputElement | undefined>(undefined);
  let cursorPos = $state(0);
  let suppressAt = $state<number | null>(null);
  let composing = $state(false);
  let focused = $state(false);
  let selectedIndex = $state(0);
  let selectionMoved = $state(false);
  let rustItems = $state<TqlCompletionItem[]>([]);
  let fetchToken = 0;

  const idTrigger = $derived(mode === "query" ? detectIdArgTrigger(value, cursorPos) : null);

  // 欄が空(バックスペースで全消しした場合も含む)の間は補完を出さない。絵文字ピッカー等と
  // 同様、何か入力されている時だけ補完する(Query モードは空文字だと cursorPos=0 が
  // 「from を出すべき文脈」に一致してしまうため、focusしただけ/全消しした直後に
  // 補完が居座らないよう明示的にガードする)。
  const hasContent = $derived(value.trim().length > 0);

  const trigger = $derived<TqlTrigger | null>(
    !focused || !hasContent || composing || cursorPos === suppressAt
      ? null
      : (idTrigger?.trigger ?? currentWordTrigger(value, cursorPos)),
  );

  // ID引数の文脈(list("...")等)ではRustを呼ばない。それ以外は都度 tql_complete を呼ぶ
  // (IPCはローカル呼び出しでネットワークを介さないため、デバウンスはせず世代カウンタで
  // 古い応答だけ無視する)。
  // なお応答待ちの間、rustItems は一つ前の文脈の候補を保持したまま新しい trigger の
  // span に対して表示されうる。ローカルIPCなのでこの窓はサブミリ秒で、応答到着時に
  // 自動的に正しい候補へ差し替わるため、ローディング状態の作り込みはあえて行わない。
  $effect(() => {
    if (!focused || !hasContent || composing || cursorPos === suppressAt || idTrigger) {
      rustItems = [];
      return;
    }
    const text = value;
    const cursor = cursorPos;
    const token = ++fetchToken;
    app
      .tqlComplete(text, charOffset(text, cursor), mode)
      .then((items) => {
        if (token === fetchToken) rustItems = items;
      })
      .catch(() => {
        if (token === fetchToken) rustItems = [];
      });
  });

  const candidates = $derived<CompletionItem[]>(
    !trigger
      ? []
      : idTrigger
        ? idCandidates(idTrigger.kind, idTrigger.query, lists, antennas, channels)
        : syntaxCandidates(rustItems),
  );
  const popoverOpen = $derived(trigger !== null && candidates.length > 0);

  // クエリ(文脈)が変わるたびに選択位置を先頭へ戻す
  $effect(() => {
    trigger;
    selectedIndex = 0;
    selectionMoved = false;
  });

  let popoverPos = $state<{ left: number; top: number } | null>(null);
  $effect(() => {
    if (!popoverOpen || !trigger || !el) {
      popoverPos = null;
      return;
    }
    const rect = el.getBoundingClientRect();
    if (el instanceof HTMLTextAreaElement) {
      const caret = getCaretCoordinates(el, trigger.start);
      popoverPos = { left: rect.left + caret.left, top: rect.top + caret.top + caret.height };
    } else {
      popoverPos = { left: rect.left, top: rect.bottom + 4 };
    }
  });

  function syncCursor() {
    const pos = el?.selectionStart ?? 0;
    if (pos !== cursorPos) suppressAt = null;
    cursorPos = pos;
  }

  async function confirmCompletion(index: number) {
    const t = trigger;
    const item = candidates[index];
    if (!t || !item) return;
    const result = applyTqlCompletion(value, t, item);
    value = result.text;
    suppressAt = result.cursor;
    await tick();
    el?.setSelectionRange(result.cursor, result.cursor);
    el?.focus();
    cursorPos = result.cursor;
  }

  function onKeydown(e: KeyboardEvent) {
    if (!popoverOpen) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selectedIndex = selectionMoved ? Math.min(selectedIndex + 1, candidates.length - 1) : 0;
      selectionMoved = true;
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIndex = selectionMoved ? Math.max(selectedIndex - 1, 0) : candidates.length - 1;
      selectionMoved = true;
      return;
    }
    if (e.key === "Tab" || e.key === "Enter") {
      if (e.key === "Enter" && !selectionMoved) {
        return; // 矢印キーで明示的に選ぶまでEnterでは確定しない(改行のつもりでEnterを押した場合の誤確定を防ぐ)
      }
      e.preventDefault();
      confirmCompletion(selectedIndex);
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      suppressAt = cursorPos;
    }
  }

  function onInputHandler() {
    syncCursor();
    suppressAt = null;
    oninput?.();
  }
</script>

{#if mode === "query"}
  <textarea
    class={invalid
      ? 'rounded-lg border border-destructive bg-muted px-2.5 py-2 font-[ui-monospace,"Cascadia_Code","SF_Mono",monospace] text-[0.82rem] text-foreground resize-y'
      : 'rounded-lg border border-border bg-muted px-2.5 py-2 font-[ui-monospace,"Cascadia_Code","SF_Mono",monospace] text-[0.82rem] text-foreground resize-y'}
    {rows}
    {placeholder}
    bind:value
    bind:this={el}
    onkeydown={onKeydown}
    onkeyup={syncCursor}
    onclick={syncCursor}
    oninput={onInputHandler}
    oncompositionstart={() => (composing = true)}
    oncompositionend={() => {
      composing = false;
      syncCursor();
    }}
    onfocus={() => {
      focused = true;
      syncCursor();
    }}
    onblur={() => {
      focused = false;
      suppressAt = cursorPos;
    }}
  ></textarea>
{:else}
  <input
    class={invalid
      ? "rounded-lg border border-destructive bg-muted px-2.5 py-2 font-[inherit] text-foreground"
      : "rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"}
    {placeholder}
    bind:value
    bind:this={el}
    onkeydown={onKeydown}
    onkeyup={syncCursor}
    onclick={syncCursor}
    oninput={onInputHandler}
    oncompositionstart={() => (composing = true)}
    oncompositionend={() => {
      composing = false;
      syncCursor();
    }}
    onfocus={() => {
      focused = true;
      syncCursor();
    }}
    onblur={() => {
      focused = false;
      suppressAt = cursorPos;
    }}
  />
{/if}

{#if popoverOpen && popoverPos}
  <CompletionPopover
    items={candidates}
    selectedIndex={selectionMoved ? selectedIndex : -1}
    left={popoverPos.left}
    top={popoverPos.top}
    onpick={confirmCompletion}
  />
{/if}
