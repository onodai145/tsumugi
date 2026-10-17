<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { app, NOTIFY_SOUND_PRESETS } from "../../lib/store.svelte";
  import { commands, playNotifySound } from "../../lib/ipc";
  import { MML_PREFIX } from "../../lib/soundChoice";
  import { TIMBRE_LIST } from "../../lib/mmlTimbres";
  import Dropdown from "../Dropdown.svelte";
  import { Button } from "$lib/components/ui/button";

  let {
    value = $bindable(""),
    allowInherit = false,
    onvalidity,
  }: {
    value: string;
    allowInherit?: boolean;
    /// MML モードで構文が通っていない間は true。親は保存ボタンを止める。
    onvalidity?: (invalid: boolean) => void;
  } = $props();

  type Mode = "inherit" | "custom" | "mml" | (string & {});
  function modeFromChoice(choice: string): Mode {
    if (!choice) return allowInherit ? "inherit" : "beep";
    if (choice.startsWith("data:")) return "custom";
    if (choice.startsWith(MML_PREFIX)) return "mml";
    return choice;
  }

  let mode = $state<Mode>(untrack(() => modeFromChoice(value)));
  let mmlText = $state(untrack(() => (value.startsWith(MML_PREFIX) ? value.slice(MML_PREFIX.length) : "")));
  let pickingSound = $state(false);
  let pickErr = $state<string | null>(null);
  let previewErr = $state<string | null>(null);

  const modeOptions = $derived([
    ...(allowInherit ? [{ value: "inherit", label: "継承（グローバル設定）" }] : []),
    ...NOTIFY_SOUND_PRESETS.map((p) => ({ value: p.id, label: p.label })),
    { value: "custom", label: "カスタム（音声ファイル）" },
    { value: "mml", label: "MML（打ち込み）" },
  ]);

  $effect(() => {
    if (mode === "inherit") value = "";
    else if (mode === "custom") {
      if (!value.startsWith("data:")) value = "";
    } else if (mode === "mml") value = MML_PREFIX + mmlText;
    else value = mode;
  });

  type MmlStatus =
    | { kind: "idle" }
    | { kind: "pending" }
    | { kind: "ok"; totalSec: number }
    | { kind: "error"; position: number; message: string }
    // IPC 失敗など、構文の判定そのものができなかった場合(位置は無い)
    | { kind: "failed"; message: string };
  let status = $state<MmlStatus>({ kind: "idle" });

  // 入力が止まって 300ms 後にパースする(初回だけ即時)。古い結果は捨てる。
  $effect(() => {
    if (mode !== "mml") return;
    const text = mmlText;
    if (!text.trim()) {
      status = { kind: "error", position: 0, message: "MML を入力してください" };
      return;
    }
    const first = untrack(() => status.kind === "idle");
    status = { kind: "pending" };
    const timer = setTimeout(
      async () => {
        try {
          const r = await commands.parseMml(text);
          if (text !== mmlText) return;
          status =
            r.status === "parsed"
              ? { kind: "ok", totalSec: r.score.totalSec ?? 0 }
              : { kind: "error", position: r.error.position, message: r.error.message };
        } catch (e) {
          if (text !== mmlText) return;
          status = { kind: "failed", message: String(e) };
        }
      },
      first ? 0 : 300,
    );
    return () => clearTimeout(timer);
  });

  $effect(() => {
    onvalidity?.(mode === "mml" && status.kind !== "ok");
  });
  onMount(() => () => onvalidity?.(false));

  async function pickSound() {
    pickErr = null;
    pickingSound = true;
    try {
      const url = await app.pickNotifySoundFile();
      if (url) value = url;
    } catch (e) {
      pickErr = String(e);
    } finally {
      pickingSound = false;
    }
  }

  function preview(choice: string) {
    previewErr = null;
    playNotifySound(choice, (e) => (previewErr = String(e)));
  }
</script>

<Dropdown bind:value={mode} options={modeOptions} />
{#if mode === "custom"}
  <div class="flex items-center gap-2.5">
    <Button type="button" variant="outline" size="sm" disabled={pickingSound} onclick={pickSound}>
      {pickingSound ? "読み込み中…" : value.startsWith("data:") ? "音声を変更" : "音声ファイルを選択"}
    </Button>
    {#if value.startsWith("data:")}
      <Button type="button" variant="outline" size="sm" onclick={() => preview(value)}>試聴</Button>
    {/if}
  </div>
{:else if mode === "mml"}
  <textarea
    class="resize-y rounded-lg border border-border bg-muted px-2.5 py-2 font-mono text-xs text-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
    rows="3"
    spellcheck="false"
    placeholder="t120 o5 l8 cdefgab>c"
    bind:value={mmlText}
    data-testid="mml-input"
  ></textarea>
  <p class="m-0 text-xs {status.kind === 'error' || status.kind === 'failed' ? 'text-destructive' : 'text-muted-foreground'}" data-testid="mml-status">
    {#if status.kind === "ok"}再生時間 {status.totalSec.toFixed(1)} 秒
    {:else if status.kind === "error"}{status.position + 1} 文字目: {status.message}
    {:else if status.kind === "failed"}構文を確認できませんでした: {status.message}
    {:else}確認中…{/if}
  </p>
  <div class="flex items-center gap-2.5">
    <Button type="button" variant="outline" size="sm" disabled={status.kind !== "ok"} onclick={() => preview(value)}>試聴</Button>
  </div>
  <details class="text-xs text-muted-foreground">
    <summary class="cursor-pointer">記法</summary>
    <pre class="m-0 mt-1 whitespace-pre-wrap font-mono">音符 c d e f g a b（# や + で半音上げ、- で半音下げ）
長さ 音符の後ろに 1 2 4 8 16 32 64、付点は .（例: c8. ）
休符 r4  タイ c4&c8
o4 オクターブ指定  &gt; で上げる  &lt; で下げる
l8 既定の長さ  t120 テンポ  v10 音量(0〜15)
@3 または @bell 音色（下の一覧。何も書かなければ @0 fm）
[cde]3 繰り返し(2〜99回、入れ子は不可)
; でトラックを分ける(最大4つ。テンポ・音色はトラックごとに書く)
全体で 5000 文字・90 秒・3000 音まで</pre>
    <p class="m-0 mt-1 font-mono" data-testid="mml-timbres">音色: {#each TIMBRE_LIST as p, i (p.number)}{i > 0 ? " / " : ""}@{p.number} {p.name}{/each}</p>
  </details>
{:else if mode !== "inherit"}
  <Button type="button" variant="outline" size="sm" onclick={() => preview(mode)}>試聴</Button>
{/if}
{#if pickErr}<p class="m-0 text-xs text-destructive">{pickErr}</p>{/if}
{#if previewErr}<p class="m-0 text-xs text-destructive">試聴に失敗しました: {previewErr}</p>{/if}
