<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import ReactionPicker from "../../input/ReactionPicker.svelte";
  import UnicodeEmoji from "../../render/UnicodeEmoji.svelte";
  import { isCustomEmojiKey, customEmojiPinKey, parseCustomEmojiPinKey } from "../../lib/emojiKey";
  import { X, GripVertical, Plus } from "@lucide/svelte";

  const accountId = $derived(app.defaultAccountId());
  const accountHost = $derived(app.accounts.find((a) => a.id === accountId)?.host);
  const pinned = $derived(app.ui.pinnedEmojis ?? []);
  let picking = $state(false);
  let err = $state<string | null>(null);

  // ドラッグ中はローカルの並び順を先行して見せ、ドロップ確定時にまとめて永続化する。
  let dragOrder = $state<string[] | null>(null);
  let draggingIndex = $state<number | null>(null);
  let activePointerId: number | null = null;
  let didReorder = false;
  const displayOrder = $derived(dragOrder ?? pinned);

  $effect(() => {
    if (accountId) app.loadEmojis(accountId).catch(() => {});
  });

  function customEmojiByName(name: string) {
    return (app.emojis[accountId] ?? []).find((e) => e.name === name);
  }

  async function apply(next: string[]) {
    err = null;
    try {
      await app.setPinnedEmojis(next);
    } catch (e) {
      err = String(e);
    }
  }

  function remove(index: number) {
    void apply(pinned.filter((_, i) => i !== index));
  }

  function add(key: string) {
    picking = false;
    // カスタム絵文字は追加元アカウントのインスタンス(host)を焼き込んで保存する。ピン留めは
    // 全アカウント共通のグローバル設定のため、host無しだと複数インスタンス利用時に同名の
    // 別絵文字と衝突しうる(lib/emojiKey.ts 参照)。
    const stored = isCustomEmojiKey(key) && accountHost ? customEmojiPinKey(parseCustomEmojiPinKey(key).name, accountHost) : key;
    if (pinned.includes(stored)) return;
    void apply([...pinned, stored]);
  }

  // HTML5 Drag-and-Drop APIはタッチ入力ではdragstart等が発火せず、Android(WebView)で
  // 並べ替えが動作しないため、マウス/タッチ両対応のPointer Eventsで実装する。
  function onPointerDown(i: number, e: PointerEvent) {
    if (e.pointerType === "mouse" && e.button !== 0) return;
    activePointerId = e.pointerId;
    draggingIndex = i;
    dragOrder = pinned;
    didReorder = false;
    // キャプチャ先はgrip自身ではなく.listにする。gripは並べ替えで再配置される
    // chip内の要素のため、キャプチャ後にDOM移動が起きると一部環境でキャプチャが
    // 外れ、ドラッグが途中で止まる恐れがある。.listは並べ替えで動かないため安全。
    (e.currentTarget as HTMLElement).closest<HTMLElement>(".list")?.setPointerCapture(e.pointerId);
    e.preventDefault();
  }

  function onPointerMove(e: PointerEvent) {
    if (draggingIndex === null || e.pointerId !== activePointerId || !dragOrder) return;
    e.preventDefault();
    const overEl = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-chip-index]");
    const i = overEl ? Number(overEl.dataset.chipIndex) : NaN;
    if (Number.isNaN(i) || i === draggingIndex) return;
    const next = [...dragOrder];
    const [moved] = next.splice(draggingIndex, 1);
    next.splice(i, 0, moved);
    dragOrder = next;
    draggingIndex = i;
    didReorder = true;
  }

  function onPointerEnd(e: PointerEvent) {
    if (e.pointerId !== activePointerId) return;
    activePointerId = null;
    draggingIndex = null;
    if (didReorder && dragOrder) void apply(dragOrder);
    dragOrder = null;
  }
</script>

<h3 class="mb-1.5 mt-0 text-base font-semibold">リアクション</h3>
<p class="mb-3.5 mt-0 text-sm text-muted-foreground">絵文字ピッカーの「ピン留め」タブに表示する絵文字を編集できます(本家Misskeyのピン留め絵文字に相当)。ドラッグで並べ替えられます。</p>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="list flex flex-wrap items-center gap-2" onpointermove={onPointerMove} onpointerup={onPointerEnd} onpointercancel={onPointerEnd}>
  {#each displayOrder as key, i (key)}
    {@const custom = isCustomEmojiKey(key) ? customEmojiByName(parseCustomEmojiPinKey(key).name) : null}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class={draggingIndex === i
        ? "flex items-center gap-1 rounded-lg border border-border bg-muted px-1.5 py-1 opacity-40"
        : "flex items-center gap-1 rounded-lg border border-border bg-muted px-1.5 py-1"}
      data-chip-index={i}
    >
      <span class="-my-1 flex touch-none cursor-grab items-center justify-center p-2 text-muted-foreground" onpointerdown={(e) => onPointerDown(i, e)} title="ドラッグで並べ替え">
        <GripVertical size={12} />
      </span>
      <span class="flex text-lg leading-none">
        {#if isCustomEmojiKey(key)}
          {#if custom}
            <img class="h-[1.3em] w-[1.3em] object-contain" src={custom.url} alt={key} />
          {:else}
            {parseCustomEmojiPinKey(key).name}
          {/if}
        {:else}
          <UnicodeEmoji char={key} />
        {/if}
      </span>
      <button type="button" class="flex rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground" onclick={() => remove(i)} title="削除"><X size={12} /></button>
    </div>
  {/each}
  <button type="button" class="flex h-[34px] w-[34px] items-center justify-center rounded-lg border border-dashed border-border text-muted-foreground hover:border-primary hover:text-primary" onclick={() => (picking = !picking)} title="ピン留めを追加">
    <Plus size={16} />
  </button>
</div>
{#if pinned.length === 0}
  <p class="mb-3.5 mt-0 text-sm text-muted-foreground">ピン留めがありません。「＋」から追加できます。</p>
{/if}

{#if picking}
  <div class="mt-3">
    <ReactionPicker {accountId} showPinned={false} onpick={add} />
  </div>
{/if}
{#if err}<p class="mt-2 mb-0 text-sm text-destructive">{err}</p>{/if}
