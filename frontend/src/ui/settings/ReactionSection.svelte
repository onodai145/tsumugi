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

<h3 class="title">リアクション</h3>
<p class="hint">絵文字ピッカーの「ピン留め」タブに表示する絵文字を編集できます（本家Misskeyのピン留め絵文字に相当）。ドラッグで並べ替えられます。</p>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="list" onpointermove={onPointerMove} onpointerup={onPointerEnd} onpointercancel={onPointerEnd}>
  {#each displayOrder as key, i (key)}
    {@const custom = isCustomEmojiKey(key) ? customEmojiByName(parseCustomEmojiPinKey(key).name) : null}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="chip" class:dragging={draggingIndex === i} data-chip-index={i}>
      <span class="grip" onpointerdown={(e) => onPointerDown(i, e)} title="ドラッグで並べ替え">
        <GripVertical size={12} />
      </span>
      <span class="glyph">
        {#if isCustomEmojiKey(key)}
          {#if custom}
            <img src={custom.url} alt={key} />
          {:else}
            {parseCustomEmojiPinKey(key).name}
          {/if}
        {:else}
          <UnicodeEmoji char={key} />
        {/if}
      </span>
      <button class="icon-btn" onclick={() => remove(i)} title="削除"><X size={12} /></button>
    </div>
  {/each}
  <button class="add-btn" onclick={() => (picking = !picking)} title="ピン留めを追加">
    <Plus size={16} />
  </button>
</div>
{#if pinned.length === 0}
  <p class="hint">ピン留めがありません。「＋」から追加できます。</p>
{/if}

{#if picking}
  <div class="picker-wrap">
    <ReactionPicker {accountId} showPinned={false} onpick={add} />
  </div>
{/if}
{#if err}<p class="err">{err}</p>{/if}

<style>
  .title {
    margin: 0 0 6px;
    font-size: 1rem;
    font-weight: 600;
  }
  .hint {
    margin: 0 0 14px;
    font-size: 0.8rem;
    color: var(--text-dim);
  }
  .list {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .chip {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
  }
  .chip.dragging {
    opacity: 0.4;
  }
  .grip {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-dim);
    cursor: grab;
    touch-action: none;
    /* アイコン自体は小さいが、実サイズのpaddingで当たり判定を広げる
       (負のmarginで打ち消す方式は隣接する削除ボタン等と当たり判定が
       重なってしまうため使わない)。 */
    padding: 8px;
    margin: -4px 0;
  }
  .glyph {
    font-size: 1.2rem;
    line-height: 1;
    display: flex;
  }
  .glyph img {
    height: 1.3em;
    width: 1.3em;
    object-fit: contain;
  }
  .icon-btn {
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    padding: 2px;
    border-radius: 4px;
    display: flex;
  }
  .icon-btn:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .add-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    border: 1px dashed var(--border);
    border-radius: 8px;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
  .add-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .picker-wrap {
    margin-top: 12px;
  }
  .err {
    color: var(--danger);
    font-size: 0.82rem;
    margin: 8px 0 0;
  }
</style>
