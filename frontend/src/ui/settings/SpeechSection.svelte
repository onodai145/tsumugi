<script lang="ts">
  import SaveBar from "./SaveBar.svelte";
  import { Button } from "$lib/components/ui/button";
  import { app } from "../../lib/store.svelte";
  import { commands, unwrap, type SpeechCoreInfo, type SpeechEngineKind, type Voice } from "../../lib/ipc";
  import { isMobilePlatform } from "../../lib/platform";
  import { creditFor, installedVoices, speechErrorText } from "../../lib/speechCore";
  import SpeechCoreSection from "./SpeechCoreSection.svelte";

  // 読み上げの全体設定(Issue #625)。外部の VOICEVOX 互換エンジン(VOICEVOX ENGINE / AivisSpeech Engine)に接続するか、
  // 内蔵の VOICEVOX CORE(段階2。デスクトップのみ)を使う。
  let enabled = $state(app.speech.enabled);
  let engine = $state<SpeechEngineKind>(app.speech.engine);
  /// 内蔵エンジンの状態(`SpeechCoreSection` が埋める)。取得済みの声の選択肢とクレジットに使う。
  let coreInfo = $state<SpeechCoreInfo | null>(null);
  let externalUrl = $state(app.speech.externalUrl);
  let defaultVoiceId = $state(app.speech.defaultVoiceId);
  let speedScale = $state(app.speech.speedScale);
  let volume = $state(app.speech.volume);

  let voices = $state<Voice[]>([]);
  let connecting = $state(false);
  let connectResult = $state<string | null>(null);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);
  let actionErr = $state<string | null>(null);

  /// 失敗の理由(`unwrap` が投げる `AppError` や、ストアが投げる `Error` の message)。
  function reason(e: unknown): string {
    return e instanceof Error ? e.message : String(e);
  }

  const statusText = $derived.by(() => {
    const s = app.speechStatus;
    if (engine === "voicevoxCore") {
      if (s.type === "ok") return "内蔵エンジンで読み上げられています";
      if (s.type === "error") return `読み上げに失敗しています(${speechErrorText(s.message)})`;
      return "";
    }
    if (s.type === "ok") return "エンジンに接続できています";
    if (s.type === "error") return `エンジンに接続できません(${s.message})`;
    return "";
  });
  /// 声の選択肢。外部は接続テストで取得したもの、内蔵は取得済みの VVM の声。
  const voiceOptions = $derived(engine === "voicevoxCore" ? installedVoices(coreInfo) : voices);
  const credit = $derived(engine === "voicevoxCore" ? creditFor(coreInfo, defaultVoiceId) : null);

  /// 入力欄の URL(未保存でよい)でエンジンに接続し、選べる声を取得する。
  async function connectTest() {
    connectResult = null;
    connecting = true;
    try {
      voices = await unwrap(commands.listSpeechVoices(externalUrl.trim()));
      connectResult = `接続できました(声 ${voices.length} 件)`;
      if (!voices.some((v) => v.id === defaultVoiceId)) defaultVoiceId = voices[0]?.id ?? "";
    } catch (e) {
      voices = [];
      connectResult = `接続できませんでした: ${reason(e)}`;
    } finally {
      connecting = false;
    }
  }

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      await app.setSpeechSettings({
        enabled,
        engine,
        externalUrl: externalUrl.trim(),
        defaultVoiceId,
        speedScale: Number(speedScale),
        volume: Number(volume),
      });
      // Rust が整えた値(末尾のスラッシュ除去など)を入力欄に戻す。
      externalUrl = app.speech.externalUrl;
      saved = true;
    } catch (e) {
      err = reason(e);
    } finally {
      busy = false;
    }
  }

  /// 保存済みの設定で、固定の文を読み上げる。
  async function trySpeak() {
    actionErr = null;
    try {
      await unwrap(commands.testSpeech("こんにちは。読み上げのテストです。"));
    } catch (e) {
      actionErr = reason(e);
    }
  }

  async function stop() {
    actionErr = null;
    try {
      await unwrap(commands.stopSpeech());
    } catch (e) {
      actionErr = reason(e);
    }
  }

  const inputClass = "rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground";
</script>

<h3 class="mb-3.5 mt-0 text-base font-semibold">読み上げ</h3>

<label class="mb-2 flex items-center gap-2 text-sm">
  <input type="checkbox" data-testid="speech-enabled" bind:checked={enabled} /> 読み上げを使う(全体スイッチ)
