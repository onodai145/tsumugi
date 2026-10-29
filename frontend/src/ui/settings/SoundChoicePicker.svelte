<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { app, NOTIFY_SOUND_PRESETS } from "../../lib/store.svelte";
  import { commands, playNotifySound } from "../../lib/ipc";
  import { MML_PREFIX } from "../../lib/soundChoice";
  import { TIMBRE_LIST } from "../../lib/mmlTimbres";
  import { computeUsage, formatUsage, type ParsedUsage } from "../../lib/mmlUsage";
  import { MML_SAMPLES, type MmlSample } from "../../lib/mmlSamples";
  import { MML_KEY_GROUPS } from "../../lib/mmlKeys";
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
    // text は、そのエラーを返した解析の入力。編集後の古い位置に下線を引かないために持つ
    | { kind: "error"; position: number; message: string; text: string }
    // IPC 失敗など、構文の判定そのものができなかった場合(位置は無い)
    | { kind: "failed"; message: string };
  let status = $state<MmlStatus>({ kind: "idle" });
  // 直近に構文が通ったときの音数と再生時間。解析中は点滅を避けて、これを出し続ける
  let lastOk = $state<ParsedUsage | null>(null);
  let editor = $state<{ focusAt(pos: number): void; insertText(text: string, opts?: { pad?: boolean }): void }>();
  // 入力があるときにサンプルを押された場合の、置き換え確認の待ち。入力を編集したら引っ込める
  let pendingSample = $state<MmlSample | null>(null);
  // 同じ画面に複数あっても id が衝突しないように
  const uid = $props.id();

  // 入力が止まって 300ms 後にパースする(初回だけ即時)。古い結果は捨てる。
  $effect(() => {
    if (mode !== "mml") return;
    const text = mmlText;
    if (!text.trim()) {
      lastOk = null;
      status = { kind: "error", position: 0, message: "MML を入力してください", text };
      return;
    }
    const first = untrack(() => status.kind === "idle");
    status = { kind: "pending" };
    const timer = setTimeout(
      async () => {
        try {
          const r = await commands.parseMml(text);
          if (text !== mmlText) return;
          if (r.status === "parsed") {
            const totalSec = r.score.totalSec ?? 0;
            lastOk = { noteCount: r.score.notes.length, totalSec };
            status = { kind: "ok", totalSec };
          } else {
            lastOk = null;
            status = { kind: "error", position: r.error.position, message: r.error.message, text };
          }
        } catch (e) {
          if (text !== mmlText) return;
          lastOk = null;
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

  // 編集したら status は pending に変わるので、位置のずれた古い下線は残らない。text の一致は念のため
  const errorPosition = $derived(status.kind === "error" && status.text === mmlText ? status.position : null);
  const usage = $derived(computeUsage(mmlText, status.kind === "ok" || status.kind === "pending" ? lastOk : null));

  $effect(() => {
    void mmlText;
    untrack(() => (pendingSample = null));
  });

  function chooseSample(sample: MmlSample) {
    if (mmlText.trim()) pendingSample = sample;
    else mmlText = sample.mml;
  }

  function replaceWithSample() {
    if (pendingSample) mmlText = pendingSample.mml;
    pendingSample = null;
  }

  function jumpToError() {
    if (errorPosition !== null) editor?.focusAt(errorPosition);
  }

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

  // MML の試聴は、生成に数分かかることがある。生成そのものは止められない(Tone.Offline は途中で止められず、
  // 結果は soundChoice のキャッシュに入る)ので、「待たない」は結果を捨てて待つのをやめるだけ。
  // run は試聴の世代で、待たない・編集・閉じる・押し直しで進めて、古い生成を鳴らさない。
  let previewRun = 0;
  let generating = $state(false);
  let generatingSec = $state(0);

  function previewMml() {
    const run = ++previewRun;
    previewErr = null;
    generating = true;
    generatingSec = 0;
    playNotifySound(value, (e) => (previewErr = String(e)), {
      isStale: () => run !== previewRun,
      onSettled: () => {
        if (run === previewRun) generating = false;
      },
    });
  }

  function stopWaiting() {
    previewRun++;
    generating = false;
  }

  $effect(() => {
    if (!generating) return;
    const timer = setInterval(() => generatingSec++, 1000);
    return () => clearInterval(timer);
  });

  // 編集したら、前の入力の生成は捨てる。MML モードを抜けたときも同じ
  $effect(() => {
    void mmlText;
    void mode;
    untrack(() => {
      if (generating) stopWaiting();
    });
  });
  // 閉じたあとに生成が終わって、急に鳴らないように
  onMount(() => () => {
    previewRun++;
  });
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
  {#await import("./MmlEditor.svelte")}
    <div class="h-20 rounded-lg border border-border bg-muted"></div>
  {:then { default: MmlEditor }}
    <MmlEditor
      bind:this={editor}
      bind:value={mmlText}
      {errorPosition}
      describedBy="{uid}-status {uid}-usage"
      invalid={status.kind === "error" || status.kind === "failed"}
    />
    {#if app.useMobileUi()}
      <!-- ソフトキーボードで切り替えが要る記号と、音符・長さ。押してもエディタからフォーカスを奪わない(キーボードを閉じない) -->
      <div class="flex flex-wrap gap-x-3 gap-y-1.5" role="group" aria-label="入力補助" data-testid="mml-keys">
        {#each MML_KEY_GROUPS as group (group.id)}
          <div class="flex flex-wrap gap-1" role="group" aria-label={group.label}>
            {#each group.keys as key (key.id)}
              <Button
                type="button"
                variant="outline"
                size="sm"
                class="min-h-10 min-w-10 px-2 font-mono"
                aria-label={key.aria}
                onmousedown={(e) => e.preventDefault()}
                onclick={() => editor?.insertText(key.insert, { pad: false })}
                data-testid="mml-key-{key.id}">{key.label}</Button
              >
            {/each}
          </div>
        {/each}
      </div>
    {/if}
  {:catch}
    <!-- エディタの chunk を読み込めなかったとき。MML の入力手段が黙って消えないように、素の入力欄に戻す -->
    <textarea
      class="resize-y rounded-lg border border-border bg-muted px-2.5 py-2 font-mono text-xs text-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
      rows="3"
      spellcheck="false"
      aria-label="MML"
      aria-describedby="{uid}-status {uid}-usage"
      bind:value={mmlText}
      data-testid="mml-input-fallback"
    ></textarea>
    <p class="m-0 text-xs text-destructive" data-testid="mml-editor-load-error">エディタを読み込めなかったため、簡易な入力欄を表示しています。</p>
  {/await}
  <p id="{uid}-status" class="m-0 text-xs {status.kind === 'error' || status.kind === 'failed' ? 'text-destructive' : 'text-muted-foreground'}" data-testid="mml-status">
    {#if status.kind === "ok"}再生時間 {status.totalSec.toFixed(1)} 秒
    {:else if status.kind === "error"}<button
        type="button"
        class="cursor-pointer rounded-md border-0 bg-transparent p-0 text-xs text-destructive underline focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
        onclick={jumpToError}
        data-testid="mml-error-jump">{status.position + 1} 文字目</button>: {status.message}
    {:else if status.kind === "failed"}構文を確認できませんでした: {status.message}
    {:else}確認中…{/if}
  </p>
  <p id="{uid}-usage" class="m-0 flex flex-wrap gap-x-3 text-xs text-muted-foreground" data-testid="mml-usage">
    {#each usage as u (u.key)}
      <span
        data-testid="mml-usage-{u.key}"
        data-level={u.level}
        class={{ "font-bold": u.level !== "normal", "text-[var(--warning)]": u.level === "near", "text-destructive": u.level === "over" }}
      >{formatUsage(u)}</span>
    {/each}
  </p>
  <div class="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground" role="group" aria-label="サンプル" data-testid="mml-samples">
    <span aria-hidden="true">サンプル</span>
    {#each MML_SAMPLES as sample (sample.id)}
      <Button type="button" variant="outline" size="xs" onclick={() => chooseSample(sample)} data-testid="mml-sample-{sample.id}">{sample.label}</Button>
    {/each}
  </div>
  {#if pendingSample}
    <div class="flex flex-wrap items-center gap-2 text-xs text-foreground" role="group" aria-label="サンプルの置き換えの確認" aria-live="polite" data-testid="mml-sample-confirm">
      <span>入力中の内容を「{pendingSample.label}」で置き換えます。</span>
      <Button type="button" variant="destructive" size="xs" onclick={replaceWithSample} data-testid="mml-sample-confirm-ok">置き換える</Button>
      <Button type="button" variant="outline" size="xs" onclick={() => (pendingSample = null)} data-testid="mml-sample-confirm-cancel">やめる</Button>
    </div>
  {/if}
  <div class="flex items-center gap-2.5">
    <Button type="button" variant="outline" size="sm" disabled={status.kind !== "ok" || generating} onclick={previewMml} data-testid="mml-preview">{generating ? `生成中… ${generatingSec} 秒` : "試聴"}</Button>
    {#if generating}
      <Button type="button" variant="outline" size="sm" onclick={stopWaiting} data-testid="mml-preview-cancel">待たない</Button>
    {/if}
    <span class="sr-only" role="status">{generating ? "音を生成しています" : ""}</span>
  </div>
  <details class="text-xs text-muted-foreground">
    <summary class="cursor-pointer">記法</summary>
    <pre class="m-0 mt-1 whitespace-pre-wrap font-mono" data-testid="mml-syntax-help">音符 c d e f g a b（# や + で半音上げ、- で半音下げ）
長さ 音符の後ろに 1 2 4 8 16 32 64、付点は .（例: c8. ）
休符 r4  タイ c4&c8（同じ高さの音をつなぐ）  c4^8（直前の音に長さを足す）
o4 オクターブ指定  &gt; で上げる  &lt; で下げる
l8 既定の長さ  t120 テンポ  v10 音量(0〜15)  v+2 v-2 音量を相対で変える(数字を省くと 1)
t=140 全トラックの初期テンポ(最初の音符・休符・和音より前、繰り返し・連符の外に 1 回だけ。トラックごとの t で上書き)
q80 音を鳴らす長さの割合(1〜100、既定 100。短くするとスタッカート)
&#123;cde&#125;4 連符(中の長さを合計して、後ろの長さ 1 つ分に収める。入れ子は不可)
'ceg'4 和音(同時に鳴らす。長さは閉じの ' の後ろに 1 つ。中は音符と &lt; &gt; だけ。音量は音数で割る。& とは組み合わせ不可、延ばすには ^)
$riff=&#123;c d e f&#125; マクロ(文字の置き換え)。$riff で展開。定義はどこに書いてもよい。名前の直後に英数字が続くときは空白で区切る($riff 4)。本体に ; と定義は書けない
@3 または @bell 音色（下の一覧。何も書かなければ @0 fm）
[cde]3 繰り返し(2〜99回、入れ子可)  [c:d]3 最後の回だけ : の後ろを飛ばす
; でトラックを分ける(最大4つ。テンポ・音色はトラックごとに書く)
// 行末までコメント  /* … */ 範囲コメント(; を含んでもトラックは分かれない。文字数には含む)
全体で 5000 文字・90 秒・3000 音まで</pre>
    <div class="mt-1 flex flex-wrap items-center gap-1" role="group" aria-label="音色" data-testid="mml-timbres">
      <span aria-hidden="true">音色（押すとカーソル位置に入る）</span>
      {#each TIMBRE_LIST as p (p.number)}
        <!-- 押してもエディタからフォーカスを奪わない(モバイルでソフトキーボードが閉じないように) -->
        <Button
          type="button"
          variant="outline"
          size="xs"
          class="font-mono"
          aria-label="@{p.number} {p.name} を挿入"
          onmousedown={(e) => e.preventDefault()}
          onclick={() => editor?.insertText(`@${p.name}`)}
          data-testid="mml-timbre-{p.name}">@{p.number} {p.name}</Button
        >
      {/each}
    </div>
  </details>
{:else if mode !== "inherit"}
  <Button type="button" variant="outline" size="sm" onclick={() => preview(mode)}>試聴</Button>
{/if}
{#if pickErr}<p class="m-0 text-xs text-destructive">{pickErr}</p>{/if}
{#if previewErr}<p class="m-0 text-xs text-destructive">試聴に失敗しました: {previewErr}</p>{/if}
