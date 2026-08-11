<script lang="ts">
  import { untrack } from "svelte";
  import { app, NOTIFY_SOUND_PRESETS, playNotifySound } from "../../lib/store.svelte";
  import Dropdown from "../Dropdown.svelte";
  import { Button } from "$lib/components/ui/button";

  let desktop = $state(app.notify.desktop);
  let sound = $state(app.notify.sound);
  let soundChoice = $state(app.notify.soundChoice ?? "");
  let pickingSound = $state(false);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  type SoundMode = "custom" | (string & {});
  function modeFromChoice(choice: string): SoundMode {
    return choice.startsWith("data:") ? "custom" : choice || "beep";
  }
  let soundMode = $state<SoundMode>(untrack(() => modeFromChoice(soundChoice)));
  const soundModeOptions = [
    ...NOTIFY_SOUND_PRESETS.map((p) => ({ value: p.id, label: p.label })),
    { value: "custom", label: "カスタム（音声ファイル）" },
  ];
  $effect(() => {
    if (soundMode === "custom") {
      if (!soundChoice.startsWith("data:")) soundChoice = "";
    } else {
      soundChoice = soundMode;
    }
  });

  async function pickSound() {
    err = null;
    pickingSound = true;
    try {
      const url = await app.pickNotifySoundFile();
      if (url) soundChoice = url;
    } catch (e) {
      err = String(e);
    } finally {
      pickingSound = false;
    }
  }

  const hasNotifyEnabledTab = $derived(
    app.groups.some((g) => g.tabs.some((t) => t.notifyDesktop || t.notifySound)),
  );

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      await app.setNotify({ desktop, sound, soundChoice });
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

<label class="mb-2 flex items-center gap-2 text-[0.88rem]"><input type="checkbox" bind:checked={desktop} /> デスクトップ通知を出す(全体スイッチ)</label>
<label class="mb-2 flex items-center gap-2 text-[0.88rem]"><input type="checkbox" bind:checked={sound} /> 通知音を鳴らす(全体スイッチ)</label>

{#if sound}
  <div class="my-1 mb-3 flex flex-col gap-1.5 text-[0.82rem]">
    <span class="text-muted-foreground">通知音の種類(既定。タブごとに上書き可)</span>
    <Dropdown bind:value={soundMode} options={soundModeOptions} />
    {#if soundMode === "custom"}
      <div class="mb-2 flex items-center gap-2 text-[0.88rem]">
        <Button type="button" variant="outline" size="xs" disabled={pickingSound} onclick={pickSound}>
          {pickingSound ? "読み込み中…" : soundChoice.startsWith("data:") ? "音声を変更" : "音声ファイルを選択"}
        </Button>
        {#if soundChoice.startsWith("data:")}
          <Button type="button" variant="outline" size="xs" onclick={() => playNotifySound(soundChoice)}>試聴</Button>
        {/if}
      </div>
    {:else}
      <Button type="button" variant="outline" size="xs" onclick={() => playNotifySound(soundMode)}>試聴</Button>
    {/if}
  </div>
{/if}

<p class="my-2 mb-4 text-[0.76rem] text-muted-foreground">
  通知は<b>通知カラムへの新着</b>、または<b>通知をONにしたタブへの新着ノート</b>で発火します。
  ここは全タブ共通のマスタースイッチで、タブごとの個別ON/OFFは各タブをダブルクリックして
  編集してください(両方ONのときのみ実際に発火します)。
  {#if !hasNotifyEnabledTab}<br /><span class="text-[var(--warning)]">※ 現在、通知がONのタブがありません。タブをダブルクリック→「このタブの通知」で有効にしてください。</span>{/if}
</p>

<div class="flex items-center justify-end gap-3">
  {#if saved}<span class="text-[0.8rem] text-[var(--success)]">保存しました</span>{/if}
  <Button type="button" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</Button>
</div>
{#if err}<p class="mt-2 mb-0 text-[0.82rem] text-destructive">{err}</p>{/if}
