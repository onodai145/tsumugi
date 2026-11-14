<script lang="ts">
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { Button } from "$lib/components/ui/button";
  import { commands, events, unwrap } from "../../lib/ipc";
  import type { LocalModelInfo, LocalModelStatus } from "../../lib/ipc";
  import { formatBytes, progressPercent, statusText } from "../../lib/localModel";
  import ConfirmDialog from "../ConfirmDialog.svelte";
  import LicenseConsentDialog from "./LicenseConsentDialog.svelte";

  // ローカル翻訳(PLaMo 3 Translate)のモデルの状態・ダウンロード・削除(Issue #603)。
  // 状態はマウント時に Rust から取り、変化は Event で受ける。設定画面を閉じてもダウンロードは続くので、
  // 開き直したときは現在の進捗がそのまま見える。
  let info = $state<LocalModelInfo | null>(null);
  let status = $state<LocalModelStatus>({ type: "notInstalled" });
  let err = $state<string | null>(null);
  let consentOpen = $state(false);
  let deleteOpen = $state(false);

  // イベントを受けるたびに進める。refresh の応答が、それより新しいイベントの状態を古いスナップショットで上書きしないために使う。
  let eventSeq = 0;

  // `infoOnly` のときは status に触れない（イベントが運んだ状態を、スナップショットで上書きしない）。
  async function refresh(infoOnly = false) {
    const seqAtRequest = eventSeq;
    const i = await unwrap(commands.getLocalModelStatus());
    info = i;
    // 応答を待つ間にイベントが届いていたら、そちらが新しい。info（同意済みなど）だけ更新する。
    if (!infoOnly && eventSeq === seqAtRequest) status = i.status;
  }

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    (async () => {
      // 状態の取得より先にイベントを購読する。取得してから購読すると、その間に届いた終了イベント
      // （検証中→取得済み、ダウンロード中→失敗）を取りこぼし、以後イベントは来ないので表示が固まる。
      try {
        const u = await events.localModelStatusChanged.listen((e) => {
          eventSeq++;
          status = e.payload.status;
          // 止まった状態では、再開できる途中のファイルの有無（partialBytes）が変わっているので、取り直す。
          if (status.type === "notInstalled" || status.type === "failed" || status.type === "ready") {
            refresh(true).catch((e) => (err = String(e)));
          }
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

  // 開始を要求してから応答が返るまで true。連打で2回目を送って、Rust の「すでにダウンロード中です」を赤字で出さないため。
  let starting = $state(false);

  async function start() {
    if (starting || !info) return;
    err = null;
    starting = true;
    try {
      await unwrap(commands.startLocalModelDownload(info.licenseVersion));
      consentOpen = false;
      await refresh();
    } catch (e) {
      consentOpen = false;
      err = String(e);
    } finally {
      starting = false;
    }
  }

  // 同意済みの版が現行と同じなら、ダイアログを出さずに開始する(版が変わったときだけ再同意)。
  function onDownload() {
    if (info?.licenseAccepted) void start();
    else consentOpen = true;
  }

  async function cancel() {
    try {
      await unwrap(commands.cancelLocalModelDownload());
    } catch (e) {
      err = String(e);
    }
  }

  async function remove() {
    deleteOpen = false;
    err = null;
    try {
      await unwrap(commands.deleteLocalModel());
      await refresh();
    } catch (e) {
      err = String(e);
    }
  }
</script>

<div class="mt-1.5 rounded-lg border border-border p-3 text-sm" data-testid="local-model-card">
  {#if !info}
    <!-- 状態を読み込むまで、初期値の「未取得」やダウンロードボタンを出さない(進行中でも一瞬ボタンが見えてしまう)。 -->
    {#if err}
      <p class="m-0 text-xs text-destructive" role="alert">{err}</p>
    {:else}
      <p class="m-0 text-muted-foreground">読み込み中…</p>
    {/if}
  {:else if !info.supported}
    <p class="m-0 text-muted-foreground">この環境では PLaMo による翻訳は使えません。</p>
  {:else}
    <div class="flex flex-wrap items-center justify-between gap-2">
      <div class="flex flex-col gap-0.5">
        <span class="font-semibold">PLaMo 3 Translate（0.8B・Q4_K_M）</span>
        <span class="text-xs text-muted-foreground" data-testid="local-model-status">{statusText(status)}</span>
        <span class="text-xs text-muted-foreground">サイズ {formatBytes(info.sizeBytes)}</span>
        {#if (status.type === "notInstalled" || status.type === "failed") && info.partialBytes > 0}
          <span class="text-xs text-muted-foreground">
            途中まで取得済み（{formatBytes(info.partialBytes)}）。ダウンロードで続きから再開します
          </span>
        {/if}
      </div>
      <div class="flex gap-2">
        {#if status.type === "notInstalled" || status.type === "failed"}
          <Button size="sm" onclick={onDownload} disabled={starting} data-testid="local-model-download">ダウンロード</Button>
        {:else if status.type === "downloading"}
          <Button size="sm" variant="secondary" onclick={cancel} data-testid="local-model-cancel">キャンセル</Button>
        {:else if status.type === "ready"}
          <Button size="sm" variant="secondary" onclick={() => (deleteOpen = true)} data-testid="local-model-delete">削除</Button>
        {/if}
      </div>
    </div>
    {#if status.type === "downloading"}
      <div
        class="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-muted"
        role="progressbar"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={progressPercent(status.downloaded, status.total)}
        data-testid="local-model-progress"
      >
        <div class="h-full bg-primary" style={`width:${progressPercent(status.downloaded, status.total)}%`}></div>
      </div>
    {/if}
    {#if err}
      <p class="mb-0 mt-2 text-xs text-destructive" role="alert">{err}</p>
    {/if}
    <p class="mb-0 mt-2 text-xs text-muted-foreground">
      Built with PLaMo ·
      <button
        type="button"
        class="border-0 bg-transparent p-0 text-xs text-primary hover:underline"
        onclick={() => info && openUrl(info.licenseUrl)}
      >
        PLaMo Community License
      </button>
    </p>
  {/if}
</div>

{#if consentOpen && info}
  <LicenseConsentDialog
    sizeBytes={info.sizeBytes}
    busy={starting}
    onAccept={start}
    onCancel={() => (consentOpen = false)}
    onOpenLicense={() => info && openUrl(info.licenseUrl)}
  />
{/if}
{#if deleteOpen}
  <ConfirmDialog
    title="モデルを削除"
    message="取得した PLaMo のモデルを削除します。再び使うには、もう一度ダウンロードが必要です。"
    confirmLabel="削除"
    danger
    onConfirm={remove}
    onCancel={() => (deleteOpen = false)}
  />
{/if}