</label>
<p class="my-1 mb-4 text-xs text-muted-foreground">
  読み上げをオンにしたカラム(カラムをダブルクリック → 「このタブの読み上げ」)に届いた新着ノートを、順番に読み上げます。
  ここは全カラム共通のマスタースイッチです(両方オンのときだけ読み上げます)。
</p>

<fieldset class="mb-3 mt-0 flex flex-wrap gap-x-4 gap-y-1 border-0 p-0 text-sm">
  <legend class="mb-1 text-muted-foreground">音声合成エンジン</legend>
  <label class="flex items-center gap-1.5">
    <input type="radio" name="speech-engine" value="external" data-testid="speech-engine-external" bind:group={engine} /> 外部のエンジン(VOICEVOX ENGINE / AivisSpeech Engine)
  </label>
  <label class="flex items-center gap-1.5">
    <input type="radio" name="speech-engine" value="voicevoxCore" data-testid="speech-engine-core" bind:group={engine} /> 内蔵の VOICEVOX CORE
  </label>
</fieldset>

{#if engine === "external"}
  <label class="mb-2 flex flex-col gap-1 text-sm">
    <span class="text-muted-foreground">音声合成エンジンの URL(VOICEVOX ENGINE / AivisSpeech Engine)</span>
    <input class={inputClass} data-testid="speech-url" placeholder="http://192.168.1.10:50021" bind:value={externalUrl} />
  </label>
  <div class="mb-2 flex items-center gap-2 text-sm">
    <Button variant="outline" disabled={connecting || !externalUrl.trim()} onclick={connectTest} data-testid="speech-connect-test">
      {connecting ? "接続中…" : "接続テスト"}
    </Button>
    {#if connectResult}<span class="break-words text-xs text-muted-foreground" data-testid="speech-connect-result">{connectResult}</span>{/if}
  </div>
{:else}
  <SpeechCoreSection bind:info={coreInfo} />
{/if}
{#if statusText}
  <p class="my-1 mb-3 text-xs {app.speechStatus.type === 'error' ? 'text-destructive' : 'text-muted-foreground'}" data-testid="speech-status">{statusText}</p>
{/if}
{#if engine === "external"}
  <p class="my-1 mb-4 text-xs text-muted-foreground">
    エンジンは別途起動してください。LAN 内のサーバーに接続する場合は、そのアドレスを指定します。
    {#if isMobilePlatform}
      Android では、端末の設定によって LAN 内の平文(http://)のサーバーに接続できないことがあります。接続テストが失敗する場合は、https:// の URL を指定してください。
    {/if}
  </p>
{/if}

<label class="mb-2 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">既定の声(カラムごとに上書きできます)</span>
  <select class={inputClass} data-testid="speech-voice" bind:value={defaultVoiceId}>
    {#if voiceOptions.length === 0}
      <option value={defaultVoiceId}>{defaultVoiceId ? `現在の声: ${defaultVoiceId}` : engine === "voicevoxCore" ? "声を取得してください" : "接続テストで声を取得してください"}</option>
    {:else}
      {#each voiceOptions as v (v.id)}<option value={v.id}>{v.label}</option>{/each}
    {/if}
  </select>
</label>
{#if credit}
  <p class="my-1 mb-3 text-xs text-muted-foreground" data-testid="speech-core-credit">クレジット表記: {credit}</p>
{/if}

<label class="mb-2 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">話速({speedScale})</span>
  <input type="range" min="0.5" max="2" step="0.1" data-testid="speech-speed" bind:value={speedScale} />
</label>
<label class="mb-3 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">音量({volume})</span>
  <input type="range" min="0.1" max="2" step="0.1" data-testid="speech-volume" bind:value={volume} />
</label>

<div class="mb-2 flex items-center gap-2 text-sm">
  <Button variant="outline" onclick={trySpeak} data-testid="speech-try">試し聞き</Button>
  <Button variant="outline" onclick={stop} data-testid="speech-stop">停止</Button>
  <span class="text-xs text-muted-foreground">試し聞きは保存済みの設定で読み上げます。</span>
</div>
{#if actionErr}<p class="my-1 break-words text-sm text-destructive">{actionErr}</p>{/if}

<p class="my-3 text-xs text-muted-foreground" data-testid="speech-credit">
  VOICEVOX 系の声を使った音声を公開する場合は、「VOICEVOX:キャラクター名」のクレジット表記と、各キャラクターの
  利用規約の遵守が必要です(VOICEVOX 音声モデル利用規約)。利用する声の規約を確認してください。
</p>

<SaveBar {busy} {saved} {err} onsave={save} />
