<script lang="ts">
  import { onMount } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import { commands, events, unwrap } from "../../lib/ipc";
  import type { PackInfo, SpeechCoreInfo, SpeechCoreTerms } from "../../lib/ipc";
  import { formatBytes, statusText } from "../../lib/localModel";
  import { voiceCharactersLabel } from "../../lib/speechCore";
  import SpeechCoreTermsDialog from "./SpeechCoreTermsDialog.svelte";

  // 内蔵 VOICEVOX CORE のエンジン一式と、声(VVM)の取得・削除(Issue #625 段階2)。
  // 状態はマウント時に Rust から取り、変化は Event(スナップショット)で受ける。設定画面を閉じても取得は続く。
  // 親(SpeechSection)が、取得済みの声の選択肢とクレジットを作れるよう、`info` を公開する。
  let { info = $bindable<SpeechCoreInfo | null>(null) }: { info?: SpeechCoreInfo | null } = $props();

  let err = $state<string | null>(null);
  /// 規約の確認を出している間だけ非 null。
  let terms = $state<SpeechCoreTerms | null>(null);
  let pending = $state<{ pack: string; sizeBytes: number } | null>(null);
  let starting = $state(false);

  // Event を受けるたびに進める。refresh の応答が、それより新しい Event の状態を古いスナップショットで上書きしないために使う。
  let eventSeq = 0;

  async function refresh() {
    const seqAtRequest = eventSeq;
    const i = await unwrap(commands.getSpeechCoreInfo());
    if (i && eventSeq === seqAtRequest) info = i;
  }

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    (async () => {
      // 状態の取得より先に購読する(取得の間に届いた終了イベントを取りこぼして、表示が固まるのを避ける)。
      try {
        const u = await events.speechCoreStatusChanged.listen((e) => {
          eventSeq++;
          info = e.payload.info;
        });
        if (disposed) u();
        else unlisten = u;
      } catch (e) {
        err = String(e);
      }
      if (disposed) return;
      try {
        await refresh();
      } catch (e) {
        err = String(e);
      }
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  const busy = $derived(
    !!info &&
      [info.engine, ...info.voices.map((v) => v.pack)].some((p) => p.status.type === "downloading" || p.status.type === "verifying"),
  );
  const engineReady = $derived(info?.engine.status.type === "ready");

  async function start(pack: string) {
    if (starting || !info) return;
    err = null;
    starting = true;
    try {
      await unwrap(commands.startSpeechCoreDownload(pack, info.termsVersion));
      terms = null;
      pending = null;
      await refresh();
    } catch (e) {
      terms = null;
      pending = null;
      err = String(e);
    } finally {
      starting = false;
    }
  }

  /// 同意済みの版なら、そのまま開始する。未同意(規約が変わったときを含む)なら、規約を出して同意を取る。
  async function onGet(pack: string, sizeBytes: number) {
    if (!info || starting) return;
    err = null;
    if (info.termsAccepted) {
      await start(pack);
      return;
    }
    try {
      terms = await unwrap(commands.getSpeechCoreTerms());
      pending = { pack, sizeBytes };
    } catch (e) {
      err = String(e);
    }
  }

  async function cancel() {
    try {
      await unwrap(commands.cancelSpeechCoreDownload());
    } catch (e) {
      err = String(e);
    }
  }

  async function remove(pack: string) {
    err = null;
    try {
      await unwrap(commands.deleteSpeechCorePack(pack));
      await refresh();
    } catch (e) {
      err = String(e);
    }
  }

  const canGet = (p: PackInfo) => p.status.type === "notInstalled" || p.status.type === "failed";
</script>

<div class="mb-3 rounded-lg border border-border p-3" data-testid="speech-core-section">
  {#if info && !info.supported}
    <p class="m-0 text-sm text-muted-foreground" data-testid="speech-core-unsupported">
      この環境では、内蔵の読み上げエンジンを使えません(対応: Linux・macOS・Windows の x64 / Linux と macOS の ARM)。外部のエンジンを使ってください。
    </p>
  {:else if info}
    <div class="mb-2 flex flex-wrap items-center gap-2 text-sm">
      <span class="font-medium" data-testid="speech-core-engine-label">エンジン一式(約 {formatBytes(info.engine.sizeBytes)})</span>
      <span class="text-xs text-muted-foreground" data-testid="speech-core-engine-status">{statusText(info.engine.status)}</span>
      {#if canGet(info.engine)}
        <Button variant="outline" size="sm" disabled={busy || starting} onclick={() => onGet("engine", info!.engine.sizeBytes)} data-testid="speech-core-engine-get">取得</Button>
      {:else if info.engine.status.type === "downloading"}
        <Button variant="outline" size="sm" onclick={cancel} data-testid="speech-core-engine-cancel">キャンセル</Button>
      {:else if info.engine.status.type === "ready"}
        <Button variant="outline" size="sm" disabled={busy} onclick={() => remove("engine")} data-testid="speech-core-engine-delete">削除</Button>
      {/if}
    </div>
    <p class="mb-2 mt-0 text-xs text-muted-foreground">エンジン一式は、CORE・ONNX Runtime・辞書です。声(VVM)は、使うものだけを取得します。</p>
    {#if !engineReady}
      <p class="mb-2 mt-0 text-xs text-muted-foreground" data-testid="speech-core-voices-hint">声の取得は、エンジン一式を取得してからできます。</p>
    {/if}
    <ul class="m-0 list-none p-0">
      {#each info.voices as v (v.id)}
        <li class="flex flex-wrap items-center gap-2 border-t border-border py-1.5 text-sm">
          <span class="min-w-0 flex-1 break-words" data-testid="speech-core-voice-{v.id}-label">
            {voiceCharactersLabel(v)}
            <span class="text-xs text-muted-foreground">(約 {formatBytes(v.pack.sizeBytes)})</span>
          </span>
          <span class="text-xs text-muted-foreground" data-testid="speech-core-voice-{v.id}-status">{statusText(v.pack.status)}</span>
          {#if canGet(v.pack)}
            <Button variant="outline" size="sm" disabled={!engineReady || busy || starting} onclick={() => onGet(v.id, v.pack.sizeBytes)} data-testid="speech-core-voice-{v.id}-get">取得</Button>
          {:else if v.pack.status.type === "downloading"}
            <Button variant="outline" size="sm" onclick={cancel} data-testid="speech-core-voice-{v.id}-cancel">キャンセル</Button>
          {:else if v.pack.status.type === "ready"}
            <Button variant="outline" size="sm" disabled={busy} onclick={() => remove(v.id)} data-testid="speech-core-voice-{v.id}-delete">削除</Button>
          {/if}
        </li>
      {/each}
    </ul>
  {:else}
    <p class="m-0 text-sm text-muted-foreground">読み込み中…</p>
  {/if}
  {#if err}<p class="my-2 break-words text-sm text-destructive" data-testid="speech-core-error">{err}</p>{/if}
</div>

{#if terms && pending}
  <SpeechCoreTermsDialog
    {terms}
    sizeBytes={pending.sizeBytes}
    busy={starting}
    onAccept={() => start(pending!.pack)}
    onCancel={() => {
      terms = null;
      pending = null;
    }}
  />
{/if}
