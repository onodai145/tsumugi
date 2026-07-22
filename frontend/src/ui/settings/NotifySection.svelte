<script lang="ts">
  import { untrack } from "svelte";
  import { app, NOTIFY_SOUND_PRESETS, playNotifySound } from "../../lib/store.svelte";
  import Dropdown from "../Dropdown.svelte";

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

<h3 class="title">通知</h3>

<label class="row"><input type="checkbox" bind:checked={desktop} /> デスクトップ通知を出す（全体スイッチ）</label>
<label class="row"><input type="checkbox" bind:checked={sound} /> 通知音を鳴らす（全体スイッチ）</label>

{#if sound}
  <div class="field">
    <span>通知音の種類（既定。タブごとに上書き可）</span>
    <Dropdown bind:value={soundMode} options={soundModeOptions} />
    {#if soundMode === "custom"}
      <div class="row">
        <button type="button" class="mini-btn" disabled={pickingSound} onclick={pickSound}>
          {pickingSound ? "読み込み中…" : soundChoice.startsWith("data:") ? "音声を変更" : "音声ファイルを選択"}
        </button>
        {#if soundChoice.startsWith("data:")}
          <button type="button" class="mini-btn" onclick={() => playNotifySound(soundChoice)}>試聴</button>
        {/if}
      </div>
    {:else}
      <button type="button" class="mini-btn" onclick={() => playNotifySound(soundMode)}>試聴</button>
    {/if}
  </div>
{/if}

<p class="hint">
  通知は<b>通知カラムへの新着</b>、または<b>通知をONにしたタブへの新着ノート</b>で発火します。
  ここは全タブ共通のマスタースイッチで、タブごとの個別ON/OFFは各タブをダブルクリックして
  編集してください（両方ONのときのみ実際に発火します）。
  {#if !hasNotifyEnabledTab}<br /><span class="warn">※ 現在、通知がONのタブがありません。タブをダブルクリック→「このタブの通知」で有効にしてください。</span>{/if}
</p>

<div class="actions">
  {#if saved}<span class="ok">保存しました</span>{/if}
  <button class="save" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</button>
</div>
{#if err}<p class="err">{err}</p>{/if}

<style>
  .title {
    margin: 0 0 14px;
    font-size: 1rem;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 0.88rem;
    margin-bottom: 8px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 4px 0 12px;
    font-size: 0.82rem;
  }
  .field > span {
    color: var(--text-dim);
  }
  .mini-btn {
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    font-size: 0.8rem;
  }
  .mini-btn:hover {
    border-color: var(--accent);
  }
  .mini-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .hint {
    font-size: 0.76rem;
    color: var(--text-dim);
    margin: 8px 0 16px;
  }
  .warn {
    color: var(--warning);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 12px;
  }
  .ok {
    font-size: 0.8rem;
    color: var(--success);
  }
  .save {
    padding: 7px 18px;
    border: none;
    border-radius: 6px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    cursor: pointer;
  }
  .save:disabled {
    opacity: 0.5;
  }
  .err {
    color: var(--danger);
    font-size: 0.82rem;
    margin: 8px 0 0;
  }
</style>
