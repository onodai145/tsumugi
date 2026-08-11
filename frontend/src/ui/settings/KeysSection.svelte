<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import {
    ACTIONS,
    effectiveChord,
    prettyChord,
    eventToChord,
    isModifierOnly,
    type KeyAction,
  } from "../../lib/keymap";
  import { Button } from "$lib/components/ui/button";

  let capturing = $state<KeyAction | null>(null);
  let err = $state<string | null>(null);
  let busy = $state(false);

  const overrides = $derived(app.ui.keymap ?? {});
  const isCustom = (action: KeyAction) => overrides[action] !== undefined;

  // 追加で固定の操作（キーマップ外・変更不可）
  const fixed: { combo: string; desc: string }[] = [
    { combo: "Ctrl / ⌘ + Enter", desc: "投稿する（投稿バー・投稿フォーム）" },
    { combo: "Esc", desc: "モーダル／リアクションピッカーを閉じる" },
  ];

  function startCapture(action: KeyAction) {
    err = null;
    capturing = action;
  }
  function cancel() {
    capturing = null;
  }

  // キャプチャ中は capture フェーズでキーを横取りして chord を確定する
  $effect(() => {
    if (!capturing) return;
    const action = capturing;
    const handler = (e: KeyboardEvent) => {
      if (isModifierOnly(e)) return; // 修飾キー単体は待つ
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        cancel();
        return;
      }
      void assign(action, eventToChord(e));
    };
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  });

  async function assign(action: KeyAction, chord: string) {
    // Esc は予約（モーダル/ピッカーを閉じる）
    if (chord === "escape") {
      err = "Esc は予約済みのため割り当てできません";
      return;
    }
    // 重複チェック（他アクションの実効キーと衝突しないか）
    const conflict = ACTIONS.find((a) => a.action !== action && effectiveChord(a.action, overrides) === chord);
    if (conflict) {
      err = `そのキーは「${conflict.label}」に割り当て済みです`;
      return;
    }
    const next = { ...overrides };
    const def = ACTIONS.find((a) => a.action === action)!.default;
    if (chord === def) delete next[action];
    else next[action] = chord;
    await save(next);
    capturing = null;
  }

  async function resetOne(action: KeyAction) {
    const next = { ...overrides };
    delete next[action];
    await save(next);
  }
  async function resetAll() {
    await save({});
  }

  async function save(next: Record<string, string>) {
    err = null;
    busy = true;
    try {
      await app.setKeymap(next);
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="mb-2 flex items-center justify-between">
  <h3 class="m-0 text-base font-semibold">キー操作</h3>
  <Button type="button" variant="outline" size="xs" disabled={busy || Object.keys(overrides).length === 0} onclick={resetAll}>
    すべて既定に戻す
  </Button>
</div>
<p class="my-1.5 mb-2.5 text-[0.76rem] text-muted-foreground">
  「変更」を押して割り当てたいキーを押してください(Esc でキャンセル)。タイムライン上でフォーカス中カラムの選択ノートを操作します。
</p>

<table class="my-1.5 w-full border-collapse text-[0.84rem]">
  <tbody>
    {#each ACTIONS as a (a.action)}
      <tr>
        <td class="w-[34%] whitespace-nowrap border-b border-border px-1.5 py-[5px] align-middle">
          {#if capturing === a.action}
            <span class="text-[0.8rem] text-primary">キー入力待ち…</span>
          {:else}
            <kbd class="inline-block rounded-[5px] border border-b-2 border-border bg-muted px-[7px] py-0.5 font-[ui-monospace,monospace] text-[0.78rem]">{prettyChord(effectiveChord(a.action, overrides))}</kbd>
            {#if isCustom(a.action)}<span class="ml-1.5 text-[0.68rem] text-primary">変更済</span>{/if}
          {/if}
        </td>
        <td class="border-b border-border px-1.5 py-[5px] align-middle text-foreground">{a.label}</td>
        <td class="w-[22%] whitespace-nowrap border-b border-border px-1.5 py-[5px] text-right align-middle">
          {#if capturing === a.action}
            <button type="button" class="ml-1 rounded-md border border-border bg-background px-2.5 py-[3px] text-[0.76rem] text-foreground" onclick={cancel}>キャンセル</button>
          {:else}
            <button type="button" class="ml-1 rounded-md border border-border bg-background px-2.5 py-[3px] text-[0.76rem] text-foreground disabled:cursor-default disabled:opacity-40" disabled={busy} onclick={() => startCapture(a.action)}>変更</button>
            {#if isCustom(a.action)}
              <button type="button" class="ml-1 rounded-md border border-border bg-background px-2.5 py-[3px] text-[0.76rem] text-foreground disabled:cursor-default disabled:opacity-40" disabled={busy} onclick={() => resetOne(a.action)}>既定</button>
            {/if}
          {/if}
        </td>
      </tr>
    {/each}
  </tbody>
</table>

{#if err}<p class="my-1.5 text-[0.82rem] text-destructive">{err}</p>{/if}

<div class="mt-3.5">
  <div class="mb-0.5 text-[0.74rem] text-muted-foreground">固定(変更不可)</div>
  <table class="my-1.5 w-full border-collapse text-[0.84rem]">
    <tbody>
      {#each fixed as f (f.combo)}
        <tr>
          <td class="w-[34%] whitespace-nowrap border-b border-border px-1.5 py-[5px] align-middle"><kbd class="inline-block rounded-[5px] border border-b-2 border-border bg-muted px-[7px] py-0.5 font-[ui-monospace,monospace] text-[0.78rem]">{f.combo}</kbd></td>
          <td class="border-b border-border px-1.5 py-[5px] align-middle text-foreground">{f.desc}</td>
          <td class="w-[22%] whitespace-nowrap border-b border-border px-1.5 py-[5px] text-right align-middle"></td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
