<script lang="ts">
  import { onMount, tick } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import { commands, events, unwrap } from "../../lib/ipc";
  import type { PackInfo, SpeechCoreInfo, SpeechCoreTerms, VoicePackInfo } from "../../lib/ipc";
  import { formatBytes, statusText, UNKNOWN_NETWORK } from "../../lib/localModel";
  import { isMobilePlatform } from "../../lib/platform";
  import { coreDownloadWarning, voiceCharactersLabel } from "../../lib/speechCore";
  import ConfirmDialog from "../ConfirmDialog.svelte";
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
  /// スマホで、回線の確認を出している間だけ非 null。
  let networkConfirm = $state<{ pack: string; sizeBytes: number; message: string } | null>(null);
  let checking = $state(false);
  /// 確認を取りやめたときに、フォーカスを戻す先(押した取得ボタン)。
  let confirmTrigger: HTMLElement | null = null;

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
    void (async () => {
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

  /// スマホで、モバイル通信・従量制・判定できないときは、同意の前に回線の確認を出す(翻訳モデルの取得と同じ流儀)。
  /// デスクトップは回線を見ない(従来どおり)。状態を取得できなくても、不明として確認を出すだけで、続ければ開始できる。
  async function onGet(pack: string, sizeBytes: number, trigger: HTMLElement | null = null) {
    if (!info || starting || checking || networkConfirm) return;
    err = null;
    if (isMobilePlatform) {
      checking = true;
      let warning: string | null;
      try {
        warning = coreDownloadWarning(await commands.getNetworkStatus(), sizeBytes, true);
      } catch {
        warning = coreDownloadWarning(UNKNOWN_NETWORK, sizeBytes, true);
      } finally {
        checking = false;
      }
      if (warning) {
        confirmTrigger = trigger;
        networkConfirm = { pack, sizeBytes, message: warning };
        return;
      }
    }
    await proceed(pack, sizeBytes);
  }

  /// 回線の確認を取りやめる。閉じたダイアログの分、フォーカスが body に落ちるので、押した取得ボタンに戻す。
  async function cancelNetworkConfirm() {
    networkConfirm = null;
    const trigger = confirmTrigger;
    confirmTrigger = null;
    await tick();
    trigger?.focus();
  }

  /// 同意済みの版なら、そのまま開始する。未同意(規約が変わったときを含む)なら、規約を出して同意を取る。
  async function proceed(pack: string, sizeBytes: number) {
    if (!info || starting) return;
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

  /// 声のモデルの絞り込み(キャラクター名の部分一致。前後の空白と大文字小文字は無視する)。
  let search = $state("");
  /// 取得済み・取得中のものを上にまとめる(それ以外は元の順)。
  const visibleVoices = $derived.by(() => {
    if (!info) return [];
    const q = search.trim().toLowerCase();
    const hit = (v: VoicePackInfo) => q === "" || v.characters.some((c) => c.name.toLowerCase().includes(q));
    const rank = (v: VoicePackInfo) => (["ready", "downloading", "verifying"].includes(v.pack.status.type) ? 0 : 1);
    return info.voices
      .map((v, i) => ({ v, i }))
      .filter(({ v }) => hit(v))
      .sort((a, b) => rank(a.v) - rank(b.v) || a.i - b.i)
      .map(({ v }) => v);
  });

  const canGet = (p: PackInfo) => p.status.type === "notInstalled" || p.status.type === "failed";
</script>

<div class="mb-3 rounded-lg border border-border p-3" data-testid="speech-core-section">
  {#if info && !info.supported}
    <p class="m-0 text-sm text-muted-foreground" data-testid="speech-core-unsupported">
      この環境では、内蔵の読み上げエンジンを使えません(対応: Linux・macOS の x64 と ARM64、Windows の x64、Android の ARM64 と x86_64)。外部のエンジンを使ってください。
    </p>
  {:else if info}
    <div class="mb-2 flex flex-wrap items-center gap-2 text-sm">
      <span class="font-medium" data-testid="speech-core-engine-label">エンジン一式(約 {formatBytes(info.engine.sizeBytes)})</span>
      <span class="text-xs text-muted-foreground" data-testid="speech-core-engine-status">{statusText(info.engine.status)}</span>
      {#if canGet(info.engine)}
        <Button variant="outline" size="sm" disabled={busy || starting || checking || !!networkConfirm} onclick={(e) => onGet("engine", info!.engine.sizeBytes, e.currentTarget)} data-testid="speech-core-engine-get">取得</Button>
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
    <p class="mb-1 mt-2 text-sm font-medium">声のモデル</p>
    <p class="mb-2 mt-0 text-xs text-muted-foreground" data-testid="speech-core-voices-intro">
      1 つのファイルに複数のキャラクターが入っています。使いたいキャラクターが入っているものを取得してください(同じキャラクターが、複数のファイルに入っていることもあります)。
    </p>
    <input
      class="mb-2 w-full rounded-lg border border-border bg-muted px-2.5 py-1.5 font-[inherit] text-sm text-foreground"
      placeholder="キャラクター名で絞り込む"
      aria-label="キャラクター名で絞り込む"
      data-testid="speech-core-search"
      bind:value={search}
    />
    {#if visibleVoices.length === 0}
      <p class="m-0 text-sm text-muted-foreground" data-testid="speech-core-no-match">該当する声がありません。</p>
    {/if}
    <ul class="m-0 list-none p-0">
      {#each visibleVoices as v (v.id)}
        <li class="border-t border-border py-1.5 text-sm" data-testid="speech-core-row-{v.id}">
          <div class="flex flex-wrap items-center gap-2">
            <span class="min-w-0 flex-1 break-words" data-testid="speech-core-voice-{v.id}-label">
              <span class="font-medium">{voiceCharactersLabel(v)}</span>
              <span class="text-xs text-muted-foreground">(約 {formatBytes(v.pack.sizeBytes)})</span>
            </span>
            <span class="text-xs text-muted-foreground" data-testid="speech-core-voice-{v.id}-status">{statusText(v.pack.status)}</span>
            {#if canGet(v.pack)}
              <Button variant="outline" size="sm" disabled={!engineReady || busy || starting || checking || !!networkConfirm} onclick={(e) => onGet(v.id, v.pack.sizeBytes, e.currentTarget)} data-testid="speech-core-voice-{v.id}-get">取得</Button>
            {:else if v.pack.status.type === "downloading"}
              <Button variant="outline" size="sm" onclick={cancel} data-testid="speech-core-voice-{v.id}-cancel">キャンセル</Button>
            {:else if v.pack.status.type === "ready"}
              <Button variant="outline" size="sm" disabled={busy} onclick={() => remove(v.id)} data-testid="speech-core-voice-{v.id}-delete">削除</Button>
            {/if}
          </div>
          <details class="mt-1 text-xs text-muted-foreground">
            <summary class="cursor-pointer">含まれるスタイル</summary>
            <ul class="my-1 list-none pl-3" data-testid="speech-core-voice-{v.id}-styles">
              {#each v.characters as c, ci (ci)}
                <li>{c.name}: {c.styles.map((st) => st.name).join("・")}</li>
              {/each}
            </ul>
          </details>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="m-0 text-sm text-muted-foreground">読み込み中…</p>
  {/if}
  {#if err}<p class="my-2 break-words text-sm text-destructive" data-testid="speech-core-error">{err}</p>{/if}
</div>

{#if networkConfirm}
  <ConfirmDialog
    title="取得の確認"
    message={networkConfirm.message}
    confirmLabel="取得"
    onConfirm={() => {
      const c = networkConfirm!;
      networkConfirm = null;
      void proceed(c.pack, c.sizeBytes);
    }}
    onCancel={cancelNetworkConfirm}
  />
{/if}

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
