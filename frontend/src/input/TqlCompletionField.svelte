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

  // フィールドがフォーカスされていない間は補完を出さない(未入力・未フォーカスの初期状態でも
  // Query モードは cursorPos=0 が「from を出すべき文脈」に一致してしまうため、
  // フォーカスなしでポップアップが出ないよう明示的にガードする)。
  const trigger = $derived<TqlTrigger | null>(
    !focused || composing || cursorPos === suppressAt
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
    if (!focused || composing || cursorPos === suppressAt || idTrigger) {
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
    class:invalid
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
    class:invalid
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

<style>
  textarea {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    font-family: ui-monospace, "Cascadia Code", "SF Mono", monospace;
    font-size: 0.82rem;
    resize: vertical;
  }
  input {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
  }
  textarea.invalid,
  input.invalid {
    border-color: var(--danger);
  }
</style>
