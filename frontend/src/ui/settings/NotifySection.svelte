<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import SoundChoicePicker from "./SoundChoicePicker.svelte";
  import { Button } from "$lib/components/ui/button";

  let desktop = $state(app.notify.desktop);
  let sound = $state(app.notify.sound);
  let soundChoice = $state(app.notify.soundChoice ?? "");
  let soundInvalid = $state(false);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  // ピッカーが非表示(通知音オフ)の間は、入力途中の未検証な下書きではなく保存済みの値を送る。
  const effectiveSoundChoice = $derived(sound ? soundChoice : (app.notify.soundChoice ?? ""));

  const hasNotifyEnabledTab = $derived(
    app.groups.some((g) => g.tabs.some((t) => t.notifyDesktop || t.notifySound)),
  );

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      await app.setNotify({ desktop, sound, soundChoice: effectiveSoundChoice });
      desktop = app.notify.desktop; // 権限拒否で false に戻る場合を反映
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h3 class="mb-3.5 mt-0 text-base font-semibold">通知</h3>

<label class="mb-2 flex items-center gap-2 text-sm"><input type="checkbox" bind:checked={desktop} /> デスクトップ通知を出す(全体スイッチ)</label>
<label class="mb-2 flex items-center gap-2 text-sm"><input type="checkbox" bind:checked={sound} /> 通知音を鳴らす(全体スイッチ)</label>

{#if sound}
  <div class="my-1 mb-3 flex flex-col gap-1.5 text-sm">
    <span class="text-muted-foreground">通知音の種類(既定。タブごとに上書き可)</span>
    <SoundChoicePicker bind:value={soundChoice} onvalidity={(v) => (soundInvalid = v)} />
  </div>
{/if}

<p class="my-2 mb-4 text-xs text-muted-foreground">
  通知は<b>通知カラムへの新着</b>、または<b>通知をONにしたタブへの新着ノート</b>で発火します。
  ここは全タブ共通のマスタースイッチで、タブごとの個別ON/OFFは各タブをダブルクリックして
  編集してください(両方ONのときのみ実際に発火します)。
  {#if !hasNotifyEnabledTab}<br /><span class="text-[var(--warning)]">※ 現在、通知がONのタブがありません。タブをダブルクリック→「このタブの通知」で有効にしてください。</span>{/if}
</p>

<div class="flex items-center justify-end gap-3">
  {#if saved}<span class="text-sm text-[var(--success)]">保存しました</span>{/if}
  <Button type="button" disabled={busy || soundInvalid} onclick={save}>{busy ? "保存中…" : "保存"}</Button>
</div>
{#if err}<p class="mt-2 mb-0 text-sm text-destructive">{err}</p>{/if}
